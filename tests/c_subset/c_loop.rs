// Loop representation: while-loops as direct statement nodes.
//
// SHAPE: (loop cond body)
//   - cond: boolean expression — loop continues while true
//   - body: statement(s) executed each iteration (typically a `seq` of assigns)
//
// Variables read or written inside cond/body are bound by the enclosing
// function's MultiBind. The loop itself does NOT bind any slots.
//
// MENTAL MODEL: the e-graph stores the program structure literally. The
// `Loop` node is just a node — it makes no claim that any variable equals
// any specific value. Reassignment within the body is fine.
//
// EXAMPLE: `while (i < n) i = i + 1`
//   (loop (lt (var $i) (var $n)) (assign $i (add (var $i) 1)))
//
// Multi-variable mutation works without tuples or SSA — just sequence
// multiple assigns inside the body:
//   `while (i < n) { sum = sum + i; i = i + 1; }`
//   (loop (lt (var $i) (var $n))
//         (seq (assign $sum (add (var $sum) (var $i)))
//              (assign $i (add (var $i) 1))))
//
// (No `LoopBody` struct needed anymore — this file used to define one for
//  the previous "loops as recursive functions" design.)

use crate::*;

// --- Tests ---

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn loop_parses_and_roundtrips() {
        // while (i < 10) i = i + 1
        // The slot $i must be bound somewhere — here we wrap it in a function.
        let src = "(fun main $i (loop (lt (var $i) 10) (assign $i (add (var $i) 1))))";
        let mut eg = EGraph::<CSubset>::default();
        let a = eg.add_expr(RecExpr::parse(src).unwrap());

        // No free slots — $i is bound by the function.
        assert!(
            eg.slots(a.id).is_empty(),
            "Function with loop should have no free slots"
        );
    }

    #[test]
    fn loop_alpha_equivalence_within_function() {
        // Two functions whose loops differ only in the iterator name.
        // The renaming is captured by the function's MultiBind.
        let a = "(fun main $i (loop (lt (var $i) 10) (assign $i (add (var $i) 1))))";
        let b = "(fun main $j (loop (lt (var $j) 10) (assign $j (add (var $j) 1))))";

        let mut eg = EGraph::<CSubset>::default();
        let id_a = eg.add_expr(RecExpr::parse(a).unwrap());
        let id_b = eg.add_expr(RecExpr::parse(b).unwrap());

        assert!(
            eg.eq(&id_a, &id_b),
            "Alpha-equivalent loops should be equal"
        );
    }

    #[test]
    fn loop_different_cond_not_equal() {
        // Same structure, different threshold — should NOT be equal.
        let a = "(fun main $i (loop (lt (var $i) 10) (assign $i (add (var $i) 1))))";
        let b = "(fun main $i (loop (lt (var $i) 20) (assign $i (add (var $i) 1))))";

        let mut eg = EGraph::<CSubset>::default();
        let id_a = eg.add_expr(RecExpr::parse(a).unwrap());
        let id_b = eg.add_expr(RecExpr::parse(b).unwrap());

        assert!(
            !eg.eq(&id_a, &id_b),
            "Different loop conditions should differ"
        );
    }

    #[test]
    fn loop_with_multiple_mutations_in_body() {
        // The new representation handles multi-variable mutation directly,
        // no tuples needed — just sequence the assigns inside the body.
        // while (i < n) { sum = sum + i; i = i + 1; }
        let src = "(fun sum_to_n $n $i $sum \
                    (loop (lt (var $i) (var $n)) \
                          (seq (assign $sum (add (var $sum) (var $i))) \
                               (assign $i (add (var $i) 1)))))";
        let mut eg = EGraph::<CSubset>::default();
        let a = eg.add_expr(RecExpr::parse(src).unwrap());

        // No free slots — $n, $i, $sum are all bound by the function.
        assert!(
            eg.slots(a.id).is_empty(),
            "Multi-variable loop in function should have no free slots"
        );
    }

    #[test]
    fn loop_inside_function_alpha_equivalent_with_locals() {
        // fun count_up(n) { i = 0; while (i < n) i = i + 1; ret i }
        // Both the parameter and the local can be renamed.
        let a = "(fun count_up $n $i \
                  (seq (assign $i 0) \
                  (seq (loop (lt (var $i) (var $n)) (assign $i (add (var $i) 1))) \
                       (ret (var $i)))))";
        let b = "(fun count_up $m $k \
                  (seq (assign $k 0) \
                  (seq (loop (lt (var $k) (var $m)) (assign $k (add (var $k) 1))) \
                       (ret (var $k)))))";

        let mut eg = EGraph::<CSubset>::default();
        let id_a = eg.add_expr(RecExpr::parse(a).unwrap());
        let id_b = eg.add_expr(RecExpr::parse(b).unwrap());

        assert!(
            eg.eq(&id_a, &id_b),
            "Alpha-equivalent functions with loops should be equal"
        );
    }
}
