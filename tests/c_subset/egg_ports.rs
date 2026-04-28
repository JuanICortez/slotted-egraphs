// Ports of the tests from ~/Tesis/Egg/equal-test/src/main.rs
//
// The egg version used De Bruijn indices (Db 0, Db 1, ...) to get
// alpha-equivalence. In slotted e-graphs we use Slots directly, so these
// translations are often shorter/cleaner.

use crate::*;

/// Run a test: check whether two expressions become equal after saturation.
/// Uses the `ConstFold` analysis so that numeric constants are folded.
fn check_equal(name: &str, a: &str, b: &str, expected: bool) {
    let rules = get_c_subset_rewrites_for::<ConstFold>();

    let expr_a: RecExpr<CSubset> = RecExpr::parse(a).unwrap();
    let expr_b: RecExpr<CSubset> = RecExpr::parse(b).unwrap();

    let mut runner: Runner<CSubset, ConstFold, (), ()> = Runner::new(ConstFold)
        .with_expr(&expr_a)
        .with_expr(&expr_b)
        .with_iter_limit(10);
    let report = runner.run(&rules);

    let id_a = runner.roots[0].clone();
    let id_b = runner.roots[1].clone();
    let are_equal = runner.egraph.eq(&id_a, &id_b);
    assert_eq!(
        are_equal, expected,
        "{}: expected {}, got {} (stop: {:?})",
        name, expected, are_equal, report.stop_reason
    );
}

// --- Test 1: constant folding + associativity ---
// egg:      (+ 4 (Fv x))  ==  (+ 3 (+ 1 (Fv x)))
// slotted:  (add 4 (var $x))  ==  (add 3 (add 1 (var $x)))

#[test]
fn egg_test_1_constant_folding() {
    check_equal(
        "constant fold: 4+x == 3+(1+x)",
        "(add 4 (var $x))",
        "(add 3 (add 1 (var $x)))",
        true,
    );
}

// --- Test 2: different free variables should NOT match ---
// egg:      (+ 4 (Fv y))  !=  (+ 3 (+ 1 (Fv x)))
// slotted:  (add 4 (var $y))  !=  (add 3 (add 1 (var $x)))

#[test]
fn egg_test_2_different_free_vars() {
    check_equal(
        "different free vars: 4+y != 3+(1+x)",
        "(add 4 (var $y))",
        "(add 3 (add 1 (var $x)))",
        false,
    );
}

// --- Test 3: identity function equivalence ---
// egg:      (Fn main 1 (Return (Db 0)))  ==  itself
//           (using De Bruijn: Db 0 refers to the function's parameter)
// slotted:  (fun main $p (ret (var $p)))  ==  (fun main $q (ret (var $q)))
//           (alpha-equivalence is automatic)

#[test]
fn egg_test_3_identity_function_alpha_equiv() {
    check_equal(
        "alpha-equiv identity fns",
        "(fun main $p (ret (var $p)))",
        "(fun main $q (ret (var $q)))",
        true,
    );
}

// --- Test 4: function with addition in body ---
// egg:      (Fn main 1 (Assign (Db 1) (+ 1 (Db 0))))
//           (the function has a parameter, and inside assigns to "Db 1" the value 1+param)
// slotted:  (fun main $p (assign $ret (add 1 (var $p))))
//           The parameter is $p; we use $ret as the assignment target name.
//           Two alpha-equivalent versions should match.

#[test]
fn egg_test_4_function_with_assignment() {
    check_equal(
        "function with addition in body (alpha-equiv)",
        "(fun main $p (assign $ret (add 1 (var $p))))",
        "(fun main $q (assign $ret (add 1 (var $q))))",
        true,
    );
}

// --- Test 5: multi-statement function requiring commutativity ---
// egg (simplified): the same sequence of assignments but with (+ a b) vs (+ b a)
//                   in the last statement, requires commutativity to prove equal.
// slotted: we use a multi-parameter function with statements in sequence,
//          where the last statement differs by argument order.

#[test]
fn egg_test_5_complex_function_commutativity() {
    // fun main($x, $y, $z) {  // all locals hoisted to function signature
    //   x = 5;
    //   y = 6;
    //   z = x + y;   // vs  z = y + x  in the second version
    // }
    let a = "(fun main $x $y $z \
              (seq (assign $x 5) \
              (seq (assign $y 6) \
                   (assign $z (add (var $x) (var $y))))))";
    let b = "(fun main $x $y $z \
              (seq (assign $x 5) \
              (seq (assign $y 6) \
                   (assign $z (add (var $y) (var $x))))))";

    check_equal(
        "complex multi-stmt function needing commutativity",
        a,
        b,
        true,
    );
}

// --- Bonus: demonstrate what slotted gives you for free ---
// Two functions that differ in parameter names AND internal variable names
// should all be alpha-equivalent without any rewrite rules.

#[test]
fn slotted_bonus_deep_alpha_equivalence() {
    // fun main(a, b) { b = 1 + a; ret b }
    //    versus
    // fun main(z, w) { w = 1 + z; ret w }
    // Both parameters AND locally-bound names differ — slotted handles it.
    //
    // All locals are hoisted into the function's MultiBind (our new design).
    let a = "(fun main $a $b (seq (assign $b (add 1 (var $a))) (ret (var $b))))";
    let b = "(fun main $z $w (seq (assign $w (add 1 (var $z))) (ret (var $w))))";

    check_equal(
        "deep alpha-equivalence (params + locals, all hoisted to fun)",
        a,
        b,
        true,
    );
}
