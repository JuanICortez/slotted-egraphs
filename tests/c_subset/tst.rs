use crate::*;

// --- Arithmetic equivalences ---

#[test]
fn c_add_commutative() {
    // x + y = y + x
    let a = "(add (var $0) (var $1))";
    let b = "(add (var $1) (var $0))";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

#[test]
fn c_add_zero_identity() {
    // x + 0 = x
    let a = "(add (var $0) 0)";
    let b = "(var $0)";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

#[test]
fn c_mul_zero() {
    // x * 0 = 0
    let a = "(mul (var $0) 0)";
    let b = "0";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

#[test]
fn c_sub_self_is_zero() {
    // x - x = 0
    let a = "(sub (var $0) (var $0))";
    let b = "0";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

#[test]
fn c_double_negation() {
    // -(-x) = x
    let a = "(neg (neg (var $0)))";
    let b = "(var $0)";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

// --- Boolean equivalences ---

#[test]
fn c_not_not() {
    // !!x = x
    let a = "(not (not (var $0)))";
    let b = "(var $0)";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

#[test]
fn c_de_morgan() {
    // !(a && b) = !a || !b
    let a = "(not (and (var $0) (var $1)))";
    let b = "(or (not (var $0)) (not (var $1)))";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

// --- Control flow ---

#[test]
fn c_ite_true_branch() {
    // if (true) a else b = a
    let a = "(ite true (var $0) (var $1))";
    let b = "(var $0)";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

#[test]
fn c_ite_negated_condition() {
    // if (!c) a else b = if (c) b else a
    let a = "(ite (not (var $0)) (var $1) (var $2))";
    let b = "(ite (var $0) (var $2) (var $1))";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

// --- Binding (alpha-equivalence) ---
//
// Functions with different parameter names but identical body shapes are
// alpha-equivalent. Slotted e-graphs handle this automatically via the
// MultiBind in `Fun`.

#[test]
fn c_alpha_equivalence() {
    // fun main($x) ret (x + x) = fun main($y) ret (y + y)
    let a = "(fun main $0 (ret (add (var $0) (var $0))))";
    let b = "(fun main $1 (ret (add (var $1) (var $1))))";
    assert_reaches(a, b, &get_c_subset_rewrites(), 1);
}

// --- Sequencing ---

#[test]
fn c_seq_nop_elimination() {
    // nop; a = a
    let a = "(seq nop (var $0))";
    let b = "(var $0)";
    assert_reaches(a, b, &get_c_subset_rewrites(), 3);
}

// --- Combined: semantic equivalence across rewrites ---

#[test]
fn c_semantic_search_example() {
    // This demonstrates the core use case: two C snippets that look
    // different syntactically but are semantically equivalent.
    //
    // Snippet 1: if (!flag) y + x else 0
    // Snippet 2: if (flag) 0 else x + y
    //
    // They are equivalent via: ite-not + add-comm
    let a = "(ite (not (var $flag)) (add (var $y) (var $x)) 0)";
    let b = "(ite (var $flag) 0 (add (var $x) (var $y)))";
    assert_reaches(a, b, &get_c_subset_rewrites(), 5);
}
