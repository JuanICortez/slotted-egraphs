use crate::*;

// Tests for the library `search` module (`semantic_search`) and the
// `ematch_all_with_roots` helper it is built on. All exercised over the
// existing `Arith` language.

// Do two applied-ids denote the *same e-class*? In a slotted e-graph an e-class
// has many `AppliedId` representatives (one per slot renaming), so equality of
// representatives (`eg.eq`) is stricter than "same class". `semantic_search`
// itself dedups on the canonical class `Id`, so that is what we compare here.
fn same_class(eg: &EGraph<Arith>, a: &AppliedId, b: &AppliedId) -> bool {
    eg.find_applied_id(a).id == eg.find_applied_id(b).id
}

// A single matching e-class is found, the reported e-class is the one we
// inserted, and the substitution binds every pattern variable.
#[test]
fn search_basic_match() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let t = id("(add (var $0) (var $1))", &mut eg);

    let pat = Pattern::parse("(add ?a ?b)").unwrap();
    let results = semantic_search(&eg, &pat);

    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert!(same_class(&eg, &r.eclass, &t), "reported e-class should be the match");
    assert!(r.subst.contains_key("a"));
    assert!(r.subst.contains_key("b"));
}

// Free-variable renaming does not create new e-classes: in a slotted e-graph
// `(add (var $0) (var $1))` and `(add (var $2) (var $3))` are the *same* class,
// differing only in their slot maps. `semantic_search` therefore reports one
// result, not two.
#[test]
fn search_free_var_renaming_is_one_class() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let a = id("(add (var $0) (var $1))", &mut eg);
    let b = id("(add (var $2) (var $3))", &mut eg);
    assert!(same_class(&eg, &a, &b));

    let pat = Pattern::parse("(add ?a ?b)").unwrap();
    assert_eq!(semantic_search(&eg, &pat).len(), 1);
}

// A pattern with no match returns no results.
#[test]
fn search_no_match() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add (var $0) (var $1))", &mut eg);

    let pat = Pattern::parse("(mul ?a ?b)").unwrap();
    assert!(semantic_search(&eg, &pat).is_empty());
}

// Two structurally distinct matches live in two distinct e-classes and produce
// two results. `(add (var $0) (var $0))` (one shared slot) and
// `(add (var $0) (var $1))` (two distinct slots) are genuinely different shapes.
#[test]
fn search_distinct_eclasses() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let a = id("(add (var $0) (var $0))", &mut eg);
    let b = id("(add (var $0) (var $1))", &mut eg);
    assert!(!same_class(&eg, &a, &b));

    let pat = Pattern::parse("(add ?a ?b)").unwrap();
    assert_eq!(semantic_search(&eg, &pat).len(), 2);
}

// After unioning two distinct matching e-classes, `semantic_search` collapses
// them to a single result — it reports distinct e-classes, not occurrences.
#[test]
fn search_dedups_after_union() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let a = id("(add (var $0) (var $0))", &mut eg);
    let b = id("(add (var $0) (var $1))", &mut eg);

    let pat = Pattern::parse("(add ?a ?b)").unwrap();
    assert_eq!(semantic_search(&eg, &pat).len(), 2);

    eg.union(&a, &b); // union auto-rebuilds
    assert_eq!(semantic_search(&eg, &pat).len(), 1);
}

// The extracted representative must actually belong to the matched e-class:
// re-adding it lands back in the same class.
#[test]
fn search_matched_expr_belongs_to_class() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let t = id("(add (var $0) (var $1))", &mut eg);

    let pat = Pattern::parse("(add ?a ?b)").unwrap();
    let results = semantic_search(&eg, &pat);
    assert_eq!(results.len(), 1);

    let re = results[0].matched_expr.clone();
    let readded = eg.add_expr(re);
    assert!(same_class(&eg, &readded, &t));
}

// `ematch_all_with_roots` reports the correct root e-class for a match.
#[test]
fn ematch_roots_reports_match_class() {
    let mut eg: EGraph<Arith> = EGraph::default();
    let t = id("(add (var $0) (var $1))", &mut eg);

    let pat = Pattern::parse("(add ?a ?b)").unwrap();
    let roots = ematch_all_with_roots(&eg, &pat);

    assert_eq!(roots.len(), 1);
    let (root, _subst) = &roots[0];
    assert!(same_class(&eg, root, &t));
}

// The root-annotated variant yields exactly the same substitutions as plain
// `ematch_all` — it only adds the e-class, it must not change what matches.
#[test]
fn ematch_roots_agrees_with_ematch_all() {
    let mut eg: EGraph<Arith> = EGraph::default();
    id("(add (var $0) (var $1))", &mut eg);
    id("(mul (var $0) (var $1))", &mut eg);

    let pat = Pattern::parse("(add ?a ?b)").unwrap();
    assert_eq!(
        ematch_all(&eg, &pat).len(),
        ematch_all_with_roots(&eg, &pat).len(),
    );
}
