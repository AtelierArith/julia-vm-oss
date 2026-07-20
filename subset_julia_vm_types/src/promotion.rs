//! Type promotion following Julia's promote_rule/promote_type pattern.
//!
//! Julia's type promotion is a three-layer system:
//! 1. `promote_rule(T, S)` - Basic rules defined per type pair
//! 2. `promote_type(T, S)` - Tries promote_rule both ways and combines results
//! 3. `promote(x, y)` - Converts values to the common type
//!
//! This module implements the equivalent for SubsetJuliaVM's shared compile-time
//! and runtime type queries.
//!
//! ## Architecture
//!
//! Promotion rules can come from two sources:
//! 1. **Julia definitions** (primary): Rules defined in `subset_julia_vm/src/julia/base/promotion.jl`
//!    are extracted at compile time and stored in a thread-local registry.
//! 2. **Rust fallback** (secondary): If a rule is not found in the Julia registry, the Rust
//!    implementation provides a fallback based on type priority.
//!
//! This design ensures:
//! - Julia code is the source of truth for promotion rules
//! - Users can extend promotion rules by adding Julia methods
//! - Rust provides sensible defaults for bootstrapping and unknown types
//!
//! Reference: julia/base/promotion.jl, julia/base/complex.jl, julia/base/bool.jl

use std::cell::RefCell;
use std::collections::HashMap;

// =============================================================================
// Promotion Rule Registry (populated from Julia definitions)
// =============================================================================

// Thread-local registry of promotion rules extracted from Julia definitions.
// Key: (Type1, Type2) tuple, Value: Result type
thread_local! {
    static PROMOTION_RULE_REGISTRY: RefCell<HashMap<(String, String), String>> = RefCell::new(HashMap::new());
    static REGISTRY_INITIALIZED: RefCell<bool> = const { RefCell::new(false) };
}

/// Register a promotion rule from Julia definitions.
/// Called during Base compilation when `promote_rule` methods are encountered.
pub fn register_promotion_rule(type1: &str, type2: &str, result: &str) {
    PROMOTION_RULE_REGISTRY.with(|registry| {
        let mut reg = registry.borrow_mut();
        reg.insert((type1.to_string(), type2.to_string()), result.to_string());
    });
}

/// Mark the registry as initialized (called after Base compilation completes).
pub fn mark_registry_initialized() {
    REGISTRY_INITIALIZED.with(|init| {
        *init.borrow_mut() = true;
    });
}

/// Check if the registry has been initialized.
pub fn is_registry_initialized() -> bool {
    REGISTRY_INITIALIZED.with(|init| *init.borrow())
}

/// Look up a promotion rule from the Julia-defined registry.
/// Returns None if not found (will fall back to Rust implementation).
fn lookup_promotion_rule(type1: &str, type2: &str) -> Option<String> {
    PROMOTION_RULE_REGISTRY.with(|registry| {
        let reg = registry.borrow();
        reg.get(&(type1.to_string(), type2.to_string())).cloned()
    })
}

/// Get the number of registered promotion rules (for debugging/testing).
pub fn get_registry_size() -> usize {
    PROMOTION_RULE_REGISTRY.with(|registry| registry.borrow().len())
}

/// Get all registered promotion rules as (type1, type2, result) tuples.
/// Used when serializing the Base cache to embed promotion rules (Issue #3025).
pub fn get_all_promotion_rules() -> Vec<(String, String, String)> {
    PROMOTION_RULE_REGISTRY.with(|registry| {
        registry
            .borrow()
            .iter()
            .map(|((t1, t2), ret)| (t1.clone(), t2.clone(), ret.clone()))
            .collect()
    })
}

/// Clear the promotion rule registry (for testing).
pub fn clear_registry() {
    PROMOTION_RULE_REGISTRY.with(|registry| registry.borrow_mut().clear());
    REGISTRY_INITIALIZED.with(|init| *init.borrow_mut() = false);
}

// =============================================================================
// Type Priority (Rust fallback)
// =============================================================================

// Issue #6735: the hardcoded numeric priority table (`type_priority`) was removed.
// `promote_type` now delegates to the registered `promote_rule` network (the
// thread-local registry populated from base/promotion.jl) and, only as a
// cache-less bootstrap fallback, to the shared `inference_core::PrimitiveNumeric`
// taxonomy and the explicit Bool/Complex/Big rules in `promote_rule_fallback`.

/// Check if a type is a floating-point type.
///
/// Issue #3508 — the canonical primitive numeric taxonomy lives in
/// [`crate::inference_core::PrimitiveNumeric`]. This wrapper forwards there
/// so the VM-side and AoT-side classifiers stay in lock-step.
pub fn is_float_type_name(ty: &str) -> bool {
    crate::inference_core::PrimitiveNumeric::from_julia_name(ty).is_some_and(|p| p.is_float())
}

/// Check if a type is an integer type. Delegates to the shared
/// [`crate::inference_core::PrimitiveNumeric`] taxonomy (Issue #3508).
pub fn is_integer_type_name(ty: &str) -> bool {
    crate::inference_core::PrimitiveNumeric::from_julia_name(ty).is_some_and(|p| p.is_integer())
}

/// Check if a type is numeric (float or integer). Delegates to the shared
/// [`crate::inference_core::PrimitiveNumeric`] taxonomy (Issue #3508).
pub fn is_numeric_type_name(ty: &str) -> bool {
    crate::inference_core::PrimitiveNumeric::from_julia_name(ty).is_some()
}

/// Extract the type parameter from a Complex type name.
/// e.g., "Complex{Float64}" -> Some("Float64")
///       "Int64" -> None
pub fn extract_complex_param(name: &str) -> Option<String> {
    if name.starts_with("Complex{") && name.ends_with('}') {
        Some(name[8..name.len() - 1].to_string())
    } else {
        None
    }
}

fn extract_rational_param(name: &str) -> Option<String> {
    if name.starts_with("Rational{") && name.ends_with('}') {
        Some(name["Rational{".len()..name.len() - 1].to_string())
    } else {
        None
    }
}

/// Check if a type name represents a Complex type.
pub fn is_complex_type(name: &str) -> bool {
    name.starts_with("Complex{") && name.ends_with('}')
}

// `promote_rule` was removed in favour of inlining `lookup_promotion_rule` and
// `promote_rule_fallback` directly inside `promote_type` (Issue #3742): the
// fallback must not preempt the registered rule in the OPPOSITE direction, so
// callers need to consult the registry both ways before falling back at all.

/// Rust fallback implementation of promote_rule.
/// Used when Julia registry doesn't have a rule (bootstrapping or unknown types).
fn promote_rule_fallback(t1: &str, t2: &str) -> Option<String> {
    // Bool promotes to any other Number (julia/base/bool.jl:6)
    // promote_rule(::Type{Bool}, ::Type{T}) where {T<:Number} = T
    if t1 == "Bool" && is_numeric_type_name(t2) {
        return Some(t2.to_string());
    }
    if t2 == "Bool" && is_numeric_type_name(t1) {
        return Some(t1.to_string());
    }

    // BigInt/BigFloat are VM primitive values but intentionally not part of
    // PrimitiveNumeric. Mirror the explicit Julia rules in base/promotion.jl
    // so fallback promotion still works when the registry is unavailable.
    if t1 == "BigFloat" && (is_numeric_type_name(t2) || t2 == "BigInt") {
        return Some("BigFloat".to_string());
    }
    if t2 == "BigFloat" && (is_numeric_type_name(t1) || t1 == "BigInt") {
        return Some("BigFloat".to_string());
    }
    if t1 == "BigInt" && is_float_type_name(t2) {
        return Some("BigFloat".to_string());
    }
    if t2 == "BigInt" && is_float_type_name(t1) {
        return Some("BigFloat".to_string());
    }
    if t1 == "BigInt" && is_integer_type_name(t2) {
        return Some("BigInt".to_string());
    }
    if t2 == "BigInt" && is_integer_type_name(t1) {
        return Some("BigInt".to_string());
    }

    if let Some(t1_elem) = extract_rational_param(t1) {
        if let Some(t2_elem) = extract_rational_param(t2) {
            let promoted_elem = promote_type(&t1_elem, &t2_elem);
            return Some(format!("Rational{{{}}}", promoted_elem));
        }
        if is_integer_type_name(t2) || t2 == "BigInt" {
            let promoted_elem = promote_type(&t1_elem, t2);
            return Some(format!("Rational{{{}}}", promoted_elem));
        }
        if is_float_type_name(t2) || t2 == "BigFloat" {
            return Some(promote_type(&t1_elem, t2));
        }
    }
    if let Some(t2_elem) = extract_rational_param(t2) {
        if is_integer_type_name(t1) || t1 == "BigInt" {
            let promoted_elem = promote_type(t1, &t2_elem);
            return Some(format!("Rational{{{}}}", promoted_elem));
        }
        if is_float_type_name(t1) || t1 == "BigFloat" {
            return Some(promote_type(t1, &t2_elem));
        }
    }

    // Complex{T} + S -> Complex{promote_type(T, S)} (julia/base/complex.jl:49-50)
    // promote_rule(::Type{Complex{T}}, ::Type{S}) where {T<:Real,S<:Real} = Complex{promote_type(T,S)}
    if let Some(t1_elem) = extract_complex_param(t1) {
        if let Some(t2_elem) = extract_complex_param(t2) {
            // Complex + Complex
            let promoted_elem = promote_type(&t1_elem, &t2_elem);
            return Some(format!("Complex{{{}}}", promoted_elem));
        } else if is_numeric_type_name(t2) {
            // Complex + Real
            let promoted_elem = promote_type(&t1_elem, t2);
            return Some(format!("Complex{{{}}}", promoted_elem));
        }
    }
    if let Some(t2_elem) = extract_complex_param(t2) {
        if is_numeric_type_name(t1) {
            // Real + Complex
            let promoted_elem = promote_type(t1, &t2_elem);
            return Some(format!("Complex{{{}}}", promoted_elem));
        }
    }

    // Float + Int -> Float (larger float wins)
    if is_float_type_name(t1) && is_integer_type_name(t2) {
        return Some(t1.to_string());
    }
    if is_float_type_name(t2) && is_integer_type_name(t1) {
        return Some(t2.to_string());
    }

    // Float + Float and any other primitive numeric pair fall through to the
    // shared `PrimitiveNumeric` taxonomy at the end of this function (Issue
    // #6735: the hardcoded `type_priority` table was removed; the registered
    // `promote_rule` network is the source of truth and the shared taxonomy is
    // the bootstrap fallback).

    // Int + Int -> Julia's promotion: wider wins; if same width, unsigned wins.
    // Issue #3742: A bare priority comparison (signed > unsigned) gave the wrong
    // answer for mixed signed/unsigned at the same width (e.g., Int8 + UInt8 →
    // Int8 instead of UInt8). The registry has rules like
    // `promote_rule(::Type{UInt8}, ::Type{Int8}) = UInt8` registered in only one
    // direction, so when the registry is empty (sjulia runs without a Base cache)
    // or the lookup misses the registered direction, this fallback must still
    // match Julia semantics.
    if is_integer_type_name(t1) && is_integer_type_name(t2) {
        let int_width = |t: &str| -> i32 {
            match t {
                "Bool" => 1,
                "Int8" | "UInt8" => 8,
                "Int16" | "UInt16" => 16,
                "Int32" | "UInt32" => 32,
                "Int64" | "UInt64" => 64,
                "Int128" | "UInt128" => 128,
                _ => 0,
            }
        };
        let is_unsigned = |t: &str| t.starts_with("UInt");
        let w1 = int_width(t1);
        let w2 = int_width(t2);
        if w1 > 0 && w2 > 0 {
            if w1 > w2 {
                return Some(t1.to_string());
            }
            if w2 > w1 {
                return Some(t2.to_string());
            }
            // Same width: unsigned wins over signed; otherwise either is fine.
            if is_unsigned(t1) {
                return Some(t1.to_string());
            }
            if is_unsigned(t2) {
                return Some(t2.to_string());
            }
            return Some(t1.to_string());
        }
        // Unrecognised width: fall through to the shared PrimitiveNumeric path.
    }

    // Shared primitive numeric fallback (Issue #3508). Most primitive pairs
    // are handled above to preserve the VM's Julia-style promote_type tests;
    // this keeps a final shared primitive path for future variants that are
    // added to `PrimitiveNumeric` before local priority tables learn them.
    if let (Some(p1), Some(p2)) = (
        crate::inference_core::PrimitiveNumeric::from_julia_name(t1),
        crate::inference_core::PrimitiveNumeric::from_julia_name(t2),
    ) {
        return Some(p1.promote(p2).julia_name().to_string());
    }

    None
}

fn valid_rule_result(result: Option<String>) -> Option<String> {
    result.filter(|r| r != "Union{}" && !r.is_empty())
}

fn combine_rule_results(r1: Option<String>, r2: Option<String>) -> Option<String> {
    match (r1, r2) {
        (Some(a), Some(b)) if a == b => Some(a),
        (Some(a), Some(b)) => Some(promote_type(&a, &b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Determine the common type for two types following Julia's promote_type.
///
/// This tries promote_rule in both directions and combines results.
/// If neither direction returns a result, defaults to "Any".
///
/// Reference: julia/base/promotion.jl:315-323
/// ```julia
/// function promote_type(::Type{T}, ::Type{S}) where {T,S}
///     promote_result(T, S, promote_rule(T,S), promote_rule(S,T))
/// end
/// ```
///
/// IMPORTANT (Issue #3742): Julia's `promote_type` tries `promote_rule(T, S)` AND
/// `promote_rule(S, T)`, both consulting the user-registered rules first. We must
/// query the Julia registry in BOTH directions before falling back to the Rust
/// priority logic. Otherwise asymmetrically registered rules (e.g.
/// `promote_rule(::Type{UInt8}, ::Type{Int8}) = UInt8` only) are masked by the
/// fallback when called as `promote_type("Int8", "UInt8")`.
pub fn promote_type(t1: &str, t2: &str) -> String {
    // Same type: no promotion needed
    if t1 == t2 {
        return t1.to_string();
    }

    // Consult the Julia-defined registry in BOTH directions first. This must
    // precede the Rust fallback because the registry encodes the canonical
    // Julia rules and is often only registered in one direction.
    if let Some(result) = combine_rule_results(
        valid_rule_result(lookup_promotion_rule(t1, t2)),
        valid_rule_result(lookup_promotion_rule(t2, t1)),
    ) {
        return result;
    }

    // Fall back to the Rust implementation in both directions for bootstrapping
    // and unknown types.
    if let Some(result) =
        combine_rule_results(promote_rule_fallback(t1, t2), promote_rule_fallback(t2, t1))
    {
        return result;
    }

    // Last resort: Any (like Julia's typejoin fallback). Issue #6735: the former
    // priority-table fallback here was removed — every numeric pair is resolved
    // either by the registered promote_rule network (above) or by the
    // PrimitiveNumeric taxonomy inside promote_rule_fallback.
    "Any".to_string()
}

/// Promote two types when at least one is Complex.
/// This is a specialized version of promote_type for Complex arithmetic.
///
/// Examples:
/// - promote_complex("Complex{Bool}", "Float64") -> "Complex{Float64}"
/// - promote_complex("Complex{Int64}", "Complex{Bool}") -> "Complex{Int64}"
/// - promote_complex("Float64", "Complex{Bool}") -> "Complex{Float64}"
pub fn promote_complex(t1: &str, t2: &str) -> String {
    // At least one must be Complex for this function to be meaningful
    let t1_elem = extract_complex_param(t1);
    let t2_elem = extract_complex_param(t2);

    match (t1_elem, t2_elem) {
        (Some(e1), Some(e2)) => {
            // Both are Complex
            let promoted = promote_type(&e1, &e2);
            format!("Complex{{{}}}", promoted)
        }
        (Some(e), None) => {
            // t1 is Complex, t2 is not
            let t2_elem = if is_numeric_type_name(t2) {
                t2.to_string()
            } else {
                "Float64".to_string()
            };
            let promoted = promote_type(&e, &t2_elem);
            format!("Complex{{{}}}", promoted)
        }
        (None, Some(e)) => {
            // t2 is Complex, t1 is not
            let t1_elem = if is_numeric_type_name(t1) {
                t1.to_string()
            } else {
                "Float64".to_string()
            };
            let promoted = promote_type(&t1_elem, &e);
            format!("Complex{{{}}}", promoted)
        }
        (None, None) => {
            // Neither is Complex - shouldn't call this function, but handle gracefully
            promote_type(t1, t2)
        }
    }
}

/// Promote element types for Complex arithmetic operations.
/// This is a convenience wrapper around promote_type for element types.
pub fn promote_element_types(elem1: &str, elem2: &str) -> String {
    promote_type(elem1, elem2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bool_promotes_to_any_number() {
        // julia/base/bool.jl:6 - Bool promotes to any other Number
        assert_eq!(promote_type("Bool", "Int64"), "Int64");
        assert_eq!(promote_type("Bool", "Float64"), "Float64");
        assert_eq!(promote_type("Int64", "Bool"), "Int64");
        assert_eq!(promote_type("Float64", "Bool"), "Float64");
    }

    #[test]
    fn test_float_int_promotion() {
        // Int + Float -> Float
        assert_eq!(promote_type("Int64", "Float64"), "Float64");
        assert_eq!(promote_type("Float64", "Int64"), "Float64");
        assert_eq!(promote_type("Int32", "Float32"), "Float32");
        assert_eq!(promote_type("Int16", "Float32"), "Float32");
        assert_eq!(promote_type("Float32", "Int8"), "Float32");
        assert_eq!(promote_type("UInt32", "Float32"), "Float32");
        assert_eq!(promote_type("Float32", "UInt64"), "Float32");
    }

    #[test]
    fn test_same_type_no_promotion() {
        assert_eq!(promote_type("Int64", "Int64"), "Int64");
        assert_eq!(promote_type("Float64", "Float64"), "Float64");
        assert_eq!(
            promote_type("Complex{Float64}", "Complex{Float64}"),
            "Complex{Float64}"
        );
    }

    #[test]
    fn test_complex_complex_promotion() {
        // julia/base/complex.jl:51-52
        assert_eq!(
            promote_type("Complex{Bool}", "Complex{Float64}"),
            "Complex{Float64}"
        );
        assert_eq!(
            promote_type("Complex{Int64}", "Complex{Bool}"),
            "Complex{Int64}"
        );
        assert_eq!(
            promote_type("Complex{Float32}", "Complex{Float64}"),
            "Complex{Float64}"
        );
    }

    #[test]
    fn test_complex_real_promotion() {
        // julia/base/complex.jl:49-50
        assert_eq!(promote_type("Complex{Bool}", "Float64"), "Complex{Float64}");
        assert_eq!(promote_type("Float64", "Complex{Bool}"), "Complex{Float64}");
        assert_eq!(
            promote_type("Complex{Int64}", "Float64"),
            "Complex{Float64}"
        );
        assert_eq!(promote_type("Int64", "Complex{Bool}"), "Complex{Int64}");
    }

    #[test]
    fn test_promote_complex_helper() {
        assert_eq!(
            promote_complex("Complex{Bool}", "Float64"),
            "Complex{Float64}"
        );
        assert_eq!(
            promote_complex("Float64", "Complex{Bool}"),
            "Complex{Float64}"
        );
        assert_eq!(
            promote_complex("Complex{Int64}", "Complex{Bool}"),
            "Complex{Int64}"
        );
    }

    #[test]
    fn test_extract_complex_param() {
        assert_eq!(
            extract_complex_param("Complex{Float64}"),
            Some("Float64".to_string())
        );
        assert_eq!(
            extract_complex_param("Complex{Bool}"),
            Some("Bool".to_string())
        );
        assert_eq!(extract_complex_param("Float64"), None);
        assert_eq!(extract_complex_param("Int64"), None);
    }

    #[test]
    fn test_integer_promotion() {
        assert_eq!(promote_type("Int32", "Int64"), "Int64");
        assert_eq!(promote_type("Int64", "Int32"), "Int64");
        assert_eq!(promote_type("Int8", "Int16"), "Int16");
    }

    #[test]
    fn test_bigint_bigfloat_fallback_promotion() {
        assert_eq!(promote_type("BigInt", "Int64"), "BigInt");
        assert_eq!(promote_type("Int64", "BigInt"), "BigInt");
        assert_eq!(promote_type("BigInt", "UInt8"), "BigInt");
        assert_eq!(promote_type("BigInt", "Float32"), "BigFloat");
        assert_eq!(promote_type("Float16", "BigInt"), "BigFloat");
        assert_eq!(promote_type("BigFloat", "UInt128"), "BigFloat");
        assert_eq!(promote_type("Int8", "BigFloat"), "BigFloat");
    }

    #[test]
    fn test_float_promotion_via_taxonomy() {
        // Issue #6735: Float×Float promotion no longer uses a hardcoded priority
        // table; it falls through to the shared PrimitiveNumeric taxonomy.
        assert_eq!(promote_type("Float32", "Float64"), "Float64");
        assert_eq!(promote_type("Float16", "Float32"), "Float32");
        assert_eq!(promote_type("Float16", "Float64"), "Float64");
    }
}
