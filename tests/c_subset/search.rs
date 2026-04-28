// SEMANTIC SEARCH OVER THE E-GRAPH
//
// Two fundamentally different questions you might ask of a program e-graph:
//
//   (a) "How many DISTINCT E-CLASSES match my pattern?"
//       This is what `semantic_search` answers. Two structurally-equivalent
//       code snippets (e.g. `(x + y)` and `(a + b)`) live in the SAME
//       e-class and count as ONE match. This is the right notion when you
//       care about "how many semantically-different things match" — e.g.
//       finding "all distinct ways to compute the result" after saturation.
//
//   (b) "How many SOURCE-LEVEL OCCURRENCES match my pattern?"
//       This is the AST-walk view: every position in the original code that
//       matches counts separately, even if those positions are equivalent.
//       e-graphs intrinsically lose this information by collapsing
//       equivalent expressions, so we'd need a separate algorithm that
//       walks the e-graph constructively from a root and counts each
//       matching enode along the way.
//
// We currently implement only (a). An (b)-style `occurrence_search` is
// planned (see backlog) — the user use case for the C semantic-search tool
// is likely to want occurrence counts in some scenarios.
//
// EXAMPLE of why these differ:
//   Program: (add (add (var $x) (var $y)) (add (var $a) (var $b)))
//   Pattern: (add ?a ?b)
//
//   Source view  : 3 occurrences (outer add, x+y, a+b)
//   E-class view : 2 matches — because (x+y) and (a+b) are structurally
//                  identical and live in the SAME e-class. The e-graph has:
//                    - 1 e-class for the outer add
//                    - 1 e-class for "add of two distinct vars" — holds
//                      BOTH (x+y) and (a+b) simultaneously.

use crate::*;

/// A result from searching the e-graph for a subexpression matching a pattern.
#[derive(Debug)]
pub struct SearchResult<L: Language> {
    /// The e-class where the match was found.
    pub eclass: AppliedId,
    /// The variable bindings from the pattern match.
    pub subst: Subst,
    /// A concrete expression extracted from the matched e-class (smallest by AST size).
    pub matched_expr: RecExpr<L>,
}

/// Search the e-graph for all DISTINCT E-CLASSES whose contents match `pattern`.
///
/// Returns one `SearchResult` per matching e-class. Two structurally-equivalent
/// code snippets live in the same e-class and produce a single result —
/// see the module doc comment for the distinction between "e-class search"
/// and "occurrence search".
///
/// Returns a `SearchResult` for each match found, including:
/// - which e-class matched
/// - the substitution (binding pattern variables to e-classes)
/// - a concrete extracted expression from that e-class
///
/// This is the core of semantic code search: insert a program, saturate with
/// rewrites, then call this to find all places that match a query pattern.
pub fn semantic_search<L: Language + 'static, N: Analysis<L> + 'static>(
    eg: &EGraph<L, N>,
    pattern: &Pattern<L>,
) -> Vec<SearchResult<L>> {
    let extractor = Extractor::<L, AstSize>::new(eg, AstSize);

    let substs = ematch_all(eg, pattern);

    let mut results = Vec::new();
    let mut seen_classes = HashSet::default();

    for subst in substs {
        // Reconstruct which e-class was matched by instantiating the pattern root.
        // The matched e-class is the one that the full pattern matches against.
        let eclass = match pattern_root_eclass(eg, pattern, &subst) {
            Some(id) => id,
            None => continue,
        };

        let canonical = eg.find_applied_id(&eclass).id;
        if seen_classes.contains(&canonical) {
            continue;
        }
        seen_classes.insert(canonical);

        let matched_expr = extractor.extract(&eclass, eg);

        results.push(SearchResult {
            eclass,
            subst,
            matched_expr,
        });
    }

    results
}

/// Given a pattern and a substitution, figure out which e-class the pattern
/// root matched against by inserting the instantiated pattern and looking it up.
fn pattern_root_eclass<L: Language, N: Analysis<L>>(
    eg: &EGraph<L, N>,
    pattern: &Pattern<L>,
    subst: &Subst,
) -> Option<AppliedId> {
    match pattern {
        Pattern::PVar(v) => subst.get(v).cloned(),
        Pattern::ENode(n, children) => {
            let mut n = n.clone();
            let mut refs: Vec<&mut AppliedId> = n.applied_id_occurrences_mut();
            if refs.len() != children.len() {
                return None;
            }
            for i in 0..refs.len() {
                *(refs[i]) = pattern_root_eclass(eg, &children[i], subst)?;
            }
            eg.lookup(&n)
        }
        Pattern::Subst(..) => None,
    }
}

// --- Tests ---

#[test]
fn search_finds_exact_match() {
    // Program: x + y
    // Query:   ?a + ?b (any addition)
    let mut eg = EGraph::<CSubset>::default();
    let _program = id("(add (var $x) (var $y))", &mut eg);

    let pattern = Pattern::parse("(add ?a ?b)").unwrap();
    let results = semantic_search(&eg, &pattern);

    assert_eq!(results.len(), 1);
}

#[test]
fn search_finds_subexpression() {
    // Program: (x + y) * z
    // Query:   ?a + ?b (find the addition inside the multiplication)
    let mut eg = EGraph::<CSubset>::default();
    let _program = id("(mul (add (var $x) (var $y)) (var $z))", &mut eg);

    let pattern = Pattern::parse("(add ?a ?b)").unwrap();
    let results = semantic_search(&eg, &pattern);

    assert_eq!(results.len(), 1);
    // The matched expression should be (add (var $x) (var $y))
    let expr_str = format!("{}", results[0].matched_expr);
    assert!(expr_str.contains("add"));
}

#[test]
fn search_finds_equivalent_after_rewrite() {
    // Program: y + x
    // Query:   x + y
    // After applying commutativity, these should be in the same e-class.
    let rewrites = get_c_subset_rewrites();
    let program_str = "(add (var $y) (var $x))";
    let query_str = "(add (var $x) (var $y))";

    let program: RecExpr<CSubset> = RecExpr::parse(program_str).unwrap();
    let query: RecExpr<CSubset> = RecExpr::parse(query_str).unwrap();

    let mut runner: Runner<CSubset, (), (), ()> =
        Runner::default().with_expr(&program).with_iter_limit(3);
    runner.run(&rewrites);

    // Now search for the query pattern in the saturated e-graph
    let query_pattern = re_to_pattern(&query);
    let results = semantic_search(&runner.egraph, &query_pattern);

    assert!(
        !results.is_empty(),
        "Should find (add (var $x) (var $y)) after commutativity rewrite"
    );
}

#[test]
fn search_no_false_positives() {
    // Program: x + y
    // Query:   ?a * ?b (multiplication — should not match)
    let mut eg = EGraph::<CSubset>::default();
    let _program = id("(add (var $x) (var $y))", &mut eg);

    let pattern = Pattern::parse("(mul ?a ?b)").unwrap();
    let results = semantic_search(&eg, &pattern);

    assert!(results.is_empty());
}

#[test]
fn search_multiple_matches() {
    // Program: (x + y) + (a + b)
    // Query:   (add ?a ?b)
    //
    // Source view: 3 occurrences of `add` (outer, x+y, a+b).
    // E-class view: 2 matches — `(x+y)` and `(a+b)` are STRUCTURALLY
    // IDENTICAL up to slot renaming, so they collapse into the SAME e-class.
    // The e-graph contains:
    //   - 1 e-class for the outer add
    //   - 1 e-class for "add of two distinct vars" (holds both x+y AND a+b)
    //
    // `semantic_search` returns DISTINCT e-classes, so the answer is 2.
    // For source-level occurrence counts, see the planned `occurrence_search`
    // (backlog).
    let mut eg = EGraph::<CSubset>::default();
    let _program = id(
        "(add (add (var $x) (var $y)) (add (var $a) (var $b)))",
        &mut eg,
    );

    let pattern = Pattern::parse("(add ?a ?b)").unwrap();
    let results = semantic_search(&eg, &pattern);

    assert_eq!(
        results.len(),
        2,
        "Expected 2 e-class matches (outer + collapsed inner pattern), got {}",
        results.len()
    );
}

#[test]
fn search_multiple_matches_structurally_distinct() {
    // Same shape as above, but the inner adds use DIFFERENT operators on the
    // children, so they don't collapse into a single e-class.
    //   (add (add (var $x) (var $y)) (add (var $x) (mul (var $y) (var $y))))
    //
    // Now we should get 3 e-class matches:
    //   - outer add
    //   - inner add (x + y)
    //   - inner add (x + y*y) — different shape from the first inner add
    let mut eg = EGraph::<CSubset>::default();
    let _program = id(
        "(add (add (var $x) (var $y)) (add (var $x) (mul (var $y) (var $y))))",
        &mut eg,
    );

    let pattern = Pattern::parse("(add ?a ?b)").unwrap();
    let results = semantic_search(&eg, &pattern);

    assert_eq!(
        results.len(),
        3,
        "Expected 3 e-class matches when inner adds are structurally distinct, got {}",
        results.len()
    );
}

#[test]
fn search_alpha_equivalent() {
    // Program: fun main($x) ret (x + x)
    // Query:   fun main($y) ret (y + y) (same function, different parameter name)
    // Slotted e-graphs should handle this automatically via MultiBind.
    let mut eg = EGraph::<CSubset>::default();
    let _program = id("(fun main $0 (ret (add (var $0) (var $0))))", &mut eg);

    let query: RecExpr<CSubset> =
        RecExpr::parse("(fun main $1 (ret (add (var $1) (var $1))))").unwrap();
    let query_pattern = re_to_pattern(&query);
    let results = semantic_search(&eg, &query_pattern);

    assert!(
        !results.is_empty(),
        "Alpha-equivalent expressions should match"
    );
}

#[test]
fn search_semantic_equivalence_ite() {
    // Program: if (!flag) (y + x) else 0
    // Query:   if (flag) 0 else (x + y)
    // These are semantically equivalent via ite-not + add-comm.
    let rewrites = get_c_subset_rewrites();
    let program_str = "(ite (not (var $flag)) (add (var $y) (var $x)) 0)";
    let query_str = "(ite (var $flag) 0 (add (var $x) (var $y)))";

    let program: RecExpr<CSubset> = RecExpr::parse(program_str).unwrap();
    let query: RecExpr<CSubset> = RecExpr::parse(query_str).unwrap();

    let mut runner: Runner<CSubset, (), (), ()> =
        Runner::default().with_expr(&program).with_iter_limit(5);
    runner.run(&rewrites);

    let query_pattern = re_to_pattern(&query);
    let results = semantic_search(&runner.egraph, &query_pattern);

    assert!(
        !results.is_empty(),
        "Should find semantically equivalent if-then-else after rewrites"
    );
}

#[test]
fn search_finds_increment_in_function() {
    // C source:
    //   int main() { int x = 0; x = x + 1; return x; }
    //
    // ABT (with x hoisted into the function's MultiBind):
    //   (fun main $x
    //     (seq (assign $x 0)
    //     (seq (assign $x (add (var $x) 1))
    //          (ret (var $x)))))
    //
    // Query: x++ — i.e., the increment statement `x = x + 1`
    //   (assign $x (add (var $x) 1))
    //
    // We should find this as a subexpression of the program. Even though
    // `$x` is bound by the function, the increment lives in its own e-class
    // whose public slot is `$x`, and the search pattern matches against it.
    let program_str = "(fun main $x \
        (seq (assign $x 0) \
        (seq (assign $x (add (var $x) 1)) \
             (ret (var $x)))))";

    let mut eg = EGraph::<CSubset>::default();
    let _program = eg.add_expr(RecExpr::parse(program_str).unwrap());

    let query: RecExpr<CSubset> = RecExpr::parse("(assign $x (add (var $x) 1))").unwrap();
    let query_pattern = re_to_pattern(&query);

    let results = semantic_search(&eg, &query_pattern);

    assert!(
        !results.is_empty(),
        "Should find the increment x = x + 1 as a subexpression"
    );
}

#[test]
fn search_finds_any_assignment_to_x() {
    // Same program, but the query uses a pattern variable for the right-hand
    // side. Should find BOTH assignments: `x = 0` and `x = x + 1`.
    let program_str = "(fun main $x \
        (seq (assign $x 0) \
        (seq (assign $x (add (var $x) 1)) \
             (ret (var $x)))))";

    let mut eg = EGraph::<CSubset>::default();
    let _program = eg.add_expr(RecExpr::parse(program_str).unwrap());

    // (assign $x ?val) — match any assignment to $x
    let query_pattern = Pattern::parse("(assign $x ?val)").unwrap();

    let results = semantic_search(&eg, &query_pattern);

    assert!(
        results.len() >= 2,
        "Should find at least 2 assignments to x, got {}",
        results.len()
    );
}

#[test]
fn search_finds_increment_under_slot_renaming() {
    // The program increments $x. The query asks for an increment of $y.
    // Even though the names differ, ematch finds the match — it builds a
    // bijection between pattern slots and e-class slots.
    //
    // This is a feature, not a bug, for semantic code search:
    //   "find an increment of SOME variable" finds increments regardless of
    //   what the variable is locally called.
    //
    // The same expression with different slot names gives the same e-class
    // shape; ematch matches them via slot renaming.
    let program_str = "(fun main $x \
        (seq (assign $x 0) \
        (seq (assign $x (add (var $x) 1)) \
             (ret (var $x)))))";

    let mut eg = EGraph::<CSubset>::default();
    let _program = eg.add_expr(RecExpr::parse(program_str).unwrap());

    // Query uses $y instead of $x — should still match via slot bijection.
    let query: RecExpr<CSubset> = RecExpr::parse("(assign $y (add (var $y) 1))").unwrap();
    let query_pattern = re_to_pattern(&query);

    let results = semantic_search(&eg, &query_pattern);

    assert!(
        !results.is_empty(),
        "Search should find the increment regardless of variable name in the query"
    );
}
