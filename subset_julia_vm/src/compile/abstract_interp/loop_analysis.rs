//! Loop analysis for type inference.
//!
//! This module provides utilities for inferring types of loop variables
//! during abstract interpretation, supporting array, tuple, and range iteration.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::{CorePrimitive, CoreType};

/// Extracts the element type from a parametric struct name.
///
/// Parses struct names like `LinRange{Float64}`, `StepRangeLen{Float64}`,
/// `UnitRange{Int64}`, etc. to extract their element type parameter.
///
/// Returns `None` if the struct is not a known iterable or has no type parameter.
fn extract_struct_element_type(name: &str) -> Option<ConcreteType> {
    // Check if this is a parametric type with {T} suffix
    if let Some(open_brace) = name.find('{') {
        if let Some(close_brace) = name.rfind('}') {
            let base_name = &name[..open_brace];
            let type_param = &name[open_brace + 1..close_brace];

            // Known iterable struct types that yield their type parameter
            let iterable_structs = [
                "LinRange",
                "StepRangeLen",
                "UnitRange",
                "StepRange",
                "OneTo",
            ];

            if iterable_structs.contains(&base_name) {
                return parse_type_param(type_param);
            }
        }
    }

    // Non-parametric iterable structs with known element types
    None
}

/// Parses a type parameter string into a ConcreteType.
fn parse_type_param(type_param: &str) -> Option<ConcreteType> {
    match type_param.trim() {
        "Int8" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))),
        "Int16" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int16,
        ))),
        "Int32" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        ))),
        "Int64" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        "Int128" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int128,
        ))),
        "UInt8" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt8,
        ))),
        "UInt16" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt16,
        ))),
        "UInt32" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt32,
        ))),
        "UInt64" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        ))),
        "UInt128" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt128,
        ))),
        "Float32" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        ))),
        "Float64" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        "Bool" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        "String" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        ))),
        "Char" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))),
        _ => None, // Unknown type parameter
    }
}

/// Extracts the element type from an iterable type.
///
/// This function determines what type each iteration of a loop will produce
/// based on the type of the iterable expression.
///
/// # Supported iterables
///
/// - **Array{T}**: Returns T (the element type)
/// - **Tuple{T1, T2, ...}**: Returns Union{T1, T2, ...} (union of all element types)
/// - **NamedTuple{names, T1, T2, ...}**: Returns Union{T1, T2, ...} (iterates field values)
/// - **UnitRange / StepRange**: Returns Int64 (ranges iterate over integers)
/// - **String**: Returns Char (strings iterate over characters)
/// - **Other**: Returns Top (unknown iteration type)
///
/// # Examples
///
/// ```
/// use subset_julia_vm::compile::abstract_interp::loop_analysis::element_type;
/// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
///
/// // Array{Int64} -> Int64
/// let array_ty = LatticeType::Concrete(ConcreteType::Array {
///     element: Box::new(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))),
/// , ndims: None });
/// assert_eq!(
///     element_type(&array_ty),
///     LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)))
/// );
///
/// // Tuple{Int64, Float64} -> Union{Int64, Float64}
/// let tuple_ty = LatticeType::Concrete(ConcreteType::Tuple {
///     elements: vec![ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)), ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))],
/// });
/// let result = element_type(&tuple_ty);
/// match result {
///     LatticeType::Union(types) => {
///         assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
///         assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))));
///     }
///     _ => panic!("Expected Union type"),
/// }
/// ```
pub fn element_type(iterable_ty: &LatticeType) -> LatticeType {
    match iterable_ty {
        // Const values: treat as their concrete type
        LatticeType::Const(cv) => element_type(&LatticeType::Concrete(cv.to_concrete_type())),

        // Array{T} -> T
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(*element.clone())
        }

        // Tuple{T1, T2, ...} -> Union{T1, T2, ...}
        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
            if elements.is_empty() {
                // Empty tuple: no elements to iterate
                LatticeType::Bottom
            } else if elements.len() == 1 {
                // Single element tuple: return that element type
                LatticeType::Concrete(elements[0].clone())
            } else {
                // Multiple element tuple: return union of all element types
                let unique_types: std::collections::BTreeSet<_> =
                    elements.iter().cloned().collect();

                if unique_types.len() == 1 {
                    // All elements have the same type
                    if let Some(ty) = unique_types.iter().next().cloned() {
                        LatticeType::Concrete(ty)
                    } else {
                        LatticeType::Bottom
                    }
                } else {
                    // Multiple different types
                    LatticeType::Union(unique_types)
                }
            }
        }

        // Tuple{T1, ..., Vararg{Tail}} -> Union(T1, ..., Tail) (Issue #3511)
        LatticeType::Concrete(ConcreteType::TupleVararg { elements, tail }) => {
            let mut unique: std::collections::BTreeSet<ConcreteType> =
                elements.iter().cloned().collect();
            unique.insert(*tail.clone());
            if unique.is_empty() {
                LatticeType::Concrete(*tail.clone())
            } else if unique.len() == 1 {
                LatticeType::Concrete(
                    unique
                        .into_iter()
                        .next()
                        .unwrap_or(ConcreteType::Core(CoreType::Any)),
                )
            } else {
                LatticeType::Union(unique)
            }
        }

        // NamedTuple{names, T1, T2, ...} -> Union{T1, T2, ...} (Issue #5082)
        //
        // Upstream `iterate(t::NamedTuple, ...)` yields the field *values* in
        // declaration order (julia/base/namedtuple.jl), so the loop variable
        // type is the union of the field value types — exactly the same shape
        // as iterating the equivalent positional tuple. Previously this fell
        // through to the `Top` catch-all, so every `for v in nt` body became
        // fully dynamic.
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            if fields.is_empty() {
                // Empty NamedTuple: no elements to iterate.
                LatticeType::Bottom
            } else {
                let unique_types: std::collections::BTreeSet<_> =
                    fields.iter().map(|(_, ty)| ty.clone()).collect();

                if unique_types.len() == 1 {
                    // All fields share the same value type.
                    if let Some(ty) = unique_types.into_iter().next() {
                        LatticeType::Concrete(ty)
                    } else {
                        LatticeType::Bottom
                    }
                } else {
                    // Heterogeneous field value types -> union.
                    LatticeType::Union(unique_types)
                }
            }
        }

        // Range{T} -> T
        LatticeType::Concrete(ConcreteType::Range { element }) => {
            LatticeType::Concrete(*element.clone())
        }

        // Dict{K, V} -> Tuple{K, V} (for (k, v) in dict pattern)
        LatticeType::Concrete(ConcreteType::Dict { key, value }) => {
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![*key.clone(), *value.clone()],
            })
        }

        // Set{T} -> T
        LatticeType::Concrete(ConcreteType::Set { element }) => {
            LatticeType::Concrete(*element.clone())
        }

        // String -> Char
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }

        // Union types: join all element types
        LatticeType::Union(types) => {
            let mut result: Option<LatticeType> = None;
            for concrete_ty in types {
                let elem_ty = element_type(&LatticeType::Concrete(concrete_ty.clone()));
                result = Some(if let Some(existing) = result {
                    existing.join(&elem_ty)
                } else {
                    elem_ty
                });
            }
            result.unwrap_or(LatticeType::Top)
        }

        // Struct types (e.g., LinRange{Float64}, StepRangeLen{Float64})
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            if let Some(elem_type) = extract_struct_element_type(name) {
                LatticeType::Concrete(elem_type)
            } else {
                // Unknown struct type, conservatively return Top
                LatticeType::Top
            }
        }

        // Generator type
        LatticeType::Concrete(ConcreteType::Generator { element }) => {
            LatticeType::Concrete(*element.clone())
        }

        // Unknown or unsupported iterable
        LatticeType::Top | LatticeType::Bottom | LatticeType::Conditional { .. } => {
            LatticeType::Top
        }

        // Concrete types that are not iterable
        _ => LatticeType::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_element_type_array_int() {
        let array_ty = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        assert_eq!(
            element_type(&array_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_array_float() {
        let array_ty = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        });
        assert_eq!(
            element_type(&array_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_element_type_tuple_homogeneous() {
        let tuple_ty = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        assert_eq!(
            element_type(&tuple_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_tuple_heterogeneous() {
        let tuple_ty = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ],
        });

        let result = element_type(&tuple_ty);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
        }
    }

    #[test]
    fn test_element_type_tuple_single() {
        let tuple_ty = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))],
        });
        assert_eq!(
            element_type(&tuple_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
    }

    #[test]
    fn test_element_type_tuple_empty() {
        let tuple_ty = LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] });
        assert_eq!(element_type(&tuple_ty), LatticeType::Bottom);
    }

    #[test]
    fn test_element_type_string() {
        let string_ty = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        assert_eq!(
            element_type(&string_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        );
    }

    #[test]
    fn test_element_type_union_arrays() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        union_types.insert(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        });

        let union_ty = LatticeType::Union(union_types);

        let result = element_type(&union_ty);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
        }
    }

    #[test]
    fn test_element_type_top() {
        assert_eq!(element_type(&LatticeType::Top), LatticeType::Top);
    }

    #[test]
    fn test_element_type_bottom() {
        assert_eq!(element_type(&LatticeType::Bottom), LatticeType::Top);
    }

    #[test]
    fn test_element_type_non_iterable() {
        let int_ty = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(element_type(&int_ty), LatticeType::Top);
    }

    #[test]
    fn test_element_type_range() {
        let range_ty = LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert_eq!(
            element_type(&range_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_dict() {
        let dict_ty = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            value: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        let result = element_type(&dict_ty);
        assert!(
            matches!(&result, LatticeType::Concrete(ConcreteType::Tuple { .. })),
            "Expected Tuple type, got {:?}",
            result
        );
        if let LatticeType::Concrete(ConcreteType::Tuple { elements }) = result {
            assert_eq!(elements.len(), 2);
            assert_eq!(
                elements[0],
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
            );
            assert_eq!(
                elements[1],
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            );
        }
    }

    #[test]
    fn test_element_type_set() {
        let set_ty = LatticeType::Concrete(ConcreteType::Set {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert_eq!(
            element_type(&set_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_linrange_float64() {
        let linrange_ty = LatticeType::Concrete(ConcreteType::Struct {
            name: "LinRange{Float64}".to_string(),
            type_id: 0,
        });
        assert_eq!(
            element_type(&linrange_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_element_type_steprangelen_float64() {
        let steprangelen_ty = LatticeType::Concrete(ConcreteType::Struct {
            name: "StepRangeLen{Float64}".to_string(),
            type_id: 0,
        });
        assert_eq!(
            element_type(&steprangelen_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_element_type_unitrange_int64() {
        let unitrange_ty = LatticeType::Concrete(ConcreteType::Struct {
            name: "UnitRange{Int64}".to_string(),
            type_id: 0,
        });
        assert_eq!(
            element_type(&unitrange_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_steprange_int64() {
        let steprange_ty = LatticeType::Concrete(ConcreteType::Struct {
            name: "StepRange{Int64}".to_string(),
            type_id: 0,
        });
        assert_eq!(
            element_type(&steprange_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_oneto_int64() {
        let oneto_ty = LatticeType::Concrete(ConcreteType::Struct {
            name: "OneTo{Int64}".to_string(),
            type_id: 0,
        });
        assert_eq!(
            element_type(&oneto_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_unknown_struct() {
        let unknown_struct_ty = LatticeType::Concrete(ConcreteType::Struct {
            name: "MyCustomType".to_string(),
            type_id: 0,
        });
        assert_eq!(element_type(&unknown_struct_ty), LatticeType::Top);
    }

    #[test]
    fn test_element_type_generator() {
        let generator_ty = LatticeType::Concrete(ConcreteType::Generator {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        });
        assert_eq!(
            element_type(&generator_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_element_type_namedtuple_homogeneous() {
        // Issue #5082: iterating a NamedTuple yields its field values in
        // order. For a homogeneous NamedTuple this collapses to the single
        // field value type (not Top).
        let nt_ty = LatticeType::Concrete(ConcreteType::NamedTuple {
            fields: vec![
                (
                    "a".to_string(),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ),
                (
                    "b".to_string(),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ),
            ],
        });
        assert_eq!(
            element_type(&nt_ty),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_element_type_namedtuple_heterogeneous() {
        // Issue #5082: a heterogeneous NamedTuple yields the union of its
        // field value types, mirroring tuple iteration.
        let nt_ty = LatticeType::Concrete(ConcreteType::NamedTuple {
            fields: vec![
                (
                    "x".to_string(),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ),
                (
                    "y".to_string(),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ),
            ],
        });
        let result = element_type(&nt_ty);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
        }
    }

    #[test]
    fn test_element_type_namedtuple_empty() {
        // An empty NamedTuple has no elements to iterate.
        let nt_ty = LatticeType::Concrete(ConcreteType::NamedTuple { fields: vec![] });
        assert_eq!(element_type(&nt_ty), LatticeType::Bottom);
    }

    #[test]
    fn test_extract_struct_element_type() {
        assert_eq!(
            extract_struct_element_type("LinRange{Float64}"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
        assert_eq!(
            extract_struct_element_type("StepRangeLen{Int64}"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            extract_struct_element_type("UnitRange{Int32}"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32
            )))
        );
        assert_eq!(
            extract_struct_element_type("StepRange{UInt64}"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64
            )))
        );
        assert_eq!(
            extract_struct_element_type("OneTo{Int64}"),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(extract_struct_element_type("MyCustomType"), None);
        assert_eq!(extract_struct_element_type("UnknownType{Float64}"), None);
    }
}
