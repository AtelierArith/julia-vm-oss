//! Transfer functions for collection operations.
//!
//! This module implements type inference for Julia's collection operations,
//! including keys, values, pairs, haskey, get, and get!.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::lattice::widening::MAX_UNION_LENGTH;
use std::collections::BTreeSet;

/// Computes the element type for heterogeneous field types.
///
/// For a small number of unique types (≤ MAX_UNION_LENGTH), returns a UnionOf
/// to preserve precise type information. For larger sets, applies widening.
/// This is used when creating Generator types for heterogeneous NamedTuples.
///
/// # Issue #1637
/// This function now returns `ConcreteType::UnionOf(types)` to preserve Union
/// information instead of collapsing to a representative type.
fn compute_field_type_union(fields: &[(String, ConcreteType)]) -> ConcreteType {
    if fields.is_empty() {
        return ConcreteType::Nothing;
    }

    // Collect unique types
    let mut unique_types: BTreeSet<ConcreteType> = BTreeSet::new();
    for (_, ty) in fields {
        unique_types.insert(ty.clone());
    }

    // If only one unique type, return it
    if unique_types.len() == 1 {
        if let Some(only) = unique_types.into_iter().next() {
            return only;
        }
        return ConcreteType::Nothing;
    }

    // If too many unique types, apply widening
    if unique_types.len() > MAX_UNION_LENGTH {
        // Check if all are numeric
        let all_numeric = unique_types.iter().all(|t| t.is_numeric());
        if all_numeric {
            let has_float = unique_types.iter().any(|t| t.is_float());
            if has_float {
                return ConcreteType::Float64;
            } else {
                return ConcreteType::Int64;
            }
        }
        // Return Any for very heterogeneous types
        return ConcreteType::Any;
    }

    // For a small number of unique types, return a UnionOf to preserve type information
    // Issue #1637: This preserves Union{Int64, Float64} instead of collapsing to Float64
    let types: Vec<ConcreteType> = unique_types.into_iter().collect();
    ConcreteType::UnionOf(types)
}

/// Transfer function for `keys` (get collection keys).
///
/// Type rules:
/// - keys(Dict{K,V}) → KeySet{K}
/// - keys(NamedTuple{...}) → Tuple{Symbol, ...}
/// - keys(Array) → Range{Int64}
///
/// # Examples
/// ```text
/// keys(Dict{String,Int64}) → Set{String}
/// keys((a=1, b=2)) → Tuple{Symbol, Symbol}
/// ```
pub fn tfunc_keys(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Dict { key, .. }) => {
            // keys returns a Set-like iterator of keys
            LatticeType::Concrete(ConcreteType::Set {
                element: key.clone(),
            })
        }
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            // keys of NamedTuple returns tuple of symbols
            let symbols = vec![ConcreteType::Symbol; fields.len()];
            LatticeType::Concrete(ConcreteType::Tuple { elements: symbols })
        }
        LatticeType::Concrete(ConcreteType::Array { .. }) => {
            // keys of Array returns range of indices
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Int64),
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `values` (get collection values).
///
/// Type rules:
/// - values(Dict{K,V}) → ValueIterator{V}
/// - values(NamedTuple{...}) → Tuple{T1, T2, ...}
/// - values(Array{T}) → Array{T}
///
/// # Examples
/// ```text
/// values(Dict{String,Int64}) → Generator{Int64}
/// values((a=1, b=2.0)) → Tuple{Int64, Float64}
/// ```
pub fn tfunc_values(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Dict { value, .. }) => {
            // values returns a Generator-like iterator of values
            LatticeType::Concrete(ConcreteType::Generator {
                element: value.clone(),
            })
        }
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            // values of NamedTuple returns tuple of field values
            let types: Vec<ConcreteType> = fields.iter().map(|(_, ty)| ty.clone()).collect();
            LatticeType::Concrete(ConcreteType::Tuple { elements: types })
        }
        LatticeType::Concrete(ConcreteType::Array { element }) => {
            // values of Array returns the array itself
            LatticeType::Concrete(ConcreteType::Array {
                element: element.clone(),
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `pairs` (get key-value pairs).
///
/// Type rules:
/// - pairs(Dict{K,V}) → PairIterator{K,V}
/// - pairs(NamedTuple{...}) → PairIterator{Symbol, T}
/// - pairs(Array{T}) → PairIterator{Int64, T}
///
/// # Examples
/// ```text
/// pairs(Dict{String,Int64}) → Generator{Tuple{String, Int64}}
/// pairs([1,2,3]) → Generator{Tuple{Int64, Int64}}
/// ```
pub fn tfunc_pairs(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Dict { key, value }) => {
            // pairs returns a Generator of (key, value) tuples
            LatticeType::Concrete(ConcreteType::Generator {
                element: Box::new(ConcreteType::Tuple {
                    elements: vec![*key.clone(), *value.clone()],
                }),
            })
        }
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            // pairs of NamedTuple returns Generator of (Symbol, value) tuples
            if fields.is_empty() {
                LatticeType::Concrete(ConcreteType::Generator {
                    element: Box::new(ConcreteType::Tuple {
                        elements: vec![ConcreteType::Symbol, ConcreteType::Nothing],
                    }),
                })
            } else {
                // Check if all fields have the same type
                let first_type = &fields[0].1;
                let all_same = fields.iter().all(|(_, ty)| ty == first_type);
                let value_type = if all_same {
                    first_type.clone()
                } else {
                    // Different types - compute the union of all field types
                    compute_field_type_union(fields)
                };
                LatticeType::Concrete(ConcreteType::Generator {
                    element: Box::new(ConcreteType::Tuple {
                        elements: vec![ConcreteType::Symbol, value_type],
                    }),
                })
            }
        }
        LatticeType::Concrete(ConcreteType::Array { element }) => {
            // pairs of Array returns Generator of (Int64, element) tuples
            LatticeType::Concrete(ConcreteType::Generator {
                element: Box::new(ConcreteType::Tuple {
                    elements: vec![ConcreteType::Int64, *element.clone()],
                }),
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `haskey` (check if key exists).
///
/// Type rules:
/// - haskey(Dict, key) → Bool
/// - haskey(NamedTuple, Symbol) → Bool
///
/// # Examples
/// ```text
/// haskey(Dict{String,Int64}, "key") → Bool
/// ```
pub fn tfunc_haskey(_args: &[LatticeType]) -> LatticeType {
    // haskey always returns Bool
    LatticeType::Concrete(ConcreteType::Bool)
}

/// Transfer function for `get` (get with default).
///
/// Type rules:
/// - get(Dict{K,V}, key::K, default::D) → Union{V, D}
/// - get(NamedTuple, Symbol, default) → Union{Field type, default type}
///
/// # Examples
/// ```text
/// get(Dict{String,Int64}, "key", 0) → Int64
/// get(Dict{String,Int64}, "key", "default") → Union{Int64, String}
/// ```
pub fn tfunc_get(args: &[LatticeType]) -> LatticeType {
    if args.len() < 3 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Dict { value, .. }) => {
            // get returns Union{value_type, default_type}
            match &args[2] {
                LatticeType::Concrete(default_type) => {
                    // If value type and default type are the same, return that type
                    if &**value == default_type {
                        LatticeType::Concrete(default_type.clone())
                    } else {
                        // Otherwise, return Union
                        let mut union_types = BTreeSet::new();
                        union_types.insert(*value.clone());
                        union_types.insert(default_type.clone());
                        LatticeType::Union(union_types)
                    }
                }
                _ => LatticeType::Top,
            }
        }
        LatticeType::Concrete(ConcreteType::NamedTuple { .. }) => {
            // For NamedTuple, we would need to look up the field type
            // For simplicity, return Union of default type and Top
            LatticeType::Top
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `get!` (get or set default).
///
/// Type rules:
/// - get!(Dict{K,V}, key::K, default::V) → V
///
/// # Examples
/// ```text
/// get!(Dict{String,Int64}, "key", 0) → Int64
/// ```
pub fn tfunc_get_bang(args: &[LatticeType]) -> LatticeType {
    if args.len() < 3 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Dict { value, .. }) => {
            // get! returns the value type (or inserts default and returns it)
            LatticeType::Concrete(*value.clone())
        }
        _ => LatticeType::Top,
    }
}

// ============================================================================
// Extended Dictionary/Collection Functions
// ============================================================================

/// Transfer function for `delete!` (remove key from dictionary).
///
/// Type rules:
/// - delete!(Dict{K,V}, key) → Dict{K,V}
///
/// # Examples
/// ```text
/// delete!(Dict{String,Int64}, "key") → Dict{String,Int64}
/// ```
pub fn tfunc_delete(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // delete! returns the modified dictionary
    args[0].clone()
}

/// Transfer function for `merge` (merge dictionaries).
///
/// Type rules:
/// - merge(Dict{K,V}, Dict{K,V}...) → Dict{K,V}
///
/// # Examples
/// ```text
/// merge(Dict{String,Int64}, Dict{String,Int64}) → Dict{String,Int64}
/// ```
pub fn tfunc_merge(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    // merge returns a new dictionary with the same type as the first
    match &args[0] {
        LatticeType::Concrete(ConcreteType::Dict { key, value }) => {
            LatticeType::Concrete(ConcreteType::Dict {
                key: key.clone(),
                value: value.clone(),
            })
        }
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            // Merging named tuples returns a named tuple
            // For simplicity, return the first tuple's type
            // In full implementation, would merge field types
            LatticeType::Concrete(ConcreteType::NamedTuple {
                fields: fields.clone(),
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `merge!` (merge dictionaries in place).
///
/// Type rules:
/// - merge!(Dict{K,V}, Dict{K,V}...) → Dict{K,V}
///
/// # Examples
/// ```text
/// merge!(Dict{String,Int64}, Dict{String,Int64}) → Dict{String,Int64}
/// ```
pub fn tfunc_merge_bang(args: &[LatticeType]) -> LatticeType {
    tfunc_merge(args) // Same type rules
}

/// Transfer function for `isempty` (check if collection is empty).
///
/// Type rules:
/// - isempty(collection) → Bool
///
/// # Examples
/// ```text
/// isempty(Array{Int64}) → Bool
/// isempty(Dict{String,Int64}) → Bool
/// isempty("") → Bool
/// ```
pub fn tfunc_isempty(_args: &[LatticeType]) -> LatticeType {
    // isempty always returns Bool
    LatticeType::Concrete(ConcreteType::Bool)
}

/// Transfer function for `in` operator (membership check).
///
/// Type rules:
/// - in(element, collection) → Bool
/// - ∈(element, collection) → Bool
///
/// # Examples
/// ```text
/// in(1, [1,2,3]) → Bool
/// in("key", Dict{String,Int64}) → Bool
/// ```
pub fn tfunc_in(_args: &[LatticeType]) -> LatticeType {
    // in always returns Bool
    LatticeType::Concrete(ConcreteType::Bool)
}

/// Transfer function for `eltype` (element type of collection).
///
/// Type rules:
/// - eltype(Array{T}) → Type{T}
/// - eltype(Range{T}) → Type{T}
///
/// Note: Returns Top since we don't have a Type lattice type
pub fn tfunc_eltype(_args: &[LatticeType]) -> LatticeType {
    // eltype returns a Type, which we represent as Top
    LatticeType::Top
}

/// Transfer function for `keytype` (key type of dictionary).
///
/// Type rules:
/// - keytype(Dict{K,V}) → Type{K}
///
/// Note: Returns Top since we don't have a Type lattice type
pub fn tfunc_keytype(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Top
}

/// Transfer function for `valtype` (value type of dictionary).
///
/// Type rules:
/// - valtype(Dict{K,V}) → Type{V}
///
/// Note: Returns Top since we don't have a Type lattice type
pub fn tfunc_valtype(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Top
}

/// Transfer function for `Set` constructor.
///
/// Type rules:
/// - Set(iterable) → Set{T} where T is element type
/// - Set{T}() → Set{T}
///
/// # Examples
/// ```text
/// Set([1,2,3]) → Set{Int64}
/// ```
pub fn tfunc_set(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        // Empty set defaults to Set{Any}
        return LatticeType::Concrete(ConcreteType::Set {
            element: Box::new(ConcreteType::Any),
        });
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element }) => {
            LatticeType::Concrete(ConcreteType::Set {
                element: element.clone(),
            })
        }
        LatticeType::Concrete(ConcreteType::Range { element }) => {
            LatticeType::Concrete(ConcreteType::Set {
                element: element.clone(),
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Dict` constructor.
///
/// Type rules:
/// - Dict(pairs...) → Dict{K,V}
/// - Dict{K,V}() → Dict{K,V}
///
/// # Examples
/// ```text
/// Dict("a" => 1, "b" => 2) → Dict{String,Int64}
/// ```
pub fn tfunc_dict(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        // Empty dict defaults to Dict{Any,Any}
        return LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::Any),
            value: Box::new(ConcreteType::Any),
        });
    }

    // For pairs, try to infer key/value types
    // This is a simplified implementation
    LatticeType::Top
}

/// Transfer function for `union` (set union).
///
/// Type rules:
/// - union(Set{T}, Set{T}...) → Set{T}
///
/// # Examples
/// ```text
/// union(Set{Int64}, Set{Int64}) → Set{Int64}
/// ```
pub fn tfunc_union(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Set { element }) => {
            LatticeType::Concrete(ConcreteType::Set {
                element: element.clone(),
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `intersect` (set intersection).
///
/// Type rules:
/// - intersect(Set{T}, Set{T}...) → Set{T}
///
/// # Examples
/// ```text
/// intersect(Set{Int64}, Set{Int64}) → Set{Int64}
/// ```
pub fn tfunc_intersect(args: &[LatticeType]) -> LatticeType {
    tfunc_union(args) // Same type rules
}

/// Transfer function for `setdiff` (set difference).
///
/// Type rules:
/// - setdiff(Set{T}, Set{T}...) → Set{T}
///
/// # Examples
/// ```text
/// setdiff(Set{Int64}, Set{Int64}) → Set{Int64}
/// ```
pub fn tfunc_setdiff(args: &[LatticeType]) -> LatticeType {
    tfunc_union(args) // Same type rules
}

/// Transfer function for `symdiff` (symmetric difference).
///
/// Type rules:
/// - symdiff(Set{T}, Set{T}...) → Set{T}
///
/// # Examples
/// ```text
/// symdiff(Set{Int64}, Set{Int64}) → Set{Int64}
/// ```
pub fn tfunc_symdiff(args: &[LatticeType]) -> LatticeType {
    tfunc_union(args) // Same type rules
}

/// Transfer function for `issubset` (subset check).
///
/// Type rules:
/// - issubset(Set{T}, Set{T}) → Bool
/// - ⊆(Set{T}, Set{T}) → Bool
pub fn tfunc_issubset(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Bool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keys_dict() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let args = vec![dict];
        let result = tfunc_keys(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Set {
                element: Box::new(ConcreteType::String)
            })
        );
    }

    #[test]
    fn test_keys_named_tuple() {
        let named_tuple = LatticeType::Concrete(ConcreteType::NamedTuple {
            fields: vec![
                ("x".to_string(), ConcreteType::Int64),
                ("y".to_string(), ConcreteType::Float64),
            ],
        });
        let args = vec![named_tuple];
        let result = tfunc_keys(&args);
        assert!(matches!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple { .. })
        ));
    }

    #[test]
    fn test_values_dict() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let args = vec![dict];
        let result = tfunc_values(&args);
        assert!(matches!(
            result,
            LatticeType::Concrete(ConcreteType::Generator { .. })
        ));
    }

    #[test]
    fn test_pairs_array() {
        let array = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Float64),
        });
        let args = vec![array];
        let result = tfunc_pairs(&args);
        assert!(matches!(
            result,
            LatticeType::Concrete(ConcreteType::Generator { .. })
        ));
    }

    #[test]
    fn test_haskey() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let key = LatticeType::Concrete(ConcreteType::String);
        let args = vec![dict, key];
        let result = tfunc_haskey(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_get_same_types() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let key = LatticeType::Concrete(ConcreteType::String);
        let default = LatticeType::Concrete(ConcreteType::Int64);
        let args = vec![dict, key, default];
        let result = tfunc_get(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_get_different_types() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let key = LatticeType::Concrete(ConcreteType::String);
        let default = LatticeType::Concrete(ConcreteType::String);
        let args = vec![dict, key, default];
        let result = tfunc_get(&args);
        assert!(matches!(result, LatticeType::Union(_)));
    }

    #[test]
    fn test_get_bang() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let key = LatticeType::Concrete(ConcreteType::String);
        let default = LatticeType::Concrete(ConcreteType::Int64);
        let args = vec![dict, key, default];
        let result = tfunc_get_bang(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_pairs_named_tuple_homogeneous() {
        // NamedTuple with all Int64 fields
        let named_tuple = LatticeType::Concrete(ConcreteType::NamedTuple {
            fields: vec![
                ("a".to_string(), ConcreteType::Int64),
                ("b".to_string(), ConcreteType::Int64),
            ],
        });
        let args = vec![named_tuple];
        let result = tfunc_pairs(&args);

        // Should return Generator{Tuple{Symbol, Int64}}
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
                assert_eq!(elements[0], ConcreteType::Symbol);
                assert_eq!(elements[1], ConcreteType::Int64);
            }
        }
    }

    #[test]
    fn test_pairs_named_tuple_heterogeneous_numeric() {
        // NamedTuple with mixed numeric types (Int64, Float64)
        let named_tuple = LatticeType::Concrete(ConcreteType::NamedTuple {
            fields: vec![
                ("x".to_string(), ConcreteType::Int64),
                ("y".to_string(), ConcreteType::Float64),
            ],
        });
        let args = vec![named_tuple];
        let result = tfunc_pairs(&args);

        // Should return Generator type (not Top!) with Union{Int64, Float64}
        // Issue #1637: Now preserves Union types instead of collapsing to Float64
        assert!(
            !matches!(&result, LatticeType::Top),
            "Should not return Top for heterogeneous numeric NamedTuple"
        );
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
                assert_eq!(elements[0], ConcreteType::Symbol);
                // Should be UnionOf(Int64, Float64) - preserving type information
                assert!(
                    matches!(&elements[1], ConcreteType::UnionOf(_)),
                    "Expected UnionOf, got {:?}",
                    &elements[1]
                );
                if let ConcreteType::UnionOf(types) = &elements[1] {
                    assert_eq!(types.len(), 2);
                    assert!(types.contains(&ConcreteType::Int64));
                    assert!(types.contains(&ConcreteType::Float64));
                }
            }
        }
    }

    #[test]
    fn test_pairs_named_tuple_empty() {
        let named_tuple = LatticeType::Concrete(ConcreteType::NamedTuple { fields: vec![] });
        let args = vec![named_tuple];
        let result = tfunc_pairs(&args);

        // Should return Generator{Tuple{Symbol, Nothing}}
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
                assert_eq!(elements[0], ConcreteType::Symbol);
                assert_eq!(elements[1], ConcreteType::Nothing);
            }
        }
    }

    #[test]
    fn test_delete() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let key = LatticeType::Concrete(ConcreteType::String);
        let args = vec![dict.clone(), key];
        let result = tfunc_delete(&args);
        assert_eq!(result, dict);
    }

    #[test]
    fn test_merge() {
        let dict = LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::String),
            value: Box::new(ConcreteType::Int64),
        });
        let args = vec![dict.clone(), dict.clone()];
        let result = tfunc_merge(&args);
        assert_eq!(result, dict);
    }

    #[test]
    fn test_isempty() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        })];
        let result = tfunc_isempty(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_in() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Int64),
            }),
        ];
        let result = tfunc_in(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_set_constructor() {
        let array = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        });
        let args = vec![array];
        let result = tfunc_set(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Set {
                element: Box::new(ConcreteType::Int64)
            })
        );
    }

    #[test]
    fn test_union_sets() {
        let set = LatticeType::Concrete(ConcreteType::Set {
            element: Box::new(ConcreteType::Int64),
        });
        let args = vec![set.clone(), set.clone()];
        let result = tfunc_union(&args);
        assert_eq!(result, set);
    }

    #[test]
    fn test_issubset() {
        let set = LatticeType::Concrete(ConcreteType::Set {
            element: Box::new(ConcreteType::Int64),
        });
        let args = vec![set.clone(), set];
        let result = tfunc_issubset(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }
}
