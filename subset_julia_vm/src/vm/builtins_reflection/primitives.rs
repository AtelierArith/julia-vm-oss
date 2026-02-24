use crate::types::JuliaType;

use super::super::error::VmError;
use super::super::value::{Value, ValueType};

/// Convert a ValueType to a JuliaType for use in fieldtypes.
pub(super) fn value_type_to_julia_type(
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
        ValueType::BigFloat => JuliaType::BigFloat,
        ValueType::Array | ValueType::ArrayOf(_) => JuliaType::Array,
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
        ValueType::Union(_) => JuliaType::Any,
        ValueType::Memory | ValueType::MemoryOf(_) => JuliaType::Any,
    }
}

/// Extract function name from a Value.
pub(super) fn extract_func_name(val: &Value) -> Result<String, VmError> {
    match val {
        Value::Function(fv) => Ok(fv.name.clone()),
        Value::Str(s) => Ok(s.clone()),
        Value::Symbol(sym) => Ok(sym.as_str().to_string()),
        _ => Err(VmError::TypeError(
            "Expected function, string, or symbol".into(),
        )),
    }
}

/// Extract types from a Value (typically a DataType representing Tuple{...}).
pub(super) fn extract_types_from_value(val: &Value) -> Result<Vec<JuliaType>, VmError> {
    match val {
        Value::DataType(JuliaType::TupleOf(types)) => Ok(types.clone()),
        Value::DataType(JuliaType::Struct(name)) if name.starts_with("Tuple{") => {
            parse_tuple_types(name)
        }
        Value::DataType(jt) => Ok(vec![jt.clone()]),
        Value::Tuple(t) => t
            .elements
            .iter()
            .map(|v| match v {
                Value::DataType(jt) => Ok(jt.clone()),
                _ => Err(VmError::TypeError("Expected type in tuple".into())),
            })
            .collect(),
        // methods(f, [Type1, Type2]) passes a Vector of types (Julia uses [...] syntax)
        Value::Array(arr) => {
            let elems = arr.borrow().to_value_vec();
            elems
                .into_iter()
                .map(|v| match v {
                    Value::DataType(jt) => Ok(jt),
                    _ => Err(VmError::TypeError("Expected type in array of types".into())),
                })
                .collect()
        }
        _ => Err(VmError::TypeError("Expected Tuple type".into())),
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

    let types: Vec<JuliaType> = inner
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(parse_type_name)
        .collect();

    Ok(types)
}

/// Parse a type name string into JuliaType.
fn parse_type_name(name: &str) -> JuliaType {
    match name {
        "Int8" => JuliaType::Int8,
        "Int16" => JuliaType::Int16,
        "Int32" => JuliaType::Int32,
        "Int64" => JuliaType::Int64,
        "Int128" => JuliaType::Int128,
        "UInt8" => JuliaType::UInt8,
        "UInt16" => JuliaType::UInt16,
        "UInt32" => JuliaType::UInt32,
        "UInt64" => JuliaType::UInt64,
        "UInt128" => JuliaType::UInt128,
        "Float16" => JuliaType::Float16,
        "Float32" => JuliaType::Float32,
        "Float64" => JuliaType::Float64,
        "Bool" => JuliaType::Bool,
        "String" => JuliaType::String,
        "Char" => JuliaType::Char,
        "Nothing" => JuliaType::Nothing,
        "Missing" => JuliaType::Missing,
        "Any" => JuliaType::Any,
        "Number" => JuliaType::Number,
        "Real" => JuliaType::Real,
        "Integer" => JuliaType::Integer,
        "AbstractFloat" => JuliaType::AbstractFloat,
        _ => JuliaType::Struct(name.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::{
        new_array_ref, ArrayData, ArrayValue, TupleValue, Value,
    };

    fn make_any_array(values: Vec<Value>) -> ArrayValue {
        let len = values.len();
        ArrayValue {
            data: ArrayData::Any(values),
            shape: vec![len],
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// extract_types_from_value: DataType(single type) returns vec of that type.
    #[test]
    fn test_extract_types_single_datatype() {
        let val = Value::DataType(JuliaType::Int64);
        let result = extract_types_from_value(&val).unwrap();
        assert_eq!(result, vec![JuliaType::Int64]);
    }

    /// extract_types_from_value: DataType(TupleOf) returns all types.
    #[test]
    fn test_extract_types_tuple_of() {
        let val = Value::DataType(JuliaType::TupleOf(vec![
            JuliaType::Int64,
            JuliaType::Float64,
        ]));
        let result = extract_types_from_value(&val).unwrap();
        assert_eq!(result, vec![JuliaType::Int64, JuliaType::Float64]);
    }

    /// extract_types_from_value: DataType(Struct("Tuple{Int64, Float64}")) parses correctly.
    #[test]
    fn test_extract_types_tuple_string() {
        let val = Value::DataType(JuliaType::Struct("Tuple{Int64, Float64}".to_string()));
        let result = extract_types_from_value(&val).unwrap();
        assert_eq!(result, vec![JuliaType::Int64, JuliaType::Float64]);
    }

    /// extract_types_from_value: Tuple of DataType values returns all types.
    #[test]
    fn test_extract_types_from_tuple() {
        let val = Value::Tuple(TupleValue {
            elements: vec![
                Value::DataType(JuliaType::Int64),
                Value::DataType(JuliaType::Struct("MyStruct".to_string())),
            ],
        });
        let result = extract_types_from_value(&val).unwrap();
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
            Value::DataType(JuliaType::Int64),
            Value::DataType(JuliaType::Float64),
        ]);
        let val = Value::Array(new_array_ref(arr));
        let result = extract_types_from_value(&val).unwrap();
        assert_eq!(result, vec![JuliaType::Int64, JuliaType::Float64]);
    }

    /// extract_types_from_value: Single-element Array works correctly.
    #[test]
    fn test_extract_types_from_single_element_array() {
        let arr = make_any_array(vec![Value::DataType(JuliaType::Bool)]);
        let val = Value::Array(new_array_ref(arr));
        let result = extract_types_from_value(&val).unwrap();
        assert_eq!(result, vec![JuliaType::Bool]);
    }

    /// extract_types_from_value: Array with non-DataType element returns error.
    #[test]
    fn test_extract_types_from_array_non_datatype_error() {
        let arr = make_any_array(vec![Value::I64(42)]);
        let val = Value::Array(new_array_ref(arr));
        let result = extract_types_from_value(&val);
        assert!(result.is_err(), "Array with non-DataType should error");
    }

    /// extract_types_from_value: unsupported Value returns error.
    #[test]
    fn test_extract_types_unsupported_value_error() {
        let val = Value::I64(42);
        let result = extract_types_from_value(&val);
        assert!(result.is_err(), "I64 value should not be extractable as types");
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
