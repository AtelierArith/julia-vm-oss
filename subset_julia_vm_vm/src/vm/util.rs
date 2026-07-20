//! Utility functions for the VM.
//!
//! This module provides various utility functions:
//! - `value_type_name`: Get the Julia type name of a Value
//! - `extract_cartesian_index_indices`: Extract indices from CartesianIndex
//! - `bind_value_to_frame`, `bind_value_to_slot`: Bind values to frame locals
//! - Type variable utilities: `is_type_variable`, `has_type_variable_param`, etc.
//!
//! For formatting functions, see the `formatting` module.

use super::error::VmError;
use super::frame::{Frame, VarTypeTag};
use super::value::{StructInstance, Value, ValueType};
use crate::inference_core::CoreType;
#[allow(unused_imports)]
use crate::rng::RngInstance;
use crate::vm::value::is_native_array_value;
use subset_julia_vm_bytecode::parse_parametric_params;

// Re-export formatting functions for backwards compatibility
pub(crate) use super::formatting::{
    expr_to_julia_string, format_float_julia, format_sprintf, format_value_print, value_to_string,
};
pub use super::formatting::{format_value, Resolved};

/// True for a module name that upstream Julia renders WITHOUT a `Main.`
/// qualification in error scopes — the top-level "root" modules (`Main`, `Base`,
/// `Core`) and the bundled stdlib modules. `Base.no_such_name` reports
/// `not defined in `Base``, not `Main.Base`; a user `module Foo` under `Main`
/// reports `Main.Foo`.
///
/// This mirrors the canonical stdlib set in
/// `crate::compile::constants::is_stdlib_module`, duplicated here (small, stable
/// list) to preserve the vm→compile layering separation — the vm module has no
/// other `crate::compile::` dependency. `check_module_scope_root_list_sync` (in
/// this crate's tests) guards against drift. Consequence of drift is only a
/// cosmetic scope-prefix mismatch on a bogus-field error, never a correctness
/// bug.
pub(crate) fn is_root_module_name(name: &str) -> bool {
    crate::module_names::is_root_module_name(name)
}

pub(crate) fn is_top_level_module_binding_scope(module_name: &str) -> bool {
    matches!(module_name, "Main" | "Base" | "Core")
}

/// Build the scope string for a module-scoped `UndefVarError` (Issue #10318).
/// sjulia stores a module value's name without the top-level `Main.` prefix
/// (`module Foo` -> `"Foo"`). Upstream Julia 1.12 renders the `UndefVarError`
/// scope as the module's qualified path: root/stdlib modules print bare
/// (`Base`, `Core`, `Printf`), `Main` itself prints `Main`, and a user module
/// defined under `Main` prints `Main.Foo`. Mirror that here.
///
/// Note: sjulia does not track a nested user module's full parent chain, so
/// `module A; module B; end; end` reports `Main.B` rather than upstream's
/// `Main.A.B` — a pre-existing module-path limitation, not introduced here.
pub(crate) fn module_scope_string(module_name: &str) -> String {
    if module_name.starts_with("Main.") || is_root_module_name(module_name) {
        // Already qualified, or a root/stdlib module that upstream prints bare.
        module_name.to_string()
    } else {
        // A user module defined under Main.
        format!("Main.{module_name}")
    }
}

/// Extract indices from a CartesianIndex struct, returning all indices from its tuple.
/// Used by IndexLoad to support A[CartesianIndex((i, j))] == A[i, j].
#[inline]
pub(crate) fn extract_cartesian_index_indices(s: &StructInstance) -> Result<Vec<i64>, VmError> {
    if &*s.struct_name != "CartesianIndex" {
        return Err(VmError::TypeError(format!(
            "expected CartesianIndex, got {}",
            s.struct_name
        )));
    }
    // CartesianIndex stores its indices in values[0] as a Tuple
    if let Some(Value::Tuple(tuple)) = s.values.first() {
        let mut indices = Vec::with_capacity(tuple.elements.len());
        for elem in &tuple.elements {
            match elem {
                Value::I64(v) => indices.push(*v),
                _ => {
                    return Err(VmError::TypeError(
                        "CartesianIndex tuple must contain I64 values".to_string(),
                    ))
                }
            }
        }
        Ok(indices)
    } else {
        Err(VmError::TypeError(
            "CartesianIndex must have a tuple field".to_string(),
        ))
    }
}

/// Extract a Julia `Integer` value as `i64` for `RegexMatch` capture indexing /
/// `haskey` (Issue #10173). Upstream `getindex(::RegexMatch, ::Integer)` and
/// `haskey(::RegexMatch, ::Integer)` accept *every* integer width (signed,
/// unsigned, `Bool`-excluded, and `BigInt`), so all of them must resolve to the
/// same capture index. Non-integer keys (`Float64`, `Symbol`, `String`, ...)
/// return `None` so the caller falls through to name lookup or a `MethodError`,
/// matching upstream where a `Float64` key has no method. Widths that overflow
/// `i64` clamp toward the nearest bound, which is guaranteed out of range for
/// the small capture vector, so the caller still raises the correct
/// `BoundsError`. `Bool` is intentionally excluded: upstream treats `m[true]`
/// as `m.captures[true]`, which is itself an error, so a non-match here (→
/// `MethodError`) is the closest faithful behavior.
pub(crate) fn regexmatch_integer_index(index: &Value) -> Option<i64> {
    match index {
        Value::I8(v) => Some(i64::from(*v)),
        Value::I16(v) => Some(i64::from(*v)),
        Value::I32(v) => Some(i64::from(*v)),
        Value::I64(v) => Some(*v),
        Value::I128(v) => {
            Some(i64::try_from(*v).unwrap_or(if v.is_negative() { i64::MIN } else { i64::MAX }))
        }
        Value::U8(v) => Some(i64::from(*v)),
        Value::U16(v) => Some(i64::from(*v)),
        Value::U32(v) => Some(i64::from(*v)),
        Value::U64(v) => Some(i64::try_from(*v).unwrap_or(i64::MAX)),
        Value::U128(v) => Some(i64::try_from(*v).unwrap_or(i64::MAX)),
        Value::BigInt(v) => Some(v.to_i64().unwrap_or(i64::MAX)),
        _ => None,
    }
}

#[inline]
pub(crate) fn value_type_name(v: &Value) -> &'static str {
    // Route the legacy native-array carrier through the shared
    // `native_array_value_ref` helper so the match below no longer holds a
    // native-array arm (Issue #3908). Matches the prior semantics: returns
    // "Array" for any native-array carrier value.
    if is_native_array_value(v) {
        return "Array";
    }
    match v {
        Value::I8(_) => "Int8",
        Value::I16(_) => "Int16",
        Value::I32(_) => "Int32",
        Value::I64(_) => "Int64",
        Value::I128(_) => "Int128",
        Value::U8(_) => "UInt8",
        Value::U16(_) => "UInt16",
        Value::U32(_) => "UInt32",
        Value::U64(_) => "UInt64",
        Value::U128(_) => "UInt128",
        Value::Bool(_) => "Bool",
        Value::F16(_) => "Float16",
        Value::F32(_) => "Float32",
        Value::F64(_) => "Float64",
        Value::BigInt(_) => "BigInt",
        Value::BigFloat(_) => "BigFloat",
        Value::Str(_) => "String",
        Value::Char(_) | Value::CharMalformed(_) => "Char",
        Value::Nothing => "Nothing",
        Value::Missing => "Missing",
        Value::Range(_) => "Range",
        Value::SliceAll => "Colon",
        Value::Struct(s) if s.is_complex() => "Complex", // Complex is now a Pure Julia struct
        Value::Struct(_) => "Struct",
        Value::StructRef(_) => "StructRef",
        Value::Rng(_) => "Rng",
        Value::Tuple(_) => "Tuple",
        Value::NamedTuple(_) => "NamedTuple",
        Value::Ref(_) => "Ref",
        Value::Generator(_) => "Base.Generator",
        Value::DataType(_) => "DataType",
        Value::RuntimeTypeVar(_) => "TypeVar",
        Value::RuntimeTypeName(_) => "Core.TypeName",
        Value::Module(_) => "Module",
        Value::Function(_) => "Function",
        Value::Closure(_) => "Function", // Closures are Functions
        Value::ComposedFunction(_) => "ComposedFunction",
        Value::Undef => "#undef",
        Value::IO(_) => "IO",
        // Macro system types
        Value::Symbol(_) => "Symbol",
        Value::Expr(_) => "Expr",
        Value::QuoteNode(_) => "QuoteNode",
        Value::LineNumberNode(_) => "LineNumberNode",
        Value::GlobalRef(_) => "GlobalRef",
        Value::Binding(_) => "Core.Binding",
        // Base.Pairs type (for kwargs...)
        Value::Pairs(_) => "Pairs",
        // Regex types
        Value::Regex(_) => "Regex",
        Value::RegexMatch(_) => "RegexMatch",
        // Enum type
        Value::Enum { .. } => "Enum",
        // Memory type
        Value::Memory(_) => "Memory",
        Value::MemoryRef(_) => "MemoryRef",
        // The legacy native-array carrier is filtered out by the early-return
        // above (Issue #3908). This wildcard satisfies Rust's exhaustiveness
        // checking and provides a safe default for any future `Value` variant:
        // return "Any".
        _ => "Any",
    }
}

/// Bind a value to a frame local variable with type coercion
/// Note: For untyped parameters (Any -> I64), we respect the actual runtime type
pub(crate) fn bind_value_to_frame(
    frame: &mut Frame,
    name: &str,
    _ty: ValueType,
    val: Value,
    struct_heap: &mut Vec<StructInstance>,
) {
    // Route the legacy native-array carrier through the shared
    // `native_array_value_ref` helper so the match below no longer holds a
    // native-array arm (Issue #3908). The native-array case stores into
    // `locals_any` with `VarTypeTag::Any`, identical to the other "any"
    // arms in the match.
    if is_native_array_value(&val) {
        frame.locals_any.insert(name.to_string(), val);
        frame.var_types.insert(name.to_string(), VarTypeTag::Any);
        return;
    }
    let tag = match &val {
        Value::I64(v) => {
            frame.locals_any.insert(name.to_string(), Value::I64(*v));
            VarTypeTag::I64
        }
        Value::F64(v) => {
            frame.locals_any.insert(name.to_string(), Value::F64(*v));
            VarTypeTag::F64
        }
        Value::Tuple(t) => {
            frame
                .locals_any
                .insert(name.to_string(), Value::Tuple(t.clone()));
            VarTypeTag::Tuple
        }
        Value::NamedTuple(nt) => {
            frame
                .locals_any
                .insert(name.to_string(), Value::NamedTuple(nt.clone()));
            VarTypeTag::NamedTuple
        }
        Value::Rng(r) => {
            frame
                .locals_any
                .insert(name.to_string(), Value::Rng(r.clone()));
            VarTypeTag::Rng
        }
        Value::Str(s) => {
            frame
                .locals_any
                .insert(name.to_string(), Value::str_new(s.clone()));
            VarTypeTag::Str
        }
        Value::Struct(s) => {
            let idx = struct_heap.len();
            struct_heap.push(s.clone());
            frame
                .locals_any
                .insert(name.to_string(), Value::StructRef(idx));
            VarTypeTag::Struct
        }
        Value::StructRef(idx) => {
            frame
                .locals_any
                .insert(name.to_string(), Value::StructRef(*idx));
            VarTypeTag::Struct
        }
        Value::Function(_)
        | Value::Closure(_)
        | Value::ComposedFunction(_)
        | Value::Module(_)
        | Value::DataType(_)
        | Value::RuntimeTypeVar(_)
        | Value::Ref(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Char(c) => {
            frame.locals_any.insert(name.to_string(), Value::Char(*c));
            VarTypeTag::Char
        }
        Value::Nothing => {
            frame.locals_any.insert(name.to_string(), Value::Nothing);
            VarTypeTag::Nothing
        }
        Value::Missing => {
            frame.locals_any.insert(name.to_string(), val.clone());
            VarTypeTag::Any
        }
        Value::Range(r) => {
            frame
                .locals_any
                .insert(name.to_string(), Value::Range(r.clone()));
            VarTypeTag::Range
        }
        Value::Generator(g) => {
            frame
                .locals_any
                .insert(name.to_string(), Value::Generator(g.clone()));
            VarTypeTag::Generator
        }
        Value::BigInt(_) | Value::BigFloat(_) | Value::IO(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::F32(v) => {
            frame.locals_any.insert(name.to_string(), Value::F32(*v));
            VarTypeTag::F32
        }
        Value::F16(v) => {
            frame.locals_any.insert(name.to_string(), Value::F16(*v));
            VarTypeTag::F16
        }
        Value::Bool(b) => {
            frame.locals_any.insert(name.to_string(), Value::Bool(*b));
            VarTypeTag::Bool
        }
        Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I128(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::NarrowInt
        }
        Value::Undef | Value::SliceAll => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Symbol(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Symbol
        }
        Value::Expr(_)
        | Value::QuoteNode(_)
        | Value::LineNumberNode(_)
        | Value::GlobalRef(_)
        | Value::Binding(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Pairs(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Regex(_) | Value::RegexMatch(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Enum { .. } => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Memory(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::MemoryRef(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        // The legacy native-array carrier is filtered out by the early-return
        // at the top of this function (Issue #3908). This wildcard satisfies
        // Rust's exhaustiveness checking and provides a safe default for any
        // future Value variant: store as `Any` in `locals_any`, matching the
        // sibling "any" arms above.
        _ => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
    };
    frame.var_types.insert(name.to_string(), tag);
}

pub(crate) fn bind_value_to_slot(
    frame: &mut Frame,
    slot: usize,
    val: Value,
    struct_heap: &mut Vec<StructInstance>,
) {
    let val = match val {
        Value::Struct(s) => {
            let idx = struct_heap.len();
            struct_heap.push(s);
            Value::StructRef(idx)
        }
        // Pass through all other Value variants unchanged (e.g., I64, F64, Bool, etc.)
        // This is intentional: only Struct needs heap allocation for local slot storage
        other => other,
    };
    frame.set_slot_value(slot, val);
    // Note: slot out of bounds is silently ignored here since this function
    // doesn't return Result. Callers should validate slot indices.
}

/// Check if a type parameter string represents a type variable (like T, S, T1)
/// rather than a concrete type (like Float64, Int64).
///
/// Type variables are typically:
/// - Single uppercase letters: T, S, R, N
/// - Uppercase letter followed by digits: T1, T2
///
/// Concrete types are:
/// - Multi-character type names: Float64, Int64, Bool, String
/// - Known short types: U8, I8, etc.
pub(crate) fn is_type_variable(param: &str) -> bool {
    if param.is_empty() {
        return false;
    }

    // A declared struct/abstract type whose name matches the type-variable shape
    // (e.g. `S2`, `W1`, `Q7`) is a real, concrete type — not a placeholder.
    // Consult the VM-local registered-type-name set first so a registered type is
    // never misclassified as a type variable along the runtime dispatch/convert
    // path, regardless of spelling (Issue #9464).
    if crate::types::is_registered_type_name(param) {
        return false;
    }

    // Must start with uppercase letter
    let first = match param.chars().next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_uppercase() {
        return false;
    }

    // Known concrete types that are short
    const KNOWN_CONCRETE: &[&str] = &[
        "U8", "I8", "U16", "I16", "U32", "I32", "U64", "I64", "U128", "I128", "F32", "F64", "Bool",
        "Any", "Char", "IO",
    ];
    if KNOWN_CONCRETE.contains(&param) {
        return false;
    }

    // Type variables are typically 1-2 characters (T, S, T1, T2)
    // Concrete types like Float64, String, Int64 are longer
    if param.len() <= 2 {
        // Allow single letter or letter+digit (T, S, T1, N1)
        param.chars().skip(1).all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Check if a parametric type pattern has a type variable as its parameter.
/// e.g., "Complex{T}" returns true, "Complex{Float64}" returns false
pub(crate) fn has_type_variable_param(type_str: &str) -> bool {
    parse_parametric_params(type_str)
        .iter()
        .any(|p| is_type_variable(p.trim()))
}

/// Infer the type parameter from a Value (for runtime struct type inference).
/// Returns a type name like "Int64", "Float64", "Bool", etc.
pub(crate) fn infer_type_param_from_value(val: &Value) -> &'static str {
    match val {
        Value::I8(_) => "Int8",
        Value::I16(_) => "Int16",
        Value::I32(_) => "Int32",
        Value::I64(_) => "Int64",
        Value::I128(_) => "Int128",
        Value::U8(_) => "UInt8",
        Value::U16(_) => "UInt16",
        Value::U32(_) => "UInt32",
        Value::U64(_) => "UInt64",
        Value::U128(_) => "UInt128",
        Value::Bool(_) => "Bool",
        Value::F16(_) => "Float16",
        Value::F32(_) => "Float32",
        Value::F64(_) => "Float64",
        Value::BigInt(_) => "BigInt",
        Value::BigFloat(_) => "BigFloat",
        Value::Str(_) => "String",
        Value::Char(_) | Value::CharMalformed(_) => "Char",
        _ => "Any", // For complex types, fall back to Any
    }
}

/// Resolve a parametric struct name with {Any} to the correct concrete type.
/// For example, "Complex{Any}" with Float64 values becomes "Complex{Float64}".
/// Returns None if the struct name doesn't need correction.
pub(crate) fn resolve_any_type_param(struct_name: &str, values: &[Value]) -> Option<String> {
    // Only handle struct names containing {Any}
    if !struct_name.contains("{Any}") {
        return None;
    }

    // Extract base name (e.g., "Complex" from "Complex{Any}")
    let brace_pos = struct_name.find('{')?;
    let base_name = &struct_name[..brace_pos];

    // Infer type from the first value (all fields should have same type for parametric structs)
    if let Some(first_val) = values.first() {
        let type_param = infer_type_param_from_value(first_val);
        if type_param != "Any" {
            return Some(format!("{}{{{}}}", base_name, type_param));
        }
    }

    None
}

/// Check if a Value is a builtin numeric type that should be handled by
/// the builtin binary operator path rather than method dispatch.
///
/// Used by `CallDynamicBinaryBoth` (call_dynamic.rs) to skip user-defined method
/// dispatch for same-type primitive operations during nary operator reduction.
/// (Issue #2437, #2439)
#[inline]
pub(crate) fn is_builtin_numeric_value(v: &Value) -> bool {
    CoreType::from_julia_name(value_type_name(v)).is_primitive_numeric()
}

/// Extract the base type name from a possibly-parametric type string.
///
/// For example, `"Rational{Int64}"` returns `"Rational"`, while `"Int64"` returns `"Int64"`.
/// This is used by all dynamic dispatch handlers for type matching.
#[inline]
pub(crate) fn extract_base_type(s: &str) -> &str {
    if let Some(idx) = s.find('{') {
        &s[..idx]
    } else {
        s
    }
}

#[inline]
pub(crate) fn strip_module_prefix(name: &str) -> &str {
    name.rfind('.').map_or(name, |idx| &name[idx + 1..])
}

#[inline]
pub(crate) fn is_dict_type_name(type_name: &str) -> bool {
    strip_module_prefix(extract_base_type(type_name)) == "Dict"
}

/// Carrier-removal stub (Issue #6731). `Value::Dict` no longer exists, so no
/// value is ever a Rust dict carrier that must be excluded from matching a
/// parametric `Dict{K,V}` annotation — this always returns `false`. Kept so the
/// binary-dispatch filters that combine it with [`is_struct_dict_bare_mismatch`]
/// need no structural change.
#[inline]
pub(crate) fn is_rust_dict_parametric_mismatch(_value: &Value, _expected_type: &str) -> bool {
    false
}

/// Carrier-removal stub (Issue #7632). Pure Julia `Dict{K,V}` is now the only
/// public Dict carrier, so a bare `::Dict` annotation is the upstream UnionAll
/// family and must match StructRef-backed Dict values. Kept so dynamic and
/// binary dispatch filters need no structural change.
#[inline]
pub(crate) fn is_struct_dict_bare_mismatch(
    _value: &Value,
    _expected_type: &str,
    _struct_heap: &[StructInstance],
) -> bool {
    false
}

// NOTE: needs_julia_promote() and promote_hardcoded() have been removed.
// All type promotion now goes through Julia's promotion.jl path,
// matching official Julia behavior. See promotion.jl for the implementation.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_scope_string_matches_upstream_wording() {
        // Issue #10318: root/stdlib modules print bare; a user module under
        // Main gets the `Main.` prefix; Main itself stays `Main`.
        assert_eq!(module_scope_string("Base"), "Base");
        assert_eq!(module_scope_string("Core"), "Core");
        assert_eq!(module_scope_string("Printf"), "Printf");
        assert_eq!(module_scope_string("LinearAlgebra"), "LinearAlgebra");
        assert_eq!(module_scope_string("Main"), "Main");
        assert_eq!(module_scope_string("SubMod9"), "Main.SubMod9");
        // Already-qualified names are not double-prefixed.
        assert_eq!(module_scope_string("Main.SubMod9"), "Main.SubMod9");
    }

    #[test]
    fn check_module_scope_root_list_sync() {
        // Issue #10318: the vm-local `is_root_module_name` mirrors the canonical
        // stdlib list in `compile::constants::is_stdlib_module`. Guard against
        // drift — every name one accepts, the other must accept (Main/Base/Core
        // are in both lists).
        for name in [
            "Main",
            "Base",
            "Core",
            "Sys",
            "LinearAlgebra",
            "Statistics",
            "Random",
            "Dates",
            "Printf",
            "Test",
            "SparseArrays",
            "Distributed",
            "SharedArrays",
            "Serialization",
            "REPL",
            "InteractiveUtils",
            "Pkg",
            "Markdown",
            "UUIDs",
            "Sockets",
            "DelimitedFiles",
            "FileWatching",
        ] {
            assert!(
                is_root_module_name(name),
                "vm is_root_module_name should accept root module {name}"
            );
            assert!(
                crate::module_names::is_root_module_name(name),
                "shared module classifier should accept {name}"
            );
        }
        // A user module is neither.
        assert!(!is_root_module_name("MyUserModule"));
        assert!(!crate::module_names::is_root_module_name("MyUserModule"));
    }

    #[test]
    fn test_is_type_variable() {
        // Type variables (should return true)
        assert!(is_type_variable("T"));
        assert!(is_type_variable("S"));
        assert!(is_type_variable("N"));
        assert!(is_type_variable("T1"));
        assert!(is_type_variable("T2"));

        // Concrete types (should return false)
        assert!(!is_type_variable("Float64"));
        assert!(!is_type_variable("Int64"));
        assert!(!is_type_variable("String"));
        assert!(!is_type_variable("Bool"));
        assert!(!is_type_variable("Complex"));
        assert!(!is_type_variable("Rational"));

        // Known short concrete types (should return false)
        assert!(!is_type_variable("U8"));
        assert!(!is_type_variable("I8"));
        assert!(!is_type_variable("F64"));
        assert!(!is_type_variable("Any"));

        // Edge cases
        assert!(!is_type_variable(""));
        assert!(!is_type_variable("lowercase"));
        assert!(!is_type_variable("123"));
    }

    #[test]
    fn test_has_type_variable_param() {
        // Type variable patterns (should return true)
        assert!(has_type_variable_param("Complex{T}"));
        assert!(has_type_variable_param("Vector{T}"));
        assert!(has_type_variable_param("Array{T, N}"));
        assert!(has_type_variable_param("Tuple{T, S}"));

        // Concrete type patterns (should return false)
        assert!(!has_type_variable_param("Complex{Float64}"));
        assert!(!has_type_variable_param("Vector{Int64}"));
        assert!(!has_type_variable_param("Array{Float64, 2}"));
        assert!(!has_type_variable_param("Tuple{Int64, String}"));

        // Non-parametric types (should return false)
        assert!(!has_type_variable_param("Complex"));
        assert!(!has_type_variable_param("Int64"));
        assert!(!has_type_variable_param("Float64"));

        // Mixed - at least one type variable (should return true)
        assert!(has_type_variable_param("Tuple{T, Int64}"));
        assert!(has_type_variable_param("Array{Float64, N}"));
    }

    #[test]
    fn test_parse_parametric_params_preserves_tuple_value_param() {
        assert_eq!(parse_parametric_params("Val{(1, 2)}"), vec!["(1, 2)"]);
        assert_eq!(
            parse_parametric_params("Tuple{Val{(1, 2)}, Int64}"),
            vec!["Val{(1, 2)}", "Int64"]
        );
        assert!(!has_type_variable_param("Val{(1, 2)}"));
        assert!(has_type_variable_param("Val{N}"));
    }

    /// Verify is_builtin_numeric_value covers all expected Value variants.
    /// This test ensures the runtime check (Value-based) stays in sync with
    /// the compile-time check (JuliaType::is_builtin_numeric in types.rs).
    /// When adding new numeric Value variants, this test should be updated.
    #[test]
    fn test_is_builtin_numeric_value_completeness() {
        // All builtin numeric values should return true
        assert!(is_builtin_numeric_value(&Value::I64(0)));
        assert!(is_builtin_numeric_value(&Value::F64(0.0)));
        assert!(is_builtin_numeric_value(&Value::F32(0.0)));
        assert!(is_builtin_numeric_value(&Value::F16(half::f16::from_f32(
            0.0
        ))));
        assert!(is_builtin_numeric_value(&Value::Bool(false)));
        assert!(is_builtin_numeric_value(&Value::I8(0)));
        assert!(is_builtin_numeric_value(&Value::I16(0)));
        assert!(is_builtin_numeric_value(&Value::I32(0)));
        assert!(is_builtin_numeric_value(&Value::I128(0)));
        assert!(is_builtin_numeric_value(&Value::U8(0)));
        assert!(is_builtin_numeric_value(&Value::U16(0)));
        assert!(is_builtin_numeric_value(&Value::U32(0)));
        assert!(is_builtin_numeric_value(&Value::U64(0)));
        assert!(is_builtin_numeric_value(&Value::U128(0)));

        // Non-numeric values should return false
        assert!(!is_builtin_numeric_value(&Value::Nothing));
        assert!(!is_builtin_numeric_value(&Value::BigInt(
            num_bigint::BigInt::from(0).into()
        )));
        assert!(!is_builtin_numeric_value(&Value::bigfloat_from_f64(0.0)));
        assert!(!is_builtin_numeric_value(&Value::str_new("x".to_string())));
        assert!(!is_builtin_numeric_value(&Value::Char('a')));
    }

    #[test]
    fn test_extract_base_type() {
        assert_eq!(extract_base_type("Rational{Int64}"), "Rational");
        assert_eq!(extract_base_type("Complex{Float64}"), "Complex");
        assert_eq!(extract_base_type("Array{Int64, 2}"), "Array");
        assert_eq!(extract_base_type("Int64"), "Int64");
        assert_eq!(extract_base_type("Vector"), "Vector");
        assert_eq!(extract_base_type("Rational{T}"), "Rational");
    }

    /// Verify that bind_value_to_frame routes typed scalar locals consistently.
    /// This prevents the regression where parameter binding and StoreAny chose
    /// different maps for the same value type. (Issue #3322)
    #[test]
    fn test_bind_value_to_frame_typed_locals_routing() {
        let mut heap = vec![];

        // F32 → locals_any with an F32 tag
        let mut frame = Frame::new();
        bind_value_to_frame(
            &mut frame,
            "x",
            ValueType::F32,
            Value::F32(1.5_f32),
            &mut heap,
        );
        assert!(
            frame.locals_any.contains_key("x"),
            "F32 should be in locals_any after bind"
        );
        assert_eq!(frame.var_types.get("x"), Some(&VarTypeTag::F32));

        // F16 → locals_any with an F16 tag
        bind_value_to_frame(
            &mut frame,
            "y",
            ValueType::F16,
            Value::F16(half::f16::from_f32(0.5)),
            &mut heap,
        );
        assert!(
            frame.locals_any.contains_key("y"),
            "F16 should be in locals_any after bind"
        );
        assert_eq!(frame.var_types.get("y"), Some(&VarTypeTag::F16));

        // Bool → locals_any with a Bool tag
        bind_value_to_frame(
            &mut frame,
            "z",
            ValueType::Bool,
            Value::Bool(true),
            &mut heap,
        );
        assert!(
            frame.locals_any.contains_key("z"),
            "Bool should be in locals_any after bind"
        );
        assert_eq!(frame.var_types.get("z"), Some(&VarTypeTag::Bool));

        // Char → locals_any with a Char tag
        bind_value_to_frame(
            &mut frame,
            "ch",
            ValueType::Char,
            Value::Char('x'),
            &mut heap,
        );
        assert!(
            frame.locals_any.contains_key("ch"),
            "Char should be in locals_any after bind"
        );
        assert_eq!(frame.var_types.get("ch"), Some(&VarTypeTag::Char));

        // I64 → locals_any with an I64 tag
        bind_value_to_frame(&mut frame, "n", ValueType::I64, Value::I64(42), &mut heap);
        assert!(
            frame.locals_any.contains_key("n"),
            "I64 should be in locals_any after bind"
        );
        assert_eq!(frame.var_types.get("n"), Some(&VarTypeTag::I64));

        // F64 → locals_any with an F64 tag
        bind_value_to_frame(&mut frame, "d", ValueType::F64, Value::F64(2.5), &mut heap);
        assert!(
            frame.locals_any.contains_key("d"),
            "F64 should be in locals_any after bind"
        );
        assert_eq!(frame.var_types.get("d"), Some(&VarTypeTag::F64));

        bind_value_to_frame(
            &mut frame,
            "sym",
            ValueType::Symbol,
            Value::Symbol(crate::vm::value::SymbolValue::new("legacy")),
            &mut heap,
        );
        assert_eq!(frame.var_types.get("sym"), Some(&VarTypeTag::Symbol));
        assert!(
            matches!(frame.get_local("sym"), Some(Value::Symbol(sym)) if sym.as_str() == "legacy")
        );
    }

    // Note (Issue #6733): test_pop_array_or_values_* was removed along with the
    // pop_array_or_values helper and PopArrayResult enum (orphaned by the removal
    // of the legacy reducer HOF VM instructions).

    #[test]
    fn test_bind_value_to_frame_memory_stays_in_any_storage() {
        use crate::vm::value::{new_memory_ref, ArrayData, ArrayElementType, MemoryValue};

        let mut frame = Frame::new();
        let mut heap = vec![];
        let mem = new_memory_ref(MemoryValue::new(
            ArrayData::I64(vec![1, 2]),
            ArrayElementType::I64,
            2,
        ));

        bind_value_to_frame(
            &mut frame,
            "m",
            ValueType::MemoryOf(ArrayElementType::I64),
            Value::Memory(mem.clone()),
            &mut heap,
        );

        assert!(matches!(
            frame.locals_any.get("m"),
            Some(Value::Memory(stored)) if std::rc::Rc::ptr_eq(stored, &mem)
        ));
        assert_eq!(frame.var_types.get("m"), Some(&VarTypeTag::Any));
        assert!(
            matches!(frame.get_local("m"), Some(Value::Memory(stored)) if std::rc::Rc::ptr_eq(&stored, &mem))
        );
    }
}
