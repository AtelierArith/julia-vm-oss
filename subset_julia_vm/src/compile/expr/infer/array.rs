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
/// - All String -> String array
/// - All Char -> Char array
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

    let all_string = elem_types.iter().all(|ty| matches!(ty, ValueType::Str));
    if all_string {
        return (ArrayElementType::String, ValueType::Str);
    }

    let all_char = elem_types.iter().all(|ty| matches!(ty, ValueType::Char));
    if all_char {
        return (ArrayElementType::Char, ValueType::Char);
    }

    let all_symbol = elem_types.iter().all(|ty| matches!(ty, ValueType::Symbol));
    if all_symbol {
        return (ArrayElementType::Symbol, ValueType::Symbol);
    }

    let all_complex_scalar = elem_types
        .iter()
        .all(|ty| matches!(ty, ValueType::ComplexF32 | ValueType::ComplexF64));
    if all_complex_scalar {
        if elem_types
            .iter()
            .any(|ty| matches!(ty, ValueType::ComplexF64))
        {
            return (ArrayElementType::ComplexF64, ValueType::ComplexF64);
        }
        return (ArrayElementType::ComplexF32, ValueType::ComplexF32);
    }

    // Check if all elements are the same struct type
    if let ValueType::Struct(first_id) = &elem_types[0] {
        let first_name = struct_name_lookup(*first_id);
        let all_same_struct = elem_types
            .iter()
            .all(|ty| matches!(ty, ValueType::Struct(id) if id == first_id));
        if all_same_struct {
            if matches!(
                first_name.as_deref(),
                Some("Complex{Float64}" | "ComplexF64")
            ) {
                return (ArrayElementType::ComplexF64, ValueType::ComplexF64);
            }
            if matches!(
                first_name.as_deref(),
                Some("Complex{Float32}" | "ComplexF32")
            ) {
                return (ArrayElementType::ComplexF32, ValueType::ComplexF32);
            }
            return (
                ArrayElementType::StructOf(*first_id),
                ValueType::Struct(*first_id),
            );
        }

        // Check for Rational type promotion: all Rational{T} for possibly different T
        // should promote to Rational{promoted_T}
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

    // Issue #6867: a mix of `Complex{T}` and `Real` elements (e.g.
    // `[1.0+0.0im, 2.0]`) promotes to `Complex{promote_type(T, R...)}`, matching
    // upstream `promote_typeof`. Without this the literal fell through to `Any`,
    // so `Complex{Float64}`-specialized methods (e.g. `norm`) never dispatched.
    // The all-`Complex` cases are already handled above; this fills the
    // Complex×Real gap left by #6851 (same-kind only).
    if let Some(result) =
        infer_mixed_complex_real_element_type(elem_types, &struct_name_lookup, &struct_id_lookup)
    {
        return result;
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

    // Issue #3549: heterogeneous arrays of `T` plus `Nothing` (or `Missing`)
    // should report `Vector{Union{Nothing, T}}` (or `Vector{Union{Missing, T}}`),
    // not `Vector{Any}`. Detect the common cases and emit a `UnionOf` element
    // type with a pre-rendered display body. Single-variant cases (e.g.
    // `[nothing]` or `[missing]`) are handled by the existing fallthrough so
    // they keep their plain `Vector{Nothing}` / `Vector{Missing}` printing
    // behaviour.
    if let Some(body) = compute_union_display(elem_types) {
        if body.contains(',') {
            return (ArrayElementType::union_from_body(&body), ValueType::Any);
        }
    }

    // Heterogeneous array
    (ArrayElementType::Any, ValueType::Any)
}

/// Julia type-name of a `ValueType` that is either a numeric `Real` primitive or
/// a `Complex{T}` struct, used for the Complex×Real literal promotion
/// (Issue #6867). Returns `None` for any value type that is neither (so the
/// caller falls back to the existing `Any` widening).
fn complex_or_real_promotion_name<F>(ty: &ValueType, struct_name_lookup: &F) -> Option<String>
where
    F: Fn(usize) -> Option<String>,
{
    let name = match ty {
        ValueType::Bool => "Bool",
        ValueType::I8 => "Int8",
        ValueType::I16 => "Int16",
        ValueType::I32 => "Int32",
        ValueType::I64 => "Int64",
        ValueType::I128 => "Int128",
        ValueType::U8 => "UInt8",
        ValueType::U16 => "UInt16",
        ValueType::U32 => "UInt32",
        ValueType::U64 => "UInt64",
        ValueType::U128 => "UInt128",
        ValueType::F16 => "Float16",
        ValueType::F32 => "Float32",
        ValueType::F64 => "Float64",
        ValueType::ComplexF32 => "Complex{Float32}",
        ValueType::ComplexF64 => "Complex{Float64}",
        ValueType::Struct(id) => {
            let name = struct_name_lookup(*id)?;
            return name.starts_with("Complex").then_some(name);
        }
        _ => return None,
    };
    Some(name.to_string())
}

/// Infer the storage element type for a `Complex{T}` × `Real` mixed array literal
/// (Issue #6867), reducing the element types with Julia's `promote_type` /
/// Complex `promote_rule`. Returns `None` (caller falls back to `Any`) unless at
/// least one element is `Complex` AND at least one is a non-`Complex` real, and
/// every element is a numeric primitive or `Complex{T}`. The pure all-`Complex`
/// literals are handled by earlier branches.
fn infer_mixed_complex_real_element_type<F, G>(
    elem_types: &[ValueType],
    struct_name_lookup: &F,
    struct_id_lookup: &G,
) -> Option<(ArrayElementType, ValueType)>
where
    F: Fn(usize) -> Option<String>,
    G: Fn(&str) -> Option<usize>,
{
    use crate::compile::promotion::{extract_complex_param, promote_complex, promote_type};

    let mut has_complex = false;
    let mut has_real = false;
    let mut promoted: Option<String> = None;
    for ty in elem_types {
        let name = complex_or_real_promotion_name(ty, struct_name_lookup)?;
        let is_complex = name.starts_with("Complex");
        has_complex |= is_complex;
        has_real |= !is_complex;
        promoted = Some(match promoted {
            None => name,
            Some(acc) => {
                if acc.starts_with("Complex") || is_complex {
                    promote_complex(&acc, &name)
                } else {
                    promote_type(&acc, &name)
                }
            }
        });
    }

    if !(has_complex && has_real) {
        return None;
    }
    let promoted = promoted?;
    // The reduction must land on a concrete `Complex{T}`; anything else (e.g.
    // a promotion that widened to `Any`) falls back to the existing behaviour.
    let inner = extract_complex_param(&promoted)?;
    match inner.as_str() {
        "Float64" => Some((ArrayElementType::ComplexF64, ValueType::ComplexF64)),
        "Float32" => Some((ArrayElementType::ComplexF32, ValueType::ComplexF32)),
        // Integer Complex (`Complex{Int64}`, ...) has no inline interleaved
        // storage tag; route through the struct-backed array path, which already
        // promotes real elements via the `Complex{T}(n, 0)` constructor. When the
        // promoted name isn't directly registered, reuse the `type_id` of an
        // existing `Complex` element of the same parameter (mirrors the
        // all-`Complex` branch's first-element fallback).
        _ => {
            let id = struct_id_lookup(&promoted).or_else(|| {
                elem_types.iter().find_map(|ty| match ty {
                    ValueType::Struct(id)
                        if struct_name_lookup(*id).as_deref() == Some(promoted.as_str()) =>
                    {
                        Some(*id)
                    }
                    _ => None,
                })
            })?;
            Some((ArrayElementType::StructOf(id), ValueType::Struct(id)))
        }
    }
}

pub(crate) fn infer_nested_array_literal_element_type(
    elem_types: &[ValueType],
    ranks: &[usize],
) -> Option<ArrayElementType> {
    if elem_types.len() != ranks.len() {
        return None;
    }
    let ValueType::ArrayOf(first, _) = elem_types.first()? else {
        return None;
    };
    let first_rank = *ranks.first()?;
    let homogeneous = elem_types
        .iter()
        .all(|ty| matches!(ty, ValueType::ArrayOf(inner, _) if inner == first));
    let same_rank = ranks.iter().all(|rank| *rank == first_rank);
    (homogeneous && same_rank)
        .then(|| ArrayElementType::Abstract(nested_array_type_name(first, first_rank)))
}

fn nested_array_type_name(element_type: &ArrayElementType, rank: usize) -> String {
    match rank {
        1 => format!("Vector{{{}}}", element_type.julia_type_name()),
        2 => format!("Matrix{{{}}}", element_type.julia_type_name()),
        n => format!("Array{{{}, {}}}", element_type.julia_type_name(), n),
    }
}

/// If the element types are a mix of one or more concrete simple types and
/// `Nothing`/`Missing`, return the inner display body for `Union{...}` matching
/// official Julia's display order. Multiple concrete numeric types are reduced
/// via numeric promotion so that `[1, nothing, 2.5]` prints as
/// `Vector{Union{Nothing, Float64}}` (Issue #3558).
///
/// Returns `None` for cases that should fall back to `Any` (e.g. mixing
/// non-promotable types like `Int64` + `String`).
fn compute_union_display(elem_types: &[ValueType]) -> Option<String> {
    fn type_name(ty: &ValueType) -> Option<&'static str> {
        match ty {
            ValueType::I64 => Some("Int64"),
            ValueType::I32 => Some("Int32"),
            ValueType::I16 => Some("Int16"),
            ValueType::I8 => Some("Int8"),
            ValueType::U64 => Some("UInt64"),
            ValueType::U32 => Some("UInt32"),
            ValueType::U16 => Some("UInt16"),
            ValueType::U8 => Some("UInt8"),
            ValueType::F64 => Some("Float64"),
            ValueType::F32 => Some("Float32"),
            ValueType::Bool => Some("Bool"),
            ValueType::Str => Some("String"),
            ValueType::Char => Some("Char"),
            _ => None,
        }
    }

    /// Numeric promotion among the simple `ValueType` numerics tracked by
    /// `type_name`. `Bool` participates as the smallest integer width.
    /// Returns `None` if either side isn't promotable.
    fn promote_numeric(a: &ValueType, b: &ValueType) -> Option<ValueType> {
        // Float64 wins over Float32 wins over any integer.
        let is_f64 = |t: &ValueType| matches!(t, ValueType::F64);
        let is_f32 = |t: &ValueType| matches!(t, ValueType::F32);
        let is_int = |t: &ValueType| {
            matches!(
                t,
                ValueType::I64
                    | ValueType::I32
                    | ValueType::I16
                    | ValueType::I8
                    | ValueType::U64
                    | ValueType::U32
                    | ValueType::U16
                    | ValueType::U8
                    | ValueType::Bool
            )
        };

        if is_f64(a) || is_f64(b) {
            if (is_f64(a) || is_f64(b) || is_f32(a) || is_f32(b) || is_int(a) || is_int(b))
                && (is_f64(a) || is_f32(a) || is_int(a))
                && (is_f64(b) || is_f32(b) || is_int(b))
            {
                return Some(ValueType::F64);
            }
            return None;
        }
        if is_f32(a) || is_f32(b) {
            if (is_f32(a) || is_int(a)) && (is_f32(b) || is_int(b)) {
                return Some(ValueType::F32);
            }
            return None;
        }
        if is_int(a) && is_int(b) {
            // For pure-integer mixes we keep `Int64` as the conservative
            // promotion (matches most subset-VM code paths). Mixing two
            // identical integer types leaves them unchanged.
            if a == b {
                return Some(a.clone());
            }
            return Some(ValueType::I64);
        }
        None
    }

    let mut has_nothing = false;
    let mut has_missing = false;
    let mut promoted: Option<ValueType> = None;
    let mut non_numeric_concrete: Option<&'static str> = None;
    for ty in elem_types {
        match ty {
            ValueType::Nothing => has_nothing = true,
            ValueType::Missing => has_missing = true,
            other => {
                // Bail early if `type_name` can't render this simple type.
                let _name = type_name(other)?;
                // Try to keep the running promotion across numerics; for
                // non-numeric simple types (Str/Char/Bool-as-string) we only
                // accept a single distinct value to avoid cross-domain mixes.
                let is_numeric = matches!(
                    other,
                    ValueType::I64
                        | ValueType::I32
                        | ValueType::I16
                        | ValueType::I8
                        | ValueType::U64
                        | ValueType::U32
                        | ValueType::U16
                        | ValueType::U8
                        | ValueType::F64
                        | ValueType::F32
                );
                if is_numeric {
                    if non_numeric_concrete.is_some() {
                        return None;
                    }
                    promoted = Some(match promoted {
                        None => other.clone(),
                        Some(prev) => promote_numeric(&prev, other)?,
                    });
                } else {
                    if promoted.is_some() {
                        return None;
                    }
                    let name = type_name(other)?;
                    match non_numeric_concrete {
                        None => non_numeric_concrete = Some(name),
                        Some(prev) if prev == name => {}
                        Some(_) => return None,
                    }
                }
            }
        }
    }

    if !has_nothing && !has_missing {
        return None;
    }

    // Order matches Julia's display: Missing first (it sorts before Nothing
    // alphabetically), then Nothing, then the concrete type.
    let mut parts: Vec<String> = Vec::new();
    if has_missing {
        parts.push("Missing".to_string());
    }
    if has_nothing {
        parts.push("Nothing".to_string());
    }
    if let Some(p) = promoted {
        parts.push(type_name(&p)?.to_string());
    } else if let Some(name) = non_numeric_concrete {
        parts.push(name.to_string());
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper closures for tests: no struct types registered
    fn no_structs(_id: usize) -> Option<String> {
        None
    }
    fn no_struct_ids(_name: &str) -> Option<usize> {
        None
    }

    // ── infer_array_element_type ──────────────────────────────────────────────

    #[test]
    fn test_empty_array_returns_any() {
        let (elem_ty, val_ty) = infer_array_element_type(&[], no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::Any),
            "Expected Any for empty array, got {:?}",
            elem_ty
        );
        assert!(matches!(val_ty, ValueType::Any));
    }

    #[test]
    fn test_all_i64_returns_i64_array() {
        let types = vec![ValueType::I64, ValueType::I64];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::I64),
            "Expected I64 array for all I64, got {:?}",
            elem_ty
        );
        assert!(matches!(val_ty, ValueType::I64));
    }

    #[test]
    fn test_all_f64_returns_f64_array() {
        let types = vec![ValueType::F64, ValueType::F64];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::F64),
            "Expected F64 array for all F64, got {:?}",
            elem_ty
        );
        assert!(matches!(val_ty, ValueType::F64));
    }

    #[test]
    fn test_mixed_i64_and_f64_promotes_to_f64() {
        let types = vec![ValueType::I64, ValueType::F64];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::F64),
            "Expected F64 array for mixed I64/F64, got {:?}",
            elem_ty
        );
        assert!(matches!(val_ty, ValueType::F64));
    }

    #[test]
    fn test_all_bool_returns_bool_array() {
        let types = vec![ValueType::Bool, ValueType::Bool];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::Bool),
            "Expected Bool array for all Bool, got {:?}",
            elem_ty
        );
        assert!(matches!(val_ty, ValueType::Bool));
    }

    #[test]
    fn test_homogeneous_nested_vector_preserves_element_type_issue_6225() {
        let types = vec![
            ValueType::ArrayOf(ArrayElementType::I64, None),
            ValueType::ArrayOf(ArrayElementType::I64, None),
        ];
        let elem_ty = infer_nested_array_literal_element_type(&types, &[1, 1]).unwrap();
        assert_eq!(
            elem_ty,
            ArrayElementType::Abstract("Vector{Int64}".to_string())
        );
    }

    #[test]
    fn test_homogeneous_nested_matrix_preserves_rank_issue_6227() {
        let types = vec![
            ValueType::ArrayOf(ArrayElementType::I64, None),
            ValueType::ArrayOf(ArrayElementType::I64, None),
        ];
        let elem_ty = infer_nested_array_literal_element_type(&types, &[2, 2]).unwrap();
        assert_eq!(
            elem_ty,
            ArrayElementType::Abstract("Matrix{Int64}".to_string())
        );
    }

    #[test]
    fn test_heterogeneous_types_returns_any() {
        // I64 + Str → Any
        let types = vec![ValueType::I64, ValueType::Str];
        let (elem_ty, val_ty) = infer_array_element_type(&types, no_structs, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::Any),
            "Expected Any for heterogeneous types, got {:?}",
            elem_ty
        );
        assert!(matches!(val_ty, ValueType::Any));
    }

    #[test]
    fn test_all_same_struct_returns_struct_of() {
        // All elements of type_id=5 → StructOf(5)
        let types = vec![ValueType::Struct(5), ValueType::Struct(5)];
        let struct_name_lookup = |id: usize| {
            if id == 5 {
                Some("Foo".to_string())
            } else {
                None
            }
        };
        let (elem_ty, val_ty) = infer_array_element_type(&types, struct_name_lookup, no_struct_ids);
        assert!(
            matches!(elem_ty, ArrayElementType::StructOf(5)),
            "Expected StructOf(5), got {:?}",
            elem_ty
        );
        assert!(matches!(val_ty, ValueType::Struct(5)));
    }
}
