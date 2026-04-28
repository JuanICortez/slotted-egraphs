#![allow(unused)]
#![allow(non_snake_case)]

use crate::*;

mod rewrite;
pub use rewrite::*;

mod my_cost;
pub use my_cost::*;

mod const_fold;
pub use const_fold::*;

mod multi_bind;
pub use multi_bind::*;

mod c_loop;
pub use c_loop::*;

mod spans;
pub use spans::*;

mod search;
pub use search::*;

mod egg_ports;
pub use egg_ports::*;

mod tst;

// A simplified C-like subset represented as an Abstract Binding Tree (ABT).
// Variables are represented as Slots, giving us alpha-equivalence for free.
//
// MENTAL MODEL: the e-graph stores PROGRAM STRUCTURE, not value claims.
//
// `(var $x)` represents the *name* `$x` — not "x's current value". Two
// occurrences of `(var $x)` are syntactically the same fragment, but the
// e-graph makes no claim about what runtime value they evaluate to.
//
// Equality in the e-graph means "interchangeable program fragments under our
// rewrite rules" — never "produces the same runtime value." That distinction
// is what lets us represent imperative programs literally.
//
// CONSEQUENCE: no SSA preprocessing required.
//
// We can keep `x = 1; x = x + 1` exactly as written. The e-graph will not
// infer `1 = x + 1` because the assignments are just nodes in a sequence,
// not equality claims. Reassignment is fine.
//
// REWRITES BECOME CONTEXT-SENSITIVE.
//
// To stay sound, rewrites need to see enough surrounding context to be sure
// the transformation is valid. Example: to substitute the value of an
// initialized variable into its first use, the rewrite pattern must match
// the literal sequence `seq(assign($x, ?init), seq(use_of_x, ?rest))` —
// only firing when both the init and the use are syntactically adjacent.
//
// This style is sometimes called "guarded rewriting" — patterns carry their
// preconditions structurally rather than relying on global analyses.
//
// DESIGN DECISIONS THAT REMAIN
//
//   1. No `Let` expression node — all variables (params + locals) are bound
//      at the function level via `Fun`'s `MultiBind`. This is purely for
//      alpha-equivalence; it does NOT imply functional/value semantics.
//
//   2. Loops are represented as direct while-statements (`Loop(cond, body)`),
//      not as recursive functions. The previous "loops as functions" design
//      was needed to side-step SSA and phi nodes — but now we don't need
//      to, and a literal while-loop is closer to the source.
//
// REQUIREMENTS ON THE TREE-SITTER → ABT PASS
//
//   1. Hoist locals into the function's `MultiBind` (for alpha-equivalence).
//   2. Normalize loop variants (`for`, `do-while`) into `while` form.
//   No SSA conversion needed.
//
// Example C code and their ABT representation:
//   int main(int x) {                fun main $x
//     return x + x;          →           (ret (add (var $x) (var $x)))
//   }
//
//   int sum(int n) {                 fun sum $n $i $sum
//     int i = 0, sum = 0;       →        (seq (assign $i 0)
//     while (i < n) {                    (seq (assign $sum 0)
//       sum = sum + i;                   (seq (loop (lt (var $i) (var $n))
//       i = i + 1;                                  (seq (assign $sum (add (var $sum) (var $i)))
//     }                                                  (assign $i (add (var $i) 1))))
//     return sum;                                  (ret (var $sum)))))
//   }
define_language! {
    pub enum CSubset {
        // Expressions
        Var(Slot) = "var",
        Num(i32),
        Add(AppliedId, AppliedId) = "add",
        Sub(AppliedId, AppliedId) = "sub",
        Mul(AppliedId, AppliedId) = "mul",
        Neg(AppliedId) = "neg",

        // Comparisons
        Eq(AppliedId, AppliedId) = "eq",
        Lt(AppliedId, AppliedId) = "lt",

        // Boolean logic
        Not(AppliedId) = "not",
        And(AppliedId, AppliedId) = "and",
        Or(AppliedId, AppliedId) = "or",

        // Control flow
        Ite(AppliedId, AppliedId, AppliedId) = "ite",
        Seq(AppliedId, AppliedId) = "seq",
        Ret(AppliedId) = "ret",

        // Functions: (fun name $p1 $p2 ... body)
        // The name is an AppliedId referencing a Symbol e-class.
        // MultiBind binds the parameters in body.
        Fun(AppliedId, MultiBind<AppliedId>) = "fun",

        // Function call: (call name arg)
        Call(AppliedId, AppliedId) = "call",

        // Assignment: (assign $x value)
        // $x is a reference to an already-bound slot (typically bound by the
        // enclosing `Fun`'s MultiBind). The e-graph stores this as a node;
        // it does NOT claim `(var $x) = value` — that would be unsound under
        // mutation. Reassignment is fine: multiple `assign` to the same slot
        // are just multiple nodes in a `seq`.
        Assign(Slot, AppliedId) = "assign",

        // While-loop: (loop cond body)
        // - cond: boolean expression — loop continues while true
        // - body: statement(s) executed each iteration
        // Variables used inside cond/body are bound up the chain (typically
        // by the enclosing function's MultiBind).
        Loop(AppliedId, AppliedId) = "loop",

        // Literals (use Symbol for nullary constants since define_language!
        // doesn't support unit variants)
        Symbol(Symbol),
    }
}
