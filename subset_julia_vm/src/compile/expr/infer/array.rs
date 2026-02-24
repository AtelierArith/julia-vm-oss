//! Array element type inference.
//!
//! Infers the appropriate storage type for array elements based on their value types.
//! Handles type promotion rules (e.g., mixed Int64/Float64 → Float64).

use crate::vm::{ArrayElementType, ValueType};

/// Infer the appropriate array element type based on the element value types.
///
/// Returns (ArrayElementType, ValueType) tuple:
/// - ArrayElementType: The storage type for the array
/// - ValueType: The target type for compiling elements
///
/// Rules:
/// - All I64 (integers) -> I64 array
/// - All numeric with at least one float -> F64 array (type promotion)
/// - All same struct type -> StructOf array
/// - Mix of integers and Rational -> Rational array (type promotion)
/// - Mixed or non-numeric -> Any array
///
/// The `struct_name_lookup` parameter allows looking up struct names by type_id
/// to enable type promotion for specific struct types like Rational and Complex.
/// The `struct_id_lookup` parameter allows looking up type_id by struct name
/// (e.g., "Complex{Int64}" -> type_id), needed for Complex promotion.
pub(crate) fn infer_array_element_type<F, G>(
    elem_types: &[ValueType],
    struct_name_lookup: F,
    struct_id_lookup: G,
) -> (ArrayElementType, ValueType)
where
    F: Fn(usize) -> Option<String>,
    G: Fn(&str) -> Option<usize>,
{
    if elem_types.is_empty() {
        // Empty array defaults to Any
        return (ArrayElementType::Any, ValueType::Any);
    }

    // Check for all-Bool first (before all-I64 since Bool can be used where I64 is expected)
    let all_bool = elem_types.iter().all(|ty| matches!(ty, ValueType::Bool));
    if all_bool {
        return (ArrayElementType::Bool, ValueType::Bool);
    }

    // Check if all elements are the same struct type
    if let ValueType::Struct(first_id) = &elem_types[0] {
        let all_same_struct = elem_types
            .iter()
            .all(|ty| matches!(ty, ValueType::Struct(id) if id == first_id));
        if all_same_struct {
            return (ArrayElementType::StructOf(*first_id), ValueType::Struct(*first_id));
        }

        // Check for Rational type promotion: all Rational{T} for possibly different T
        // should promote to Rational{promoted_T}
        let first_name = struct_name_lookup(*first_id);
        let all_rational = first_name
            .as_ref()
            .map(|n| n.starts_with("Rational"))
            .unwrap_or(false)
            && elem_types.iter().all(|ty| {
                if let ValueType::Struct(id) = ty {
                    struct_name_lookup(*id)
                        .map(|n| n.starts_with("Rational"))
                        .unwrap_or(false)
                } else {
                    false
                }
            });

        if all_rational {
            // All Rational - use the first one's type_id (promotion happens at runtime)
            return (
                ArrayElementType::StructOf(*first_id),
                ValueType::Struct(*first_id),
            );
        }

        // Check for Complex type promotion: all Complex{T} for possibly different T
        let all_complex = first_name
            .as_ref()
            .map(|n| n.starts_with("Complex"))
            .unwrap_or(false)
            && elem_types.iter().all(|ty| {
                if let ValueType::Struct(id) = ty {
                    struct_name_lookup(*id)
                        .map(|n| n.starts_with("Complex"))
                        .unwrap_or(false)
                } else {
                    false
                }
            });

        if all_complex {
            // All Complex - promote to the widest type
            // Collect all element type names to find the promoted type
            let elem_type_names: Vec<String> = elem_types
                .iter()
                .filter_map(|ty| {
                    if let ValueType::Struct(id) = ty {
                        struct_name_lookup(*id)
                    } else {
                        None
                    }
                })
                .collect();

            // Find the widest Complex type (Complex{Float64} > Complex{Int64} etc.)
            let has_float64 = elem_type_names.iter().any(|n| n.contains("Float64"));
            let has_float32 = elem_type_names.iter().any(|n| n.contains("Float32"));

            let promoted_name = if has_float64 {
                "Complex{Float64}"
            } else if has_float32 {
                "Complex{Float32}"
            } else {
                // Default to Complex{Int64} for integer complex types
                "Complex{Int64}"
            };

            // Look up the promoted type_id
            if let Some(type_id) = struct_id_lookup(promoted_name) {
                return (
                    ArrayElementType::StructOf(type_id),
                    ValueType::Struct(type_id),
                );
            }
            // Fallback to first element's type
            return (
                ArrayElementType::StructOf(*first_id),
                ValueType::Struct(*first_id),
            );
        }
    }

    // Check for mixed integers and Rational: promote to Rational
    let has_rational = elem_types.iter().any(|ty| {
        if let ValueType::Struct(id) = ty {
            struct_name_lookup(*id)
                .map(|n| n.starts_with("Rational"))
                .unwrap_or(false)
        } else {
            false
        }
    });
    let all_int_or_rational = elem_types.iter().all(|ty| match ty {
        ValueType::I64 => true,
        ValueType::Struct(id) => struct_name_lookup(*id)
            .map(|n| n.starts_with("Rational"))
            .unwrap_or(false),
        _ => false,
    });
    if has_rational && all_int_or_rational {
        // Find the Rational type_id
        if let Some(rational_id) = elem_types.iter().find_map(|ty| {
            if let ValueType::Struct(id) = ty {
                if struct_name_lookup(*id)
                    .map(|n| n.starts_with("Rational"))
                    .unwrap_or(false)
                {
                    Some(*id)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            return (
                ArrayElementType::StructOf(rational_id),
                ValueType::Struct(rational_id),
            );
        }
    }

    // Check if all elements are I64 (integers)
    let all_i64 = elem_types.iter().all(|ty| matches!(ty, ValueType::I64));
    if all_i64 {
        return (ArrayElementType::I64, ValueType::I64);
    }

    // Check if all elements are numeric (I64 or F64)
    let all_numeric = elem_types
        .iter()
        .all(|ty| matches!(ty, ValueType::I64 | ValueType::F64));
    if all_numeric {
        // Type promotion: if any float, use F64
        return (ArrayElementType::F64, ValueType::F64);
    }

    // Heterogeneous array
    (ArrayElementType::Any, ValueType::Any)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper closures for tests: no struct types registered
    fn no_structs(_id: usize) -> Option<String> { None }
    fn no_struct_ids(_name: &str) -> Option<usize> { None }

    // ── infer_array_element_type ──────────────────────────────────────────────

    #[test]
    fn test_empty_array_returns_any() {
        let (elem_ty, val_ty) = infer_array_element_type(&[], no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::Any),
            "Expected Any for empty array, got {:?}", elem_ty
        );
        assert!(matches!(val_ty, ValueType::Any));
    }

    #[test]
    fn test_all_i64_returns_i64_array() {
        let types = vec![ValueType::I64, ValueType::I64];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::I64),
            "Expected I64 array for all I64, got {:?}", elem_ty
        );
        assert!(matches!(val_ty, ValueType::I64));
    }

    #[test]
    fn test_all_f64_returns_f64_array() {
        let types = vec![ValueType::F64, ValueType::F64];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::F64),
            "Expected F64 array for all F64, got {:?}", elem_ty
        );
        assert!(matches!(val_ty, ValueType::F64));
    }

    #[test]
    fn test_mixed_i64_and_f64_promotes_to_f64() {
        let types = vec![ValueType::I64, ValueType::F64];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::F64),
            "Expected F64 array for mixed I64/F64, got {:?}", elem_ty
        );
        assert!(matches!(val_ty, ValueType::F64));
    }

    #[test]
    fn test_all_bool_returns_bool_array() {
        let types = vec![ValueType::Bool, ValueType::Bool];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::Bool),
            "Expected Bool array for all Bool, got {:?}", elem_ty
        );
        assert!(matches!(val_ty, ValueType::Bool));
    }

    #[test]
    fn test_heterogeneous_types_returns_any() {
        // I64 + Str → Any
        let types = vec![ValueType::I64, ValueType::Str];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::Any),
            "Expected Any for heterogeneous types, got {:?}", elem_ty
        );
        assert!(matches!(val_ty, ValueType::Any));
    }

    #[test]
    fn test_all_same_struct_returns_struct_of() {
        // All elements of type_id=5 → StructOf(5)
        let types = vec![ValueType::Struct(5), ValueType::Struct(5)];
        let struct_name_lookup = |id: usize| if id == 5 { Some("Foo".to_string()) } else { None };
        let (elem_ty, val_ty) = infer_array_element_type(&types, struct_name_lookup, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::StructOf(5)),
            "Expected StructOf(5), got {:?}", elem_ty
        );
        assert!(matches!(val_ty, ValueType::Struct(5)));
    }
}
