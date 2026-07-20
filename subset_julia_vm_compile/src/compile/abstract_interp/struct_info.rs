//! Struct type information for abstract interpretation.
//!
//! This module provides StructTypeInfo, which tracks field types for struct definitions
//! during type inference.

use crate::compile::context::StructInfo;
use crate::compile::context::StructRegistry;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::{CorePrimitive, CoreType};
use crate::runtime_types::ValueType;
use std::collections::HashMap;

/// Struct type information with field types as LatticeType.
///
/// This is a type-inference-friendly version of StructInfo that uses
/// LatticeType instead of ValueType for field types.
#[derive(Debug, Clone)]
pub struct StructTypeInfo {
    pub type_id: usize,
    pub is_mutable: bool,
    /// Field names in constructor/declaration order.
    ///
    /// The `fields` map is convenient for lookup, but default constructors
    /// map positional arguments to declared fields. Keeping the order here
    /// lets inference attach constructor argument types to immutable fields
    /// without changing the public field lookup representation.
    pub field_order: Vec<String>,
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
        let mut field_order: Vec<_> = fields.keys().cloned().collect();
        field_order.sort();
        Self {
            type_id,
            is_mutable,
            field_order,
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

    /// Gets field names in constructor/declaration order.
    pub fn field_order(&self) -> &[String] {
        &self.field_order
    }
}

/// Resolve the inference-only field-layout projection by its canonical Julia
/// spelling. Identity is carried by `StructTypeInfo::type_id` and by the
/// caller's `ConcreteType::Struct`; this map is only a lexical metadata index,
/// not the declaration authority retired by Issue #11046.
pub fn lookup_struct_type_info<'a>(
    infos: &'a HashMap<String, StructTypeInfo>,
    name: &str,
) -> Option<&'a StructTypeInfo> {
    infos.get(name)
}

/// Converts a StructInfo to StructTypeInfo by converting ValueType to LatticeType.
///
/// Note: This conversion does NOT use struct_table, so struct field types
/// that are themselves structs will fall back to `Top`. Use
/// `StructTypeInfo::from_with_struct_table` when struct_table is available.
impl From<&StructInfo> for StructTypeInfo {
    fn from(struct_info: &StructInfo) -> Self {
        let field_order = struct_info
            .fields
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
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
            field_order,
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
    pub fn from_with_struct_table(struct_info: &StructInfo, struct_table: &StructRegistry) -> Self {
        let field_order = struct_info
            .fields
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
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
            field_order,
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
    struct_table: Option<&StructRegistry>,
) -> LatticeType {
    match value_type {
        // Integer types - preserve precision
        ValueType::I8 => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        }
        ValueType::I16 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int16,
        ))),
        ValueType::I32 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        ))),
        ValueType::I64 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        ValueType::I128 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int128,
        ))),
        ValueType::U8 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt8,
        ))),
        ValueType::U16 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt16,
        ))),
        ValueType::U32 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt32,
        ))),
        ValueType::U64 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        ))),
        ValueType::U128 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt128,
        ))),
        ValueType::BigInt => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt,
        ))),

        // Float types
        ValueType::F32 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        ))),
        ValueType::F64 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        ValueType::BigFloat => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat,
        ))),

        // Boolean
        ValueType::Bool => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        }

        // String types
        ValueType::Str => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        ))),
        ValueType::Char => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }

        // Special types
        ValueType::Nothing => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        ))),
        ValueType::Symbol => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Symbol,
        ))),

        // Array types
        ValueType::Array => LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Any)), // Unknown element type
            ndims: None,
        }),
        ValueType::ArrayOf(elem_type, _) => {
            // Issue #5083: propagate the array element type into the lattice so
            // `a[i]` infers a concrete element type and downstream numeric
            // scans can use unboxed access. Previously every element type other
            // than I64/F64/Bool/String/Char/Any/Abstract collapsed to `Top`,
            // erasing the element type entirely.
            let concrete_elem = array_element_to_concrete(elem_type, struct_table);
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(concrete_elem),
                ndims: None,
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
        ValueType::Union(types) => {
            let variants = types
                .iter()
                .filter_map(
                    |ty| match value_type_to_lattice_with_table(ty, struct_table) {
                        LatticeType::Concrete(ct) => Some(ct),
                        _ => None,
                    },
                )
                .collect();
            LatticeType::Union(variants)
        }

        // Other types default to Top
        _ => LatticeType::Top,
    }
}

/// Converts an [`ArrayElementType`] into the [`ConcreteType`] used by the
/// inference lattice (Issue #5083).
///
/// Keeping every concrete scalar element type (not just `Int64`/`Float64`)
/// lets `a[i]` infer a precise element type so numeric scans can avoid boxing.
/// Heterogeneous / non-storage tags (`Any`, `Struct`, `TupleOf`, `UnionOf`,
/// `Abstract`) fall back to `Any`, which is still strictly more useful than the
/// previous `Top` collapse because it preserves the fact that the value is an
/// `Array`.
fn array_element_to_concrete(
    elem_type: &crate::runtime_types::ArrayElementType,
    struct_table: Option<&StructRegistry>,
) -> ConcreteType {
    use crate::runtime_types::ArrayElementType;
    match elem_type {
        ArrayElementType::I8 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
        ArrayElementType::I16 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
        ArrayElementType::I32 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ArrayElementType::I64 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ArrayElementType::I128 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)),
        ArrayElementType::U8 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        ArrayElementType::U16 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
        ArrayElementType::U32 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
        ArrayElementType::U64 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        ArrayElementType::U128 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)),
        ArrayElementType::F16 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)),
        ArrayElementType::F32 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ArrayElementType::F64 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ArrayElementType::Bool => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
        // SubString{String} shares the runtime String value type; treat its
        // element type as String for inference purposes.
        ArrayElementType::String | ArrayElementType::SubString => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
        }
        ArrayElementType::Char => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)),
        ArrayElementType::Symbol => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
        ArrayElementType::Nothing => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
        }
        ArrayElementType::ComplexF32 => ConcreteType::Struct {
            name: "Complex{Float32}".to_string(),
            type_id: 0,
        },
        ArrayElementType::ComplexF64 => ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        },
        // Concrete struct element arrays resolve via the same path as scalar
        // struct fields, reusing `value_type_to_lattice_with_table`.
        ArrayElementType::StructOf(type_id)
        | ArrayElementType::StructInlineOf(type_id, _)
        | ArrayElementType::StructInlineF64(type_id, _) => {
            match value_type_to_lattice_with_table(&ValueType::Struct(*type_id), struct_table) {
                LatticeType::Concrete(ct) => ct,
                _ => ConcreteType::Core(CoreType::Any),
            }
        }
        // Heterogeneous / abstract / non-storage tags: keep the array shape but
        // widen the element to `Any` (strictly better than `Top`).
        ArrayElementType::Struct
        | ArrayElementType::Any
        | ArrayElementType::TupleOf(_)
        | ArrayElementType::UnionOf(_)
        | ArrayElementType::Abstract(_)
        | ArrayElementType::Structured(_) => ConcreteType::Core(CoreType::Any),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;

    #[test]
    fn test_struct_type_info_new() {
        let mut fields = HashMap::new();
        fields.insert(
            "x".to_string(),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        fields.insert(
            "y".to_string(),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );

        let info = StructTypeInfo::new(1, false, fields, false);

        assert_eq!(info.type_id, 1);
        assert!(!info.is_mutable);
        assert_eq!(
            info.get_field_type("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            info.get_field_type("y"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            )))
        );
        assert_eq!(info.get_field_type("z"), None);
    }

    #[test]
    fn test_struct_type_info_has_field() {
        let mut fields = HashMap::new();
        fields.insert(
            "name".to_string(),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        let info = StructTypeInfo::new(1, false, fields, false);

        assert!(info.has_field("name"));
        assert!(!info.has_field("age"));
    }

    #[test]
    fn test_value_type_to_lattice_primitives() {
        assert_eq!(
            value_type_to_lattice(&ValueType::I64),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            value_type_to_lattice(&ValueType::F64),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
        assert_eq!(
            value_type_to_lattice(&ValueType::Bool),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
        assert_eq!(
            value_type_to_lattice(&ValueType::Str),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
    }

    #[test]
    fn test_value_type_to_lattice_array() {
        use crate::runtime_types::ArrayElementType;

        let array_type = ValueType::ArrayOf(ArrayElementType::I64, None);
        assert_eq!(
            value_type_to_lattice(&array_type),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ndims: None
            })
        );

        let any_array_type = ValueType::ArrayOf(ArrayElementType::Any, None);
        assert_eq!(
            value_type_to_lattice(&any_array_type),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None
            })
        );
    }

    /// Issue #5083: previously every element type other than I64/F64/Bool/
    /// String/Char/Any/Abstract was dropped to `Top`, which erased the array
    /// element type during inference and forced boxed element access. All
    /// scalar element types must now propagate into `ConcreteType::Array`.
    #[test]
    fn test_value_type_to_lattice_array_preserves_scalar_eltypes_issue_5083() {
        use crate::runtime_types::ArrayElementType;

        let cases = [
            (
                ArrayElementType::F32,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
            ),
            (
                ArrayElementType::I8,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ),
            (
                ArrayElementType::I16,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ),
            (
                ArrayElementType::I32,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ),
            (
                ArrayElementType::I128,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)),
            ),
            (
                ArrayElementType::U8,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
            ),
            (
                ArrayElementType::U16,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
            ),
            (
                ArrayElementType::U32,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
            ),
            (
                ArrayElementType::U64,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
            ),
            (
                ArrayElementType::U128,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)),
            ),
            (
                ArrayElementType::Symbol,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
            ),
        ];

        for (elem, expected) in cases {
            assert_eq!(
                value_type_to_lattice(&ValueType::ArrayOf(elem.clone(), None)),
                LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(expected.clone()),
                    ndims: None
                }),
                "ArrayOf({:?}) should preserve element type {:?}",
                elem,
                expected
            );
        }
    }

    /// Issue #5083: `Complex{Float32}` / `Complex{Float64}` element types should
    /// propagate as the corresponding struct concrete type rather than `Top`.
    #[test]
    fn test_value_type_to_lattice_array_preserves_complex_eltype_issue_5083() {
        use crate::runtime_types::ArrayElementType;

        assert_eq!(
            value_type_to_lattice(&ValueType::ArrayOf(ArrayElementType::ComplexF64, None)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Struct {
                    name: "Complex{Float64}".to_string(),
                    type_id: 0,
                }),
                ndims: None
            })
        );
        assert_eq!(
            value_type_to_lattice(&ValueType::ArrayOf(ArrayElementType::ComplexF32, None)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Struct {
                    name: "Complex{Float32}".to_string(),
                    type_id: 0,
                }),
                ndims: None
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
        let mut struct_table: StructRegistry = StructRegistry::new();
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
        let mut struct_table: StructRegistry = StructRegistry::new();
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
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            struct_type_info.get_field_type("y"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            )))
        );
    }

    #[test]
    fn test_value_type_union_to_lattice_preserves_field_union_issue_4270() {
        let result =
            value_type_to_lattice(&ValueType::Union(vec![ValueType::I64, ValueType::Nothing]));

        match result {
            LatticeType::Union(types) => {
                assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))));
                assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing
                ))));
            }
            other => panic!("expected union lattice type, got {:?}", other),
        }
    }
}
