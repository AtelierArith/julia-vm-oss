//! Transfer functions for iterator operations.
//!
//! This module implements type inference for Julia's iterator protocol,
//! including iterate, length, eachindex, enumerate, and zip.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::lattice::widening::MAX_UNION_LENGTH;
use std::collections::BTreeSet;

/// Extracts the element type from a struct iterable (e.g., LinRange{Float64}).
///
/// Returns the element type if the struct is a known iterable type,
/// otherwise returns None.
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
    None
}

/// Computes the union of element types from a tuple.
///
/// If all elements have the same type, returns that type.
/// Otherwise, returns a union type (simplified to the first element type
/// if the union would be too large, to avoid type explosion).
fn compute_element_union(elements: &[ConcreteType]) -> ConcreteType {
    if elements.is_empty() {
        return ConcreteType::Nothing;
    }

    // Check if all elements have the same type
    let first = &elements[0];
    let all_same = elements.iter().all(|e| e == first);
    if all_same {
        return first.clone();
    }

    // Collect unique types
    let mut unique_types: BTreeSet<ConcreteType> = BTreeSet::new();
    for elem in elements {
        unique_types.insert(elem.clone());
    }

    // If only one unique type after dedup, return it
    if unique_types.len() == 1 {
        if let Some(only) = unique_types.into_iter().next() {
            return only;
        }
        return first.clone();
    }

    // If too many unique types (more than MAX_UNION_LENGTH), use widening
    if unique_types.len() > MAX_UNION_LENGTH {
        // Widen to the most general numeric type if all are numeric
        let all_numeric = unique_types.iter().all(|t| t.is_numeric());
        if all_numeric {
            // Check if any is float, if so return Float64, otherwise Int64
            let has_float = unique_types.iter().any(|t| t.is_float());
            if has_float {
                return ConcreteType::Float64;
            } else {
                return ConcreteType::Int64;
            }
        }
        // For truly heterogeneous types, we'd return Top, but since we're
        // trying to reduce Top fallbacks, we return the first type as a
        // conservative approximation
        return first.clone();
    }

    // Return the first element type as a representative
    // Note: Ideally we'd return a proper Union here, but ConcreteType
    // doesn't directly support unions. The caller (tfunc_iterate) will
    // handle this appropriately.
    first.clone()
}

/// Parses a type parameter string into a ConcreteType.
fn parse_type_param(type_param: &str) -> Option<ConcreteType> {
    match type_param.trim() {
        "Int8" => Some(ConcreteType::Int8),
        "Int16" => Some(ConcreteType::Int16),
        "Int32" => Some(ConcreteType::Int32),
        "Int64" => Some(ConcreteType::Int64),
        "Int128" => Some(ConcreteType::Int128),
        "UInt8" => Some(ConcreteType::UInt8),
        "UInt16" => Some(ConcreteType::UInt16),
        "UInt32" => Some(ConcreteType::UInt32),
        "UInt64" => Some(ConcreteType::UInt64),
        "UInt128" => Some(ConcreteType::UInt128),
        "Float32" => Some(ConcreteType::Float32),
        "Float64" => Some(ConcreteType::Float64),
        "Bool" => Some(ConcreteType::Bool),
        "String" => Some(ConcreteType::String),
        "Char" => Some(ConcreteType::Char),
        _ => None, // Unknown type parameter
    }
}

/// Transfer function for `iterate` (iterator protocol).
///
/// Type rules:
/// - iterate(Array{T}) → Union{Nothing, Tuple{T, Int64}}
/// - iterate(Range{T}) → Union{Nothing, Tuple{T, Int64}}
/// - iterate(Dict{K,V}) → Union{Nothing, Tuple{Pair{K,V}, Int64}}
///
/// # Examples
/// ```text
/// iterate([1,2,3]) → Union{Nothing, Tuple{Int64, Int64}}
/// iterate(1:10) → Union{Nothing, Tuple{Int64, Int64}}
/// ```
pub fn tfunc_iterate(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element }) => {
            // iterate returns Union{Nothing, Tuple{T, State}}
            let mut union_types = BTreeSet::new();
            union_types.insert(ConcreteType::Nothing);
            union_types.insert(ConcreteType::Tuple {
                elements: vec![*element.clone(), ConcreteType::Int64],
            });
            LatticeType::Union(union_types)
        }
        LatticeType::Concrete(ConcreteType::Range { element }) => {
            let mut union_types = BTreeSet::new();
            union_types.insert(ConcreteType::Nothing);
            union_types.insert(ConcreteType::Tuple {
                elements: vec![*element.clone(), ConcreteType::Int64],
            });
            LatticeType::Union(union_types)
        }
        LatticeType::Concrete(ConcreteType::Dict { key, value }) => {
            // Dict iteration returns Pair{K,V}
            let pair_type = ConcreteType::Tuple {
                elements: vec![*key.clone(), *value.clone()],
            };
            let mut union_types = BTreeSet::new();
            union_types.insert(ConcreteType::Nothing);
            union_types.insert(ConcreteType::Tuple {
                elements: vec![pair_type, ConcreteType::Int64],
            });
            LatticeType::Union(union_types)
        }
        LatticeType::Concrete(ConcreteType::String) => {
            // String iteration returns Char
            let mut union_types = BTreeSet::new();
            union_types.insert(ConcreteType::Nothing);
            union_types.insert(ConcreteType::Tuple {
                elements: vec![ConcreteType::Char, ConcreteType::Int64],
            });
            LatticeType::Union(union_types)
        }
        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
            if elements.is_empty() {
                // Empty tuple always returns Nothing
                LatticeType::Concrete(ConcreteType::Nothing)
            } else {
                // For non-empty tuples, compute the union of all element types
                // iterate returns Union{Nothing, Tuple{<element_union>, Int64}}
                let element_union = compute_element_union(elements);
                let mut union_types = BTreeSet::new();
                union_types.insert(ConcreteType::Nothing);
                union_types.insert(ConcreteType::Tuple {
                    elements: vec![element_union, ConcreteType::Int64],
                });
                LatticeType::Union(union_types)
            }
        }
        // Struct iterables (LinRange, StepRangeLen, UnitRange, StepRange, OneTo)
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            if let Some(elem_type) = extract_struct_element_type(name) {
                let mut union_types = BTreeSet::new();
                union_types.insert(ConcreteType::Nothing);
                union_types.insert(ConcreteType::Tuple {
                    elements: vec![elem_type, ConcreteType::Int64],
                });
                LatticeType::Union(union_types)
            } else {
                LatticeType::Top
            }
        }
        // Generator type
        LatticeType::Concrete(ConcreteType::Generator { element }) => {
            let mut union_types = BTreeSet::new();
            union_types.insert(ConcreteType::Nothing);
            union_types.insert(ConcreteType::Tuple {
                elements: vec![*element.clone(), ConcreteType::Int64],
            });
            LatticeType::Union(union_types)
        }
        // Set type
        LatticeType::Concrete(ConcreteType::Set { element }) => {
            let mut union_types = BTreeSet::new();
            union_types.insert(ConcreteType::Nothing);
            union_types.insert(ConcreteType::Tuple {
                elements: vec![*element.clone(), ConcreteType::Int64],
            });
            LatticeType::Union(union_types)
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `length` (collection size).
///
/// Type rules:
/// - length(Collection) → Int64
///
/// # Examples
/// ```text
/// length([1,2,3]) → Int64
/// length("hello") → Int64
/// ```
pub fn tfunc_length_iter(_args: &[LatticeType]) -> LatticeType {
    // length always returns Int64
    LatticeType::Concrete(ConcreteType::Int64)
}

/// Transfer function for `eachindex` (index iterator).
///
/// Type rules:
/// - eachindex(Array) → Range{Int64}
/// - eachindex(Dict) → KeySet
///
/// # Examples
/// ```text
/// eachindex([1,2,3]) → Range{Int64}
/// ```
pub fn tfunc_eachindex(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { .. }) => {
            // eachindex for arrays returns a range
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Int64),
            })
        }
        LatticeType::Concrete(ConcreteType::Dict { key, .. }) => {
            // eachindex for dicts returns the key type iterator
            // For simplicity, return a Set of keys
            LatticeType::Concrete(ConcreteType::Set {
                element: key.clone(),
            })
        }
        LatticeType::Concrete(ConcreteType::String) => {
            // eachindex for strings returns a range
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Int64),
            })
        }
        // Struct iterables (LinRange, StepRangeLen, etc.) use integer indices
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            if extract_struct_element_type(name).is_some() {
                LatticeType::Concrete(ConcreteType::Range {
                    element: Box::new(ConcreteType::Int64),
                })
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `enumerate` (index-value iterator).
///
/// Type rules:
/// - enumerate(Array{T}) → Generator{Tuple{Int64, T}}
///
/// # Examples
/// ```text
/// enumerate([1,2,3]) → Generator{Tuple{Int64, Int64}}
/// ```
pub fn tfunc_enumerate(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element }) => {
            // enumerate returns a Generator of (index, element) pairs
            LatticeType::Concrete(ConcreteType::Generator {
                element: Box::new(ConcreteType::Tuple {
                    elements: vec![ConcreteType::Int64, *element.clone()],
                }),
            })
        }
        LatticeType::Concrete(ConcreteType::Range { element }) => {
            LatticeType::Concrete(ConcreteType::Generator {
                element: Box::new(ConcreteType::Tuple {
                    elements: vec![ConcreteType::Int64, *element.clone()],
                }),
            })
        }
        // Struct iterables (LinRange, StepRangeLen, etc.)
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            if let Some(elem_type) = extract_struct_element_type(name) {
                LatticeType::Concrete(ConcreteType::Generator {
                    element: Box::new(ConcreteType::Tuple {
                        elements: vec![ConcreteType::Int64, elem_type],
                    }),
                })
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `zip` (parallel iteration).
///
/// Type rules:
/// - zip(Array{T}, Array{U}) → Generator{Tuple{T, U}}
///
/// # Examples
/// ```text
/// zip([1,2,3], ["a","b","c"]) → Generator{Tuple{Int64, String}}
/// ```
pub fn tfunc_zip(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // Extract element types from all arguments
    let mut element_types = Vec::new();
    for arg in args {
        match arg {
            LatticeType::Concrete(ConcreteType::Array { element }) => {
                element_types.push(*element.clone());
            }
            LatticeType::Concrete(ConcreteType::Range { element }) => {
                element_types.push(*element.clone());
            }
            // Struct iterables (LinRange, StepRangeLen, etc.)
            LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
                if let Some(elem_type) = extract_struct_element_type(name) {
                    element_types.push(elem_type);
                } else {
                    return LatticeType::Top;
                }
            }
            _ => return LatticeType::Top,
        }
    }

    // zip returns a Generator of tuples
    LatticeType::Concrete(ConcreteType::Generator {
        element: Box::new(ConcreteType::Tuple {
            elements: element_types,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iterate_array() {
        let array = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        });
        let args = vec![array];
        let result = tfunc_iterate(&args);
        assert!(matches!(result, LatticeType::Union(_)));
    }

    #[test]
    fn test_iterate_range() {
        let range = LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Int64),
        });
        let args = vec![range];
        let result = tfunc_iterate(&args);
        assert!(matches!(result, LatticeType::Union(_)));
    }

    #[test]
    fn test_iterate_string() {
        let string = LatticeType::Concrete(ConcreteType::String);
        let args = vec![string];
        let result = tfunc_iterate(&args);
        assert!(matches!(result, LatticeType::Union(_)));
    }

    #[test]
    fn test_length_iter() {
        let array = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Float64),
        });
        let args = vec![array];
        let result = tfunc_length_iter(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_eachindex_array() {
        let array = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        });
        let args = vec![array];
        let result = tfunc_eachindex(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Int64)
            })
        );
    }

    #[test]
    fn test_enumerate_array() {
        let array = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Float64),
        });
        let args = vec![array];
        let result = tfunc_enumerate(&args);
        assert!(matches!(
            result,
            LatticeType::Concrete(ConcreteType::Generator { .. })
        ));
    }

    #[test]
    fn test_zip_two_arrays() {
        let array1 = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        });
        let array2 = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::String),
        });
        let args = vec![array1, array2];
        let result = tfunc_zip(&args);
        assert!(matches!(
            result,
            LatticeType::Concrete(ConcreteType::Generator { .. })
        ));
    }

    // Tests for struct iterable type inference
    #[test]
    fn test_iterate_linrange_float64() {
        let linrange = LatticeType::Concrete(ConcreteType::Struct {
            name: "LinRange{Float64}".to_string(),
            type_id: 0,
        });
        let args = vec![linrange];
        let result = tfunc_iterate(&args);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert!(types.contains(&ConcreteType::Nothing));
            // Check that Float64 is in the tuple element
            let has_float64_tuple = types.iter().any(|t| {
                matches!(t, ConcreteType::Tuple { elements } if elements.first() == Some(&ConcreteType::Float64))
            });
            assert!(has_float64_tuple, "Expected tuple with Float64 element");
        }
    }

    #[test]
    fn test_iterate_steprangelen_float64() {
        let steprangelen = LatticeType::Concrete(ConcreteType::Struct {
            name: "StepRangeLen{Float64}".to_string(),
            type_id: 0,
        });
        let args = vec![steprangelen];
        let result = tfunc_iterate(&args);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert!(types.contains(&ConcreteType::Nothing));
            let has_float64_tuple = types.iter().any(|t| {
                matches!(t, ConcreteType::Tuple { elements } if elements.first() == Some(&ConcreteType::Float64))
            });
            assert!(has_float64_tuple, "Expected tuple with Float64 element");
        }
    }

    #[test]
    fn test_iterate_unitrange_int64() {
        let unitrange = LatticeType::Concrete(ConcreteType::Struct {
            name: "UnitRange{Int64}".to_string(),
            type_id: 0,
        });
        let args = vec![unitrange];
        let result = tfunc_iterate(&args);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert!(types.contains(&ConcreteType::Nothing));
            let has_int64_tuple = types.iter().any(|t| {
                matches!(t, ConcreteType::Tuple { elements } if elements.first() == Some(&ConcreteType::Int64))
            });
            assert!(has_int64_tuple, "Expected tuple with Int64 element");
        }
    }

    #[test]
    fn test_enumerate_linrange_float64() {
        let linrange = LatticeType::Concrete(ConcreteType::Struct {
            name: "LinRange{Float64}".to_string(),
            type_id: 0,
        });
        let args = vec![linrange];
        let result = tfunc_enumerate(&args);
        assert!(
            matches!(&result, LatticeType::Concrete(ConcreteType::Generator { .. })),
            "Expected Generator type, got {:?}",
            result
        );
        if let LatticeType::Concrete(ConcreteType::Generator { element }) = result {
            assert!(
                matches!(element.as_ref(), ConcreteType::Tuple { .. }),
                "Expected Tuple, got {:?}",
                element
            );
            if let ConcreteType::Tuple { elements } = element.as_ref() {
                assert_eq!(elements.len(), 2);
                assert_eq!(elements[0], ConcreteType::Int64); // index
                assert_eq!(elements[1], ConcreteType::Float64); // element
            }
        }
    }

    #[test]
    fn test_zip_linrange_with_array() {
        let linrange = LatticeType::Concrete(ConcreteType::Struct {
            name: "LinRange{Float64}".to_string(),
            type_id: 0,
        });
        let array = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        });
        let args = vec![linrange, array];
        let result = tfunc_zip(&args);
        assert!(
            matches!(&result, LatticeType::Concrete(ConcreteType::Generator { .. })),
            "Expected Generator type, got {:?}",
            result
        );
        if let LatticeType::Concrete(ConcreteType::Generator { element }) = result {
            assert!(
                matches!(element.as_ref(), ConcreteType::Tuple { .. }),
                "Expected Tuple, got {:?}",
                element
            );
            if let ConcreteType::Tuple { elements } = element.as_ref() {
                assert_eq!(elements.len(), 2);
                assert_eq!(elements[0], ConcreteType::Float64); // from LinRange
                assert_eq!(elements[1], ConcreteType::Int64); // from Array
            }
        }
    }

    #[test]
    fn test_eachindex_linrange() {
        let linrange = LatticeType::Concrete(ConcreteType::Struct {
            name: "LinRange{Float64}".to_string(),
            type_id: 0,
        });
        let args = vec![linrange];
        let result = tfunc_eachindex(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Int64)
            })
        );
    }

    #[test]
    fn test_iterate_unknown_struct() {
        let unknown = LatticeType::Concrete(ConcreteType::Struct {
            name: "MyCustomType".to_string(),
            type_id: 0,
        });
        let args = vec![unknown];
        let result = tfunc_iterate(&args);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_iterate_homogeneous_tuple() {
        // Tuple of all Int64 elements
        let tuple = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Int64,
                ConcreteType::Int64,
                ConcreteType::Int64,
            ],
        });
        let args = vec![tuple];
        let result = tfunc_iterate(&args);

        // Should return Union{Nothing, Tuple{Int64, Int64}}
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert!(types.contains(&ConcreteType::Nothing));
            let has_int64_tuple = types.iter().any(|t| {
                matches!(t, ConcreteType::Tuple { elements }
                    if elements.len() == 2
                    && elements[0] == ConcreteType::Int64
                    && elements[1] == ConcreteType::Int64)
            });
            assert!(
                has_int64_tuple,
                "Expected tuple with Int64 element and Int64 state"
            );
        }
    }

    #[test]
    fn test_iterate_heterogeneous_tuple() {
        // Tuple of different types: (Int64, Float64, String)
        let tuple = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Int64,
                ConcreteType::Float64,
                ConcreteType::String,
            ],
        });
        let args = vec![tuple];
        let result = tfunc_iterate(&args);

        // Should return a Union type (not Top!)
        assert!(
            !matches!(&result, LatticeType::Top),
            "Should not return Top for heterogeneous tuple"
        );
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert!(types.contains(&ConcreteType::Nothing));
            // Should have a Tuple type for the iteration result
            let has_tuple = types
                .iter()
                .any(|t| matches!(t, ConcreteType::Tuple { .. }));
            assert!(has_tuple, "Expected a tuple type in the union");
        }
    }

    #[test]
    fn test_iterate_empty_tuple() {
        let tuple = LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] });
        let args = vec![tuple];
        let result = tfunc_iterate(&args);

        // Empty tuple iteration returns Nothing immediately
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Nothing));
    }
}
