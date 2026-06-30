//! Native word-size Julia type aliases.
//!
//! The Julia `Int` / `UInt` aliases are normally the platform word-size integer
//! (`Int64` on a 64-bit host, `Int32` on a 32-bit host). SubsetJuliaVM, however,
//! has a **uniform 64-bit integer model**: integer literals always lower to
//! `Value::I64` / `ValueType::I64` (`compile/utils.rs`) and there is no 32-bit
//! native integer carrier for default literals. Tying the `Int` / `UInt` *type
//! aliases* to `usize::BITS` therefore breaks the VM's own internal consistency
//! on 32-bit targets (wasm32): a parameter annotated `::Int` would resolve to
//! `Int32`, but the `Int64` literal/runtime value passed to it would never
//! match — so any user function with an `::Int` parameter fails to dispatch with
//! a spurious `MethodError` on wasm32 while working on 64-bit native (Issue
//! #7310).
//!
//! Because the integer carrier is uniformly `Int64`, `Int` / `UInt` must always
//! resolve to `Int64` / `UInt64`, independent of the host pointer width. (The
//! genuinely platform-dependent `Sys.WORD_SIZE` is computed separately from
//! `usize::BITS` and is unaffected.)

use super::JuliaType;

/// Canonical concrete type name for Julia's native signed word alias `Int`.
pub(crate) fn native_int_type_name() -> &'static str {
    "Int64"
}

/// Canonical concrete type name for Julia's native unsigned word alias `UInt`.
pub(crate) fn native_uint_type_name() -> &'static str {
    "UInt64"
}

/// Canonical `JuliaType` for Julia's native signed word alias `Int`.
pub(crate) fn native_int_julia_type() -> JuliaType {
    JuliaType::Int64
}

/// Canonical `JuliaType` for Julia's native unsigned word alias `UInt`.
pub(crate) fn native_uint_julia_type() -> JuliaType {
    JuliaType::UInt64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Int` / `UInt` must always alias the 64-bit integer types, regardless of
    /// the host pointer width, because the VM's integer carrier is uniformly
    /// `Int64`. On a 32-bit target (wasm32) a pointer-width-derived `Int32`
    /// alias would never match the `Int64` literals it is compared against,
    /// breaking dispatch (Issue #7310).
    #[test]
    fn native_word_aliases_are_always_64_bit() {
        assert_eq!(native_int_type_name(), "Int64");
        assert_eq!(native_uint_type_name(), "UInt64");
        assert_eq!(native_int_julia_type(), JuliaType::Int64);
        assert_eq!(native_uint_julia_type(), JuliaType::UInt64);
    }
}
