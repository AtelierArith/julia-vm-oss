//! Numeric integer identity/equality helpers shared by `==`/`===` value
//! comparison in the VM core (Issue #6334: extracted from `vm/mod.rs`).

use super::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NumericInteger {
    NonNegative(u128),
    Negative(i128),
}

pub(super) fn signed_integer_value(value: i128) -> NumericInteger {
    if value >= 0 {
        NumericInteger::NonNegative(value.cast_unsigned())
    } else {
        NumericInteger::Negative(value)
    }
}

pub(super) fn numeric_integer_value(value: &Value) -> Option<NumericInteger> {
    match value {
        Value::I8(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I16(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I32(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I64(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I128(v) => Some(signed_integer_value(*v)),
        Value::U8(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U16(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U32(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U64(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U128(v) => Some(NumericInteger::NonNegative(*v)),
        _ => None,
    }
}

pub(super) fn numeric_integer_values_equal(left: &Value, right: &Value) -> Option<bool> {
    Some(numeric_integer_value(left)? == numeric_integer_value(right)?)
}

pub(super) fn numeric_integer_values_identical(left: &Value, right: &Value) -> Option<bool> {
    let left_integer = numeric_integer_value(left)?;
    let right_integer = numeric_integer_value(right)?;
    Some(
        std::mem::discriminant(left) == std::mem::discriminant(right)
            && left_integer == right_integer,
    )
}

/// Exact ordering of an arbitrary fixed-width integer (`Int8`…`Int128` /
/// `UInt8`…`UInt128`, captured as a [`NumericInteger`]) against an `f64`, with NO
/// rounding of the integer to `f64` (Issue #8187 / #8199). Returns `None` iff `f`
/// is `NaN`.
///
/// The naive `(i as f64).cmp(f)` loses precision once `|i|` exceeds the float's
/// exact-integer range (`2^53` for `Float64`, `2^24` for `Float32`), which made
/// e.g. `9007199254740993 == 9.007199254740992e15` wrongly `true`. This mirrors
/// the value-based comparison upstream Julia performs in `base/float.jl` (the
/// `for Ti in (Int64,UInt64,Int128,UInt128), Tf in (Float32,Float64)` @eval
/// block): compare the integer against `floor(f)` — exact and integer-valued —
/// then refine by the fractional part, never widening the integer. A `Float32`
/// or `Float16` operand is compared by losslessly widening it to `f64` first
/// (every such value is exactly representable as `f64`), so the result is
/// precision-independent.
pub(crate) fn cmp_integer_to_f64(n: NumericInteger, f: f64) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    if f.is_nan() {
        return None;
    }
    // `floor(f)` is integer-valued (or ±Inf), so the comparison against it is
    // exact; the only fractional information left is whether `f > floor(f)`.
    let f_floor = f.floor();
    let cmp_to_floor = cmp_integer_to_integral_f64(n, f_floor);
    Some(match cmp_to_floor {
        // n == floor(f) but f has a positive fractional part => n < f.
        Ordering::Equal if f > f_floor => Ordering::Less,
        other => other,
    })
}

/// Ordering of `n` against an *integer-valued* (or ±Inf) `f64` `g`, exact for the
/// full `i128`/`u128` ranges. `g` must not be `NaN` and must equal `g.floor()`.
//
// The `g as u128` / `g as i128` casts are explicitly range-guarded above (`g`
// non-negative and `< 2^128` for the unsigned cast; `g` negative and `>= -2^127`
// for the signed cast) and `g` is integer-valued, so each cast is exact and
// sign-preserving — the sign-loss / truncation lints do not apply.
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn cmp_integer_to_integral_f64(n: NumericInteger, g: f64) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    // `u128::MAX < 2^128`; the first f64 at/above the `u128` range is `2^128`.
    const TWO_POW_128: f64 = 340_282_366_920_938_463_463_374_607_431_768_211_456.0;
    // `i128::MIN == -2^127` exactly; anything strictly below is out of range.
    const NEG_TWO_POW_127: f64 = -170_141_183_460_469_231_731_687_303_715_884_105_728.0;
    match n {
        NumericInteger::NonNegative(u) => {
            if g < 0.0 {
                return Ordering::Greater; // non-negative int > negative/-Inf float
            }
            if g >= TWO_POW_128 {
                return Ordering::Less; // u128 < 2^128 <= g (incl. +Inf)
            }
            // `g` is integer-valued in `[0, 2^128)`, so the cast is exact.
            u.cmp(&(g as u128))
        }
        NumericInteger::Negative(s) => {
            // `s < 0` by construction.
            if g >= 0.0 {
                return Ordering::Less; // negative int < non-negative/+Inf float
            }
            if g < NEG_TWO_POW_127 {
                return Ordering::Greater; // s >= i128::MIN > g (incl. -Inf)
            }
            // `g` is integer-valued in `[-2^127, 0)`, so the cast is exact.
            s.cmp(&(g as i128))
        }
    }
}

/// The exact `f64` value of a fixed-width IEEE float `Value` (`Float16` /
/// `Float32` / `Float64`). `Float16`→`f64` and `Float32`→`f64` are lossless.
/// Returns `None` for any other value — notably `BigFloat`, which keeps its own
/// promotion-based comparison path (a concrete value-based shortcut would break
/// `big % big == 1.5`-style coercion, see `reference_mixed_int_float_comparison_paths`).
fn fixed_float_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::F64(v) => Some(*v),
        Value::F32(v) => Some(f64::from(*v)),
        Value::F16(v) => Some(v.to_f64()),
        _ => None,
    }
}

/// If `(left, right)` — in either order — is a mixed fixed-width integer / fixed
/// IEEE-float pair, returns the exact value-based ordering of `left` vs `right`
/// (Issue #8199, generalizing the `Int64`/`Float64` case of #8187 to every
/// `Int*`/`UInt*` × `Float16`/`Float32`/`Float64` mix). The inner `None` means
/// the float operand is `NaN` (callers map it to op-specific results: every
/// relational op is `false`, `!=` is `true`, `==` is `false`). The outer `None`
/// means it is not such a pair — leave the existing promote/dispatch handling
/// untouched.
pub(crate) fn mixed_int_float_ordering(
    left: &Value,
    right: &Value,
) -> Option<Option<std::cmp::Ordering>> {
    use std::cmp::Ordering;
    if let (Some(n), Some(f)) = (numeric_integer_value(left), fixed_float_as_f64(right)) {
        return Some(cmp_integer_to_f64(n, f));
    }
    if let (Some(f), Some(n)) = (fixed_float_as_f64(left), numeric_integer_value(right)) {
        return Some(cmp_integer_to_f64(n, f).map(Ordering::reverse));
    }
    None
}

/// Value-based `==` for a mixed fixed-width integer / fixed IEEE-float pair
/// (Issue #8199). `Some(true/false)` when `(left, right)` is such a pair (`NaN`
/// and unequal values → `false`), `None` when it is not — leaving non-mixed
/// equality handling to the caller.
pub(crate) fn mixed_int_float_values_equal(left: &Value, right: &Value) -> Option<bool> {
    mixed_int_float_ordering(left, right).map(|ord| ord == Some(std::cmp::Ordering::Equal))
}

/// `true` iff `value` is a fixed IEEE float (`Float16`/`Float32`/`Float64`) equal
/// to negative zero. Unlike `==`, `isequal` distinguishes `-0.0` from `+0.0`, so
/// `isequal(0, -0.0)` is `false` even though `0 == -0.0` (the integer operand is
/// always `+0`). Used to apply that rule at the mixed integer/float `isequal`
/// sites for every width.
pub(crate) fn is_negative_zero_fixed_float(value: &Value) -> bool {
    let f = match value {
        Value::F64(v) => *v,
        Value::F32(v) => f64::from(*v),
        Value::F16(v) => v.to_f64(),
        _ => return false,
    };
    f == 0.0 && f.is_sign_negative()
}

#[cfg(test)]
mod cmp_i64_f64_tests {
    use super::{cmp_integer_to_f64, signed_integer_value};
    use std::cmp::Ordering;

    /// Convenience: exact `i64`-vs-`f64` ordering through the general primitive.
    fn cmp_i64_to_f64(i: i64, f: f64) -> Option<Ordering> {
        cmp_integer_to_f64(signed_integer_value(i128::from(i)), f)
    }

    #[test]
    fn exact_beyond_2pow53_does_not_round() {
        // 2^53 + 1 vs the f64 that 2^53 rounds to (2^53). Must NOT be equal.
        let i = 9_007_199_254_740_993_i64; // 2^53 + 1
        let f = 9_007_199_254_740_992.0_f64; // 2^53
        assert_eq!(cmp_i64_to_f64(i, f), Some(Ordering::Greater));
        // The reverse value where the integer rounds UP under naive widening.
        let i2 = 9_007_199_254_740_995_i64; // 2^53 + 3 -> rounds to 2^53 + 4
        let f2 = 9_007_199_254_740_996.0_f64; // 2^53 + 4
        assert_eq!(cmp_i64_to_f64(i2, f2), Some(Ordering::Less));
    }

    #[test]
    fn small_and_exact_values() {
        assert_eq!(cmp_i64_to_f64(1, 1.0), Some(Ordering::Equal));
        assert_eq!(cmp_i64_to_f64(2, 2.5), Some(Ordering::Less));
        assert_eq!(cmp_i64_to_f64(3, 2.5), Some(Ordering::Greater));
        assert_eq!(cmp_i64_to_f64(0, -0.0), Some(Ordering::Equal));
        assert_eq!(cmp_i64_to_f64(-1, -1.0), Some(Ordering::Equal));
    }

    #[test]
    fn boundary_and_nonfinite() {
        // typemax(Int64) vs Float64(typemax) == 2^63 (rounds up, so int < float).
        assert_eq!(
            cmp_i64_to_f64(i64::MAX, 9_223_372_036_854_775_808.0),
            Some(Ordering::Less)
        );
        // typemin(Int64) == -2^63 exactly.
        assert_eq!(
            cmp_i64_to_f64(i64::MIN, -9_223_372_036_854_775_808.0),
            Some(Ordering::Equal)
        );
        assert_eq!(cmp_i64_to_f64(0, f64::INFINITY), Some(Ordering::Less));
        assert_eq!(
            cmp_i64_to_f64(0, f64::NEG_INFINITY),
            Some(Ordering::Greater)
        );
        assert_eq!(cmp_i64_to_f64(0, f64::NAN), None);
    }
}

#[cfg(test)]
mod mixed_int_float_tests {
    use super::{cmp_integer_to_f64, mixed_int_float_ordering, NumericInteger};
    use crate::vm::Value;
    use std::cmp::Ordering;

    #[test]
    fn uint64_beyond_2pow53_exact() {
        // UInt64(2^53 + 1) vs the f64 2^53 rounds to. Must NOT be equal (#8199).
        let n = NumericInteger::NonNegative(9_007_199_254_740_993); // 2^53 + 1
        let f = 9_007_199_254_740_992.0_f64; // 2^53
        assert_eq!(cmp_integer_to_f64(n, f), Some(Ordering::Greater));
    }

    #[test]
    fn uint128_and_int128_huge_values() {
        // 2^100 + 1 (exact integer) vs f64 2^100 (exact power of two).
        let big = (1_u128 << 100) + 1;
        let n = NumericInteger::NonNegative(big);
        let f = 2.0_f64.powi(100);
        assert_eq!(cmp_integer_to_f64(n, f), Some(Ordering::Greater));
        // i128::MIN == -2^127 exactly.
        let n_min = NumericInteger::Negative(i128::MIN);
        let f_min = -(2.0_f64.powi(127));
        assert_eq!(cmp_integer_to_f64(n_min, f_min), Some(Ordering::Equal));
        // u128::MAX < 2^128 == first representable f64 above the range.
        let n_max = NumericInteger::NonNegative(u128::MAX);
        assert_eq!(
            cmp_integer_to_f64(n_max, 2.0_f64.powi(128)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn float32_operand_is_exact_via_f64() {
        // Float32 value beyond its 2^24 exact-integer range, compared to a larger
        // Int64. The Float32 widens losslessly to f64; the integer never rounds.
        let f32_val = 16_777_216.0_f32; // 2^24, exactly representable in Float32
        let f = f64::from(f32_val);
        let n = NumericInteger::NonNegative(16_777_217); // 2^24 + 1
        assert_eq!(cmp_integer_to_f64(n, f), Some(Ordering::Greater));
    }

    #[test]
    fn pair_detection_either_order_and_widths() {
        // (UInt64, Float64) and (Float64, UInt64) — symmetric, value-based.
        let i = Value::U64(9_007_199_254_740_993);
        let f = Value::F64(9_007_199_254_740_992.0);
        assert_eq!(
            mixed_int_float_ordering(&i, &f),
            Some(Some(Ordering::Greater))
        );
        assert_eq!(mixed_int_float_ordering(&f, &i), Some(Some(Ordering::Less)));
        // (Int64, Float32) mixed pair is detected.
        let i64v = Value::I64(16_777_217);
        let f32v = Value::F32(16_777_216.0);
        assert_eq!(
            mixed_int_float_ordering(&i64v, &f32v),
            Some(Some(Ordering::Greater))
        );
        // Non-mixed pairs and BigFloat are NOT claimed.
        assert_eq!(
            mixed_int_float_ordering(&Value::I64(1), &Value::I64(2)),
            None
        );
        assert_eq!(
            mixed_int_float_ordering(&Value::F64(1.0), &Value::F64(2.0)),
            None
        );
    }

    #[test]
    fn nan_pair_yields_inner_none() {
        let i = Value::U64(5);
        let nan = Value::F64(f64::NAN);
        // Still a mixed pair (outer Some) but NaN (inner None).
        assert_eq!(mixed_int_float_ordering(&i, &nan), Some(None));
        assert_eq!(mixed_int_float_ordering(&nan, &i), Some(None));
    }
}
