//! Value module - Julia runtime values.
//!
//! This module contains all the runtime value types for the Julia VM.
//!
//! # Module Organization
//!
//! - `array_data.rs`: ArrayData enum for type-segregated storage
//! - `array_element.rs`: ArrayElementType for element type descriptors
//! - `array_macros.rs`: Macros for ArrayData dispatch
//! - `array_value/`: ArrayValue struct for N-D arrays (access, mutation sub-modules)
//! - `container.rs`: Small container types (Generator, Dict, Set, etc.)
//! - `io.rs`: IO-related types
//! - `macro_.rs`: Macro system types (Symbol, etc.)
//! - `metadata.rs`: Module and Function metadata
//! - `range.rs`: RangeValue for lazy ranges
//! - `struct_instance.rs`: StructInstance for user-defined structs
//! - `tuple.rs`: TupleValue
//! - `value_enum.rs`: Value enum and ValueType

// Submodules
mod array_data;
mod array_element;
mod array_wrapper;
#[macro_use]
pub mod array_macros;
mod array_value;
// Container value types, split by kind out of the former `container.rs`
// (Issue #6835).
mod composed_function;
mod dict;
pub mod enum_registry;
mod expr;
mod generator;
mod io;
mod macro_;
mod memory_value;
mod metadata;
mod named_tuple;
mod pairs;
mod predicates;
mod range;
mod regex;
mod set;
mod static_real;
mod struct_instance;
mod tuple;
mod value_enum;

// Re-exports from submodules
pub use array_data::{ArrayData, BitPackedBoolData};
pub use array_element::ArrayElementType;
pub(crate) use array_element::{array_element_type_to_julia_type, julia_array_type_for_ndims};
pub(crate) use array_value::{native_array_ref_from_value, native_array_ref_value};
pub(crate) use array_wrapper::{
    array_wrapper_shape_and_offset, array_wrapper_shape_from_tuple,
    array_wrapper_value_from_array_value, array_wrapper_value_from_array_value_inline,
    is_array_wrapper_struct_name,
};
// `pub` so host/FFI/test code can read a self-contained inline `Array{T,N}`
// wrapper (the host-return boundary's output since #6864) back into an
// `ArrayValue` without the `struct_heap`.
pub(crate) use array_value::is_native_array_value;
pub use array_value::native_array_value_ref;
pub use array_wrapper::array_wrapper_value_to_array_value;
// Public re-export of the owned `ArrayValue` constructor so integration tests
// and other external-crate consumers can wrap an `ArrayValue` in the legacy
// native-array carrier through the shared helper instead of constructing the
// native-array variant via `new_array_ref` directly (Issue #3908).
pub use array_value::native_array_value_from_array;
pub use array_value::{
    new_array_ref, new_typed_array_ref, ArrayRef, ArrayValue, TypedArrayRef, TypedArrayValue,
};
pub use composed_function::ComposedFunctionValue;
pub use dict::{DictIter, DictKey, DictValue};
pub use expr::ExprValue;
pub use generator::{GeneratorCallable, GeneratorValue};
pub use named_tuple::NamedTupleValue;
pub use pairs::PairsValue;
pub use set::SetValue;
// Shared predicates over `Value` (Issue #4875). Crate-internal: only
// VM builtins / exec sites need these — no external consumer should
// pattern-match the scalar-carrier set independently.
pub use io::{IOKind, IORef, IOValue};
pub use macro_::{GlobalRefValue, LineNumberNodeValue, SymbolValue};
pub use memory_value::{new_memory_ref, MemoryRef, MemoryRefValue, MemoryValue};
pub use metadata::{ClosureValue, FunctionValue, ModuleValue};
pub(crate) use predicates::is_scalar_carrier;
pub use range::{RangeElementType, RangeValue};
pub use regex::{RegexMatchValue, RegexValue};
pub use static_real::{
    static_add, static_matmat, static_matvec, static_scalar_mul, static_sub, InlineElemTag,
    StaticArrayInlineData, StaticElem, StaticRealValue,
};
pub use struct_instance::{
    is_complex_type_name, is_rational_type_name, StructInstance, COMPLEX_STRUCT_NAME,
};
pub use tuple::TupleValue;
pub(crate) use value_enum::value_type_for_struct_instance;
pub use value_enum::{
    new_ref, RefCellRef, RuntimeTypeNameValue, RuntimeTypeVarValue, Value, ValueType,
};

use std::fmt;
use std::ops::{Add, Deref, Div, Mul, Neg, Rem, Sub};
use std::rc::Rc;
use std::str::FromStr;

use num_traits::{Signed, ToPrimitive};

/// Reference-backed BigInt carrier.
///
/// Upstream Julia models `BigInt` as a mutable heap object, so `===` compares
/// object identity rather than numeric value. Cloning this wrapper preserves
/// the `Rc` identity; arithmetic and constructors allocate fresh wrappers
/// (Issue #4886).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RustBigInt(Rc<num_bigint::BigInt>);

impl RustBigInt {
    pub fn new(value: num_bigint::BigInt) -> Self {
        Self(Rc::new(value))
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn as_inner(&self) -> &num_bigint::BigInt {
        self.0.as_ref()
    }

    pub fn abs(&self) -> Self {
        Self::new(self.0.as_ref().abs())
    }

    pub fn to_i64(&self) -> Option<i64> {
        self.0.as_ref().to_i64()
    }
}

impl Default for RustBigInt {
    fn default() -> Self {
        Self::from(0)
    }
}

impl Deref for RustBigInt {
    type Target = num_bigint::BigInt;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl fmt::Display for RustBigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RustBigInt {
    type Err = <num_bigint::BigInt as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        num_bigint::BigInt::from_str(s).map(Self::new)
    }
}

impl From<num_bigint::BigInt> for RustBigInt {
    fn from(value: num_bigint::BigInt) -> Self {
        Self::new(value)
    }
}

impl PartialEq<num_bigint::BigInt> for RustBigInt {
    fn eq(&self, other: &num_bigint::BigInt) -> bool {
        self.as_inner() == other
    }
}

impl PartialOrd<num_bigint::BigInt> for RustBigInt {
    fn partial_cmp(&self, other: &num_bigint::BigInt) -> Option<std::cmp::Ordering> {
        self.as_inner().partial_cmp(other)
    }
}

macro_rules! impl_bigint_from_primitive {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for RustBigInt {
                fn from(value: $ty) -> Self {
                    Self::new(num_bigint::BigInt::from(value))
                }
            }
        )*
    };
}

impl_bigint_from_primitive!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128);

impl Neg for RustBigInt {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.0.as_ref().clone())
    }
}

macro_rules! impl_bigint_binary_op {
    ($trait:ident, $method:ident) => {
        impl $trait for RustBigInt {
            type Output = RustBigInt;

            fn $method(self, rhs: RustBigInt) -> Self::Output {
                RustBigInt::new(self.0.as_ref().clone().$method(rhs.0.as_ref().clone()))
            }
        }

        impl $trait<num_bigint::BigInt> for RustBigInt {
            type Output = RustBigInt;

            fn $method(self, rhs: num_bigint::BigInt) -> Self::Output {
                RustBigInt::new(self.0.as_ref().clone().$method(rhs))
            }
        }

        impl<'a, 'b> $trait<&'b RustBigInt> for &'a RustBigInt {
            type Output = RustBigInt;

            fn $method(self, rhs: &'b RustBigInt) -> Self::Output {
                RustBigInt::new(self.0.as_ref().clone().$method(rhs.0.as_ref().clone()))
            }
        }

        impl<'a> $trait<num_bigint::BigInt> for &'a RustBigInt {
            type Output = RustBigInt;

            fn $method(self, rhs: num_bigint::BigInt) -> Self::Output {
                RustBigInt::new(self.0.as_ref().clone().$method(rhs))
            }
        }
    };
}

impl_bigint_binary_op!(Add, add);
impl_bigint_binary_op!(Sub, sub);
impl_bigint_binary_op!(Mul, mul);
impl_bigint_binary_op!(Div, div);
impl_bigint_binary_op!(Rem, rem);

/// Reference-backed BigFloat carrier; see [`RustBigInt`] for the identity
/// rationale (Issue #4886).
#[derive(Debug, Clone, PartialEq)]
pub struct RustBigFloat(Rc<astro_float::BigFloat>);

impl RustBigFloat {
    pub fn new(value: astro_float::BigFloat) -> Self {
        Self(Rc::new(value))
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn from_f64(value: f64, precision: usize) -> Self {
        Self::new(astro_float::BigFloat::from_f64(value, precision))
    }

    pub fn parse(
        s: &str,
        radix: astro_float::Radix,
        precision: usize,
        rounding: astro_float::RoundingMode,
        consts: &mut astro_float::Consts,
    ) -> Self {
        Self::new(astro_float::BigFloat::parse(
            s, radix, precision, rounding, consts,
        ))
    }

    pub fn neg(&self) -> Self {
        Self::new(self.0.as_ref().clone().neg())
    }

    pub fn add(&self, rhs: &Self, precision: usize, rounding: astro_float::RoundingMode) -> Self {
        Self::new(
            self.0
                .as_ref()
                .clone()
                .add(rhs.0.as_ref(), precision, rounding),
        )
    }

    pub fn sub(&self, rhs: &Self, precision: usize, rounding: astro_float::RoundingMode) -> Self {
        Self::new(
            self.0
                .as_ref()
                .clone()
                .sub(rhs.0.as_ref(), precision, rounding),
        )
    }

    pub fn mul(&self, rhs: &Self, precision: usize, rounding: astro_float::RoundingMode) -> Self {
        Self::new(
            self.0
                .as_ref()
                .clone()
                .mul(rhs.0.as_ref(), precision, rounding),
        )
    }

    pub fn div(&self, rhs: &Self, precision: usize, rounding: astro_float::RoundingMode) -> Self {
        Self::new(
            self.0
                .as_ref()
                .clone()
                .div(rhs.0.as_ref(), precision, rounding),
        )
    }

    pub fn abs(&self) -> Self {
        Self::new(self.0.as_ref().clone().abs())
    }

    /// Remainder `self % rhs` (sign follows `self`, like Julia `rem`/`%`).
    /// `astro_float`'s `rem` is exact and needs no precision/rounding
    /// (Issue #6796). Caller handles the `rhs == 0` → NaN case.
    pub fn rem(&self, rhs: &Self) -> Self {
        Self::new(self.0.as_ref().rem(rhs.0.as_ref()))
    }

    /// Round toward −∞ (Julia `floor`). `astro_float`'s `floor` is exact and
    /// needs no precision/rounding (Issue #6801).
    pub fn floor(&self) -> Self {
        Self::new(self.0.as_ref().floor())
    }

    /// Round toward +∞ (Julia `ceil`); exact (Issue #6801).
    pub fn ceil(&self) -> Self {
        Self::new(self.0.as_ref().ceil())
    }

    /// Round toward zero (Julia `trunc`). `astro_float`'s `int` returns the
    /// integer part, which is round-toward-zero (Issue #6801).
    pub fn trunc(&self) -> Self {
        Self::new(self.0.as_ref().int())
    }

    /// Round to the nearest integer, ties to even (Julia's default `round` /
    /// `RoundNearest`). `round(0, ToEven)` keeps 0 fractional binary positions,
    /// matching `round(big(2.5)) == 2.0` (Issue #6801).
    pub fn round_nearest_even(&self) -> Self {
        Self::new(self.0.as_ref().round(0, astro_float::RoundingMode::ToEven))
    }

    /// `self ^ exp` via `astro_float`'s general power (exp(exp·ln(self))),
    /// which handles negative and fractional exponents. Needs the shared
    /// constant cache `consts` for the internal ln/exp (Issue #6790).
    pub fn pow(
        &self,
        exp: &Self,
        precision: usize,
        rounding: astro_float::RoundingMode,
        consts: &mut astro_float::Consts,
    ) -> Self {
        Self::new(
            self.0
                .as_ref()
                .pow(exp.0.as_ref(), precision, rounding, consts),
        )
    }

    /// Exact integer value as a `num_bigint::BigInt`, or `None` when the value
    /// is non-finite or has a fractional part. Used by the integer constructors
    /// `(::Type{<:Integer})(::BigFloat)` (Issue #6890), which raise an
    /// `InexactError` on `None` — matching upstream `Int(big(2.5))`.
    pub fn to_bigint_exact(&self) -> Option<num_bigint::BigInt> {
        let inner = self.0.as_ref();
        if inner.is_nan() || inner.is_inf_pos() || inner.is_inf_neg() {
            return None;
        }
        if inner.is_zero() {
            return Some(num_bigint::BigInt::from(0));
        }
        // Integer-valued check: `trunc(self) == self`.
        let truncated = inner.int();
        if inner.cmp(&truncated) != Some(0) {
            return None;
        }
        bigfloat_decimal_to_bigint(&truncated.to_string())
    }
}

/// Parse `astro_float`'s decimal `Display` of an *integer-valued* `BigFloat`
/// (normalized `[-]D[.F…]e[+-]N`, single leading digit) into an exact
/// `num_bigint::BigInt`. Returns `None` if the rendered value is not actually
/// integral (defensive; the caller already verified `trunc(x) == x`).
fn bigfloat_decimal_to_bigint(raw: &str) -> Option<num_bigint::BigInt> {
    let (mantissa, exp) = match raw.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().ok()?),
        None => (raw, 0i64),
    };
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.trim_start_matches(['+', '-']);
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    // Significant digits with the decimal point removed; the true value is
    // `<digits> * 10^(exp - frac_len)`.
    let digits = format!("{int_part}{frac_part}");
    let mut s = digits.trim_start_matches('0').to_string();
    if s.is_empty() {
        s.push('0');
    }
    let shift = exp - frac_part.len() as i64;
    if shift >= 0 {
        for _ in 0..shift {
            s.push('0');
        }
    } else {
        // Drop the trailing fractional digits; they must all be zero for the
        // value to be an integer.
        let drop = usize::try_from(-shift).ok()?;
        if s.len() < drop || !s.as_bytes()[s.len() - drop..].iter().all(|&b| b == b'0') {
            return None;
        }
        s.truncate(s.len() - drop);
        if s.is_empty() {
            s.push('0');
        }
    }
    let value = num_bigint::BigInt::from_str(&s).ok()?;
    Some(if negative { -value } else { value })
}

impl Default for RustBigFloat {
    fn default() -> Self {
        Self::from_f64(0.0, BIGFLOAT_PRECISION)
    }
}

impl Neg for RustBigFloat {
    type Output = Self;

    fn neg(self) -> Self::Output {
        RustBigFloat::neg(&self)
    }
}

impl Deref for RustBigFloat {
    type Target = astro_float::BigFloat;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl fmt::Display for RustBigFloat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RustBigFloat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut consts = astro_float::Consts::new().map_err(|e| e.to_string())?;
        Ok(Self::parse(
            s,
            astro_float::Radix::Dec,
            BIGFLOAT_PRECISION,
            astro_float::RoundingMode::ToEven,
            &mut consts,
        ))
    }
}

impl From<astro_float::BigFloat> for RustBigFloat {
    fn from(value: astro_float::BigFloat) -> Self {
        Self::new(value)
    }
}

pub use astro_float::Consts as BigFloatConsts;
pub use astro_float::RoundingMode as BigFloatRoundingMode;

/// Default precision for new BigFloat values (in bits).
/// This is the initial value; it can be changed via setprecision.
pub const BIGFLOAT_DEFAULT_PRECISION: usize = 256;

/// Mutable global precision for BigFloat.
/// Uses std::sync::atomic for thread-safe access.
use std::sync::atomic::{AtomicUsize, Ordering};
static BIGFLOAT_PRECISION_GLOBAL: AtomicUsize = AtomicUsize::new(BIGFLOAT_DEFAULT_PRECISION);

/// Get the current default precision for BigFloat (in bits).
pub fn get_bigfloat_precision() -> usize {
    BIGFLOAT_PRECISION_GLOBAL.load(Ordering::SeqCst)
}

/// Set the default precision for BigFloat (in bits).
/// Returns the previous precision.
pub fn set_bigfloat_precision(precision: usize) -> usize {
    BIGFLOAT_PRECISION_GLOBAL.swap(precision, Ordering::SeqCst)
}

/// Legacy constant for backward compatibility.
/// Prefer using get_bigfloat_precision() for dynamic precision support.
pub const BIGFLOAT_PRECISION: usize = BIGFLOAT_DEFAULT_PRECISION;

/// Global rounding mode for BigFloat operations.
/// Uses AtomicU8 to store the rounding mode enum.
/// Rounding modes: 0=ToEven (RoundNearest), 1=ToZero, 2=Up, 3=Down, 4=FromZero, 5=ToOdd, 6=None
static BIGFLOAT_ROUNDING_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // Default: ToEven (RoundNearest)

/// Get the current rounding mode for BigFloat operations.
/// Returns the mode as a u8: 0=ToEven, 1=ToZero, 2=Up, 3=Down, 4=FromZero, 5=ToOdd
pub fn get_bigfloat_rounding_mode() -> u8 {
    BIGFLOAT_ROUNDING_MODE.load(Ordering::SeqCst)
}

/// Set the rounding mode for BigFloat operations.
/// Returns the previous mode.
pub fn set_bigfloat_rounding_mode(mode: u8) -> u8 {
    BIGFLOAT_ROUNDING_MODE.swap(mode, Ordering::SeqCst)
}

/// Convert a rounding mode u8 to BigFloatRoundingMode.
pub fn u8_to_bigfloat_rounding_mode(mode: u8) -> BigFloatRoundingMode {
    match mode {
        0 => BigFloatRoundingMode::ToEven, // RoundNearest
        1 => BigFloatRoundingMode::ToZero,
        2 => BigFloatRoundingMode::Up,
        3 => BigFloatRoundingMode::Down,
        4 => BigFloatRoundingMode::FromZero,
        5 => BigFloatRoundingMode::ToOdd,
        _ => BigFloatRoundingMode::ToEven, // Default
    }
}

/// Get the current BigFloat rounding mode as BigFloatRoundingMode.
pub fn get_bigfloat_rounding() -> BigFloatRoundingMode {
    u8_to_bigfloat_rounding_mode(get_bigfloat_rounding_mode())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── get/set_bigfloat_precision ────────────────────────────────────────────

    #[test]
    fn test_value_bigfloat_default_precision_is_256() {
        // The default must equal the constant — verify the initial state
        // (nextest runs each test in its own process, so state is fresh)
        assert_eq!(get_bigfloat_precision(), BIGFLOAT_DEFAULT_PRECISION);
    }

    #[test]
    fn test_value_set_bigfloat_precision_returns_old_value() {
        let old = set_bigfloat_precision(512);
        let returned_old = set_bigfloat_precision(old); // restore
        assert_eq!(returned_old, 512);
    }

    #[test]
    fn test_value_get_bigfloat_precision_reflects_set() {
        let original = get_bigfloat_precision();
        set_bigfloat_precision(128);
        assert_eq!(get_bigfloat_precision(), 128);
        set_bigfloat_precision(original); // restore
    }

    // ── get/set_bigfloat_rounding_mode ────────────────────────────────────────

    #[test]
    fn test_value_default_rounding_mode_is_zero() {
        // Default is 0 = ToEven (RoundNearest)
        assert_eq!(get_bigfloat_rounding_mode(), 0);
    }

    #[test]
    fn test_value_set_rounding_mode_returns_old_value() {
        let old = set_bigfloat_rounding_mode(2); // Up
        let returned_old = set_bigfloat_rounding_mode(old); // restore
        assert_eq!(returned_old, 2);
    }

    #[test]
    fn test_value_get_rounding_mode_reflects_set() {
        let original = get_bigfloat_rounding_mode();
        set_bigfloat_rounding_mode(3); // Down
        assert_eq!(get_bigfloat_rounding_mode(), 3);
        set_bigfloat_rounding_mode(original); // restore
    }

    // ── u8_to_bigfloat_rounding_mode ─────────────────────────────────────────

    #[test]
    fn test_value_u8_to_rounding_mode_to_even() {
        assert!(matches!(
            u8_to_bigfloat_rounding_mode(0),
            BigFloatRoundingMode::ToEven
        ));
    }

    #[test]
    fn test_value_u8_to_rounding_mode_to_zero() {
        assert!(matches!(
            u8_to_bigfloat_rounding_mode(1),
            BigFloatRoundingMode::ToZero
        ));
    }

    #[test]
    fn test_value_u8_to_rounding_mode_up() {
        assert!(matches!(
            u8_to_bigfloat_rounding_mode(2),
            BigFloatRoundingMode::Up
        ));
    }

    #[test]
    fn test_value_u8_to_rounding_mode_down() {
        assert!(matches!(
            u8_to_bigfloat_rounding_mode(3),
            BigFloatRoundingMode::Down
        ));
    }

    #[test]
    fn test_value_u8_to_rounding_mode_from_zero() {
        assert!(matches!(
            u8_to_bigfloat_rounding_mode(4),
            BigFloatRoundingMode::FromZero
        ));
    }

    #[test]
    fn test_value_u8_to_rounding_mode_to_odd() {
        assert!(matches!(
            u8_to_bigfloat_rounding_mode(5),
            BigFloatRoundingMode::ToOdd
        ));
    }

    #[test]
    fn test_value_u8_to_rounding_mode_unknown_defaults_to_even() {
        assert!(matches!(
            u8_to_bigfloat_rounding_mode(99),
            BigFloatRoundingMode::ToEven
        ));
    }

    // ── get_bigfloat_rounding ─────────────────────────────────────────────────

    #[test]
    fn test_value_get_bigfloat_rounding_returns_enum() {
        let original = get_bigfloat_rounding_mode();
        set_bigfloat_rounding_mode(1); // ToZero
        assert!(matches!(
            get_bigfloat_rounding(),
            BigFloatRoundingMode::ToZero
        ));
        set_bigfloat_rounding_mode(original); // restore
    }
}
