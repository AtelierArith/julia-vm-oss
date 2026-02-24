//! Type promotion following Julia's promote_rule/promote_type pattern.
//!
//! Julia's type promotion is a three-layer system:
//! 1. `promote_rule(T, S)` - Basic rules defined per type pair
//! 2. `promote_type(T, S)` - Tries promote_rule both ways and combines results
//! 3. `promote(x, y)` - Converts values to the common type
//!
//! This module implements the equivalent for SubsetJuliaVM's compile-time type inference.
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

/// Type priority for numeric types.
/// Higher value means wider type in the promotion hierarchy.
/// Following Julia's promotion rules: Float64 > Float32 > Int64 > ... > Bool
fn type_priority(ty: &str) -> i32 {
    match ty {
        "Float64" => 100,
        "Float32" => 90,
        "Float16" => 88,
        "Int128" => 85,
        "Int64" => 80,
        "Int32" => 70,
        "Int16" => 60,
        "Int8" => 50,
        "UInt128" => 45,
        "UInt64" => 44,
        "UInt32" => 43,
        "UInt16" => 42,
        "UInt8" => 41,
        "Bool" => 10,
        _ => 0, // Unknown type
    }
}

/// Check if a type is a floating-point type.
pub fn is_float_type_name(ty: &str) -> bool {
    matches!(ty, "Float64" | "Float32" | "Float16")
}

/// Check if a type is an integer type.
pub fn is_integer_type_name(ty: &str) -> bool {
    matches!(
        ty,
        "Int128"
            | "Int64"
            | "Int32"
            | "Int16"
            | "Int8"
            | "UInt128"
            | "UInt64"
            | "UInt32"
            | "UInt16"
            | "UInt8"
            | "Bool"
    )
}

/// Check if a type is numeric (float or integer).
pub fn is_numeric_type_name(ty: &str) -> bool {
    is_float_type_name(ty) || is_integer_type_name(ty)
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

/// Check if a type name represents a Complex type.
pub fn is_complex_type(name: &str) -> bool {
    name.starts_with("Complex{") && name.ends_with('}')
}

/// Fundamental promotion rule for two types.
/// Returns None if no promotion rule is defined for this pair.
///
/// This function first checks the Julia-defined promotion rule registry,
/// then falls back to the Rust implementation for bootstrapping and unknown types.
///
/// This follows Julia's promote_rule pattern:
/// - `promote_rule(::Type{Bool}, ::Type{T}) where {T<:Number} = T`
/// - `promote_rule(::Type{Complex{T}}, ::Type{S}) = Complex{promote_type(T,S)}`
fn promote_rule(t1: &str, t2: &str) -> Option<String> {
    // Same type: return as-is
    if t1 == t2 {
        return Some(t1.to_string());
    }

    // First, check the Julia-defined promotion rule registry
    if let Some(result) = lookup_promotion_rule(t1, t2) {
        // Filter out Union{} (Bottom) which means no rule defined
        if result != "Union{}" && !result.is_empty() {
            return Some(result);
        }
    }

    // Fall back to Rust implementation for bootstrapping and unknown types
    promote_rule_fallback(t1, t2)
}

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

    // Float + Float -> larger Float
    if is_float_type_name(t1) && is_float_type_name(t2) {
        let p1 = type_priority(t1);
        let p2 = type_priority(t2);
        return Some(if p1 >= p2 {
            t1.to_string()
        } else {
            t2.to_string()
        });
    }

    // Int + Int -> larger Int
    if is_integer_type_name(t1) && is_integer_type_name(t2) {
        let p1 = type_priority(t1);
        let p2 = type_priority(t2);
        return Some(if p1 >= p2 {
            t1.to_string()
        } else {
            t2.to_string()
        });
    }

    None
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
pub fn promote_type(t1: &str, t2: &str) -> String {
    // Same type: no promotion needed
    if t1 == t2 {
        return t1.to_string();
    }

    // Try promote_rule in both directions
    if let Some(result) = promote_rule(t1, t2) {
        return result;
    }
    if let Some(result) = promote_rule(t2, t1) {
        return result;
    }

    // Fallback: if both are numeric, use priority-based promotion
    let p1 = type_priority(t1);
    let p2 = type_priority(t2);
    if p1 > 0 && p2 > 0 {
        return if p1 >= p2 {
            t1.to_string()
        } else {
            t2.to_string()
        };
    }

    // Last resort: Any (like Julia's typejoin fallback)
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
    fn test_type_priority() {
        assert!(type_priority("Float64") > type_priority("Int64"));
        assert!(type_priority("Int64") > type_priority("Int32"));
        assert!(type_priority("Int32") > type_priority("Bool"));
        assert_eq!(type_priority("Unknown"), 0);
    }
}
