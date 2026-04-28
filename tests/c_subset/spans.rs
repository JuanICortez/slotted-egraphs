// Source-position tracking for the C-subset language.
//
// IDEA: attach source spans (byte offsets) to e-classes via the `Analysis`
// trait. When the e-graph merges two e-classes (e.g. via commutativity),
// `Analysis::merge` automatically unions the spans. So an e-class's data
// always reflects every source position where that exact (semantic) thing
// appeared.
//
// SCOPE: single source file, no incremental updates. `Span` only carries
// byte offsets; multi-file support would add a `file_id` field.
//
// USAGE:
//   1. Parse the source: `let (expr, span_tree) = parse_with_spans(src)?;`
//   2. Insert with spans: `add_expr_with_spans(&mut eg, &expr, &span_tree)`
//   3. Search:            `semantic_search_with_spans(&eg, &pattern)`
//   4. Each result includes the source positions of every place that
//      e-class appeared.

use crate::*;

// --- Span ---

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    /// Extract the substring covered by this span.
    pub fn slice<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }
}

// --- SpanTree: parallels RecExpr's shape ---

#[derive(Clone, Debug)]
pub struct SpanTree {
    pub span: Span,
    pub children: Vec<SpanTree>,
}

impl SpanTree {
    pub fn leaf(span: Span) -> Self {
        SpanTree {
            span,
            children: Vec::new(),
        }
    }

    pub fn node(span: Span, children: Vec<SpanTree>) -> Self {
        SpanTree { span, children }
    }
}

// --- SpanAnalysis: stores the set of source spans per e-class ---

#[derive(Default, Clone)]
pub struct SpanAnalysis;

impl Analysis<CSubset> for SpanAnalysis {
    type Data = SmallHashSet<Span>;

    /// New e-nodes start with no recorded spans. Spans are populated AFTER
    /// insertion via `analysis_data_mut`. E-nodes synthesized by rewrites
    /// (not from source) correctly stay at empty until they're merged with
    /// e-classes that have spans.
    fn make(_eg: &EGraph<CSubset, Self>, _enode: &CSubset) -> SmallHashSet<Span> {
        SmallHashSet::default()
    }

    /// On e-class merge (caused by union or rewrite), union the span sets.
    fn merge(l: SmallHashSet<Span>, r: SmallHashSet<Span>) -> SmallHashSet<Span> {
        let mut out = l;
        for s in r {
            out.insert(s);
        }
        out
    }
}

// --- add_expr_with_spans: span-aware insertion ---

/// Insert a `RecExpr` into the e-graph, recording each sub-expression's
/// source span in the analysis data.
///
/// Panics if the structure of `span_tree` doesn't match `expr` (one child
/// per `applied_id_occurrence`).
pub fn add_expr_with_spans(
    eg: &mut EGraph<CSubset, SpanAnalysis>,
    expr: &RecExpr<CSubset>,
    span_tree: &SpanTree,
) -> AppliedId {
    // Children first: recursively insert each sub-expression. The result
    // is a list of AppliedIds for the children.
    assert_eq!(
        expr.children.len(),
        span_tree.children.len(),
        "SpanTree shape doesn't match RecExpr: expected {} children, got {}",
        expr.children.len(),
        span_tree.children.len()
    );

    let child_ids: Vec<AppliedId> = expr
        .children
        .iter()
        .zip(span_tree.children.iter())
        .map(|(c, st)| add_expr_with_spans(eg, c, st))
        .collect();

    // Build the e-node by patching the AppliedId placeholders in `expr.node`
    // with the actual AppliedIds from the children.
    let mut node = expr.node.clone();
    {
        let mut refs: Vec<&mut AppliedId> = node.applied_id_occurrences_mut();
        assert_eq!(
            refs.len(),
            child_ids.len(),
            "applied_id_occurrences count mismatches children count"
        );
        for (i, slot) in refs.iter_mut().enumerate() {
            **slot = child_ids[i].clone();
        }
    }

    // Insert the node into the e-graph
    let id = eg.add_syn(node);

    // Record the source span for this e-class
    eg.analysis_data_mut(id.id).insert(span_tree.span);

    id
}

// --- Position-tracking parser ---

/// Parse a source string into a `RecExpr` AND a `SpanTree` of byte offsets.
///
/// The `SpanTree` mirrors the structure of the `RecExpr`: one child per
/// `AppliedId` field. Slots (e.g. `$x`) and operator keywords (e.g. `add`)
/// do NOT contribute SpanTree children — they're part of their parent node.
pub fn parse_with_spans(src: &str) -> Result<(RecExpr<CSubset>, SpanTree), String> {
    let expr: RecExpr<CSubset> =
        RecExpr::parse(src).map_err(|e| format!("RecExpr::parse failed: {:?}", e))?;
    let (span_tree, rest) = parse_span_tree(src, 0)?;

    // Verify the rest of the source is just whitespace
    if !src[rest..].trim().is_empty() {
        return Err(format!("trailing input: {:?}", &src[rest..]));
    }

    Ok((expr, span_tree))
}

fn skip_whitespace(src: &str, mut offset: usize) -> usize {
    let bytes = src.as_bytes();
    while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
        offset += 1;
    }
    offset
}

/// Consume an atom (ident, number, slot, or pvar) starting at `offset`.
/// Returns the offset just past the end of the atom.
fn consume_atom(src: &str, mut offset: usize) -> usize {
    let bytes = src.as_bytes();
    while offset < bytes.len() {
        let c = bytes[offset];
        if c.is_ascii_whitespace() || c == b'(' || c == b')' || c == b'[' || c == b']' {
            break;
        }
        offset += 1;
    }
    offset
}

/// Parse a single expression starting at `offset`, returning its SpanTree
/// and the offset just past the end of the expression.
fn parse_span_tree(src: &str, offset: usize) -> Result<(SpanTree, usize), String> {
    let bytes = src.as_bytes();
    let mut offset = skip_whitespace(src, offset);

    if offset >= bytes.len() {
        return Err("unexpected end of input".to_string());
    }

    let start = offset;

    if bytes[offset] == b'(' {
        // Paren form: (op child1 child2 ...)
        offset += 1; // consume '('

        // Skip whitespace, then the operator atom
        offset = skip_whitespace(src, offset);
        offset = consume_atom(src, offset);

        let mut children = Vec::new();
        loop {
            offset = skip_whitespace(src, offset);

            if offset >= bytes.len() {
                return Err("unterminated paren expression".to_string());
            }

            if bytes[offset] == b')' {
                offset += 1; // consume ')'
                break;
            }

            // Slots and pattern-vars don't contribute SpanTree children —
            // they're args of the operator, not separate sub-expressions.
            let c = bytes[offset];
            if c == b'$' || c == b'?' {
                offset = consume_atom(src, offset);
            } else {
                let (child, next) = parse_span_tree(src, offset)?;
                children.push(child);
                offset = next;
            }
        }

        Ok((SpanTree::node(Span::new(start, offset), children), offset))
    } else {
        // Atom: bare ident, number, etc. — leaf in the SpanTree.
        let end = consume_atom(src, offset);
        if end == offset {
            return Err(format!(
                "unexpected character at offset {}: {:?}",
                offset,
                src[offset..].chars().next().unwrap_or(' ')
            ));
        }
        Ok((SpanTree::leaf(Span::new(start, end)), end))
    }
}

// --- Search with span lookup ---

/// A `SearchResult` augmented with the source positions where the matched
/// e-class appears in the original program.
#[derive(Debug)]
pub struct SearchResultWithSpans {
    pub eclass: AppliedId,
    pub subst: Subst,
    pub matched_expr: RecExpr<CSubset>,
    /// All source positions where this e-class was inserted. Multiple spans
    /// arise when structurally-equivalent expressions appear in multiple
    /// places (they collapse into one e-class) or when rewrites union
    /// e-classes that started at different source positions.
    pub spans: SmallHashSet<Span>,
}

/// Like `semantic_search`, but typed for the `SpanAnalysis` and additionally
/// returns the source positions of every match.
pub fn semantic_search_with_spans(
    eg: &EGraph<CSubset, SpanAnalysis>,
    pattern: &Pattern<CSubset>,
) -> Vec<SearchResultWithSpans> {
    let results = semantic_search(eg, pattern);
    results
        .into_iter()
        .map(|r| {
            let spans = eg.analysis_data(r.eclass.id).clone();
            SearchResultWithSpans {
                eclass: r.eclass,
                subst: r.subst,
                matched_expr: r.matched_expr,
                spans,
            }
        })
        .collect()
}

// --- Module-internal tests for the analysis & insertion machinery ---

#[cfg(test)]
mod analysis_tests {
    use super::*;

    #[test]
    fn span_analysis_make_returns_empty() {
        let eg = EGraph::<CSubset, SpanAnalysis>::default();
        let enode = CSubset::Num(42);
        let data = SpanAnalysis::make(&eg, &enode);
        assert!(data.is_empty(), "make should produce no spans");
    }

    #[test]
    fn span_analysis_merge_unions() {
        let mut a: SmallHashSet<Span> = SmallHashSet::default();
        a.insert(Span::new(0, 5));
        a.insert(Span::new(10, 15));

        let mut b: SmallHashSet<Span> = SmallHashSet::default();
        b.insert(Span::new(10, 15)); // overlap with a
        b.insert(Span::new(20, 25));

        let merged = SpanAnalysis::merge(a, b);
        assert_eq!(merged.len(), 3, "merge should union the two sets");
    }

    #[test]
    fn add_expr_with_spans_records_root_span() {
        // Insert a numeric literal at a known position; verify the span.
        let mut eg = EGraph::<CSubset, SpanAnalysis>::default();
        let expr: RecExpr<CSubset> = RecExpr::parse("42").unwrap();
        let span_tree = SpanTree::leaf(Span::new(0, 2));

        let id = add_expr_with_spans(&mut eg, &expr, &span_tree);
        let spans = eg.analysis_data(id.id);

        assert_eq!(spans.len(), 1);
        assert!(spans.contains(&Span::new(0, 2)));
    }

    #[test]
    fn add_expr_with_spans_records_subexpr_spans() {
        // (add 1 2) at byte offsets [0, 9].
        // Children: 1 at [5, 6], 2 at [7, 8].
        let mut eg = EGraph::<CSubset, SpanAnalysis>::default();
        let expr: RecExpr<CSubset> = RecExpr::parse("(add 1 2)").unwrap();
        let span_tree = SpanTree::node(
            Span::new(0, 9),
            vec![
                SpanTree::leaf(Span::new(5, 6)),
                SpanTree::leaf(Span::new(7, 8)),
            ],
        );

        let root = add_expr_with_spans(&mut eg, &expr, &span_tree);

        // Root e-class has the outer span
        assert!(eg.analysis_data(root.id).contains(&Span::new(0, 9)));

        // Find the children's e-classes and check their spans
        // (each child is its own e-class; we look them up by parsing & lookup)
        let one_expr: RecExpr<CSubset> = RecExpr::parse("1").unwrap();
        let two_expr: RecExpr<CSubset> = RecExpr::parse("2").unwrap();
        let one_id = lookup_rec_expr(&one_expr, &eg).expect("1 should be in egraph");
        let two_id = lookup_rec_expr(&two_expr, &eg).expect("2 should be in egraph");

        assert!(eg.analysis_data(one_id.id).contains(&Span::new(5, 6)));
        assert!(eg.analysis_data(two_id.id).contains(&Span::new(7, 8)));
    }

    #[test]
    fn parse_with_spans_simple_atom() {
        let src = "42";
        let (_expr, st) = parse_with_spans(src).unwrap();
        assert_eq!(st.span, Span::new(0, 2));
        assert!(st.children.is_empty());
    }

    #[test]
    fn parse_with_spans_paren_form() {
        // "(add 1 2)"
        //  012345678
        let src = "(add 1 2)";
        let (_expr, st) = parse_with_spans(src).unwrap();

        // Root span covers the whole expression
        assert_eq!(st.span, Span::new(0, 9));
        // Two AppliedId children (the 1 and the 2)
        assert_eq!(st.children.len(), 2);
        assert_eq!(st.children[0].span, Span::new(5, 6)); // "1"
        assert_eq!(st.children[1].span, Span::new(7, 8)); // "2"
    }

    #[test]
    fn parse_with_spans_slot_does_not_count_as_child() {
        // "(var $x)" — Var has a Slot, not an AppliedId, so 0 SpanTree children
        //  01234567
        let src = "(var $x)";
        let (_expr, st) = parse_with_spans(src).unwrap();
        assert_eq!(st.span, Span::new(0, 8));
        assert!(
            st.children.is_empty(),
            "var with slot has no SpanTree children"
        );
    }

    #[test]
    fn parse_with_spans_nested() {
        // "(add (var $x) (var $y))"
        //  0123456789012345678901234
        //            1111111111222222
        let src = "(add (var $x) (var $y))";
        let (_expr, st) = parse_with_spans(src).unwrap();

        assert_eq!(st.span, Span::new(0, 23));
        assert_eq!(st.children.len(), 2);
        assert_eq!(st.children[0].span, Span::new(5, 13)); // "(var $x)"
        assert_eq!(st.children[1].span, Span::new(14, 22)); // "(var $y)"
    }

    #[test]
    fn parse_with_spans_end_to_end_inserts_spans() {
        // Full pipeline: parse → insert → verify spans landed on the right e-classes.
        let src = "(add (var $x) (var $y))";
        let mut eg = EGraph::<CSubset, SpanAnalysis>::default();

        let (expr, span_tree) = parse_with_spans(src).unwrap();
        let root = add_expr_with_spans(&mut eg, &expr, &span_tree);

        // Outer add: span [0, 23]
        assert!(eg.analysis_data(root.id).contains(&Span::new(0, 23)));

        // (var $x): span [5, 13]
        let vx: RecExpr<CSubset> = RecExpr::parse("(var $x)").unwrap();
        let vx_id = lookup_rec_expr(&vx, &eg).unwrap();
        assert!(eg.analysis_data(vx_id.id).contains(&Span::new(5, 13)));

        // (var $y): span [14, 22]
        let vy: RecExpr<CSubset> = RecExpr::parse("(var $y)").unwrap();
        let vy_id = lookup_rec_expr(&vy, &eg).unwrap();
        assert!(eg.analysis_data(vy_id.id).contains(&Span::new(14, 22)));
    }

    #[test]
    fn merge_via_union_unions_spans() {
        // Insert (add x y) at one position and (add y x) at another.
        // Manually union them; the merged e-class should have BOTH spans.
        let mut eg = EGraph::<CSubset, SpanAnalysis>::default();

        let xy: RecExpr<CSubset> = RecExpr::parse("(add (var $x) (var $y))").unwrap();
        let yx: RecExpr<CSubset> = RecExpr::parse("(add (var $y) (var $x))").unwrap();

        let xy_spans = SpanTree::node(
            Span::new(0, 23),
            vec![
                SpanTree::leaf(Span::new(5, 12)),
                SpanTree::leaf(Span::new(14, 22)),
            ],
        );
        let yx_spans = SpanTree::node(
            Span::new(100, 123),
            vec![
                SpanTree::leaf(Span::new(105, 112)),
                SpanTree::leaf(Span::new(114, 122)),
            ],
        );

        let id_xy = add_expr_with_spans(&mut eg, &xy, &xy_spans);
        let id_yx = add_expr_with_spans(&mut eg, &yx, &yx_spans);

        // Manually union the two e-classes (simulating commutativity)
        eg.union(&id_xy, &id_yx);

        // After union, the merged e-class should carry both root spans
        let merged_spans = eg.analysis_data(id_xy.id);
        assert!(
            merged_spans.contains(&Span::new(0, 23)),
            "merged e-class should retain xy's span"
        );
        assert!(
            merged_spans.contains(&Span::new(100, 123)),
            "merged e-class should retain yx's span"
        );
    }

    #[test]
    fn search_returns_source_positions() {
        // HEADLINE DEMO. Program with two structurally-identical inner adds:
        //   "(add (add (var $x) (var $y)) (add (var $a) (var $b)))"
        //    0    5    10   15   20  25   30   35   40   45   50
        //
        // semantic_search returns 2 e-class matches (outer + collapsed inner).
        // But the inner-add e-class carries TWO spans — one for (x+y), one
        // for (a+b) — so the source-level info is preserved.
        let src = "(add (add (var $x) (var $y)) (add (var $a) (var $b)))";

        let mut eg = EGraph::<CSubset, SpanAnalysis>::default();
        let (expr, span_tree) = parse_with_spans(src).unwrap();
        let _root = add_expr_with_spans(&mut eg, &expr, &span_tree);

        let pattern = Pattern::parse("(add ?a ?b)").unwrap();
        let results = semantic_search_with_spans(&eg, &pattern);

        // 2 distinct e-class matches (outer add, and the inner-add class)
        assert_eq!(results.len(), 2, "expected 2 e-class matches");

        // Total spans across all matches should be 3 (outer + x+y + a+b)
        let total_spans: usize = results.iter().map(|r| r.spans.len()).sum();
        assert_eq!(
            total_spans, 3,
            "expected 3 source-level occurrences across all matches, got {}",
            total_spans
        );

        // One e-class should have exactly 1 span (the outer add)
        // Another should have exactly 2 spans (the two inner adds)
        let mut span_counts: Vec<usize> = results.iter().map(|r| r.spans.len()).collect();
        span_counts.sort();
        assert_eq!(span_counts, vec![1, 2]);

        // Sanity: verify the spans match the actual source positions of the inner adds.
        let inner_add_class = results.iter().find(|r| r.spans.len() == 2).unwrap();
        let inner_xy_span = src.find("(add (var $x) (var $y))").unwrap();
        let inner_xy_end = inner_xy_span + "(add (var $x) (var $y))".len();
        let inner_ab_span = src.rfind("(add (var $a) (var $b))").unwrap();
        let inner_ab_end = inner_ab_span + "(add (var $a) (var $b))".len();

        assert!(inner_add_class
            .spans
            .contains(&Span::new(inner_xy_span, inner_xy_end)));
        assert!(inner_add_class
            .spans
            .contains(&Span::new(inner_ab_span, inner_ab_end)));
    }

    #[test]
    fn search_spans_after_rewrite_merge() {
        // Insert (add (var $x) (var $y)) and (add (var $y) (var $x)) at
        // different positions, apply commutativity to merge them, and verify
        // the merged e-class carries spans from BOTH original positions.
        //
        // To put them in the same e-graph at distinct source positions, we
        // build a contrived single-source-string:
        //   "(seq (add (var $x) (var $y)) (add (var $y) (var $x)))"
        //
        // The two inner adds are structurally distinct (x+y vs y+x), so
        // initially they're in separate e-classes. After applying add-comm,
        // they merge into one e-class — which then has BOTH source spans.
        let src = "(seq (add (var $x) (var $y)) (add (var $y) (var $x)))";

        let mut eg = EGraph::<CSubset, SpanAnalysis>::default();
        let (expr, span_tree) = parse_with_spans(src).unwrap();
        let _root = add_expr_with_spans(&mut eg, &expr, &span_tree);

        // Record spans BEFORE rewriting
        let xy_position = src.find("(add (var $x) (var $y))").unwrap();
        let xy_end = xy_position + "(add (var $x) (var $y))".len();
        let yx_position = src.find("(add (var $y) (var $x))").unwrap();
        let yx_end = yx_position + "(add (var $y) (var $x))".len();

        // Apply commutativity to merge x+y with y+x
        let rules = get_c_subset_rewrites_for::<SpanAnalysis>();
        let mut runner: Runner<CSubset, SpanAnalysis, (), ()> =
            Runner::new(SpanAnalysis).with_egraph(eg).with_iter_limit(3);
        runner.run(&rules);
        let eg = &runner.egraph;

        // Now look up the (add (var $x) (var $y)) e-class — its analysis data
        // should contain BOTH spans because of the commutativity merge.
        let xy_re: RecExpr<CSubset> = RecExpr::parse("(add (var $x) (var $y))").unwrap();
        let xy_id = lookup_rec_expr(&xy_re, eg).unwrap();
        let spans = eg.analysis_data(xy_id.id);

        assert!(
            spans.contains(&Span::new(xy_position, xy_end)),
            "merged e-class should retain xy's span"
        );
        assert!(
            spans.contains(&Span::new(yx_position, yx_end)),
            "merged e-class should retain yx's span (after add-comm merge)"
        );
    }
}
