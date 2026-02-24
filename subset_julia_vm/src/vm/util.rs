//! Utility functions for the VM.
//!
//! This module provides various utility functions:
//! - `value_type_name`: Get the Julia type name of a Value
//! - `extract_cartesian_index_indices`: Extract indices from CartesianIndex
//! - `pop_array_or_values`: Pop array from stack with type handling
//! - `bind_value_to_frame`, `bind_value_to_slot`: Bind values to frame locals
//! - Type variable utilities: `is_type_variable`, `has_type_variable_param`, etc.
//!
//! For formatting functions, see the `formatting` module.

use super::error::VmError;
use super::frame::{Frame, VarTypeTag};
use super::stack_ops::StackOps;
use super::value::{new_array_ref, ArrayRef, ArrayValue, StructInstance, Value, ValueType};
#[allow(unused_imports)]
use crate::rng::RngInstance;

// Re-export formatting functions for backwards compatibility
pub(crate) use super::formatting::{
    expr_to_julia_string, format_float_julia, format_sprintf, format_value, value_to_string,
};

/// Extract indices from a CartesianIndex struct, returning all indices from its tuple.
/// Used by IndexLoad to support A[CartesianIndex((i, j))] == A[i, j].
#[inline]
pub(crate) fn extract_cartesian_index_indices(s: &StructInstance) -> Result<Vec<i64>, VmError> {
    if s.struct_name != "CartesianIndex" {
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

#[inline]
pub(crate) fn value_type_name(v: &Value) -> &'static str {
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
        Value::Char(_) => "Char",
        Value::Nothing => "Nothing",
        Value::Missing => "Missing",
        Value::Array(_) => "Array",
        Value::Range(_) => "Range",
        Value::SliceAll => "Colon",
        Value::Struct(s) if s.is_complex() => "Complex", // Complex is now a Pure Julia struct
        Value::Struct(_) => "Struct",
        Value::StructRef(_) => "StructRef",
        Value::Rng(_) => "Rng",
        Value::Tuple(_) => "Tuple",
        Value::NamedTuple(_) => "NamedTuple",
        Value::Dict(_) => "Dict",
        Value::Set(_) => "Set",
        Value::Ref(_) => "Ref",
        Value::Generator(_) => "Base.Generator",
        Value::DataType(_) => "DataType",
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
        // Base.Pairs type (for kwargs...)
        Value::Pairs(_) => "Pairs",
        // Regex types
        Value::Regex(_) => "Regex",
        Value::RegexMatch(_) => "RegexMatch",
        // Enum type
        Value::Enum { .. } => "Enum",
        // Memory type
        Value::Memory(_) => "Memory",
    }
}

/// Convert a Memory value to a 1-D Array reference (Issue #2764).
/// Memory{T} is stored as ArrayValue with shape=[length].
pub(crate) fn memory_to_array_ref(mem: &super::value::MemoryRef) -> ArrayRef {
    let borrowed = mem.borrow();
    new_array_ref(ArrayValue::new(borrowed.data.clone(), vec![borrowed.len()]))
}

/// Result of popping an array that may contain struct elements
pub(crate) enum PopArrayResult {
    /// F64 array (can be used with legacy HOF path)
    F64Array(ArrayRef),
    /// Value array with shape (for struct arrays, use value-based HOF path)
    Values {
        values: Vec<Value>,
        shape: Vec<usize>,
    },
}

/// Pop an array from the stack, preserving struct elements as Values
/// Returns either an F64Array or a Values result for struct arrays
pub(crate) fn pop_array_or_values(st: &mut Vec<Value>) -> Result<PopArrayResult, VmError> {
    use super::value::ArrayData;

    /// Macro to reduce duplication for numeric ArrayData → f64 conversion arms.
    /// Converts each element via `as f64`, clones shape, drops borrow.
    macro_rules! numeric_to_f64 {
        ($v:expr, $arr_borrow:expr) => {{
            let data: Vec<f64> = $v.iter().map(|&x| x as f64).collect();
            let shape = $arr_borrow.shape.clone();
            drop($arr_borrow);
            Ok(PopArrayResult::F64Array(new_array_ref(
                ArrayValue::from_f64(data, shape),
            )))
        }};
    }

    match st.pop_value()? {
        Value::Array(arr) => {
            let arr_borrow = arr.borrow();
            match &arr_borrow.data {
                // For F64 arrays, return as-is
                ArrayData::F64(_) => {
                    drop(arr_borrow);
                    Ok(PopArrayResult::F64Array(arr))
                }
                // StructRefs need value-based processing
                ArrayData::StructRefs(refs) => {
                    let values: Vec<Value> =
                        refs.iter().map(|&idx| Value::StructRef(idx)).collect();
                    let shape = arr_borrow.shape.clone();
                    drop(arr_borrow);
                    Ok(PopArrayResult::Values { values, shape })
                }
                // Any array with values
                ArrayData::Any(v) => {
                    let values = v.clone();
                    let shape = arr_borrow.shape.clone();
                    drop(arr_borrow);
                    Ok(PopArrayResult::Values { values, shape })
                }
                // String/Char arrays are not supported for HOF
                ArrayData::String(_) | ArrayData::Char(_) => Err(VmError::TypeError(
                    "String/Char arrays not supported for map/filter".to_string(),
                )),
                // Convert I64 to Values (preserving integer type)
                ArrayData::I64(v) => {
                    let values: Vec<Value> = v.iter().map(|&x| Value::I64(x)).collect();
                    let shape = arr_borrow.shape.clone();
                    drop(arr_borrow);
                    Ok(PopArrayResult::Values { values, shape })
                }
                // Other numeric types - convert to f64 array
                ArrayData::F32(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::I8(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::I16(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::I32(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::U8(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::U16(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::U32(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::U64(v) => numeric_to_f64!(v, arr_borrow),
                ArrayData::Bool(v) => {
                    let data: Vec<f64> = v.iter().map(|&x| if x { 1.0 } else { 0.0 }).collect();
                    let shape = arr_borrow.shape.clone();
                    drop(arr_borrow);
                    Ok(PopArrayResult::F64Array(new_array_ref(
                        ArrayValue::from_f64(data, shape),
                    )))
                }
            }
        }
        // Memory → Array conversion (Issue #2764)
        Value::Memory(mem) => {
            let arr = memory_to_array_ref(&mem);
            let arr_borrow = arr.borrow();
            match &arr_borrow.data {
                ArrayData::F64(_) => {
                    drop(arr_borrow);
                    Ok(PopArrayResult::F64Array(arr))
                }
                _ => {
                    let values: Vec<Value> = (0..arr_borrow.len())
                        .filter_map(|i| arr_borrow.data.get_value(i))
                        .collect();
                    let shape = arr_borrow.shape.clone();
                    drop(arr_borrow);
                    Ok(PopArrayResult::Values { values, shape })
                }
            }
        }
        other => Err(VmError::TypeError(format!(
            "expected Array, got {:?}",
            value_type_name(&other)
        ))),
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
    let tag = match &val {
        Value::I64(v) => {
            frame.locals_i64.insert(name.to_string(), *v);
            VarTypeTag::I64
        }
        Value::F64(v) => {
            frame.locals_f64.insert(name.to_string(), *v);
            VarTypeTag::F64
        }
        Value::Array(a) => {
            frame.locals_array.insert(name.to_string(), a.clone());
            VarTypeTag::Array
        }
        Value::Tuple(t) => {
            frame.locals_tuple.insert(name.to_string(), t.clone());
            VarTypeTag::Tuple
        }
        Value::NamedTuple(nt) => {
            frame
                .locals_named_tuple
                .insert(name.to_string(), nt.clone());
            VarTypeTag::NamedTuple
        }
        Value::Dict(d) => {
            frame.locals_dict.insert(name.to_string(), d.clone());
            VarTypeTag::Dict
        }
        Value::Set(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Rng(r) => {
            frame.locals_rng.insert(name.to_string(), r.clone());
            VarTypeTag::Rng
        }
        Value::Str(s) => {
            frame.locals_str.insert(name.to_string(), s.clone());
            VarTypeTag::Str
        }
        Value::Struct(s) => {
            let idx = struct_heap.len();
            struct_heap.push(s.clone());
            frame.locals_struct.insert(name.to_string(), idx);
            VarTypeTag::Struct
        }
        Value::StructRef(idx) => {
            frame.locals_struct.insert(name.to_string(), *idx);
            VarTypeTag::Struct
        }
        Value::Function(_)
        | Value::Closure(_)
        | Value::ComposedFunction(_)
        | Value::Module(_)
        | Value::DataType(_)
        | Value::Ref(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Char(c) => {
            frame.locals_char.insert(name.to_string(), *c);
            VarTypeTag::Char
        }
        Value::Nothing => {
            frame.locals_nothing.insert(name.to_string());
            VarTypeTag::Nothing
        }
        Value::Missing => {
            frame.locals_any.insert(name.to_string(), val.clone());
            VarTypeTag::Any
        }
        Value::Range(r) => {
            frame.locals_range.insert(name.to_string(), r.clone());
            VarTypeTag::Range
        }
        Value::Generator(g) => {
            frame.locals_generator.insert(name.to_string(), g.clone());
            VarTypeTag::Generator
        }
        Value::BigInt(_) | Value::BigFloat(_) | Value::IO(_) => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::F32(v) => {
            frame.locals_f32.insert(name.to_string(), *v);
            VarTypeTag::F32
        }
        Value::F16(v) => {
            frame.locals_f16.insert(name.to_string(), *v);
            VarTypeTag::F16
        }
        Value::Bool(b) => {
            frame.locals_bool.insert(name.to_string(), *b);
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
            frame.locals_narrow_int.insert(name.to_string(), val);
            VarTypeTag::NarrowInt
        }
        Value::Undef | Value::SliceAll => {
            frame.locals_any.insert(name.to_string(), val);
            VarTypeTag::Any
        }
        Value::Symbol(_)
        | Value::Expr(_)
        | Value::QuoteNode(_)
        | Value::LineNumberNode(_)
        | Value::GlobalRef(_) => {
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
        Value::Memory(mem) => {
            frame
                .locals_array
                .insert(name.to_string(), memory_to_array_ref(mem));
            VarTypeTag::Array
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
    if let Some(slot_ref) = frame.locals_slots.get_mut(slot) {
        *slot_ref = Some(val);
    }
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
    if let Some(start) = type_str.find('{') {
        if let Some(end) = type_str.rfind('}') {
            let param = &type_str[start + 1..end];
            // Handle multiple type params like "Tuple{T, S}" - check if any is a type variable
            param.split(',').any(|p| is_type_variable(p.trim()))
        } else {
            false
        }
    } else {
        false
    }
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
        Value::Char(_) => "Char",
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
/// This is the runtime counterpart of `JuliaType::is_builtin_numeric()` in types.rs.
/// Both functions must cover the same set of types — when adding new numeric
/// `Value` variants, update both this function and `JuliaType::is_builtin_numeric()`.
///
/// Used by `CallDynamicBinaryBoth` (call_dynamic.rs) to skip user-defined method
/// dispatch for same-type primitive operations during nary operator reduction.
/// (Issue #2437, #2439)
#[inline]
pub(crate) fn is_builtin_numeric_value(v: &Value) -> bool {
    matches!(
        v,
        Value::I64(_)
            | Value::F64(_)
            | Value::F32(_)
            | Value::F16(_)
            | Value::Bool(_)
            | Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
    )
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

/// Score how well an expected type pattern matches an actual type.
///
/// Returns a priority score used by all `CallDynamic*` dispatch handlers to select
/// the most specific matching method. Higher scores = more specific match.
///
/// **Score values:**
/// - `4` — Exact match: `"Rational{Int64}"` == `"Rational{Int64}"`
/// - `3` — Type variable parametric match: `"Rational{T}"` matches `"Rational{Int64}"`
/// - `2` — Non-parametric base match: `"Rational"` matches `"Rational{Int64}"`
/// - `2` — Array family match: `"Array"` matches `"Vector"`, `"Matrix"`, `"Array"`
/// - `0` — No match (caller should try `check_subtype` and assign score `1` if it matches)
///
/// **Note:** Subtype checking (score 1) is not done here because it requires `&self` (VM state).
/// Callers should fall back to `check_subtype()` when this function returns 0.
///
/// # Arguments
/// * `expected` — The candidate method's declared parameter type (e.g., `"Rational{T}"`)
/// * `actual` — The runtime argument's type name (e.g., `"Rational{BigInt}"`)
/// * `actual_base` — Pre-computed base of `actual` via `extract_base_type()` (e.g., `"Rational"`)
#[inline]
pub(crate) fn score_type_match(expected: &str, actual: &str, actual_base: &str) -> u32 {
    if expected == actual {
        4 // Exact match (highest priority)
    } else {
        let expected_base = extract_base_type(expected);
        if expected_base == actual_base {
            if !expected.contains('{') {
                2 // Non-parametric base match: "Rational" matches "Rational{Int64}"
            } else if expected_base == "Tuple" {
                // Tuple covariance (Issue #2524): Tuple{Any} matches Tuple{Int64}
                // Check if arity matches and all expected params are "Any"
                tuple_covariant_score(expected, actual).unwrap_or_default()
            } else if has_type_variable_param(expected) {
                3 // Type variable pattern: "Rational{T}" matches "Rational{Int64}"
            } else {
                0 // Concrete parametric mismatch: "Rational{Int64}" != "Rational{BigInt}"
            }
        } else if expected == "Array"
            && (actual_base == "Vector" || actual_base == "Matrix" || actual_base == "Array")
        {
            2 // Array family match
        } else {
            0 // No match — caller should try check_subtype for score 1
        }
    }
}

/// Check if a Value::Dict (Rust-backed) should be excluded from matching
/// a parametric Dict{K,V} type annotation.
///
/// Value::Dict (Rust-backed) should NOT be dispatched to Pure Julia Dict functions
/// that expect StructRef with field access (.slots, .keys, etc.).
/// This prevents "GetFieldByName: expected struct, got Dict" errors.
/// (Issue #2748)
#[inline]
pub(crate) fn is_rust_dict_parametric_mismatch(value: &Value, expected_type: &str) -> bool {
    if !matches!(value, Value::Dict(_)) {
        return false;
    }
    let expected_base = extract_base_type(expected_type);
    expected_base == "Dict" && expected_type.contains('{')
}

/// Score Tuple covariant matching (Issue #2524).
/// Tuple is covariant: Tuple{Int64} <: Tuple{Any}, Tuple{Int64} <: Tuple{Number}.
/// Returns Some(score) if arity matches, None otherwise.
fn tuple_covariant_score(expected: &str, actual: &str) -> Option<u32> {
    let expected_params = parse_parametric_params(expected);
    let actual_params = parse_parametric_params(actual);

    // Arity must match for parametric Tuple matching
    if expected_params.len() != actual_params.len() {
        return None;
    }

    // Check if all expected params are "Any" → score 3 (like type variable match)
    if expected_params.iter().all(|p| *p == "Any") {
        return Some(3);
    }

    // Check if expected has type variable params → score 3
    if expected_params.iter().any(|p| is_type_variable(p)) {
        return Some(3);
    }

    None // Concrete param mismatch — caller should try check_subtype for score 1
}

/// Parse type parameters from a parametric type string.
/// "Tuple{Int64, Float64}" → ["Int64", "Float64"]
/// "Tuple{}" → []
#[inline]
pub(crate) fn parse_parametric_params(type_str: &str) -> Vec<&str> {
    let start = match type_str.find('{') {
        Some(idx) => idx + 1,
        None => return vec![],
    };
    let end = match type_str.rfind('}') {
        Some(idx) => idx,
        None => return vec![],
    };
    let inner = &type_str[start..end];
    if inner.is_empty() {
        return vec![];
    }
    // Split by comma, respecting nested braces
    let mut result = Vec::new();
    let mut depth = 0;
    let mut last_start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                result.push(inner[last_start..i].trim());
                last_start = i + 1;
            }
            _ => {}
        }
    }
    result.push(inner[last_start..].trim());
    result
}

// NOTE: needs_julia_promote() and promote_hardcoded() have been removed.
// All type promotion now goes through Julia's promotion.jl path,
// matching official Julia behavior. See promotion.jl for the implementation.

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(is_builtin_numeric_value(&Value::U8(0)));
        assert!(is_builtin_numeric_value(&Value::U16(0)));
        assert!(is_builtin_numeric_value(&Value::U32(0)));
        assert!(is_builtin_numeric_value(&Value::U64(0)));

        // Non-numeric values should return false
        assert!(!is_builtin_numeric_value(&Value::Nothing));
        assert!(!is_builtin_numeric_value(&Value::Str("x".to_string())));
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

    #[test]
    fn test_score_type_match() {
        // Exact match → 4
        assert_eq!(
            score_type_match("Rational{Int64}", "Rational{Int64}", "Rational"),
            4
        );
        assert_eq!(score_type_match("Int64", "Int64", "Int64"), 4);

        // Type variable parametric → 3
        assert_eq!(
            score_type_match("Rational{T}", "Rational{Int64}", "Rational"),
            3
        );
        assert_eq!(
            score_type_match("Complex{T}", "Complex{Float64}", "Complex"),
            3
        );

        // Non-parametric base → 2
        assert_eq!(
            score_type_match("Rational", "Rational{Int64}", "Rational"),
            2
        );
        assert_eq!(
            score_type_match("Complex", "Complex{Float64}", "Complex"),
            2
        );

        // Array family → 2
        assert_eq!(score_type_match("Array", "Vector{Int64}", "Vector"), 2);
        assert_eq!(score_type_match("Array", "Matrix{Float64}", "Matrix"), 2);
        assert_eq!(score_type_match("Array", "Array{Int64}", "Array"), 2);

        // Concrete parametric mismatch → 0
        assert_eq!(
            score_type_match("Rational{Int64}", "Rational{BigInt}", "Rational"),
            0
        );
        assert_eq!(
            score_type_match("Complex{Float64}", "Complex{Int64}", "Complex"),
            0
        );

        // Unrelated types → 0
        assert_eq!(score_type_match("String", "Int64", "Int64"), 0);
        assert_eq!(
            score_type_match("Float64", "Rational{Int64}", "Rational"),
            0
        );
    }

    /// Verify that bind_value_to_frame routes F32/F16/Bool to their dedicated
    /// typed locals maps (not locals_any). This prevents the regression where
    /// parameter binding used locals_any while StoreAny used typed maps. (Issue #3322)
    #[test]
    fn test_bind_value_to_frame_typed_locals_routing() {
        let mut heap = vec![];

        // F32 → locals_f32
        let mut frame = Frame::new();
        bind_value_to_frame(&mut frame, "x", ValueType::F32, Value::F32(1.5_f32), &mut heap);
        assert!(
            frame.locals_f32.contains_key("x"),
            "F32 should be in locals_f32 after bind"
        );
        assert!(
            !frame.locals_any.contains_key("x"),
            "F32 should NOT be in locals_any after bind"
        );

        // F16 → locals_f16
        bind_value_to_frame(
            &mut frame,
            "y",
            ValueType::F16,
            Value::F16(half::f16::from_f32(0.5)),
            &mut heap,
        );
        assert!(
            frame.locals_f16.contains_key("y"),
            "F16 should be in locals_f16 after bind"
        );
        assert!(
            !frame.locals_any.contains_key("y"),
            "F16 should NOT be in locals_any after bind"
        );

        // Bool → locals_bool
        bind_value_to_frame(
            &mut frame,
            "z",
            ValueType::Bool,
            Value::Bool(true),
            &mut heap,
        );
        assert!(
            frame.locals_bool.contains_key("z"),
            "Bool should be in locals_bool after bind"
        );
        assert!(
            !frame.locals_any.contains_key("z"),
            "Bool should NOT be in locals_any after bind"
        );

        // I64 → locals_i64 (sanity check for core type)
        bind_value_to_frame(&mut frame, "n", ValueType::I64, Value::I64(42), &mut heap);
        assert!(
            frame.locals_i64.contains_key("n"),
            "I64 should be in locals_i64 after bind"
        );
        assert!(
            !frame.locals_any.contains_key("n"),
            "I64 should NOT be in locals_any after bind"
        );

        // F64 → locals_f64 (sanity check for core type)
        bind_value_to_frame(
            &mut frame,
            "d",
            ValueType::F64,
            Value::F64(2.5),
            &mut heap,
        );
        assert!(
            frame.locals_f64.contains_key("d"),
            "F64 should be in locals_f64 after bind"
        );
        assert!(
            !frame.locals_any.contains_key("d"),
            "F64 should NOT be in locals_any after bind"
        );
    }
}
