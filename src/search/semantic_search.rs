use crate::{
    ematch_all_with_roots, Analysis, AppliedId, AstSize, EGraph, Extractor, Id, Language, Pattern,
    RecExpr, Subst,
};
use std::collections::HashSet;

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
pub fn semantic_search<L: Language + 'static, N: Analysis<L> + 'static>(
    eg: &EGraph<L, N>,
    pattern: &Pattern<L>,
) -> Vec<SearchResult<L>> {
    let mut results = Vec::new();
    let mut seen_classes: HashSet<Id> = HashSet::default();

    let extractor = Extractor::<L, AstSize>::new(eg, AstSize);

    for (eclass, subst) in ematch_all_with_roots(eg, pattern) {
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
        })
    }

    results
}
