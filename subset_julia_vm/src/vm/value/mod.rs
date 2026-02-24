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
#[macro_use]
pub mod array_macros;
mod array_value;
mod container;
mod io;
mod macro_;
mod memory_value;
mod metadata;
mod range;
mod regex;
mod struct_instance;
mod tuple;
mod value_enum;

// Re-exports from submodules
pub use array_data::ArrayData;
pub use array_element::ArrayElementType;
pub use array_value::{
    new_array_ref, new_typed_array_ref, ArrayRef, ArrayValue, TypedArrayRef, TypedArrayValue,
};
pub use container::{
    ComposedFunctionValue, DictIter, DictKey, DictValue, ExprValue, GeneratorValue,
    NamedTupleValue, PairsValue, SetValue,
};
pub use io::{IOKind, IORef, IOValue};
pub use macro_::{GlobalRefValue, LineNumberNodeValue, SymbolValue};
pub use memory_value::{new_memory_ref, MemoryRef, MemoryValue};
pub use metadata::{ClosureValue, FunctionValue, ModuleValue};
pub use range::RangeValue;
pub use regex::{RegexMatchValue, RegexValue};
pub use struct_instance::{StructInstance, COMPLEX_STRUCT_NAME};
pub use tuple::TupleValue;
pub use value_enum::{Value, ValueType};

// Re-export BigInt for use in other modules
pub use num_bigint::BigInt as RustBigInt;

// Re-export BigFloat for use in other modules
pub use astro_float::BigFloat as RustBigFloat;
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
        assert!(matches!(get_bigfloat_rounding(), BigFloatRoundingMode::ToZero));
        set_bigfloat_rounding_mode(original); // restore
    }
}
