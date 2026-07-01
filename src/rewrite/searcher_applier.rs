use crate::*;

/// Finds matches for the LHS of a rewrite rule.
///
/// The associated type `Match` lets custom searchers return richer data than a
/// plain `Subst` when needed (e.g. a `ContainsSearcher` over an unordered
/// collection). The default `Pattern<L>` impl uses `type Match = Subst`.
pub trait Searcher<L: Language, N: Analysis<L>> {
    type Match;

    /// Search the whole e-graph and return all matches.
    fn search(&self, eg: &EGraph<L, N>) -> Vec<Self::Match>;

    /// Search within a single e-class.
    ///
    /// The default implementation calls [`Self::search`] and returns all
    /// results, which is correct but potentially slow. Override when you can
    /// restrict the search to a single class efficiently.
    fn search_eclass(&self, eg: &EGraph<L, N>, _id: Id) -> Vec<Self::Match> {
        self.search(eg)
    }

    /// The pattern-variable names this searcher can bind.
    ///
    /// Used by [`ConditionalApplier`] to validate that a condition only
    /// references bound variables. Returns an empty list by default (meaning
    /// "no validation"; custom searchers that do not expose named vars may keep
    /// this default).
    fn vars(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Consumes matches produced by a [`Searcher`] and mutates the e-graph.
pub trait Applier<L: Language, N: Analysis<L>> {
    type Match;

    /// Apply all matches to the e-graph. Returns the ids of e-classes that
    /// were touched (for stats/hooks). Order within the returned vec is
    /// unspecified; an empty vec is valid when the ids are not tracked.
    fn apply(&self, eg: &mut EGraph<L, N>, matches: Vec<Self::Match>) -> Vec<Id>;
}

// ---------------------------------------------------------------------------
// Pattern<L> as Searcher
// ---------------------------------------------------------------------------

impl<L: Language, N: Analysis<L>> Searcher<L, N> for Pattern<L> {
    type Match = Subst;

    fn search(&self, eg: &EGraph<L, N>) -> Vec<Subst> {
        ematch_all(eg, self)
    }

    fn search_eclass(&self, eg: &EGraph<L, N>, id: Id) -> Vec<Subst> {
        ematch_eclass(eg, self, id)
    }

    fn vars(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_pvars(self, &mut out);
        out
    }
}

fn collect_pvars<L: Language>(pat: &Pattern<L>, out: &mut Vec<String>) {
    match pat {
        Pattern::PVar(v) => {
            if !out.contains(v) {
                out.push(v.clone());
            }
        }
        Pattern::ENode(_, children) => {
            for c in children {
                collect_pvars(c, out);
            }
        }
        Pattern::Subst(b, x, t) => {
            collect_pvars(b, out);
            collect_pvars(x, out);
            collect_pvars(t, out);
        }
    }
}

// ---------------------------------------------------------------------------
// PatternApplier
// ---------------------------------------------------------------------------

/// An [`Applier`] that unions each matched LHS with a RHS pattern.
///
/// Directly wraps `EGraph::union_instantiations`, reusing the same tested
/// union path as `Rewrite::new` / `Rewrite::new_if`.
pub struct PatternApplier<L: Language> {
    pub lhs: Pattern<L>,
    pub rhs: Pattern<L>,
    pub rule_name: String,
}

impl<L: Language + 'static, N: Analysis<L>> Applier<L, N> for PatternApplier<L> {
    type Match = Subst;

    fn apply(&self, eg: &mut EGraph<L, N>, matches: Vec<Subst>) -> Vec<Id> {
        for subst in matches {
            eg.union_instantiations(&self.lhs, &self.rhs, &subst, Some(self.rule_name.clone()));
        }
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// CondFn — condition trait usable in ConditionalApplier
// ---------------------------------------------------------------------------

/// A condition checked before an [`Applier`] fires.
///
/// Automatically implemented for all `Fn(&Subst, &EGraph<L,N>) -> bool + 'static`
/// closures (including the return values of [`slot_free_in`], [`or`], [`and`],
/// [`not`]), so existing closure-based conditions continue to work.
///
/// Named struct types ([`SlotFreeIn`], [`VarsDistinct`]) implement this trait
/// directly, enabling conditions to be stored and composed as values rather
/// than opaque closures.
pub trait CondFn<L: Language, N: Analysis<L>>: 'static {
    fn check(&self, subst: &Subst, eg: &EGraph<L, N>) -> bool;
}

impl<F, L: Language, N: Analysis<L>> CondFn<L, N> for F
where
    F: Fn(&Subst, &EGraph<L, N>) -> bool + 'static,
{
    fn check(&self, subst: &Subst, eg: &EGraph<L, N>) -> bool {
        self(subst, eg)
    }
}

// ---------------------------------------------------------------------------
// ConditionalApplier
// ---------------------------------------------------------------------------

/// Wraps an [`Applier`] with a [`CondFn`]: only applies to matches that pass
/// the condition.
pub struct ConditionalApplier<A, C> {
    pub applier: A,
    pub cond: C,
}

impl<L: Language, N: Analysis<L>, A, C> Applier<L, N> for ConditionalApplier<A, C>
where
    A: Applier<L, N, Match = Subst>,
    C: CondFn<L, N>,
{
    type Match = Subst;

    fn apply(&self, eg: &mut EGraph<L, N>, matches: Vec<Subst>) -> Vec<Id> {
        // Partition with &EGraph first, then apply with &mut EGraph — never
        // interleave the borrows.
        let passing: Vec<Subst> = matches
            .into_iter()
            .filter(|s| self.cond.check(s, eg))
            .collect();
        self.applier.apply(eg, passing)
    }
}

// ---------------------------------------------------------------------------
// Concrete CondFn types
// ---------------------------------------------------------------------------

/// Condition: the given `slot` does not appear free in the e-class bound to `var`.
///
/// Equivalent to the `slot_free_in` closure combinator but storable as a named value.
pub struct SlotFreeIn {
    pub slot: Slot,
    pub var: String,
}

impl<L: Language, N: Analysis<L>> CondFn<L, N> for SlotFreeIn {
    fn check(&self, subst: &Subst, _eg: &EGraph<L, N>) -> bool {
        !subst[&self.var].slots().contains(&self.slot)
    }
}

/// Condition: pattern variables `a` and `b` are bound to distinct e-classes.
pub struct VarsDistinct {
    pub a: String,
    pub b: String,
}

impl<L: Language, N: Analysis<L>> CondFn<L, N> for VarsDistinct {
    fn check(&self, subst: &Subst, _eg: &EGraph<L, N>) -> bool {
        subst[&self.a].id != subst[&self.b].id
    }
}

// ---------------------------------------------------------------------------
// MultiSearcher
// ---------------------------------------------------------------------------

/// Combines multiple [`Searcher`]s whose matches share a common `Subst` type.
///
/// Semantics: the *join* of all component matches — a combined `Subst` for
/// each combination where shared pattern variables bind to equal e-classes.
/// This is a cartesian product filtered by compatibility, so **document that
/// combining two unconstrained searchers can blow up**; it is intended for
/// cases where shared variables prune heavily (e.g. "an assign to `$x` AND a
/// use of `$x`").
pub struct MultiSearcher<S> {
    pub searchers: Vec<S>,
}

impl<L: Language, N: Analysis<L>, S> Searcher<L, N> for MultiSearcher<S>
where
    S: Searcher<L, N, Match = Subst>,
{
    type Match = Subst;

    fn search(&self, eg: &EGraph<L, N>) -> Vec<Subst> {
        if self.searchers.is_empty() {
            return Vec::new();
        }

        let all: Vec<Vec<Subst>> = self.searchers.iter().map(|s| s.search(eg)).collect();

        // Fold via nested-loop join: left ⋈ right on shared vars.
        all.into_iter().reduce(|acc, next| join_substs(acc, next, eg)).unwrap_or_default()
    }

    fn vars(&self) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.searchers {
            for v in s.vars() {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
        out
    }
}

fn join_substs<L: Language, N: Analysis<L>>(
    left: Vec<Subst>,
    right: Vec<Subst>,
    eg: &EGraph<L, N>,
) -> Vec<Subst> {
    let mut out = Vec::new();
    for l in &left {
        for r in &right {
            if let Some(merged) = merge_substs(l, r, eg) {
                out.push(merged);
            }
        }
    }
    out
}

fn merge_substs<L: Language, N: Analysis<L>>(
    a: &Subst,
    b: &Subst,
    eg: &EGraph<L, N>,
) -> Option<Subst> {
    let mut merged = a.clone();
    for (k, v) in b {
        if let Some(existing) = merged.get(k) {
            if !eg.eq(existing, v) {
                return None;
            }
        } else {
            merged.insert(k.clone(), v.clone());
        }
    }
    Some(merged)
}
