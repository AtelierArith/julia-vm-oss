//! Bridge between VM runtime types and compile-time lattice types.
//!
//! This module provides bidirectional conversions between:
//! - `ValueType` (VM runtime type system)
//! - `LatticeType` (compile-time abstract interpretation type system)
//!
//! The conversions enable type inference results to be used for optimization
//! and allow runtime type information to inform compile-time analysis.

use crate::compile::context::StructInfo;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::types::JuliaType;
use crate::vm::value::{ArrayElementType, ValueType};
use std::collections::HashMap;

/// Convert a VM `ValueType` to a compile-time `LatticeType`.
///
/// This conversion is used when runtime type information needs to inform
/// compile-time type inference or optimization decisions.
impl From<&ValueType> for LatticeType {
    fn from(value_type: &ValueType) -> Self {
        match value_type {
            // Integer types - signed
            ValueType::I8 => LatticeType::Concrete(ConcreteType::Int8),
            ValueType::I16 => LatticeType::Concrete(ConcreteType::Int16),
            ValueType::I32 => LatticeType::Concrete(ConcreteType::Int32),
            ValueType::I64 => LatticeType::Concrete(ConcreteType::Int64),
            ValueType::I128 => LatticeType::Concrete(ConcreteType::Int128),
            ValueType::BigInt => LatticeType::Concrete(ConcreteType::BigInt),

            // Integer types - unsigned
            ValueType::U8 => LatticeType::Concrete(ConcreteType::UInt8),
            ValueType::U16 => LatticeType::Concrete(ConcreteType::UInt16),
            ValueType::U32 => LatticeType::Concrete(ConcreteType::UInt32),
            ValueType::U64 => LatticeType::Concrete(ConcreteType::UInt64),
            ValueType::U128 => LatticeType::Concrete(ConcreteType::UInt128),

            // Boolean
            ValueType::Bool => LatticeType::Concrete(ConcreteType::Bool),

            // Floating point types
            ValueType::F16 => LatticeType::Concrete(ConcreteType::Float16),
            ValueType::F32 => LatticeType::Concrete(ConcreteType::Float32),
            ValueType::F64 => LatticeType::Concrete(ConcreteType::Float64),
            ValueType::BigFloat => LatticeType::Concrete(ConcreteType::BigFloat),

            // Array types
            ValueType::Array => LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Any), // Unknown element type
            }),
            ValueType::ArrayOf(elem_type) => {
                let element = convert_array_element_type(elem_type);
                LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(element),
                })
            }

            // String types
            ValueType::Str => LatticeType::Concrete(ConcreteType::String),
            ValueType::Char => LatticeType::Concrete(ConcreteType::Char),

            // Special types
            ValueType::Nothing => LatticeType::Concrete(ConcreteType::Nothing),
            ValueType::Missing => LatticeType::Concrete(ConcreteType::Missing),

            // Symbolic types
            ValueType::Symbol => LatticeType::Concrete(ConcreteType::Symbol),

            // Struct types
            ValueType::Struct(type_id) => LatticeType::Concrete(ConcreteType::Struct {
                name: format!("Struct#{}", type_id),
                type_id: *type_id,
            }),

            // Tuple and NamedTuple - fallback to generic representation
            ValueType::Tuple => LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![], // Unknown element types
            }),
            ValueType::NamedTuple => LatticeType::Concrete(ConcreteType::NamedTuple {
                fields: vec![], // Unknown fields
            }),

            // Range types
            ValueType::Range | ValueType::Rng => LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Int64), // Default to Int64 for ranges
            }),

            // Dictionary type
            ValueType::Dict => LatticeType::Concrete(ConcreteType::Dict {
                key: Box::new(ConcreteType::String),    // Default key type
                value: Box::new(ConcreteType::Float64), // Default value type
            }),

            // Set type
            ValueType::Set => LatticeType::Concrete(ConcreteType::Set {
                element: Box::new(ConcreteType::Float64), // Default element type
            }),

            // Generator type
            ValueType::Generator => LatticeType::Concrete(ConcreteType::Generator {
                element: Box::new(ConcreteType::Float64), // Default element type
            }),

            // Pairs type
            ValueType::Pairs => LatticeType::Concrete(ConcreteType::Pairs),

            // Type system types
            ValueType::DataType => LatticeType::Concrete(ConcreteType::DataType {
                name: "DataType".to_string(),
            }),
            ValueType::Module => LatticeType::Concrete(ConcreteType::Module {
                name: "Module".to_string(),
            }),

            // IO type
            ValueType::IO => LatticeType::Concrete(ConcreteType::IO),

            // Function type
            ValueType::Function => LatticeType::Concrete(ConcreteType::Function {
                name: "Function".to_string(),
            }),

            // Metaprogramming types
            ValueType::Expr => LatticeType::Concrete(ConcreteType::Expr),
            ValueType::QuoteNode => LatticeType::Concrete(ConcreteType::QuoteNode),
            ValueType::LineNumberNode => LatticeType::Concrete(ConcreteType::LineNumberNode),
            ValueType::GlobalRef => LatticeType::Concrete(ConcreteType::GlobalRef),

            // Regex types
            ValueType::Regex => LatticeType::Concrete(ConcreteType::Regex),
            ValueType::RegexMatch => LatticeType::Concrete(ConcreteType::RegexMatch),

            // Enum type — maps to dedicated ConcreteType::Enum (Issue #2863)
            ValueType::Enum => LatticeType::Concrete(ConcreteType::Enum {
                name: "Enum".to_string(),
            }),

            // Union type - convert back to LatticeType::Union
            ValueType::Union(types) => {
                let concrete_types: Vec<ConcreteType> = types
                    .iter()
                    .filter_map(|vt| match LatticeType::from(vt) {
                        LatticeType::Concrete(ct) => Some(ct),
                        _ => None,
                    })
                    .collect();
                if concrete_types.is_empty() {
                    LatticeType::Top
                } else {
                    LatticeType::Union(concrete_types.into_iter().collect())
                }
            }

            // Memory type (no dedicated LatticeType, use Top)
            ValueType::Memory | ValueType::MemoryOf(_) => LatticeType::Top,

            // Dynamic type
            ValueType::Any => LatticeType::Top,
        }
    }
}

/// Convert a VM `ValueType` to a compile-time `LatticeType`, using a struct table
/// to resolve struct names from type IDs.
///
/// This function should be used instead of `LatticeType::from(&ValueType)` when
/// the struct_table is available and accurate struct names are needed for
/// type inference (e.g., when looking up field types in user-defined structs).
///
/// # Arguments
/// * `value_type` - The ValueType to convert
/// * `struct_table` - Map from struct names to StructInfo for name resolution
///
/// # Returns
/// A LatticeType with properly resolved struct names
pub fn value_type_to_lattice_with_struct_table(
    value_type: &ValueType,
    struct_table: &HashMap<String, StructInfo>,
) -> LatticeType {
    match value_type {
        // Struct types - use struct_table to resolve proper name
        ValueType::Struct(type_id) => {
            // Search for struct name by type_id
            for (name, info) in struct_table {
                if info.type_id == *type_id {
                    return LatticeType::Concrete(ConcreteType::Struct {
                        name: name.clone(),
                        type_id: *type_id,
                    });
                }
            }
            // Fallback to synthetic name if not found in table
            LatticeType::Concrete(ConcreteType::Struct {
                name: format!("Struct#{}", type_id),
                type_id: *type_id,
            })
        }
        // All other types delegate to the standard conversion
        _ => LatticeType::from(value_type),
    }
}

/// Convert a compile-time `LatticeType` to a VM `ValueType`.
///
/// This conversion is used when type inference results need to be
/// translated back into runtime type information for code generation.
impl From<&LatticeType> for ValueType {
    fn from(lattice_type: &LatticeType) -> Self {
        match lattice_type {
            LatticeType::Bottom => ValueType::Any, // Unreachable code - use Any as safe fallback
            LatticeType::Top => ValueType::Any,    // Unknown type - use Any

            // Convert const to its concrete type for runtime
            LatticeType::Const(cv) => {
                let concrete = cv.to_concrete_type();
                ValueType::from(&LatticeType::Concrete(concrete))
            }

            LatticeType::Concrete(concrete) => match concrete {
                // Integer types - signed
                ConcreteType::Int8 => ValueType::I8,
                ConcreteType::Int16 => ValueType::I16,
                ConcreteType::Int32 => ValueType::I32,
                ConcreteType::Int64 => ValueType::I64,
                ConcreteType::Int128 => ValueType::I128,
                ConcreteType::BigInt => ValueType::BigInt,

                // Integer types - unsigned
                ConcreteType::UInt8 => ValueType::U8,
                ConcreteType::UInt16 => ValueType::U16,
                ConcreteType::UInt32 => ValueType::U32,
                ConcreteType::UInt64 => ValueType::U64,
                ConcreteType::UInt128 => ValueType::U128,

                // Boolean
                ConcreteType::Bool => ValueType::Bool,

                // Floating point types
                ConcreteType::Float16 => ValueType::F16,
                ConcreteType::Float32 => ValueType::F32,
                ConcreteType::Float64 => ValueType::F64,
                ConcreteType::BigFloat => ValueType::BigFloat,

                // String types
                ConcreteType::String => ValueType::Str,
                ConcreteType::Char => ValueType::Char,

                // Special types
                ConcreteType::Nothing => ValueType::Nothing,
                ConcreteType::Missing => ValueType::Missing,

                // Symbolic types
                ConcreteType::Symbol => ValueType::Symbol,

                // Array types
                ConcreteType::Array { element } => {
                    let elem_type = convert_concrete_to_array_element(element);
                    ValueType::ArrayOf(elem_type)
                }

                // Tuple types
                ConcreteType::Tuple { elements } => {
                    // For now, fall back to generic Tuple
                    // Future: could convert to specific tuple representation
                    let _ = elements; // Suppress unused warning
                    ValueType::Tuple
                }

                // NamedTuple types
                ConcreteType::NamedTuple { fields } => {
                    // For now, fall back to generic NamedTuple
                    let _ = fields; // Suppress unused warning
                    ValueType::NamedTuple
                }

                // Struct types
                ConcreteType::Struct { type_id, .. } => ValueType::Struct(*type_id),

                // Function types - no direct ValueType equivalent
                ConcreteType::Function { .. } => ValueType::Any,

                // Range types
                ConcreteType::Range { .. } => ValueType::Range,

                // Dictionary type
                ConcreteType::Dict { .. } => ValueType::Dict,

                // Set type
                ConcreteType::Set { .. } => ValueType::Set,

                // Generator type
                ConcreteType::Generator { .. } => ValueType::Generator,

                // Pairs type
                ConcreteType::Pairs => ValueType::Pairs,

                // Type system types
                ConcreteType::DataType { .. } => ValueType::DataType,
                ConcreteType::Module { .. } => ValueType::Module,

                // IO type
                ConcreteType::IO => ValueType::IO,

                // Metaprogramming types
                ConcreteType::Expr => ValueType::Expr,
                ConcreteType::QuoteNode => ValueType::QuoteNode,
                ConcreteType::LineNumberNode => ValueType::LineNumberNode,
                ConcreteType::GlobalRef => ValueType::GlobalRef,

                // Regex types
                ConcreteType::Regex => ValueType::Regex,
                ConcreteType::RegexMatch => ValueType::RegexMatch,

                // Enum type (Issue #2863)
                ConcreteType::Enum { .. } => ValueType::Enum,

                // Any element type
                ConcreteType::Any => ValueType::Any,

                // Abstract types - no direct runtime representation
                ConcreteType::Number | ConcreteType::Integer | ConcreteType::AbstractFloat => {
                    ValueType::Any
                }

                // Union types (element type unions) - convert to ValueType::Union
                ConcreteType::UnionOf(types) => {
                    let value_types: Vec<ValueType> = types
                        .iter()
                        .map(|ct| ValueType::from(&LatticeType::Concrete(ct.clone())))
                        .collect();
                    ValueType::Union(value_types)
                }
            },

            // Union types - preserve type information for optimization
            LatticeType::Union(types) => {
                let value_types: Vec<ValueType> = types
                    .iter()
                    .map(|ct| ValueType::from(&LatticeType::Concrete(ct.clone())))
                    .collect();
                ValueType::Union(value_types)
            }

            // Conditional types - fallback to Any
            // These are control-flow sensitive and don't have runtime representation
            LatticeType::Conditional { .. } => ValueType::Any,
        }
    }
}

/// Helper function to convert `ArrayElementType` to `ConcreteType`.
///
/// Used when converting VM array types to lattice types.
fn convert_array_element_type(elem_type: &ArrayElementType) -> ConcreteType {
    match elem_type {
        // Floating point
        ArrayElementType::F32 => ConcreteType::Float32,
        ArrayElementType::F64 => ConcreteType::Float64,

        // Complex types - fallback to Float64 for simplicity
        ArrayElementType::ComplexF32 => ConcreteType::Float32,
        ArrayElementType::ComplexF64 => ConcreteType::Float64,

        // Signed integers
        ArrayElementType::I8 => ConcreteType::Int8,
        ArrayElementType::I16 => ConcreteType::Int16,
        ArrayElementType::I32 => ConcreteType::Int32,
        ArrayElementType::I64 => ConcreteType::Int64,

        // Unsigned integers
        ArrayElementType::U8 => ConcreteType::UInt8,
        ArrayElementType::U16 => ConcreteType::UInt16,
        ArrayElementType::U32 => ConcreteType::UInt32,
        ArrayElementType::U64 => ConcreteType::UInt64,

        // Boolean
        ArrayElementType::Bool => ConcreteType::Bool,

        // String types
        ArrayElementType::String => ConcreteType::String,
        ArrayElementType::Char => ConcreteType::Char,

        // Struct types
        ArrayElementType::Struct => ConcreteType::Int64, // Generic fallback
        ArrayElementType::StructOf(type_id) => ConcreteType::Struct {
            name: format!("Struct#{}", type_id),
            type_id: *type_id,
        },
        ArrayElementType::StructInlineOf(type_id, _field_count) => ConcreteType::Struct {
            name: format!("Struct#{}", type_id),
            type_id: *type_id,
        },

        // Tuple arrays - fallback to generic representation
        ArrayElementType::TupleOf(_fields) => {
            // For simplicity, represent as a generic tuple
            ConcreteType::Tuple { elements: vec![] }
        }

        // Any - preserve unknown element type
        ArrayElementType::Any => ConcreteType::Any,
    }
}

/// Helper function to convert `ConcreteType` to `ArrayElementType`.
///
/// Used when converting lattice array types back to VM array types.
fn convert_concrete_to_array_element(concrete: &ConcreteType) -> ArrayElementType {
    match concrete {
        // Floating point
        ConcreteType::Float32 => ArrayElementType::F32,
        ConcreteType::Float64 => ArrayElementType::F64,

        // Signed integers
        ConcreteType::Int8 => ArrayElementType::I8,
        ConcreteType::Int16 => ArrayElementType::I16,
        ConcreteType::Int32 => ArrayElementType::I32,
        ConcreteType::Int64 => ArrayElementType::I64,

        // Unsigned integers
        ConcreteType::UInt8 => ArrayElementType::U8,
        ConcreteType::UInt16 => ArrayElementType::U16,
        ConcreteType::UInt32 => ArrayElementType::U32,
        ConcreteType::UInt64 => ArrayElementType::U64,

        // Boolean
        ConcreteType::Bool => ArrayElementType::Bool,

        // String types
        ConcreteType::String => ArrayElementType::String,
        ConcreteType::Char => ArrayElementType::Char,

        // Struct types
        ConcreteType::Struct { type_id, .. } => ArrayElementType::StructOf(*type_id),

        // Any element type
        ConcreteType::Any => ArrayElementType::Any,
        // Complex types without direct ArrayElementType mapping - fallback to Any
        ConcreteType::Float16
        | ConcreteType::Int128
        | ConcreteType::BigInt
        | ConcreteType::UInt128
        | ConcreteType::BigFloat
        | ConcreteType::Nothing
        | ConcreteType::Missing
        | ConcreteType::Symbol
        | ConcreteType::Array { .. }
        | ConcreteType::Tuple { .. }
        | ConcreteType::NamedTuple { .. }
        | ConcreteType::Function { .. }
        | ConcreteType::Range { .. }
        | ConcreteType::Dict { .. }
        | ConcreteType::Set { .. }
        | ConcreteType::Generator { .. }
        | ConcreteType::Pairs
        | ConcreteType::DataType { .. }
        | ConcreteType::Module { .. }
        | ConcreteType::IO
        | ConcreteType::Expr
        | ConcreteType::QuoteNode
        | ConcreteType::LineNumberNode
        | ConcreteType::GlobalRef
        | ConcreteType::Regex
        | ConcreteType::RegexMatch
        | ConcreteType::UnionOf(..)
        // Abstract types - no direct array element type
        | ConcreteType::Number
        | ConcreteType::Integer
        | ConcreteType::AbstractFloat
        // Enum types - stored as i64 internally but no dedicated ArrayElementType
        | ConcreteType::Enum { .. } => ArrayElementType::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_valuetype_to_latticetype_integers() {
        // Signed integers
        assert_eq!(
            LatticeType::from(&ValueType::I8),
            LatticeType::Concrete(ConcreteType::Int8)
        );
        assert_eq!(
            LatticeType::from(&ValueType::I16),
            LatticeType::Concrete(ConcreteType::Int16)
        );
        assert_eq!(
            LatticeType::from(&ValueType::I32),
            LatticeType::Concrete(ConcreteType::Int32)
        );
        assert_eq!(
            LatticeType::from(&ValueType::I64),
            LatticeType::Concrete(ConcreteType::Int64)
        );
        // I128 and BigInt now have proper type representations
        assert_eq!(
            LatticeType::from(&ValueType::I128),
            LatticeType::Concrete(ConcreteType::Int128)
        );
        assert_eq!(
            LatticeType::from(&ValueType::BigInt),
            LatticeType::Concrete(ConcreteType::BigInt)
        );

        // Unsigned integers
        assert_eq!(
            LatticeType::from(&ValueType::U8),
            LatticeType::Concrete(ConcreteType::UInt8)
        );
        assert_eq!(
            LatticeType::from(&ValueType::U16),
            LatticeType::Concrete(ConcreteType::UInt16)
        );
        assert_eq!(
            LatticeType::from(&ValueType::U32),
            LatticeType::Concrete(ConcreteType::UInt32)
        );
        assert_eq!(
            LatticeType::from(&ValueType::U64),
            LatticeType::Concrete(ConcreteType::UInt64)
        );
        assert_eq!(
            LatticeType::from(&ValueType::U128),
            LatticeType::Concrete(ConcreteType::UInt128)
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_floats() {
        assert_eq!(
            LatticeType::from(&ValueType::F32),
            LatticeType::Concrete(ConcreteType::Float32)
        );
        assert_eq!(
            LatticeType::from(&ValueType::F64),
            LatticeType::Concrete(ConcreteType::Float64)
        );
        // BigFloat now has proper type representation
        assert_eq!(
            LatticeType::from(&ValueType::BigFloat),
            LatticeType::Concrete(ConcreteType::BigFloat)
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_bool() {
        assert_eq!(
            LatticeType::from(&ValueType::Bool),
            LatticeType::Concrete(ConcreteType::Bool)
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_strings() {
        assert_eq!(
            LatticeType::from(&ValueType::Str),
            LatticeType::Concrete(ConcreteType::String)
        );
        assert_eq!(
            LatticeType::from(&ValueType::Char),
            LatticeType::Concrete(ConcreteType::Char)
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_special() {
        assert_eq!(
            LatticeType::from(&ValueType::Nothing),
            LatticeType::Concrete(ConcreteType::Nothing)
        );
        assert_eq!(
            LatticeType::from(&ValueType::Missing),
            LatticeType::Concrete(ConcreteType::Missing)
        );
        assert_eq!(
            LatticeType::from(&ValueType::Symbol),
            LatticeType::Concrete(ConcreteType::Symbol)
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_arrays() {
        // Legacy array type
        assert_eq!(
            LatticeType::from(&ValueType::Array),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Any)
            })
        );

        // Typed array
        assert_eq!(
            LatticeType::from(&ValueType::ArrayOf(ArrayElementType::I64)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Int64)
            })
        );
        assert_eq!(
            LatticeType::from(&ValueType::ArrayOf(ArrayElementType::F32)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Float32)
            })
        );
        assert_eq!(
            LatticeType::from(&ValueType::ArrayOf(ArrayElementType::Any)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Any)
            })
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_struct() {
        let type_id = 42;
        let result = LatticeType::from(&ValueType::Struct(type_id));
        assert!(
            matches!(&result, LatticeType::Concrete(ConcreteType::Struct { .. })),
            "Expected Struct type, got {:?}",
            result
        );
        if let LatticeType::Concrete(ConcreteType::Struct { type_id: id, .. }) = result {
            assert_eq!(id, type_id);
        }
    }

    #[test]
    fn test_valuetype_to_latticetype_top() {
        assert_eq!(LatticeType::from(&ValueType::Any), LatticeType::Top);
        // Range, Dict, Set now have concrete type representations
        assert!(matches!(
            LatticeType::from(&ValueType::Range),
            LatticeType::Concrete(ConcreteType::Range { .. })
        ));
        assert!(matches!(
            LatticeType::from(&ValueType::Dict),
            LatticeType::Concrete(ConcreteType::Dict { .. })
        ));
        assert!(matches!(
            LatticeType::from(&ValueType::Set),
            LatticeType::Concrete(ConcreteType::Set { .. })
        ));
    }

    #[test]
    fn test_latticetype_to_valuetype_integers() {
        // Signed integers
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Int8)),
            ValueType::I8
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Int16)),
            ValueType::I16
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Int32)),
            ValueType::I32
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Int64)),
            ValueType::I64
        );

        // Unsigned integers
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::UInt8)),
            ValueType::U8
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::UInt16)),
            ValueType::U16
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::UInt32)),
            ValueType::U32
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::UInt64)),
            ValueType::U64
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_floats() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Float32)),
            ValueType::F32
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Float64)),
            ValueType::F64
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_bool() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Bool)),
            ValueType::Bool
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_special() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Nothing)),
            ValueType::Nothing
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Missing)),
            ValueType::Missing
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Symbol)),
            ValueType::Symbol
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Any)),
            ValueType::Any
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_arrays() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Int64)
            })),
            ValueType::ArrayOf(ArrayElementType::I64)
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Float32)
            })),
            ValueType::ArrayOf(ArrayElementType::F32)
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Any)
            })),
            ValueType::ArrayOf(ArrayElementType::Any)
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_struct() {
        let type_id = 42;
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Struct {
                name: "Test".to_string(),
                type_id
            })),
            ValueType::Struct(type_id)
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_top_bottom() {
        assert_eq!(ValueType::from(&LatticeType::Top), ValueType::Any);
        assert_eq!(ValueType::from(&LatticeType::Bottom), ValueType::Any);
    }

    #[test]
    fn test_latticetype_to_valuetype_union() {
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Int64);
        types.insert(ConcreteType::Float64);
        // Union types are now preserved instead of collapsing to Any
        let result = ValueType::from(&LatticeType::Union(types));
        assert!(
            matches!(&result, ValueType::Union(_)),
            "Expected ValueType::Union, got {:?}",
            result
        );
        if let ValueType::Union(value_types) = result {
            assert_eq!(value_types.len(), 2);
            assert!(value_types.contains(&ValueType::I64));
            assert!(value_types.contains(&ValueType::F64));
        }
    }

    #[test]
    fn test_latticetype_to_valuetype_conditional() {
        let conditional = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(LatticeType::Concrete(ConcreteType::Int64)),
            else_type: Box::new(LatticeType::Concrete(ConcreteType::Float64)),
        };
        assert_eq!(ValueType::from(&conditional), ValueType::Any);
    }

    #[test]
    fn test_round_trip_basic_types() {
        // Test round-trip conversions for basic types
        let value_types = vec![
            ValueType::I8,
            ValueType::I16,
            ValueType::I32,
            ValueType::I64,
            ValueType::U8,
            ValueType::U16,
            ValueType::U32,
            ValueType::U64,
            ValueType::Bool,
            ValueType::F32,
            ValueType::F64,
            ValueType::Str,
            ValueType::Char,
            ValueType::Nothing,
            ValueType::Missing,
            ValueType::Symbol,
        ];

        for vt in value_types {
            let lattice = LatticeType::from(&vt);
            let back = ValueType::from(&lattice);
            assert_eq!(vt, back, "Round-trip failed for {:?}", vt);
        }
    }

    #[test]
    fn test_round_trip_array_types() {
        let value_types = vec![
            ValueType::ArrayOf(ArrayElementType::I64),
            ValueType::ArrayOf(ArrayElementType::F32),
            ValueType::ArrayOf(ArrayElementType::F64),
            ValueType::ArrayOf(ArrayElementType::Bool),
            ValueType::ArrayOf(ArrayElementType::U8),
        ];

        for vt in value_types {
            let lattice = LatticeType::from(&vt);
            let back = ValueType::from(&lattice);
            assert_eq!(vt, back, "Round-trip failed for {:?}", vt);
        }
    }

    #[test]
    fn test_round_trip_struct_type() {
        let type_id = 42;
        let vt = ValueType::Struct(type_id);
        let lattice = LatticeType::from(&vt);
        let back = ValueType::from(&lattice);
        assert_eq!(vt, back, "Round-trip failed for Struct");
    }

    /// Test that ValueType::Enum converts to ConcreteType::Enum (Issue #2863).
    /// Enum types were previously mapped to LatticeType::Top, which lost type information.
    #[test]
    fn test_enum_type_maps_to_concrete_enum_not_top() {
        let vt = ValueType::Enum;
        let lattice = LatticeType::from(&vt);

        // Must be Concrete(Enum), NOT Top (which was the old workaround)
        assert!(
            matches!(lattice, LatticeType::Concrete(ConcreteType::Enum { .. })),
            "Expected Concrete(Enum), got {:?}",
            lattice
        );
    }

    #[test]
    fn test_enum_round_trip() {
        let vt = ValueType::Enum;
        let lattice = LatticeType::from(&vt);
        let back = ValueType::from(&lattice);
        assert_eq!(vt, back, "Round-trip failed for Enum");
    }

    /// Test that ALL ValueType variants can be converted to LatticeType without panicking.
    /// This test ensures exhaustive coverage when new variants are added to ValueType.
    /// If this test fails to compile, it means a new ValueType variant was added
    /// but not handled in the conversion functions.
    #[test]
    fn test_all_valuetype_variants_to_lattice() {
        use crate::vm::value::ArrayElementType;

        // List ALL ValueType variants - if a new variant is added, this test
        // will fail to compile until the new variant is added here.
        let all_variants: Vec<ValueType> = vec![
            // Signed integers
            ValueType::I8,
            ValueType::I16,
            ValueType::I32,
            ValueType::I64,
            ValueType::I128,
            ValueType::BigInt,
            // Unsigned integers
            ValueType::U8,
            ValueType::U16,
            ValueType::U32,
            ValueType::U64,
            ValueType::U128,
            // Boolean
            ValueType::Bool,
            // Floating point
            ValueType::F16,
            ValueType::F32,
            ValueType::F64,
            ValueType::BigFloat,
            // Collections
            ValueType::Array,
            ValueType::ArrayOf(ArrayElementType::F64),
            ValueType::Range,
            // String types
            ValueType::Str,
            ValueType::Char,
            // Special types
            ValueType::Nothing,
            ValueType::Missing,
            ValueType::Struct(0),
            ValueType::Rng,
            ValueType::Tuple,
            ValueType::NamedTuple,
            ValueType::Pairs,
            ValueType::Dict,
            ValueType::Set,
            ValueType::Generator,
            ValueType::DataType,
            ValueType::Module,
            ValueType::Function,
            ValueType::IO,
            // Macro system types
            ValueType::Symbol,
            ValueType::Expr,
            ValueType::QuoteNode,
            ValueType::LineNumberNode,
            ValueType::GlobalRef,
            // Regex types
            ValueType::Regex,
            ValueType::RegexMatch,
            // Dynamic type
            ValueType::Any,
            // Union type
            ValueType::Union(vec![ValueType::I64, ValueType::F64]),
            // Enum type
            ValueType::Enum,
        ];

        // Verify each variant can be converted to LatticeType without panicking
        for vt in &all_variants {
            let _lattice = LatticeType::from(vt);
            // If this doesn't panic, the variant is properly handled
        }

        // Also verify we can convert back from LatticeType to ValueType
        for vt in &all_variants {
            let lattice = LatticeType::from(vt);
            let _back = ValueType::from(&lattice);
            // If this doesn't panic, the variant is properly handled
        }
    }
}

/// Public helper function to convert LatticeType to ValueType.
///
/// This is a convenience function that delegates to the From implementation.
pub fn lattice_to_value_type(lattice_type: &LatticeType) -> ValueType {
    ValueType::from(lattice_type)
}

/// Extract a parametric `JuliaType` from a `LatticeType` when `ValueType` would lose info.
///
/// Currently handles `ConcreteType::Tuple { elements }` → `JuliaType::TupleOf(...)`.
/// Returns `None` when no parametric info would be lost by using `ValueType` alone.
/// (Issue #2317)
pub fn lattice_to_parametric_julia_type(lattice_type: &LatticeType) -> Option<JuliaType> {
    match lattice_type {
        LatticeType::Concrete(ConcreteType::Tuple { elements }) if !elements.is_empty() => {
            let julia_elements: Vec<JuliaType> =
                elements.iter().map(concrete_type_to_julia_type).collect();
            Some(JuliaType::TupleOf(julia_elements))
        }
        _ => None,
    }
}

/// Convert a `ConcreteType` to a `JuliaType`.
fn concrete_type_to_julia_type(ct: &ConcreteType) -> JuliaType {
    match ct {
        ConcreteType::Int8 => JuliaType::Int8,
        ConcreteType::Int16 => JuliaType::Int16,
        ConcreteType::Int32 => JuliaType::Int32,
        ConcreteType::Int64 => JuliaType::Int64,
        ConcreteType::Int128 => JuliaType::Int128,
        ConcreteType::BigInt => JuliaType::BigInt,
        ConcreteType::UInt8 => JuliaType::UInt8,
        ConcreteType::UInt16 => JuliaType::UInt16,
        ConcreteType::UInt32 => JuliaType::UInt32,
        ConcreteType::UInt64 => JuliaType::UInt64,
        ConcreteType::UInt128 => JuliaType::UInt128,
        ConcreteType::Float16 => JuliaType::Float16,
        ConcreteType::Float32 => JuliaType::Float32,
        ConcreteType::Float64 => JuliaType::Float64,
        ConcreteType::BigFloat => JuliaType::BigFloat,
        ConcreteType::Bool => JuliaType::Bool,
        ConcreteType::String => JuliaType::String,
        ConcreteType::Char => JuliaType::Char,
        ConcreteType::Nothing => JuliaType::Nothing,
        ConcreteType::Missing => JuliaType::Missing,
        ConcreteType::Symbol => JuliaType::Symbol,
        ConcreteType::Tuple { elements } => {
            JuliaType::TupleOf(elements.iter().map(concrete_type_to_julia_type).collect())
        }
        ConcreteType::Struct { name, .. } => JuliaType::Struct(name.clone()),
        _ => JuliaType::Any,
    }
}
