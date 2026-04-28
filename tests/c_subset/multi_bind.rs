use crate::*;

/// A multi-slot binder — generalizes `Bind<T>` to bind multiple slots at once.
///
/// Useful for constructs that bind several names simultaneously, like a function
/// declaration with multiple parameters: `fun f(x, y, z) body`.
///
/// All slots in `slots` are treated as *private* (bound) in the body.
#[derive(Hash, PartialEq, Eq, Debug, Clone, PartialOrd, Ord)]
pub struct MultiBind<T> {
    pub slots: Vec<Slot>,
    pub elem: T,
}

impl<L: LanguageChildren> LanguageChildren for MultiBind<L> {
    // --- mutable iterators ---

    fn all_slot_occurrences_iter_mut(&mut self) -> impl Iterator<Item = &mut Slot> {
        self.slots
            .iter_mut()
            .chain(self.elem.all_slot_occurrences_iter_mut())
    }

    fn public_slot_occurrences_iter_mut(&mut self) -> impl Iterator<Item = &mut Slot> {
        // A public slot in the body is only public to the outside if it's not
        // shadowed by any of our bound slots.
        let bound: Vec<Slot> = self.slots.clone();
        self.elem
            .public_slot_occurrences_iter_mut()
            .filter(move |s| !bound.contains(*s))
    }

    fn applied_id_occurrences_iter_mut(&mut self) -> impl Iterator<Item = &mut AppliedId> {
        self.elem.applied_id_occurrences_iter_mut()
    }

    // --- immutable iterators ---

    fn all_slot_occurrences_iter(&self) -> impl Iterator<Item = &Slot> {
        self.slots
            .iter()
            .chain(self.elem.all_slot_occurrences_iter())
    }

    fn public_slot_occurrences_iter(&self) -> impl Iterator<Item = &Slot> {
        let bound: Vec<Slot> = self.slots.clone();
        self.elem
            .public_slot_occurrences_iter()
            .filter(move |s| !bound.contains(*s))
    }

    fn applied_id_occurrences_iter(&self) -> impl Iterator<Item = &AppliedId> {
        self.elem.applied_id_occurrences_iter()
    }

    // --- syntax ---

    fn to_syntax(&self) -> Vec<SyntaxElem> {
        let mut v: Vec<SyntaxElem> = self.slots.iter().map(|s| SyntaxElem::Slot(*s)).collect();
        v.extend(self.elem.to_syntax());
        v
    }

    fn from_syntax(elems: &[SyntaxElem]) -> Option<Self> {
        // Consume all leading slot elements as bound names,
        // then parse the remainder as the body.
        let mut slots = Vec::new();
        let mut i = 0;
        while i < elems.len() {
            if let SyntaxElem::Slot(s) = &elems[i] {
                slots.push(*s);
                i += 1;
            } else {
                break;
            }
        }
        let elem = L::from_syntax(&elems[i..])?;
        Some(MultiBind { slots, elem })
    }

    // --- weak shape (for alpha-equivalence) ---

    fn weak_shape_impl(&mut self, m: &mut (SlotMap, u32)) {
        // Save the original slots, rename them to fresh numeric ones,
        // process the body (which will see the renamed slots),
        // then remove the bindings so outer scopes don't see them.
        let originals: Vec<Slot> = self.slots.clone();
        for slot in self.slots.iter_mut() {
            let fresh = Slot::numeric(m.1);
            m.1 += 1;
            m.0.insert(*slot, fresh);
            *slot = fresh;
        }
        self.elem.weak_shape_impl(m);
        for s in originals {
            m.0.remove(s);
        }
    }
}

// --- Unit tests for MultiBind in isolation ---

#[cfg(test)]
mod tests {
    use super::*;

    define_language! {
        pub enum TestLang {
            Var(Slot) = "var",
            // fn with multi-binder: (fn $x $y body)
            Fn(MultiBind<AppliedId>) = "fn",
            App(AppliedId, AppliedId) = "app",
        }
    }

    #[test]
    fn multi_bind_alpha_equivalence() {
        // (fn $0 $1 (var $0))  ==  (fn $2 $3 (var $2))
        // Both bind two slots; body references the first. Should be equivalent.
        let mut eg = EGraph::<TestLang>::default();
        let a = eg.add_expr(RecExpr::parse("(fn $0 $1 (var $0))").unwrap());
        let b = eg.add_expr(RecExpr::parse("(fn $2 $3 (var $2))").unwrap());

        assert!(eg.eq(&a, &b), "Alpha-equivalent multi-binders should match");
    }

    #[test]
    fn multi_bind_different_body_not_equal() {
        // (fn $0 $1 (var $0))  !=  (fn $0 $1 (var $1))
        // These are NOT alpha-equivalent because the body references different bound slots.
        let mut eg = EGraph::<TestLang>::default();
        let a = eg.add_expr(RecExpr::parse("(fn $0 $1 (var $0))").unwrap());
        let b = eg.add_expr(RecExpr::parse("(fn $0 $1 (var $1))").unwrap());

        assert!(!eg.eq(&a, &b), "Different bodies should not be equal");
    }

    #[test]
    fn multi_bind_zero_slots() {
        // A multi-bind with zero slots is just the body.
        let mut eg = EGraph::<TestLang>::default();
        let a = eg.add_expr(RecExpr::parse("(fn (var $0))").unwrap());
        // The inner var $0 is a free variable and should be visible from outside.
        let slots = eg.slots(a.id);
        assert_eq!(
            slots.len(),
            1,
            "fn with zero binders should expose body's free slots"
        );
    }

    #[test]
    fn multi_bind_shadowing() {
        // The outer $0 is free in (var $0), but gets shadowed inside (fn $0 ...).
        // (app (var $0) (fn $0 (var $0)))
        // The outer (var $0) is free, the inner (var $0) is bound by fn.
        let mut eg = EGraph::<TestLang>::default();
        let a = eg.add_expr(RecExpr::parse("(app (var $0) (fn $0 (var $0)))").unwrap());

        // Only one free slot: the outer $0
        let slots = eg.slots(a.id);
        assert_eq!(slots.len(), 1, "Only outer $0 should be free");
    }
}
