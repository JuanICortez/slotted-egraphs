#![allow(unused)]

use crate::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// `Searcher::vars` is parameterised over N, but `vars()` itself doesn't use N.
// These wrappers pin N = () so callers don't need UFCS at every call site.
fn pattern_vars(pat: &Pattern<Arith>) -> Vec<String> {
    <Pattern<Arith> as Searcher<Arith, ()>>::vars(pat)
}

fn ms_vars(ms: &MultiSearcher<Pattern<Arith>>) -> Vec<String> {
    <MultiSearcher<Pattern<Arith>> as Searcher<Arith, ()>>::vars(ms)
}

/// Saturate an e-graph by running `rewrites` until no change, starting from `expr`.
fn saturate<L: Language + 'static, N: Analysis<L> + Default + 'static>(
    expr: &str,
    rewrites: &[Rewrite<L, N>],
) -> EGraph<L, N> {
    let re: RecExpr<L> = RecExpr::parse(expr).unwrap();
    let mut eg = EGraph::new(N::default());
    eg.add_syn_expr(re);
    while apply_rewrites(&mut eg, rewrites) {}
    eg
}

/// Apply rewrites exactly once.
fn apply_once<L: Language + 'static, N: Analysis<L> + Default + 'static>(
    expr: &str,
    rewrites: &[Rewrite<L, N>],
) -> EGraph<L, N> {
    let re: RecExpr<L> = RecExpr::parse(expr).unwrap();
    let mut eg = EGraph::new(N::default());
    eg.add_syn_expr(re);
    apply_rewrites(&mut eg, rewrites);
    eg
}

/// Check whether two parsed expressions are in the same e-class.
fn in_same_class<L: Language>(a: &str, b: &str, eg: &EGraph<L>) -> bool {
    let ra: RecExpr<L> = RecExpr::parse(a).unwrap();
    let rb: RecExpr<L> = RecExpr::parse(b).unwrap();
    let Some(ia) = lookup_rec_expr(&ra, eg) else { return false };
    let Some(ib) = lookup_rec_expr(&rb, eg) else { return false };
    eg.eq(&ia, &ib)
}

// ---------------------------------------------------------------------------
// Phase 1 — Pattern<L> as Searcher
// ---------------------------------------------------------------------------

// `search` returns the same number of matches as `ematch_all`.
#[test]
fn pattern_searcher_search_agrees_with_ematch_all() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add (var $0) (var $1))", &mut eg);
    id("(add (var $0) (var $0))", &mut eg);
    id("(mul (var $0) (var $1))", &mut eg);

    let pat = Pattern::<Arith>::parse("(add ?a ?b)").unwrap();
    assert_eq!(
        Searcher::search(&pat, &eg).len(),
        ematch_all(&eg, &pat).len(),
        "`Searcher::search` must agree with `ematch_all`"
    );
}

// The sum of `search_eclass` counts over every live id equals the total from `search`.
#[test]
fn pattern_searcher_eclass_partitions_total() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add (var $0) (var $1))", &mut eg);
    id("(add (var $0) (var $0))", &mut eg);
    id("(mul (var $0) (var $1))", &mut eg);

    let pat = Pattern::<Arith>::parse("(add ?a ?b)").unwrap();
    let total = Searcher::search(&pat, &eg).len();
    let by_class: usize = eg.ids().iter().map(|&i| pat.search_eclass(&eg, i).len()).sum();

    assert_eq!(
        total, by_class,
        "sum of per-class matches must equal full-search count"
    );
}

// `vars()` enumerates exactly the pattern variables, with no duplicates.
#[test]
fn pattern_searcher_vars_correct() {
    let pat = Pattern::<Arith>::parse("(add ?a ?b)").unwrap();
    let vars = pattern_vars(&pat);
    assert_eq!(vars.len(), 2);
    assert!(vars.contains(&"a".to_string()));
    assert!(vars.contains(&"b".to_string()));
}

// A pattern with a repeated variable reports it only once.
#[test]
fn pattern_searcher_vars_deduplicates() {
    let pat = Pattern::<Arith>::parse("(add ?a ?a)").unwrap();
    let vars = pattern_vars(&pat);
    assert_eq!(vars.len(), 1);
    assert!(vars.contains(&"a".to_string()));
}

// A ground pattern (no pvars) returns an empty vars list.
#[test]
fn pattern_searcher_vars_empty_for_ground() {
    let pat = Pattern::<Arith>::parse("(add 1 2)").unwrap();
    assert!(pattern_vars(&pat).is_empty());
}

// ---------------------------------------------------------------------------
// Phase 2 — Rewrite::from_parts equivalence with Rewrite::new
// ---------------------------------------------------------------------------

// `from_parts` with Pattern + PatternApplier must make the same equation hold
// as `Rewrite::new` after one step.
#[test]
fn from_parts_add_comm_equivalent_to_new() {
    let rw_new = {
        let lhs = Pattern::<Arith>::parse("(add ?a ?b)").unwrap();
        let rhs = Pattern::<Arith>::parse("(add ?b ?a)").unwrap();
        Rewrite::from_parts(
            lhs.clone(),
            PatternApplier { lhs, rhs, rule_name: "add-comm".to_string() },
        )
    };

    // `(add 1 2)` and `(add 2 1)` start in different e-classes.
    // After one add-comm step they must be equal — same as Rewrite::new.
    let eg_old = apply_once::<Arith, ()>(
        "(add 1 2)",
        &[Rewrite::new("add-comm", "(add ?a ?b)", "(add ?b ?a)")],
    );
    let eg_new = apply_once::<Arith, ()>("(add 1 2)", &[rw_new]);

    assert!(
        in_same_class("(add 1 2)", "(add 2 1)", &eg_old),
        "Rewrite::new version must make add-comm hold"
    );
    assert!(
        in_same_class("(add 1 2)", "(add 2 1)", &eg_new),
        "from_parts version must make add-comm hold"
    );
}

// Same check for mul-comm.
#[test]
fn from_parts_mul_comm_equivalent_to_new() {
    let rw_new = {
        let lhs = Pattern::<Arith>::parse("(mul ?a ?b)").unwrap();
        let rhs = Pattern::<Arith>::parse("(mul ?b ?a)").unwrap();
        Rewrite::from_parts(
            lhs.clone(),
            PatternApplier { lhs, rhs, rule_name: "mul-comm".to_string() },
        )
    };

    let eg_old = apply_once::<Arith, ()>(
        "(mul 3 4)",
        &[Rewrite::new("mul-comm", "(mul ?a ?b)", "(mul ?b ?a)")],
    );
    let eg_new = apply_once::<Arith, ()>("(mul 3 4)", &[rw_new]);

    assert!(in_same_class("(mul 3 4)", "(mul 4 3)", &eg_old));
    assert!(in_same_class("(mul 3 4)", "(mul 4 3)", &eg_new));
}

// `from_parts` saturation agrees with `Rewrite::new` saturation over a richer
// expression that triggers multiple rule applications.
#[test]
fn from_parts_saturates_same_as_new() {
    let make_comm = |name: &str, op: &str| {
        let lhs = Pattern::<Arith>::parse(&format!("({op} ?a ?b)")).unwrap();
        let rhs = Pattern::<Arith>::parse(&format!("({op} ?b ?a)")).unwrap();
        Rewrite::<Arith>::from_parts(
            lhs.clone(),
            PatternApplier { lhs, rhs, rule_name: name.to_string() },
        )
    };

    let old_rules = vec![
        Rewrite::<Arith>::new("add-comm", "(add ?a ?b)", "(add ?b ?a)"),
        Rewrite::<Arith>::new("mul-comm", "(mul ?a ?b)", "(mul ?b ?a)"),
    ];
    let new_rules = vec![make_comm("add-comm", "add"), make_comm("mul-comm", "mul")];

    let expr = "(mul (add 1 2) (add 3 4))";
    let eg_old = saturate::<Arith, ()>(expr, &old_rules);
    let eg_new = saturate::<Arith, ()>(expr, &new_rules);

    // Both saturated graphs must agree on the same set of equations.
    assert!(in_same_class("(mul (add 1 2) (add 3 4))", "(mul (add 2 1) (add 4 3))", &eg_old));
    assert!(in_same_class("(mul (add 1 2) (add 3 4))", "(mul (add 2 1) (add 4 3))", &eg_new));
    assert_eq!(
        eg_old.progress(),
        eg_new.progress(),
        "saturated e-graphs must have identical progress measures"
    );
}

// ---------------------------------------------------------------------------
// Phase 3a — ConditionalApplier with closure equivalent to new_if
// ---------------------------------------------------------------------------

// A `ConditionalApplier` wrapping a closure must produce the same result
// as the direct `Rewrite::new_if` closure path.
#[test]
fn conditional_with_closure_equivalent_to_new_if() {
    // Rule: add-comm, but only when ?a ≠ ?b (different e-class ids).
    // Using closures:
    let rw_new_if = Rewrite::<Arith>::new_if(
        "add-comm-neq",
        "(add ?a ?b)",
        "(add ?b ?a)",
        |subst, _eg| subst["a"].id != subst["b"].id,
    );
    // Using ConditionalApplier with a closure (closure implements CondFn via blanket impl):
    let rw_parts = {
        let lhs = Pattern::<Arith>::parse("(add ?a ?b)").unwrap();
        let rhs = Pattern::<Arith>::parse("(add ?b ?a)").unwrap();
        Rewrite::<Arith>::from_parts(
            lhs.clone(),
            ConditionalApplier {
                applier: PatternApplier { lhs, rhs, rule_name: "add-comm-neq".to_string() },
                cond: |subst: &Subst, _eg: &EGraph<Arith>| subst["a"].id != subst["b"].id,
            },
        )
    };

    // (add 1 2): ?a=1, ?b=2 — different classes → rule fires.
    let eg1 = apply_once::<Arith, ()>("(add 1 2)", &[rw_new_if]);
    let eg2 = apply_once::<Arith, ()>("(add 1 2)", &[rw_parts]);
    assert!(in_same_class("(add 1 2)", "(add 2 1)", &eg1));
    assert!(in_same_class("(add 1 2)", "(add 2 1)", &eg2));
}

// ---------------------------------------------------------------------------
// Phase 3b — SlotFreeIn unit tests
// ---------------------------------------------------------------------------

// SlotFreeIn::check returns false when the slot IS in the matched applied-id.
#[test]
fn slot_free_in_blocked_when_slot_present() {
    let mut eg: EGraph<Arith> = EGraph::default();
    // (var $1) has $1 as a free slot, so applying the identity map gives an
    // AppliedId whose .slots() includes $1.
    let v = id("(var $1)", &mut eg);

    let subst: Subst = [("b".to_string(), v.clone())].into_iter().collect();
    let cond = SlotFreeIn { slot: Slot::numeric(1), var: "b".to_string() };

    assert!(
        !cond.check(&subst, &eg),
        "SlotFreeIn should be false when slot $1 IS present in 'b'"
    );
}

// SlotFreeIn::check returns true when the slot is NOT in the matched applied-id.
#[test]
fn slot_free_in_passes_when_slot_absent() {
    let mut eg: EGraph<Arith> = EGraph::default();
    // Number(42) has no slots, so $1 ∉ its applied-id.slots().
    let n = id("42", &mut eg);

    let subst: Subst = [("b".to_string(), n.clone())].into_iter().collect();
    let cond = SlotFreeIn { slot: Slot::numeric(1), var: "b".to_string() };

    assert!(
        cond.check(&subst, &eg),
        "SlotFreeIn should be true when slot $1 is NOT present in 'b'"
    );
}

// End-to-end: a rule guarded by SlotFreeIn fires only when the slot is absent.
// Uses `my-let-unused`: (let $1 ?b ?t) → ?b  when $1 ∉ slots(?b).
#[test]
fn slot_free_in_equivalent_to_new_if_end_to_end() {
    let make_let_unused = |rule: Rewrite<Arith>| -> bool {
        // Input: (let $1 42 (var $0))  — 42 doesn't mention $1 → rule fires.
        let eg = apply_once::<Arith, ()>("(let $1 42 (var $0))", &[rule]);
        in_same_class("(let $1 42 (var $0))", "42", &eg)
    };

    let rw_new_if = Rewrite::new_if(
        "my-let-unused",
        "(let $1 ?b ?t)",
        "?b",
        |subst, _| !subst["b"].slots().contains(&Slot::numeric(1)),
    );
    let rw_parts = {
        let lhs = Pattern::<Arith>::parse("(let $1 ?b ?t)").unwrap();
        let rhs = Pattern::<Arith>::parse("?b").unwrap();
        Rewrite::from_parts(
            lhs.clone(),
            ConditionalApplier {
                applier: PatternApplier {
                    lhs,
                    rhs,
                    rule_name: "my-let-unused".to_string(),
                },
                cond: SlotFreeIn { slot: Slot::numeric(1), var: "b".to_string() },
            },
        )
    };

    assert!(make_let_unused(rw_new_if), "Rewrite::new_if let-unused should fire");
    assert!(make_let_unused(rw_parts), "ConditionalApplier+SlotFreeIn let-unused should fire");
}

// The rule must NOT fire when the bound slot is present in ?b.
#[test]
fn slot_free_in_blocked_end_to_end() {
    // (let $1 (var $1) (var $0)) — ?b = (var $1) which mentions $1 → rule blocked.
    let rw = {
        let lhs = Pattern::<Arith>::parse("(let $1 ?b ?t)").unwrap();
        let rhs = Pattern::<Arith>::parse("?b").unwrap();
        Rewrite::<Arith>::from_parts(
            lhs.clone(),
            ConditionalApplier {
                applier: PatternApplier { lhs, rhs, rule_name: "my-let-unused".to_string() },
                cond: SlotFreeIn { slot: Slot::numeric(1), var: "b".to_string() },
            },
        )
    };

    let eg = apply_once::<Arith, ()>("(let $1 (var $1) (var $0))", &[rw]);
    assert!(
        !in_same_class("(let $1 (var $1) (var $0))", "(var $1)", &eg),
        "rule must not fire when $1 IS in 'b'"
    );
}

// ---------------------------------------------------------------------------
// Phase 3c — VarsDistinct unit tests
// ---------------------------------------------------------------------------

// VarsDistinct returns false when both variables bind to the same e-class id.
#[test]
fn vars_distinct_blocked_when_same_id() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let x = id("42", &mut eg);

    let subst: Subst = [
        ("a".to_string(), x.clone()),
        ("b".to_string(), x.clone()),
    ]
    .into_iter()
    .collect();

    let cond: VarsDistinct = VarsDistinct { a: "a".to_string(), b: "b".to_string() };
    assert!(
        !cond.check(&subst, &eg),
        "VarsDistinct must be false when 'a' and 'b' share the same e-class id"
    );
}

// VarsDistinct returns true for genuinely different e-class ids.
#[test]
fn vars_distinct_passes_for_different_ids() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let x = id("1", &mut eg);
    let y = id("2", &mut eg);

    let subst: Subst = [
        ("a".to_string(), x.clone()),
        ("b".to_string(), y.clone()),
    ]
    .into_iter()
    .collect();

    let cond = VarsDistinct { a: "a".to_string(), b: "b".to_string() };
    assert!(
        cond.check(&subst, &eg),
        "VarsDistinct must be true when 'a' and 'b' are in different e-classes"
    );
}

// End-to-end: a rule guarded by VarsDistinct fires only when both vars are distinct.
// Rule: (add ?a ?b) → (add ?b ?a)  only when ?a ≠ ?b.
// (add 1 1) has ?a = ?b = 1 → blocked. (add 1 2) has distinct → fires.
#[test]
fn vars_distinct_end_to_end() {
    let make_rw = || {
        let lhs = Pattern::<Arith>::parse("(add ?a ?b)").unwrap();
        let rhs = Pattern::<Arith>::parse("(add ?b ?a)").unwrap();
        Rewrite::<Arith>::from_parts(
            lhs.clone(),
            ConditionalApplier {
                applier: PatternApplier { lhs, rhs, rule_name: "add-comm-neq".to_string() },
                cond: VarsDistinct { a: "a".to_string(), b: "b".to_string() },
            },
        )
    };

    // (add 1 2): distinct → fires.
    let eg_fires = apply_once::<Arith, ()>("(add 1 2)", &[make_rw()]);
    assert!(in_same_class("(add 1 2)", "(add 2 1)", &eg_fires));

    // (add 1 1): same e-class for both → blocked.
    let eg_blocked = apply_once::<Arith, ()>("(add 1 1)", &[make_rw()]);
    assert!(
        !in_same_class("(add 1 1)", "(add 2 2)", &eg_blocked),
        "(add 1 1) should not be rewritten by a rule that requires distinct vars"
    );
}

// ---------------------------------------------------------------------------
// Phase 4 — MultiSearcher
// ---------------------------------------------------------------------------

// A single-component MultiSearcher returns the same matches as the pattern alone.
#[test]
fn multi_searcher_single_component_equals_pattern() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add 1 2)", &mut eg);
    id("(add 3 4)", &mut eg);
    id("(mul 1 2)", &mut eg);

    let pat = Pattern::<Arith>::parse("(add ?a ?b)").unwrap();
    let expected = Searcher::search(&pat, &eg).len();

    let ms = MultiSearcher { searchers: vec![pat] };
    assert_eq!(
        ms.search(&eg).len(),
        expected,
        "single-component MultiSearcher must equal plain Pattern search"
    );
}

// Empty MultiSearcher returns no matches.
#[test]
fn multi_searcher_empty_returns_empty() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add 1 2)", &mut eg);

    let ms: MultiSearcher<Pattern<Arith>> = MultiSearcher { searchers: vec![] };
    assert!(ms.search(&eg).is_empty());
}

// Two-component join on a shared variable finds the combined match.
//
// Graph: (add 42 (var $0)) and (mul 42 (var $1)).
// Searcher 1: "(add ?x ?y)" — matches {x: 42, y: var($0)}.
// Searcher 2: "(mul ?x ?z)" — matches {x: 42, z: var($1)}.
// Join on ?x (both bind to 42) → 1 combined match {x:42, y:var($0), z:var($1)}.
#[test]
fn multi_searcher_joins_on_shared_var() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add 42 (var $0))", &mut eg);
    id("(mul 42 (var $1))", &mut eg);

    let ms = MultiSearcher {
        searchers: vec![
            Pattern::<Arith>::parse("(add ?x ?y)").unwrap(),
            Pattern::<Arith>::parse("(mul ?x ?z)").unwrap(),
        ],
    };

    let matches = ms.search(&eg);
    assert_eq!(matches.len(), 1, "should find exactly one combined match");

    let combined = &matches[0];
    assert!(combined.contains_key("x"));
    assert!(combined.contains_key("y"));
    assert!(combined.contains_key("z"));
}

// When the shared variable binds to different e-classes in each component,
// the join produces no combined match.
//
// Graph: (add 1 (var $0)) and (mul 2 (var $1)).
// Searcher 1: "(add ?x ?y)" — x=1.
// Searcher 2: "(mul ?x ?z)" — x=2.
// 1 ≠ 2 → join is empty.
#[test]
fn multi_searcher_no_match_when_bindings_incompatible() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add 1 (var $0))", &mut eg);
    id("(mul 2 (var $1))", &mut eg);

    let ms = MultiSearcher {
        searchers: vec![
            Pattern::<Arith>::parse("(add ?x ?y)").unwrap(),
            Pattern::<Arith>::parse("(mul ?x ?z)").unwrap(),
        ],
    };

    assert_eq!(
        ms.search(&eg).len(),
        0,
        "incompatible bindings for shared ?x must yield no match"
    );
}

// `vars()` returns the union of component vars without duplicates.
#[test]
fn multi_searcher_vars_is_union() {
    let ms = MultiSearcher {
        searchers: vec![
            Pattern::<Arith>::parse("(add ?x ?y)").unwrap(),
            Pattern::<Arith>::parse("(mul ?x ?z)").unwrap(),
        ],
    };

    let vars = ms_vars(&ms);
    // ?x appears in both components — must appear only once.
    assert_eq!(vars.len(), 3, "union of {{x,y}} and {{x,z}} has 3 distinct vars");
    assert!(vars.contains(&"x".to_string()));
    assert!(vars.contains(&"y".to_string()));
    assert!(vars.contains(&"z".to_string()));
}

// A multi-searcher with no shared vars is a cartesian product (all combinations).
#[test]
fn multi_searcher_cartesian_product_when_no_shared_vars() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add 1 2)", &mut eg);  // 1 match for (add ?a ?b)
    id("(add 3 4)", &mut eg);  // 1 more match for (add ?a ?b) — 2 total
    id("(mul 5 6)", &mut eg);  // 1 match for (mul ?c ?d)

    let ms = MultiSearcher {
        searchers: vec![
            Pattern::<Arith>::parse("(add ?a ?b)").unwrap(),
            Pattern::<Arith>::parse("(mul ?c ?d)").unwrap(),
        ],
    };

    // No shared vars → full cartesian product: 2 × 1 = 2 combined matches.
    assert_eq!(ms.search(&eg).len(), 2);
}

// A multi-searcher can drive a Rewrite::from_parts for a rule that requires
// two simultaneous pattern matches — impossible to express as a single Pattern.
// Rule: "if (add ?x ?y) and (mul ?x ?z) both exist, union (add ?x ?y) with (add ?z ?y)."
// We only check that the rule fires (the union is made), not the semantic content.
#[test]
fn multi_searcher_drives_a_rewrite() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add 42 1)", &mut eg);
    id("(mul 42 2)", &mut eg);

    // Before: (add 42 1) and (add 2 1) are different classes.
    assert!(!in_same_class("(add 42 1)", "(add 2 1)", &eg));

    let lhs_rhs = Pattern::<Arith>::parse("(add ?x ?y)").unwrap();
    let rhs_pat = Pattern::<Arith>::parse("(add ?z ?y)").unwrap();

    let rw = Rewrite::from_parts(
        MultiSearcher {
            searchers: vec![
                Pattern::<Arith>::parse("(add ?x ?y)").unwrap(),
                Pattern::<Arith>::parse("(mul ?x ?z)").unwrap(),
            ],
        },
        PatternApplier {
            lhs: lhs_rhs,
            rhs: rhs_pat,
            rule_name: "cross-op-comm".to_string(),
        },
    );

    apply_rewrites(&mut eg, &[rw]);

    // After: the rule unified (add 42 1) with (add 2 1).
    assert!(in_same_class("(add 42 1)", "(add 2 1)", &eg));
}
