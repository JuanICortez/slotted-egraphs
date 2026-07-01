use crate::*;

// A small language exercising `MultiBind`: a multi-argument function
// `(fun $x $y ... body)` that binds several slots at once over `body`.
// This is the multi-slot generalization of the `Bind`-based `lam`.
define_language! {
    pub enum MultiLang {
        Fun(MultiBind<AppliedId>) = "fun",
        Var(Slot) = "var",
        Add(AppliedId, AppliedId) = "add",
        Number(u32),
    }
}

// Parsing then printing then parsing again should be stable: `MultiBind`'s
// `to_syntax`/`from_syntax` must round-trip the leading run of bound slots.
#[test]
fn multibind_parse_roundtrip() {
    for s in [
        "(fun $0 (var $0))",
        "(fun $0 $1 (add (var $0) (var $1)))",
        "(fun $0 $1 $2 (add (var $0) (add (var $1) (var $2))))",
    ] {
        let re: RecExpr<MultiLang> = RecExpr::parse(s).unwrap();
        let printed = re.to_string();
        let re2: RecExpr<MultiLang> = RecExpr::parse(&printed).unwrap();
        assert_eq!(printed, re2.to_string(), "round-trip changed `{s}`");
    }
}

// Alpha-equivalent multi-binders (same body shape, different bound-slot names)
// must land in the same e-class.
#[test]
fn multibind_alpha_equivalence() {
    let mut eg: EGraph<MultiLang> = EGraph::default();

    let a = id("(fun $0 $1 (add (var $0) (var $1)))", &mut eg);
    let b = id("(fun $2 $3 (add (var $2) (var $3)))", &mut eg);
    assert!(eg.eq(&a, &b), "renamed bound slots should be alpha-equivalent");

    // Three binders, still alpha-equivalent.
    let c = id("(fun $0 $1 $2 (add (var $0) (add (var $1) (var $2))))", &mut eg);
    let d = id("(fun $9 $8 $7 (add (var $9) (add (var $8) (var $7))))", &mut eg);
    assert!(eg.eq(&c, &d));
}

// Structurally different bodies must stay in distinct e-classes even when the
// bound slots line up.
#[test]
fn multibind_distinct_bodies() {
    let mut eg: EGraph<MultiLang> = EGraph::default();

    let a = id("(fun $0 $1 (add (var $0) (var $1)))", &mut eg);
    let b = id("(fun $0 $1 (add (var $0) (var $0)))", &mut eg);
    assert!(!eg.eq(&a, &b));
}

// The *order* of the bound slots is significant: a body referencing the first
// binder is not equal to one referencing the second.
#[test]
fn multibind_slot_order_matters() {
    let mut eg: EGraph<MultiLang> = EGraph::default();

    let first = id("(fun $0 $1 (var $0))", &mut eg);
    let second = id("(fun $0 $1 (var $1))", &mut eg);
    assert!(!eg.eq(&first, &second));
}

// A bound-but-unused slot still participates in the shape; two such terms are
// alpha-equivalent as long as the *used* binder lines up.
#[test]
fn multibind_unused_bound_slot() {
    let mut eg: EGraph<MultiLang> = EGraph::default();

    let a = id("(fun $0 $1 (var $0))", &mut eg);
    let b = id("(fun $7 $9 (var $7))", &mut eg);
    assert!(eg.eq(&a, &b), "unused second binder must not affect the shape");
}

// Free (public) slots must survive to the outside of the binder: two funs are
// equal only when their free slots agree, regardless of the bound-slot names.
#[test]
fn multibind_free_slots_are_public() {
    let mut eg: EGraph<MultiLang> = EGraph::default();

    // `$1` is free in both; only the bound slot differs -> equal.
    let a = id("(fun $0 (var $1))", &mut eg);
    let b = id("(fun $5 (var $1))", &mut eg);
    assert!(eg.eq(&a, &b));

    // Different free slot -> not equal.
    let c = id("(fun $0 (var $2))", &mut eg);
    assert!(!eg.eq(&a, &c));
}
