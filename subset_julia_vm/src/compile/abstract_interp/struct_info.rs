//! Struct type information for abstract interpretation.
//!
//! This module provides StructTypeInfo, which tracks field types for struct definitions
//! during type inference.

use crate::compile::context::StructInfo;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::vm::ValueType;
use std::collections::HashMap;

/// Struct type information with field types as LatticeType.
///
/// This is a type-inference-friendly version of StructInfo that uses
/// LatticeType instead of ValueType for field types.
#[derive(Debug, Clone)]
pub struct StructTypeInfo {
    pub type_id: usize,
    pub is_mutable: bool,
    /// Map from field name to field type
    pub fields: HashMap<String, LatticeType>,
    pub has_inner_constructor: bool,
}

impl StructTypeInfo {
    /// Creates a new StructTypeInfo with the given fields.
    pub fn new(
        type_id: usize,
        is_mutable: bool,
        fields: HashMap<String, LatticeType>,
        has_inner_constructor: bool,
    ) -> Self {
        Self {
            type_id,
            is_mutable,
            fields,
            has_inner_constructor,
        }
    }

    /// Gets the type of a field by name.
    ///
    /// Returns Some(LatticeType) if the field exists, None otherwise.
    pub fn get_field_type(&self, field_name: &str) -> Option<&LatticeType> {
        self.fields.get(field_name)
    }

    /// Checks if a field exists in this struct.
    pub fn has_field(&self, field_name: &str) -> bool {
        self.fields.contains_key(field_name)
    }
}

/// Converts a StructInfo to StructTypeInfo by converting ValueType to LatticeType.
///
/// Note: This conversion does NOT use struct_table, so struct field types
/// that are themselves structs will fall back to `Top`. Use
/// `StructTypeInfo::from_with_struct_table` when struct_table is available.
impl From<&StructInfo> for StructTypeInfo {
    fn from(struct_info: &StructInfo) -> Self {
        let fields = struct_info
            .fields
            .iter()
            .map(|(name, value_type)| {
                let lattice_type = value_type_to_lattice(value_type);
                (name.clone(), lattice_type)
            })
            .collect();

        Self {
            type_id: struct_info.type_id,
            is_mutable: struct_info.is_mutable,
            fields,
            has_inner_constructor: struct_info.has_inner_constructor,
        }
    }
}

impl StructTypeInfo {
    /// Converts a StructInfo to StructTypeInfo, using the struct_table to resolve
    /// struct names from type IDs in field types.
    ///
    /// This should be preferred over `From<&StructInfo>` when struct_table is available,
    /// as it allows proper resolution of struct field types that are themselves structs.
    ///
    /// # Arguments
    /// * `struct_info` - The StructInfo to convert
    /// * `struct_table` - Map from struct names to StructInfo for name resolution
    pub fn from_with_struct_table(
        struct_info: &StructInfo,
        struct_table: &HashMap<String, StructInfo>,
    ) -> Self {
        let fields = struct_info
            .fields
            .iter()
            .map(|(name, value_type)| {
                let lattice_type = value_type_to_lattice_with_table(value_type, Some(struct_table));
                (name.clone(), lattice_type)
            })
            .collect();

        Self {
            type_id: struct_info.type_id,
            is_mutable: struct_info.is_mutable,
            fields,
            has_inner_constructor: struct_info.has_inner_constructor,
        }
    }
}

/// Converts a ValueType to a LatticeType.
///
/// This is used when converting from StructInfo (which uses ValueType)
/// to StructTypeInfo (which uses LatticeType).
fn value_type_to_lattice(value_type: &ValueType) -> LatticeType {
    value_type_to_lattice_with_table(value_type, None)
}

/// Converts a ValueType to a LatticeType, using the struct_table to resolve struct names.
///
/// When `struct_table` is provided, this function can convert `ValueType::Struct(type_id)`
/// to a proper `ConcreteType::Struct { name, type_id }` by looking up the struct name.
///
/// # Arguments
/// * `value_type` - The ValueType to convert
/// * `struct_table` - Optional struct table for resolving struct type_ids to names
pub fn value_type_to_lattice_with_table(
    value_type: &ValueType,
    struct_table: Option<&HashMap<String, StructInfo>>,
) -> LatticeType {
    match value_type {
        // Integer types - preserve precision
        ValueType::I8 => LatticeType::Concrete(ConcreteType::Int8),
        ValueType::I16 => LatticeType::Concrete(ConcreteType::Int16),
        ValueType::I32 => LatticeType::Concrete(ConcreteType::Int32),
        ValueType::I64 => LatticeType::Concrete(ConcreteType::Int64),
        ValueType::I128 => LatticeType::Concrete(ConcreteType::Int128),
        ValueType::U8 => LatticeType::Concrete(ConcreteType::UInt8),
        ValueType::U16 => LatticeType::Concrete(ConcreteType::UInt16),
        ValueType::U32 => LatticeType::Concrete(ConcreteType::UInt32),
        ValueType::U64 => LatticeType::Concrete(ConcreteType::UInt64),
        ValueType::U128 => LatticeType::Concrete(ConcreteType::UInt128),
        ValueType::BigInt => LatticeType::Concrete(ConcreteType::BigInt),

        // Float types
        ValueType::F32 => LatticeType::Concrete(ConcreteType::Float32),
        ValueType::F64 => LatticeType::Concrete(ConcreteType::Float64),
        ValueType::BigFloat => LatticeType::Concrete(ConcreteType::BigFloat),

        // Boolean
        ValueType::Bool => LatticeType::Concrete(ConcreteType::Bool),

        // String types
        ValueType::Str => LatticeType::Concrete(ConcreteType::String),
        ValueType::Char => LatticeType::Concrete(ConcreteType::Char),

        // Special types
        ValueType::Nothing => LatticeType::Concrete(ConcreteType::Nothing),
        ValueType::Symbol => LatticeType::Concrete(ConcreteType::Symbol),

        // Array types
        ValueType::Array => LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Any), // Unknown element type
        }),
        ValueType::ArrayOf(elem_type) => {
            // Extract element type from ArrayElementType
            use crate::vm::ArrayElementType;
            let concrete_elem = match elem_type {
                ArrayElementType::I64 => ConcreteType::Int64,
                ArrayElementType::F64 => ConcreteType::Float64,
                ArrayElementType::Bool => ConcreteType::Bool,
                ArrayElementType::String => ConcreteType::String,
                ArrayElementType::Char => ConcreteType::Char,
                ArrayElementType::Any => ConcreteType::Any,
                _ => return LatticeType::Top, // Unsupported element type
            };
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(concrete_elem),
            })
        }

        // Struct types - use struct_table if available
        ValueType::Struct(type_id) => {
            if let Some(table) = struct_table {
                // Search for struct name by type_id
                for (name, info) in table {
                    if info.type_id == *type_id {
                        return LatticeType::Concrete(ConcreteType::Struct {
                            name: name.clone(),
                            type_id: *type_id,
                        });
                    }
                }
            }
            // Could not resolve struct name, return Struct with synthetic name
            // This is better than Top as it preserves the fact that it's a struct
            LatticeType::Concrete(ConcreteType::Struct {
                name: format!("Struct#{}", type_id),
                type_id: *type_id,
            })
        }

        // Tuple and other collection types
        ValueType::Tuple | ValueType::NamedTuple | ValueType::Dict | ValueType::Set => {
            LatticeType::Top // Generic tuple/collection without element info
        }

        // Any and other dynamic types
        ValueType::Any => LatticeType::Top,

        // Other types default to Top
        _ => LatticeType::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;

    #[test]
    fn test_struct_type_info_new() {
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), LatticeType::Concrete(ConcreteType::Int64));
        fields.insert(
            "y".to_string(),
            LatticeType::Concrete(ConcreteType::Float64),
        );

        let info = StructTypeInfo::new(1, false, fields, false);

        assert_eq!(info.type_id, 1);
        assert!(!info.is_mutable);
        assert_eq!(
            info.get_field_type("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(
            info.get_field_type("y"),
            Some(&LatticeType::Concrete(ConcreteType::Float64))
        );
        assert_eq!(info.get_field_type("z"), None);
    }

    #[test]
    fn test_struct_type_info_has_field() {
        let mut fields = HashMap::new();
        fields.insert(
            "name".to_string(),
            LatticeType::Concrete(ConcreteType::String),
        );

        let info = StructTypeInfo::new(1, false, fields, false);

        assert!(info.has_field("name"));
        assert!(!info.has_field("age"));
    }

    #[test]
    fn test_value_type_to_lattice_primitives() {
        assert_eq!(
            value_type_to_lattice(&ValueType::I64),
            LatticeType::Concrete(ConcreteType::Int64)
        );
        assert_eq!(
            value_type_to_lattice(&ValueType::F64),
            LatticeType::Concrete(ConcreteType::Float64)
        );
        assert_eq!(
            value_type_to_lattice(&ValueType::Bool),
            LatticeType::Concrete(ConcreteType::Bool)
        );
        assert_eq!(
            value_type_to_lattice(&ValueType::Str),
            LatticeType::Concrete(ConcreteType::String)
        );
    }

    #[test]
    fn test_value_type_to_lattice_array() {
        use crate::vm::ArrayElementType;

        let array_type = ValueType::ArrayOf(ArrayElementType::I64);
        assert_eq!(
            value_type_to_lattice(&array_type),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Int64)
            })
        );

        let any_array_type = ValueType::ArrayOf(ArrayElementType::Any);
        assert_eq!(
            value_type_to_lattice(&any_array_type),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Any)
            })
        );
    }

    #[test]
    fn test_value_type_to_lattice_struct_without_table() {
        let struct_type = ValueType::Struct(42);
        // Without struct_table, Struct returns synthetic name (better than Top)
        assert_eq!(
            value_type_to_lattice(&struct_type),
            LatticeType::Concrete(ConcreteType::Struct {
                name: "Struct#42".to_string(),
                type_id: 42,
            })
        );
    }

    #[test]
    fn test_value_type_to_lattice_struct_with_table() {
        // Create a struct_table with a test struct
        let mut struct_table: HashMap<String, StructInfo> = HashMap::new();
        struct_table.insert(
            "Point".to_string(),
            StructInfo {
                type_id: 42,
                is_mutable: false,
                fields: vec![
                    ("x".to_string(), ValueType::F64),
                    ("y".to_string(), ValueType::F64),
                ],
                has_inner_constructor: false,
            },
        );

        let struct_type = ValueType::Struct(42);

        // With struct_table, Struct should resolve to the correct type
        let result = value_type_to_lattice_with_table(&struct_type, Some(&struct_table));
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Struct {
                name: "Point".to_string(),
                type_id: 42,
            })
        );
    }

    #[test]
    fn test_value_type_to_lattice_struct_unknown_type_id() {
        // Create a struct_table with a test struct
        let mut struct_table: HashMap<String, StructInfo> = HashMap::new();
        struct_table.insert(
            "Point".to_string(),
            StructInfo {
                type_id: 42,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );

        // Unknown type_id should return Struct with synthetic name (better than Top)
        let struct_type = ValueType::Struct(999);
        let result = value_type_to_lattice_with_table(&struct_type, Some(&struct_table));
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Struct {
                name: "Struct#999".to_string(),
                type_id: 999,
            })
        );
    }

    #[test]
    fn test_from_struct_info() {
        let struct_info = StructInfo {
            type_id: 10,
            is_mutable: true,
            fields: vec![
                ("x".to_string(), ValueType::I64),
                ("y".to_string(), ValueType::F64),
            ],
            has_inner_constructor: false,
        };

        let struct_type_info = StructTypeInfo::from(&struct_info);

        assert_eq!(struct_type_info.type_id, 10);
        assert!(struct_type_info.is_mutable);
        assert_eq!(
            struct_type_info.get_field_type("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(
            struct_type_info.get_field_type("y"),
            Some(&LatticeType::Concrete(ConcreteType::Float64))
        );
    }
}
