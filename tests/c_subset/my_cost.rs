use crate::*;

use std::cmp::Ordering;

// Note: with `Let` removed from CSubset, we no longer need a custom cost
// function that penalizes it. The library's built-in `AstSize` is sufficient
// for extracting the smallest equivalent term.
//
// If we later add constructs we want to avoid in extracted output (for
// example, keeping loop unrollings out of extraction), we can reintroduce
// a custom `CostFunction<CSubset>` impl here.
