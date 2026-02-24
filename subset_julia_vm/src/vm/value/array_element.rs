//! Array element type definitions for homogeneous typed arrays.

use serde::{Deserialize, Serialize};

/// Element type for arrays
/// Note: Copy removed to allow TupleOf(Vec<ArrayElementType>)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ArrayElementType {
    // Floating point types
    F32,
    #[default]
    F64,
    // Complex types: Complex{T} stored as interleaved T
    // Julia: Complex{Float32}, Complex{Float64}
    ComplexF32,
    ComplexF64,
    // Signed integer types
    I8,
    I16,
    I32,
    I64,
    // Unsigned integer types
    U8,
    U16,
    U32,
    U64,
    // Other types
    Bool,
    String,
    Char,
    Struct,
    StructOf(usize),
    /// isbits struct with inline AoS storage
    /// (type_id, field_count) for get/set without struct_defs lookup
    StructInlineOf(usize, usize),
    Any,
    /// Homogeneous tuple array: stores field types for AoS layout
    /// Example: Tuple{Int64, Float64} -> TupleOf(vec![I64, F64])
    /// Storage: ArrayData::Any with interleaved fields [a1, b1, a2, b2, ...]
    TupleOf(Vec<ArrayElementType>),
}

impl ArrayElementType {
    /// Check if this is a complex type (Complex{T})
    pub fn is_complex(&self) -> bool {
        matches!(
            self,
            ArrayElementType::ComplexF32 | ArrayElementType::ComplexF64
        )
    }

    /// Get the underlying scalar type for complex types
    /// Returns None for non-complex types
    pub fn complex_scalar_type(&self) -> Option<ArrayElementType> {
        match self {
            ArrayElementType::ComplexF32 => Some(ArrayElementType::F32),
            ArrayElementType::ComplexF64 => Some(ArrayElementType::F64),
            _ => None,
        }
    }

    /// Create a complex type from a scalar type
    /// Returns None if the scalar type doesn't support complex
    pub fn as_complex(scalar: ArrayElementType) -> Option<ArrayElementType> {
        match scalar {
            ArrayElementType::F32 => Some(ArrayElementType::ComplexF32),
            ArrayElementType::F64 => Some(ArrayElementType::ComplexF64),
            _ => None, // Could extend to I64, I32 etc. if needed
        }
    }

    /// Check if this is a tuple array type (TupleOf)
    pub fn is_tuple(&self) -> bool {
        matches!(self, ArrayElementType::TupleOf(_))
    }

    /// Get tuple field types if this is a TupleOf
    pub fn tuple_field_types(&self) -> Option<&Vec<ArrayElementType>> {
        match self {
            ArrayElementType::TupleOf(types) => Some(types),
            _ => None,
        }
    }

    /// Get the arity (number of fields) for tuple arrays
    pub fn tuple_arity(&self) -> Option<usize> {
        match self {
            ArrayElementType::TupleOf(types) => Some(types.len()),
            _ => None,
        }
    }

    /// Check if this type is isbits (can be stored inline)
    pub fn is_isbits(&self) -> bool {
        match self {
            ArrayElementType::F32
            | ArrayElementType::F64
            | ArrayElementType::I8
            | ArrayElementType::I16
            | ArrayElementType::I32
            | ArrayElementType::I64
            | ArrayElementType::U8
            | ArrayElementType::U16
            | ArrayElementType::U32
            | ArrayElementType::U64
            | ArrayElementType::Bool
            | ArrayElementType::Char
            | ArrayElementType::ComplexF32
            | ArrayElementType::ComplexF64 => true,
            ArrayElementType::TupleOf(fields) => fields.iter().all(|f| f.is_isbits()),
            ArrayElementType::StructInlineOf(_, _) => true,
            _ => false,
        }
    }

    /// Check if this is an inline struct array type
    pub fn is_struct_inline(&self) -> bool {
        matches!(self, ArrayElementType::StructInlineOf(_, _))
    }

    /// Get struct inline info (type_id, field_count) if this is a StructInlineOf
    pub fn struct_inline_info(&self) -> Option<(usize, usize)> {
        match self {
            ArrayElementType::StructInlineOf(type_id, field_count) => {
                Some((*type_id, *field_count))
            }
            _ => None,
        }
    }

    /// Convert to ValueType
    pub fn to_value_type(&self) -> super::ValueType {
        match self {
            ArrayElementType::F32 => super::ValueType::F32,
            ArrayElementType::F64 => super::ValueType::F64,
            ArrayElementType::ComplexF32 => super::ValueType::Struct(0),
            ArrayElementType::ComplexF64 => super::ValueType::Struct(0),
            ArrayElementType::I8 => super::ValueType::I8,
            ArrayElementType::I16 => super::ValueType::I16,
            ArrayElementType::I32 => super::ValueType::I32,
            ArrayElementType::I64 => super::ValueType::I64,
            ArrayElementType::U8 => super::ValueType::U8,
            ArrayElementType::U16 => super::ValueType::U16,
            ArrayElementType::U32 => super::ValueType::U32,
            ArrayElementType::U64 => super::ValueType::U64,
            ArrayElementType::Bool => super::ValueType::Bool,
            ArrayElementType::String => super::ValueType::Str,
            ArrayElementType::Char => super::ValueType::Char,
            ArrayElementType::Struct => super::ValueType::Any,
            ArrayElementType::StructOf(id) => super::ValueType::Struct(*id),
            ArrayElementType::StructInlineOf(id, _) => super::ValueType::Struct(*id),
            ArrayElementType::Any => super::ValueType::Any,
            ArrayElementType::TupleOf(_) => super::ValueType::Tuple,
        }
    }

    /// Create from ValueType
    pub fn from_value_type(vt: &super::ValueType) -> Self {
        match vt {
            super::ValueType::F32 => ArrayElementType::F32,
            super::ValueType::F64 => ArrayElementType::F64,
            super::ValueType::I8 => ArrayElementType::I8,
            super::ValueType::I16 => ArrayElementType::I16,
            super::ValueType::I32 => ArrayElementType::I32,
            super::ValueType::I64 => ArrayElementType::I64,
            super::ValueType::U8 => ArrayElementType::U8,
            super::ValueType::U16 => ArrayElementType::U16,
            super::ValueType::U32 => ArrayElementType::U32,
            super::ValueType::U64 => ArrayElementType::U64,
            super::ValueType::Bool => ArrayElementType::Bool,
            super::ValueType::Str => ArrayElementType::String,
            super::ValueType::Char => ArrayElementType::Char,
            super::ValueType::Struct(id) => ArrayElementType::StructOf(*id),
            super::ValueType::Tuple => ArrayElementType::Any,
            _ => ArrayElementType::Any,
        }
    }
}

impl ArrayElementType {
    /// Get the Julia type name for this element type (for display purposes)
    /// E.g., I64 -> "Int64", F64 -> "Float64"
    pub fn julia_type_name(&self) -> String {
        match self {
            ArrayElementType::F32 => "Float32".to_string(),
            ArrayElementType::F64 => "Float64".to_string(),
            ArrayElementType::ComplexF32 => "Complex{Float32}".to_string(),
            ArrayElementType::ComplexF64 => "Complex{Float64}".to_string(),
            ArrayElementType::I8 => "Int8".to_string(),
            ArrayElementType::I16 => "Int16".to_string(),
            ArrayElementType::I32 => "Int32".to_string(),
            ArrayElementType::I64 => "Int64".to_string(),
            ArrayElementType::U8 => "UInt8".to_string(),
            ArrayElementType::U16 => "UInt16".to_string(),
            ArrayElementType::U32 => "UInt32".to_string(),
            ArrayElementType::U64 => "UInt64".to_string(),
            ArrayElementType::Bool => "Bool".to_string(),
            ArrayElementType::String => "String".to_string(),
            ArrayElementType::Char => "Char".to_string(),
            ArrayElementType::Struct => "Any".to_string(),
            ArrayElementType::StructOf(_) => "Any".to_string(), // Struct name would need lookup
            ArrayElementType::StructInlineOf(_, _) => "Any".to_string(),
            ArrayElementType::Any => "Any".to_string(),
            ArrayElementType::TupleOf(field_types) => {
                let type_names: Vec<String> =
                    field_types.iter().map(|t| t.julia_type_name()).collect();
                format!("Tuple{{{}}}", type_names.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_complex ────────────────────────────────────────────────────────────

    #[test]
    fn test_is_complex_for_complex_variants() {
        assert!(ArrayElementType::ComplexF32.is_complex());
        assert!(ArrayElementType::ComplexF64.is_complex());
    }

    #[test]
    fn test_is_complex_false_for_scalar_variants() {
        assert!(!ArrayElementType::F64.is_complex());
        assert!(!ArrayElementType::I64.is_complex());
        assert!(!ArrayElementType::Any.is_complex());
    }

    // ── complex_scalar_type ───────────────────────────────────────────────────

    #[test]
    fn test_complex_scalar_type_complex_f64_returns_f64() {
        assert_eq!(
            ArrayElementType::ComplexF64.complex_scalar_type(),
            Some(ArrayElementType::F64)
        );
    }

    #[test]
    fn test_complex_scalar_type_complex_f32_returns_f32() {
        assert_eq!(
            ArrayElementType::ComplexF32.complex_scalar_type(),
            Some(ArrayElementType::F32)
        );
    }

    #[test]
    fn test_complex_scalar_type_returns_none_for_non_complex() {
        assert_eq!(ArrayElementType::F64.complex_scalar_type(), None);
        assert_eq!(ArrayElementType::I64.complex_scalar_type(), None);
    }

    // ── as_complex ────────────────────────────────────────────────────────────

    #[test]
    fn test_as_complex_f64_returns_complex_f64() {
        assert_eq!(
            ArrayElementType::as_complex(ArrayElementType::F64),
            Some(ArrayElementType::ComplexF64)
        );
    }

    #[test]
    fn test_as_complex_f32_returns_complex_f32() {
        assert_eq!(
            ArrayElementType::as_complex(ArrayElementType::F32),
            Some(ArrayElementType::ComplexF32)
        );
    }

    #[test]
    fn test_as_complex_integer_returns_none() {
        assert_eq!(ArrayElementType::as_complex(ArrayElementType::I64), None);
    }

    // ── is_tuple / tuple_arity / tuple_field_types ────────────────────────────

    #[test]
    fn test_is_tuple_for_tuple_of() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        assert!(t.is_tuple());
    }

    #[test]
    fn test_is_tuple_false_for_non_tuple() {
        assert!(!ArrayElementType::F64.is_tuple());
        assert!(!ArrayElementType::Any.is_tuple());
    }

    #[test]
    fn test_tuple_arity_returns_field_count() {
        let t = ArrayElementType::TupleOf(vec![
            ArrayElementType::I64,
            ArrayElementType::F64,
            ArrayElementType::Bool,
        ]);
        assert_eq!(t.tuple_arity(), Some(3));
    }

    #[test]
    fn test_tuple_arity_none_for_non_tuple() {
        assert_eq!(ArrayElementType::F64.tuple_arity(), None);
    }

    #[test]
    fn test_tuple_field_types_returns_inner_vec() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        let fields = t.tuple_field_types().unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], ArrayElementType::I64);
        assert_eq!(fields[1], ArrayElementType::F64);
    }

    // ── is_isbits ─────────────────────────────────────────────────────────────

    #[test]
    fn test_is_isbits_for_primitives() {
        assert!(ArrayElementType::F64.is_isbits());
        assert!(ArrayElementType::I64.is_isbits());
        assert!(ArrayElementType::Bool.is_isbits());
        assert!(ArrayElementType::ComplexF64.is_isbits());
    }

    #[test]
    fn test_is_isbits_false_for_heap_types() {
        assert!(!ArrayElementType::String.is_isbits());
        assert!(!ArrayElementType::Any.is_isbits());
        assert!(!ArrayElementType::Struct.is_isbits());
    }

    #[test]
    fn test_is_isbits_for_tuple_of_primitives() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        assert!(t.is_isbits(), "TupleOf(I64, F64) should be isbits");
    }

    #[test]
    fn test_is_isbits_false_for_tuple_containing_non_isbits() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::String]);
        assert!(!t.is_isbits(), "TupleOf(I64, String) should NOT be isbits");
    }

    // ── julia_type_name ───────────────────────────────────────────────────────

    #[test]
    fn test_julia_type_name_primitives() {
        assert_eq!(ArrayElementType::F64.julia_type_name(), "Float64");
        assert_eq!(ArrayElementType::I64.julia_type_name(), "Int64");
        assert_eq!(ArrayElementType::Bool.julia_type_name(), "Bool");
        assert_eq!(ArrayElementType::U8.julia_type_name(), "UInt8");
    }

    #[test]
    fn test_julia_type_name_complex() {
        assert_eq!(ArrayElementType::ComplexF64.julia_type_name(), "Complex{Float64}");
        assert_eq!(ArrayElementType::ComplexF32.julia_type_name(), "Complex{Float32}");
    }

    #[test]
    fn test_julia_type_name_tuple_of() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        assert_eq!(t.julia_type_name(), "Tuple{Int64, Float64}");
    }

    #[test]
    fn test_julia_type_name_any_variants() {
        assert_eq!(ArrayElementType::Any.julia_type_name(), "Any");
        assert_eq!(ArrayElementType::Struct.julia_type_name(), "Any");
    }
}
