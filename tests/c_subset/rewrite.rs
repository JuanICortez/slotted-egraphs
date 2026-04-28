use crate::*;

/// Returns all C-subset rewrite rules, generic over the analysis type.
/// Use `get_c_subset_rewrites()` for the default `()` analysis.
pub fn get_c_subset_rewrites_for<N: Analysis<CSubset> + 'static>() -> Vec<Rewrite<CSubset, N>> {
    vec![
        // Arithmetic identities
        Rewrite::new("add-comm", "(add ?a ?b)", "(add ?b ?a)"),
        Rewrite::new("add-assoc1", "(add ?a (add ?b ?c))", "(add (add ?a ?b) ?c)"),
        Rewrite::new("add-assoc2", "(add (add ?a ?b) ?c)", "(add ?a (add ?b ?c))"),
        Rewrite::new("add-zero", "(add ?a 0)", "?a"),
        Rewrite::new("mul-comm", "(mul ?a ?b)", "(mul ?b ?a)"),
        Rewrite::new("mul-one", "(mul ?a 1)", "?a"),
        Rewrite::new("mul-zero", "(mul ?a 0)", "0"),
        Rewrite::new("sub-self", "(sub ?a ?a)", "0"),
        Rewrite::new("neg-neg", "(neg (neg ?a))", "?a"),
        Rewrite::new("sub-to-add-neg", "(sub ?a ?b)", "(add ?a (neg ?b))"),
        // Boolean identities
        Rewrite::new("not-not", "(not (not ?a))", "?a"),
        Rewrite::new("and-comm", "(and ?a ?b)", "(and ?b ?a)"),
        Rewrite::new("or-comm", "(or ?a ?b)", "(or ?b ?a)"),
        Rewrite::new("and-true", "(and ?a true)", "?a"),
        Rewrite::new("and-false", "(and ?a false)", "false"),
        Rewrite::new("or-true", "(or ?a true)", "true"),
        Rewrite::new("or-false", "(or ?a false)", "?a"),
        Rewrite::new(
            "de-morgan-and",
            "(not (and ?a ?b))",
            "(or (not ?a) (not ?b))",
        ),
        Rewrite::new(
            "de-morgan-or",
            "(not (or ?a ?b))",
            "(and (not ?a) (not ?b))",
        ),
        // Control flow
        Rewrite::new("ite-true", "(ite true ?a ?b)", "?a"),
        Rewrite::new("ite-false", "(ite false ?a ?b)", "?b"),
        Rewrite::new("ite-not", "(ite (not ?c) ?a ?b)", "(ite ?c ?b ?a)"),
        // Sequencing
        Rewrite::new("seq-nop-left", "(seq nop ?a)", "?a"),
        Rewrite::new("seq-nop-right", "(seq ?a nop)", "?a"),
    ]
}

/// Returns all C-subset rewrite rules for the default `()` analysis.
pub fn get_c_subset_rewrites() -> Vec<Rewrite<CSubset>> {
    get_c_subset_rewrites_for::<()>()
}
