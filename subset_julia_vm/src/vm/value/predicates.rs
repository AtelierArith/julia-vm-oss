//! Shared predicates over `Value` for runtime dispatch.
//!
//! Centralizes "which `Value` variants does upstream Julia treat as X"
//! checks so the set stays consistent across builtins. Issue #4875
//! introduces the first such predicate, `is_scalar_carrier`, after two
//! consecutive same-class bugs (#4814 scalar `getindex`, #4871
//! `length(::Char)`) where each builtin maintained its own implicit
//! list of scalar 0-dim collection carriers and the lists drifted.

use super::value_enum::Value;

/// Returns `true` for `Value` variants upstream Julia treats as
/// 0-dimensional collections — every `Number` subtype and every
/// `AbstractChar` subtype. For these values upstream guarantees:
///
/// - `length(x) == 1`
/// - `x[1] === x`
/// - `x[i]` for `i != 1` raises `BoundsError`
/// - `eltype(x) === typeof(x)` (not yet wired to this predicate)
/// - `firstindex(x) == lastindex(x) == 1` (not yet wired)
/// - `ndims(x) == 0` (not yet wired)
///
/// Used by `Length` (`vm/builtins_collections.rs`) and `IndexLoad`
/// (`vm/exec/array_index.rs`). Future scalar-aware builtins should
/// delegate here rather than re-enumerating the carrier set.
///
/// `Symbol`, `Nothing`, and `Missing` are deliberately excluded —
/// upstream Julia raises `MethodError` on `:foo[1]`, `nothing[1]`,
/// and `missing[1]`. `Struct` values are also excluded because their
/// `getindex` / `length` dispatch is method-table driven and runs on
/// a different code path.
pub(crate) fn is_scalar_carrier(v: &Value) -> bool {
    matches!(
        v,
        // Number subtypes
        Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F16(_)
            | Value::F32(_)
            | Value::F64(_)
            | Value::BigInt(_)
            | Value::BigFloat(_)
            | Value::Bool(_)
            // AbstractChar
            | Value::Char(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::SymbolValue;

    #[test]
    fn numeric_and_char_are_scalar_carriers() {
        assert!(is_scalar_carrier(&Value::I64(5)));
        assert!(is_scalar_carrier(&Value::F64(2.5)));
        assert!(is_scalar_carrier(&Value::Bool(true)));
        assert!(is_scalar_carrier(&Value::Char('A')));
        assert!(is_scalar_carrier(&Value::I32(7)));
        assert!(is_scalar_carrier(&Value::U8(255)));
        assert!(is_scalar_carrier(&Value::F32(1.5)));
    }

    #[test]
    fn nothing_missing_symbol_are_not_scalar_carriers() {
        // Upstream Julia raises MethodError on `:foo[1]`, `nothing[1]`,
        // `missing[1]` — exclude them.
        assert!(!is_scalar_carrier(&Value::Nothing));
        assert!(!is_scalar_carrier(&Value::Missing));
        assert!(!is_scalar_carrier(&Value::Symbol(SymbolValue::new("foo"))));
    }

    #[test]
    fn string_is_not_a_scalar_carrier() {
        // Strings are AbstractString, not a 0-dim collection; they have
        // their own getindex semantics (byte/Char indexing) and a
        // proper `length`. Exclude.
        assert!(!is_scalar_carrier(&Value::Str("hi".to_string())));
    }
}
