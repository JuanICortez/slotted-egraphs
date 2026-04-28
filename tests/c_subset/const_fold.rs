use crate::*;

/// A constant value that the analysis can propagate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Const {
    Int(i32),
    Bool(bool),
}

#[derive(Default)]
pub struct ConstFold;

impl Analysis<CSubset> for ConstFold {
    type Data = Option<Const>;

    fn merge(x: Option<Const>, y: Option<Const>) -> Option<Const> {
        match (x, y) {
            (Some(x), Some(y)) => {
                assert_eq!(x, y);
                Some(x)
            }
            (Some(x), _) => Some(x),
            (_, Some(y)) => Some(y),
            _ => None,
        }
    }

    fn make(eg: &EGraph<CSubset, Self>, enode: &CSubset) -> Option<Const> {
        let get_int = |id: &AppliedId| match eg.analysis_data(id.id).as_ref()? {
            Const::Int(n) => Some(*n),
            _ => None,
        };
        let get_bool = |id: &AppliedId| match eg.analysis_data(id.id).as_ref()? {
            Const::Bool(b) => Some(*b),
            _ => None,
        };

        match enode {
            // Integer literals
            CSubset::Num(n) => Some(Const::Int(*n)),

            // Boolean literals
            CSubset::Symbol(s) => match s.as_str() {
                "true" => Some(Const::Bool(true)),
                "false" => Some(Const::Bool(false)),
                _ => None,
            },

            // Arithmetic
            CSubset::Add(a, b) => Some(Const::Int(get_int(a)? + get_int(b)?)),
            CSubset::Sub(a, b) => Some(Const::Int(get_int(a)? - get_int(b)?)),
            CSubset::Mul(a, b) => Some(Const::Int(get_int(a)? * get_int(b)?)),
            CSubset::Neg(a) => Some(Const::Int(-get_int(a)?)),

            // Comparisons
            CSubset::Eq(a, b) => Some(Const::Bool(get_int(a)? == get_int(b)?)),
            CSubset::Lt(a, b) => Some(Const::Bool(get_int(a)? < get_int(b)?)),

            // Boolean logic
            CSubset::Not(a) => Some(Const::Bool(!get_bool(a)?)),
            CSubset::And(a, b) => Some(Const::Bool(get_bool(a)? && get_bool(b)?)),
            CSubset::Or(a, b) => Some(Const::Bool(get_bool(a)? || get_bool(b)?)),

            // If-then-else: if the condition is known, propagate the chosen branch
            CSubset::Ite(c, t, f) => {
                if get_bool(c)? {
                    eg.analysis_data(t.id).clone()
                } else {
                    eg.analysis_data(f.id).clone()
                }
            }

            _ => None,
        }
    }

    fn modify(eg: &mut EGraph<CSubset, Self>, id: Id) {
        let data = eg.analysis_data(id).clone();
        if let Some(c) = data {
            let added = match c {
                Const::Int(n) => eg.add(CSubset::Num(n)),
                Const::Bool(b) => {
                    let sym = if b {
                        Symbol::from("true")
                    } else {
                        Symbol::from("false")
                    };
                    eg.add(CSubset::Symbol(sym))
                }
            };
            eg.union(&added, &eg.mk_identity_applied_id(id));
        }
    }
}

#[test]
fn const_fold_arithmetic() {
    // 2 + (3 * 4) = 14
    let start = RecExpr::parse("(add 2 (mul 3 4))").unwrap();

    let mut eg = EGraph::<CSubset, ConstFold>::default();
    let i = eg.add_expr(start);

    assert_eq!(eg.analysis_data(i.id), &Some(Const::Int(14)));

    let result: RecExpr<CSubset> = extract::<CSubset, ConstFold, AstSize>(&i, &eg);
    let expected = RecExpr::parse("14").unwrap();
    assert_eq!(result, expected);
}

#[test]
fn const_fold_negation() {
    // -(5 - 3) = -2
    let start = RecExpr::parse("(neg (sub 5 3))").unwrap();

    let mut eg = EGraph::<CSubset, ConstFold>::default();
    let i = eg.add_expr(start);

    assert_eq!(eg.analysis_data(i.id), &Some(Const::Int(-2)));
}

#[test]
fn const_fold_comparison() {
    // 3 < 5 = true
    let start = RecExpr::parse("(lt 3 5)").unwrap();

    let mut eg = EGraph::<CSubset, ConstFold>::default();
    let i = eg.add_expr(start);

    assert_eq!(eg.analysis_data(i.id), &Some(Const::Bool(true)));
}

#[test]
fn const_fold_boolean_logic() {
    // !(true && false) = true  (via De Morgan: true || true)
    let start = RecExpr::parse("(not (and true false))").unwrap();

    let mut eg = EGraph::<CSubset, ConstFold>::default();
    let i = eg.add_expr(start);

    assert_eq!(eg.analysis_data(i.id), &Some(Const::Bool(true)));
}

#[test]
fn const_fold_ite() {
    // if (1 < 2) 42 else 0 = 42
    let start = RecExpr::parse("(ite (lt 1 2) 42 0)").unwrap();

    let mut eg = EGraph::<CSubset, ConstFold>::default();
    let i = eg.add_expr(start);

    assert_eq!(eg.analysis_data(i.id), &Some(Const::Int(42)));

    let result: RecExpr<CSubset> = extract::<CSubset, ConstFold, AstSize>(&i, &eg);
    let expected = RecExpr::parse("42").unwrap();
    assert_eq!(result, expected);
}

#[test]
fn const_fold_no_fold_with_variable() {
    // x + 3 cannot be folded
    let start = RecExpr::parse("(add (var $0) 3)").unwrap();

    let mut eg = EGraph::<CSubset, ConstFold>::default();
    let i = eg.add_expr(start);

    assert_eq!(eg.analysis_data(i.id), &None);
}

#[test]
fn const_fold_union_propagates() {
    // After unioning x with 5, (add x 3) should fold to 8
    let mut eg = EGraph::<CSubset, ConstFold>::default();
    let x = eg.add_expr(RecExpr::parse("(var $0)").unwrap());
    let five = eg.add_expr(RecExpr::parse("5").unwrap());
    let expr = eg.add_expr(RecExpr::parse("(add (var $0) 3)").unwrap());

    eg.union(&x, &five);

    assert_eq!(eg.analysis_data(expr.id), &Some(Const::Int(8)));
}
