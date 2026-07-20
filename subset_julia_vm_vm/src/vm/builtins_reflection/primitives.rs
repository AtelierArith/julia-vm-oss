use crate::types::JuliaType;
use crate::vm::value::is_native_array_value;

use super::super::error::VmError;
use super::super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, StructInstance, Value, ValueType,
};

/// Convert a ValueType to a JuliaType for use in fieldtypes.
///
/// This is the canonical VM-side `ValueType → JuliaType` conversion
/// (Issue #5916): `vm/type_objects.rs` delegates here instead of keeping a
/// parallel copy. Unions are preserved structurally (`Union{...}`), with the
/// empty union mapping to `JuliaType::Bottom` (`Union{}`).
pub(in crate::vm) fn value_type_to_julia_type(
    vt: &ValueType,
    struct_defs: &[super::super::StructDefInfo],
) -> JuliaType {
    match vt {
        ValueType::I8 => JuliaType::Int8,
        ValueType::I16 => JuliaType::Int16,
        ValueType::I32 => JuliaType::Int32,
        ValueType::I64 => JuliaType::Int64,
        ValueType::I128 => JuliaType::Int128,
        ValueType::BigInt => JuliaType::BigInt,
        ValueType::U8 => JuliaType::UInt8,
        ValueType::U16 => JuliaType::UInt16,
        ValueType::U32 => JuliaType::UInt32,
        ValueType::U64 => JuliaType::UInt64,
        ValueType::U128 => JuliaType::UInt128,
        ValueType::Bool => JuliaType::Bool,
        ValueType::F16 => JuliaType::Float16,
        ValueType::F32 => JuliaType::Float32,
        ValueType::F64 => JuliaType::Float64,
        ValueType::ComplexF32 => JuliaType::Struct("Complex{Float32}".to_string()),
        ValueType::ComplexF64 => JuliaType::Struct("Complex{Float64}".to_string()),
        ValueType::BigFloat => JuliaType::BigFloat,
        ValueType::Array | ValueType::ArrayOf(_, _) => JuliaType::Array,
        ValueType::Range => JuliaType::UnitRange,
        ValueType::Str => JuliaType::String,
        ValueType::Char => JuliaType::Char,
        ValueType::Nothing => JuliaType::Nothing,
        ValueType::Missing => JuliaType::Missing,
        ValueType::Struct(type_id) => {
            if let Some(def) = struct_defs.get(*type_id) {
                JuliaType::Struct(def.name.clone())
            } else {
                JuliaType::Any
            }
        }
        ValueType::Tuple => JuliaType::Tuple,
        ValueType::NamedTuple => JuliaType::NamedTuple,
        ValueType::Dict => JuliaType::Dict,
        ValueType::Set => JuliaType::Set,
        ValueType::DataType => JuliaType::DataType,
        ValueType::Module => JuliaType::Module,
        ValueType::IO => JuliaType::IO,
        ValueType::Function => JuliaType::Function,
        ValueType::Pairs => JuliaType::Pairs,
        ValueType::Symbol => JuliaType::Symbol,
        ValueType::Expr => JuliaType::Expr,
        ValueType::QuoteNode => JuliaType::QuoteNode,
        ValueType::LineNumberNode => JuliaType::LineNumberNode,
        ValueType::GlobalRef => JuliaType::GlobalRef,
        ValueType::Rng | ValueType::Generator | ValueType::Any => JuliaType::Any,
        ValueType::Regex => JuliaType::Struct("Regex".to_string()),
        ValueType::RegexMatch => JuliaType::Struct("RegexMatch".to_string()),
        ValueType::Enum => JuliaType::Any,
        ValueType::Union(types) => {
            if types.is_empty() {
                JuliaType::Bottom
            } else {
                JuliaType::Union(
                    types
                        .iter()
                        .map(|ty| value_type_to_julia_type(ty, struct_defs))
                        .collect(),
                )
            }
        }
        ValueType::Memory | ValueType::MemoryOf(_) => JuliaType::Any,
    }
}

/// Extract function name from a Value.
///
/// Accepts ordinary callables (functions, closures), `Symbol`/`String` name
/// carriers, and `DataType` callables such as constructors (`Int64`, `Bool`,
/// `Float64`). Routing `DataType` here lets reflection helpers — `methods`,
/// `which`, `hasmethod`, and the `infer_effects` / `infer_exception_type`
/// surface — look type callables up by their type name, matching how `nameof`
/// keys a constructor by its type name (Issue #4987).
pub(super) fn extract_func_name(val: &Value) -> Result<String, VmError> {
    match val {
        Value::Function(fv) => Ok(fv.name.clone()),
        Value::Closure(cv) => Ok(cv.name.clone()),
        Value::Str(s) => Ok(s.to_string()),
        Value::Symbol(sym) => Ok(sym.as_str().to_string()),
        Value::DataType(jt) => Ok(jt.name().to_string()),
        _ => Err(VmError::TypeError(
            "Expected function, string, or symbol".into(),
        )),
    }
}

/// Extract types from a Value (typically a DataType representing Tuple{...}).
pub(super) fn extract_types_from_value(
    val: &Value,
    struct_heap: &[StructInstance],
) -> Result<Vec<JuliaType>, VmError> {
    fn extract_types_from_values(values: Vec<Value>) -> Result<Vec<JuliaType>, VmError> {
        values
            .into_iter()
            .map(|v| match v {
                Value::DataType(jt) => Ok(*jt),
                _ => Err(VmError::TypeError("Expected type in array of types".into())),
            })
            .collect()
    }

    match val {
        Value::DataType(jt) => match jt.as_ref() {
            JuliaType::TupleOf(types) => Ok(types.clone()),
            // Bare Tuple still denotes a zero-argument signature filter here.
            JuliaType::Tuple => Ok(vec![]),
            JuliaType::Struct(name) if name.starts_with("Tuple{") => parse_tuple_types(name),
            other => Ok(vec![other.clone()]),
        },
        Value::Tuple(t) => t
            .elements
            .iter()
            .map(|v| match v {
                Value::DataType(jt) => Ok(*jt.clone()),
                _ => Err(VmError::TypeError("Expected type in tuple".into())),
            })
            .collect(),
        // methods(f, [Type1, Type2]) passes a Vector of types (Julia uses [...] syntax).
        // Route the legacy native array carrier through `native_array_value_ref`
        // so the unwrap stays centralized while #3908 retires the native container.
        _ if is_native_array_value(val) => {
            let Some(arr) = native_array_value_ref(val) else {
                return Err(VmError::TypeError("Expected Tuple type".into()));
            };
            extract_types_from_values(arr.borrow().to_value_vec())
        }
        _ => {
            if let Some(arr) = array_wrapper_value_to_array_value(val, struct_heap)? {
                return extract_types_from_values(arr.to_value_vec());
            }
            Err(VmError::TypeError("Expected Tuple type".into()))
        }
    }
}

/// Extract `(function_name, arg_types)` from a signature tuple type such as
/// `Tuple{typeof(f), Int64}` used by `Core.Compiler.return_type(sig)`.
pub(super) fn extract_signature_tuple_from_value(
    val: &Value,
) -> Result<Option<(String, Vec<JuliaType>)>, VmError> {
    let types = match val {
        Value::DataType(jt) => match jt.as_ref() {
            JuliaType::TupleOf(types) => types,
            JuliaType::Struct(name) if name.starts_with("Tuple{") => {
                let parsed = parse_tuple_types(name)?;
                return extract_signature_tuple_from_types(parsed);
            }
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };

    extract_signature_tuple_from_types(types.clone())
}

fn extract_signature_tuple_from_types(
    types: Vec<JuliaType>,
) -> Result<Option<(String, Vec<JuliaType>)>, VmError> {
    let Some((first, rest)) = types.split_first() else {
        return Ok(None);
    };
    let Some(func_name) = extract_typeof_function_name(first) else {
        return Ok(None);
    };
    Ok(Some((func_name, rest.to_vec())))
}

fn extract_typeof_function_name(ty: &JuliaType) -> Option<String> {
    let name = ty.name();
    name.strip_prefix("typeof(")
        .and_then(|s| s.strip_suffix(')'))
        .map(str::to_string)
}

/// Extract keyword names from hasmethod(f, types, kwnames).
pub(super) fn extract_kw_names_from_value(val: &Value) -> Result<Vec<String>, VmError> {
    match val {
        Value::Tuple(tuple) => tuple
            .elements
            .iter()
            .map(|v| match v {
                Value::Symbol(sym) => Ok(sym.as_str().to_string()),
                _ => Err(VmError::TypeError(
                    "Expected tuple of Symbols for keyword names".into(),
                )),
            })
            .collect(),
        _ => Err(VmError::TypeError(
            "Expected tuple of Symbols for keyword names".into(),
        )),
    }
}

/// Parse "Tuple{T1, T2, ...}" string into Vec<JuliaType>.
fn parse_tuple_types(type_str: &str) -> Result<Vec<JuliaType>, VmError> {
    let inner = type_str
        .strip_prefix("Tuple{")
        .and_then(|s| s.strip_suffix("}"))
        .ok_or_else(|| VmError::TypeError("Invalid Tuple type format".into()))?;

    if inner.is_empty() {
        return Ok(vec![]);
    }

    let types: Vec<JuliaType> = split_top_level_commas(inner)
        .into_iter()
        .map(|s| parse_type_name(s.trim()))
        .collect();

    Ok(types)
}

/// Parse a type name string into JuliaType.
fn parse_type_name(name: &str) -> JuliaType {
    JuliaType::from_name_or_struct(name)
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '{' => depth = depth.saturating_add(1),
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let part = input[start..idx].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    let part = input[start..].trim();
    if !part.is_empty() {
        parts.push(part);
    }

    parts
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::vm::value::{
        native_array_value_from_array, new_memory_ref, ArrayData, ArrayElementType, ArrayValue,
        MemoryRefValue, MemoryValue, TupleValue, Value,
    };

    fn make_any_array(values: Vec<Value>) -> ArrayValue {
        let len = values.len();
        ArrayValue {
            data: ArrayData::Any(values),
            shape: vec![len],
            struct_type_id: None,
            element_type_override: None,
            array_type_override: None,
            shared_parent: None,
        }
    }

    /// Wrap an `ArrayValue` in the transitional native array carrier for use
    /// in the reflection tests. Delegates to the shared
    /// [`native_array_value_from_array`] so the test-only constructor stays in
    /// sync with the runtime helper while #3908 retires the native container.
    fn array_value(arr: ArrayValue) -> Value {
        native_array_value_from_array(arr)
    }

    /// extract_types_from_value: DataType(single type) returns vec of that type.
    #[test]
    fn test_extract_types_single_datatype() {
        let val = Value::DataType(Box::new(JuliaType::Int64));
        let result = extract_types_from_value(&val, &[]).unwrap();
        assert_eq!(result, vec![JuliaType::Int64]);
    }

    /// extract_types_from_value: DataType(TupleOf) returns all types.
    #[test]
    fn test_extract_types_tuple_of() {
        let val = Value::DataType(Box::new(JuliaType::TupleOf(vec![
            JuliaType::Int64,
            JuliaType::Float64,
        ])));
        let result = extract_types_from_value(&val, &[]).unwrap();
        assert_eq!(result, vec![JuliaType::Int64, JuliaType::Float64]);
    }

    /// extract_types_from_value: DataType(Struct("Tuple{Int64, Float64}")) parses correctly.
    #[test]
    fn test_extract_types_tuple_string() {
        let val = Value::DataType(Box::new(JuliaType::Struct(
            "Tuple{Int64, Float64}".to_string(),
        )));
        let result = extract_types_from_value(&val, &[]).unwrap();
        assert_eq!(result, vec![JuliaType::Int64, JuliaType::Float64]);
    }

    #[test]
    fn test_extract_types_tuple_string_preserves_nested_union_issue_4270() {
        let val = Value::DataType(Box::new(JuliaType::Struct(
            "Tuple{Vector{Union{Int64, Nothing}}}".to_string(),
        )));
        let result = extract_types_from_value(&val, &[]).unwrap();
        // The nested Union is canonicalized (Issue #5066): the singleton
        // `Nothing` sorts ahead of the `isbits` `Int64`, matching upstream's
        // `Union{Nothing, Int64}`.
        assert_eq!(
            result,
            vec![JuliaType::VectorOf(Box::new(JuliaType::Union(vec![
                JuliaType::Nothing,
                JuliaType::Int64,
            ])))]
        );
    }

    /// extract_types_from_value: Tuple of DataType values returns all types.
    #[test]
    fn test_extract_types_from_tuple() {
        let val = Value::Tuple(TupleValue {
            elements: vec![
                Value::DataType(Box::new(JuliaType::Int64)),
                Value::DataType(Box::new(JuliaType::Struct("MyStruct".to_string()))),
            ],
        });
        let result = extract_types_from_value(&val, &[]).unwrap();
        assert_eq!(
            result,
            vec![JuliaType::Int64, JuliaType::Struct("MyStruct".to_string())]
        );
    }

    /// extract_types_from_value: Array of DataType values returns all types (Issue #3273).
    /// This is the key regression test — methods(f, [Type1, Type2]) uses Vector syntax.
    #[test]
    fn test_extract_types_from_array() {
        let arr = make_any_array(vec![
            Value::DataType(Box::new(JuliaType::Int64)),
            Value::DataType(Box::new(JuliaType::Float64)),
        ]);
        let val = array_value(arr);
        let result = extract_types_from_value(&val, &[]).unwrap();
        assert_eq!(result, vec![JuliaType::Int64, JuliaType::Float64]);
    }

    #[test]
    fn test_extract_types_from_array_wrapper_issue_6649() {
        let mut memory = MemoryValue::undef_typed(&ArrayElementType::Any, 2);
        memory
            .set(1, Value::DataType(Box::new(JuliaType::Int64)))
            .unwrap();
        memory
            .set(2, Value::DataType(Box::new(JuliaType::Float64)))
            .unwrap();
        let storage = Value::MemoryRef(Box::new(MemoryRefValue::first(new_memory_ref(memory))));
        let size = Value::Tuple(TupleValue::new(vec![Value::I64(2)]));
        let wrapper =
            StructInstance::with_name(0, "Array{DataType,1}".to_string(), vec![storage, size]);
        let struct_heap = vec![wrapper];

        let result = extract_types_from_value(&Value::StructRef(0), &struct_heap).unwrap();
        assert_eq!(result, vec![JuliaType::Int64, JuliaType::Float64]);
    }

    /// extract_types_from_value: Single-element Array works correctly.
    #[test]
    fn test_extract_types_from_single_element_array() {
        let arr = make_any_array(vec![Value::DataType(Box::new(JuliaType::Bool))]);
        let val = array_value(arr);
        let result = extract_types_from_value(&val, &[]).unwrap();
        assert_eq!(result, vec![JuliaType::Bool]);
    }

    /// extract_types_from_value: Array with non-DataType element returns error.
    #[test]
    fn test_extract_types_from_array_non_datatype_error() {
        let arr = make_any_array(vec![Value::I64(42)]);
        let val = array_value(arr);
        let result = extract_types_from_value(&val, &[]);
        assert!(result.is_err(), "Array with non-DataType should error");
    }

    /// extract_types_from_value: unsupported Value returns error.
    #[test]
    fn test_extract_types_unsupported_value_error() {
        let val = Value::I64(42);
        let result = extract_types_from_value(&val, &[]);
        assert!(
            result.is_err(),
            "I64 value should not be extractable as types"
        );
    }

    /// parse_tuple_types: empty Tuple{} returns empty vec.
    #[test]
    fn test_parse_tuple_types_empty() {
        let result = parse_tuple_types("Tuple{}").unwrap();
        assert!(result.is_empty());
    }

    /// parse_type_name: known types map correctly.
    #[test]
    fn test_parse_type_name_known() {
        assert_eq!(parse_type_name("Int64"), JuliaType::Int64);
        assert_eq!(parse_type_name("Float64"), JuliaType::Float64);
        assert_eq!(parse_type_name("Bool"), JuliaType::Bool);
        assert_eq!(parse_type_name("String"), JuliaType::String);
        assert_eq!(parse_type_name("Any"), JuliaType::Any);
        assert_eq!(parse_type_name("Number"), JuliaType::Number);
    }

    /// parse_type_name: unknown types become Struct.
    #[test]
    fn test_parse_type_name_unknown() {
        assert_eq!(
            parse_type_name("MyCustomType"),
            JuliaType::Struct("MyCustomType".to_string())
        );
    }
}
