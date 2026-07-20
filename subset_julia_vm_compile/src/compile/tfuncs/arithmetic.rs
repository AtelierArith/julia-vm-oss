//! Transfer functions for arithmetic and comparison operations.
//!
//! This module implements type inference for Julia's arithmetic operations,
//! following Julia's type promotion rules.
//!
//! Uses the centralized promotion module which implements Julia's
//! promote_rule/promote_type pattern.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::promotion::{extract_complex_param, promote_type};
use crate::inference_core::{CorePrimitive, CoreType};

/// Transfer function for the `+` operator.
///
/// Type rules:
/// - Int + Int → Int
/// - Float + Float → Float
/// - Int + Float → Float (promotion)
/// - Complex{T} + Complex{S} → Complex{promote(T, S)}
/// - Real + Complex{T} → Complex{promote(Real, T)}
/// - Numeric unions are handled conservatively
///
/// # Examples
/// ```text
/// +(Int64, Int64) → Int64
/// +(Int64, Float64) → Float64
/// +(Float64, Float64) → Float64
/// +(Complex{Float64}, Complex{Float64}) → Complex{Float64}
/// +(Float64, Complex{Bool}) → Complex{Float64}
/// ```
pub fn tfunc_add(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom
    // This indicates unreachable code (e.g., inside a for loop over an empty tuple)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    let left = match &args[0] {
        LatticeType::Const(cv) => LatticeType::Concrete(cv.to_concrete_type()),
        other => other.clone(),
    };
    let right = match &args[1] {
        LatticeType::Const(cv) => LatticeType::Concrete(cv.to_concrete_type()),
        other => other.clone(),
    };

    if let Some(distributed) = distribute_binary_union(&left, &right, tfunc_add) {
        return distributed;
    }

    // Extract Complex element types if present
    let left_complex_elem = extract_complex_element_type(&left);
    let right_complex_elem = extract_complex_element_type(&right);

    // Handle Complex arithmetic
    if left_complex_elem.is_some() || right_complex_elem.is_some() {
        return promote_complex_arithmetic(&left, &right, left_complex_elem, right_complex_elem);
    }

    match (&left, &right) {
        // Int + Int → Int
        (LatticeType::Concrete(a), LatticeType::Concrete(b))
            if a.is_integer() && b.is_integer() =>
        {
            // Promote to larger integer type if needed
            promote_integer_types(a, b)
        }

        // Float + Float → Float
        (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a.is_float() && b.is_float() => {
            // Promote to larger float type
            promote_float_types(a, b)
        }

        // Int + Float or Float + Int → Float
        (LatticeType::Concrete(a), LatticeType::Concrete(b))
            if a.is_numeric() && b.is_numeric() =>
        {
            // If either is float, result is float
            if a.is_float() || b.is_float() {
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                )))
            } else {
                promote_integer_types(a, b)
            }
        }

        // Union types: be conservative
        _ if left.is_numeric() && right.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        )),

        // Unknown or non-numeric types
        _ => LatticeType::Top,
    }
}

/// Transfer function for the `-` operator.
///
/// Handles both unary negation (-x) and binary subtraction (x - y).
/// Unary negation preserves the operand type (Julia semantics: `-x::T → T`).
/// This is type-preserving for *any* concrete operand, not only the lattice's
/// `is_numeric()` primitives: `Complex{T}` (and other user numeric structs)
/// negate to their own type, but the empty-table engine cannot prove
/// `Complex{Float64} <: Number`, so gating on `is_numeric()` wrongly widened
/// `-c::ComplexF64` to `Top`/`Any`. Mirroring the legacy pre-scan / upstream
/// Julia (`typeof(-(1.0+2.0im)) === ComplexF64`), preserve the concrete operand
/// type unconditionally (Issue #6601). Binary subtraction follows the same
/// promotion rules as `+`.
pub fn tfunc_sub(args: &[LatticeType]) -> LatticeType {
    // Handle unary negation: -x preserves the type
    if args.len() == 1 {
        let operand = match &args[0] {
            LatticeType::Const(cv) => LatticeType::Concrete(cv.to_concrete_type()),
            other => other.clone(),
        };
        return match &operand {
            LatticeType::Bottom => LatticeType::Bottom,
            // Negation preserves the concrete operand type (numeric primitives
            // and numeric structs like `Complex{T}` alike).
            LatticeType::Concrete(_) => operand,
            _ => LatticeType::Top,
        };
    }
    // Binary subtraction follows the same type rules as addition
    tfunc_add(args)
}

/// Transfer function for the `*` operator.
///
/// Follows the same promotion rules as `+`.
pub fn tfunc_mul(args: &[LatticeType]) -> LatticeType {
    // `*` on strings is concatenation -> String (Julia: `"a" * "b" == "ab"`).
    if args.len() == 2
        && (matches!(
            &args[0],
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        ) || matches!(
            &args[1],
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        ))
    {
        return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
    }
    // Multiplication otherwise follows the same type rules as addition.
    tfunc_add(args)
}

/// Transfer function for the `^` (power) operator.
///
/// Julia type rules: numeric powers mirror addition (`Int^Int -> Int`,
/// any-Float -> Float, `Complex^_ -> Complex`); a String base repeats
/// (`"ab"^3 == "ababab"`, type `String`).
pub fn tfunc_pow(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }
    let left = match &args[0] {
        LatticeType::Const(cv) => LatticeType::Concrete(cv.to_concrete_type()),
        other => other.clone(),
    };
    let right = match &args[1] {
        LatticeType::Const(cv) => LatticeType::Concrete(cv.to_concrete_type()),
        other => other.clone(),
    };

    // `s^n` repeats a String (`^` repeats; `*` concatenates).
    if let LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) =
        &left
    {
        return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
    }
    if let (
        LatticeType::Concrete(base),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))),
    ) = (&left, &right)
    {
        if base.is_float() {
            return left;
        }
    }
    // Numeric powers follow the same promotion as addition.
    tfunc_add(args)
}

/// Transfer function for the `/` operator.
///
/// In Julia, division always returns a Float64 (unlike `÷` which returns Int).
/// When at least one operand is a `BigInt`, `/` dispatches to `DivBigFloat`
/// and returns `BigFloat` (Issue #8900: matches upstream Julia where `big(a) / big(b)`
/// always yields BigFloat). `BigFloat` × numeric also dispatches to `DivBigFloat`
/// and returns `BigFloat`. We mirror that here for correct type inference.
///
/// # Examples
/// ```text
/// /(Int64, Int64) → Float64
/// /(Float64, Float64) → Float64
/// /(BigInt, BigInt) → BigFloat
/// /(BigInt, Int64) → BigFloat
/// /(BigFloat, BigFloat) → BigFloat
/// ```
pub fn tfunc_div(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    let left = match &args[0] {
        LatticeType::Const(cv) => LatticeType::Concrete(cv.to_concrete_type()),
        other => other.clone(),
    };
    let right = match &args[1] {
        LatticeType::Const(cv) => LatticeType::Concrete(cv.to_concrete_type()),
        other => other.clone(),
    };

    if let Some(distributed) = distribute_binary_union(&left, &right, tfunc_div) {
        return distributed;
    }

    match (&left, &right) {
        // BigFloat × numeric (or numeric × BigFloat) → BigFloat
        // Matches `DivBigFloat` runtime dispatch.
        (LatticeType::Concrete(a), LatticeType::Concrete(b))
            if (matches!(
                a,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat))
            ) && b.is_numeric())
                || (matches!(
                    b,
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat))
                ) && a.is_numeric()) =>
        {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            )))
        }
        // BigInt × {BigInt, integer} → BigFloat
        // Issue #8900: BigInt `/` (float division) returns BigFloat, matching upstream Julia
        // where `big(a) / big(b)` always yields BigFloat. Only `÷` (div/IntDiv) returns BigInt.
        (LatticeType::Concrete(a), LatticeType::Concrete(b))
            if (matches!(
                a,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
            ) && b.is_integer())
                || (matches!(
                    b,
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
                ) && a.is_integer()) =>
        {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            )))
        }
        // Numeric / Numeric → Float64
        _ if left.is_numeric() && right.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        )),

        // Unknown or non-numeric types
        _ => LatticeType::Top,
    }
}

/// Transfer function for the `==` operator.
///
/// Equality comparison always returns Bool.
pub fn tfunc_eq(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    // == always returns Bool for any types
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
}

/// Transfer function for the `<` operator.
///
/// Less-than comparison returns Bool for comparable types.
pub fn tfunc_lt(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match (&args[0], &args[1]) {
        // Numeric < Numeric → Bool
        _ if args[0].is_numeric() && args[1].is_numeric() => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        }

        // String < String → Bool
        (
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))),
        ) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),

        // Char < Char → Bool
        (
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))),
        ) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),

        // Unknown or non-comparable types
        _ => LatticeType::Top,
    }
}

/// Transfer function for the `<=` operator.
pub fn tfunc_le(args: &[LatticeType]) -> LatticeType {
    tfunc_lt(args) // Same type rules as <
}

/// Transfer function for the `>` operator.
pub fn tfunc_gt(args: &[LatticeType]) -> LatticeType {
    tfunc_lt(args) // Same type rules as <
}

/// Transfer function for the `>=` operator.
pub fn tfunc_ge(args: &[LatticeType]) -> LatticeType {
    tfunc_lt(args) // Same type rules as <
}

/// Transfer function for the `!` operator (logical negation).
///
/// Logical negation always yields `Bool` (`typeof(!x) === Bool` in upstream
/// Julia whenever `!` dispatches, e.g. `!true`, `!isempty(x)`). This matches
/// the legacy pre-scan, which types `UnaryOp::Not` as `Bool` unconditionally.
/// Previously the engine widened a non-`Bool` operand to `Top`, leaving the
/// `Assign`-RHS slot `Any`; returning `Bool` removes that divergence so the
/// pre-scan seam can route `UnaryOp` through the shared engine (Issue #6601).
pub fn tfunc_not(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    // `!` is logical negation: its result type is always `Bool`.
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
}

fn distribute_binary_union(
    left: &LatticeType,
    right: &LatticeType,
    tfunc: fn(&[LatticeType]) -> LatticeType,
) -> Option<LatticeType> {
    if !matches!(left, LatticeType::Union(_)) && !matches!(right, LatticeType::Union(_)) {
        return None;
    }

    let left_members = union_distribution_members(left);
    let right_members = union_distribution_members(right);
    let mut result: Option<LatticeType> = None;
    for lhs in &left_members {
        for rhs in &right_members {
            let candidate = tfunc(&[lhs.clone(), rhs.clone()]);
            if matches!(candidate, LatticeType::Top | LatticeType::Bottom) {
                continue;
            }
            result = Some(match result {
                Some(acc) => acc.join(&candidate),
                None => candidate,
            });
        }
    }
    Some(result.unwrap_or(LatticeType::Top))
}

fn union_distribution_members(ty: &LatticeType) -> Vec<LatticeType> {
    match ty {
        LatticeType::Union(members) => members.iter().cloned().map(LatticeType::Concrete).collect(),
        other => vec![other.clone()],
    }
}

/// Helper: promote two integer types to their common type.
/// Uses the centralized promote_type following Julia's promotion rules.
fn promote_integer_types(a: &ConcreteType, b: &ConcreteType) -> LatticeType {
    // Convert to type names and use centralized promotion
    if let (Some(name_a), Some(name_b)) = (a.to_type_name(), b.to_type_name()) {
        let result_name = promote_type(&name_a, &name_b);
        if let Some(result_type) = ConcreteType::from_type_name(&result_name) {
            return LatticeType::Concrete(result_type);
        }
    }
    // Fallback to Int64 if conversion fails
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )))
}

/// Helper: promote two float types to their common type.
/// Uses the centralized promote_type following Julia's promotion rules.
fn promote_float_types(a: &ConcreteType, b: &ConcreteType) -> LatticeType {
    // Convert to type names and use centralized promotion
    if let (Some(name_a), Some(name_b)) = (a.to_type_name(), b.to_type_name()) {
        let result_name = promote_type(&name_a, &name_b);
        if let Some(result_type) = ConcreteType::from_type_name(&result_name) {
            return LatticeType::Concrete(result_type);
        }
    }
    // Fallback to Float64 if conversion fails
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Float64,
    )))
}

/// Extract the element type from a Complex struct type, if present.
/// Uses the centralized promotion module.
/// Returns Some("Float64"), Some("Int64"), Some("Bool"), etc. for Complex{T}
fn extract_complex_element_type(ty: &LatticeType) -> Option<String> {
    match ty {
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => extract_complex_param(name),
        _ => None,
    }
}

/// Promote element type for Complex arithmetic.
/// Uses the centralized promotion module following Julia's promote_rule/promote_type pattern.
fn promote_element_type(elem1: &str, elem2: &str) -> String {
    promote_type(elem1, elem2)
}

/// Promote a real type to element type string for Complex arithmetic.
/// Uses the ConcreteType::to_type_name() method for conversion.
fn real_to_element_type(ty: &LatticeType) -> String {
    match ty {
        LatticeType::Concrete(ct) => ct.to_type_name().unwrap_or_else(|| "Float64".to_string()),
        LatticeType::Const(cv) => cv
            .to_concrete_type()
            .to_type_name()
            .unwrap_or_else(|| "Float64".to_string()),
        _ => "Float64".to_string(),
    }
}

/// Handle Complex arithmetic type promotion.
/// Returns the result type for operations involving Complex numbers.
fn promote_complex_arithmetic(
    left: &LatticeType,
    right: &LatticeType,
    left_elem: Option<String>,
    right_elem: Option<String>,
) -> LatticeType {
    let result_elem = match (left_elem, right_elem) {
        // Complex + Complex -> Complex{promote(T1, T2)}
        (Some(e1), Some(e2)) => promote_element_type(&e1, &e2),
        // Complex + Real -> Complex{promote(T, Real)}
        (Some(e), None) => promote_element_type(&e, &real_to_element_type(right)),
        // Real + Complex -> Complex{promote(Real, T)}
        (None, Some(e)) => promote_element_type(&real_to_element_type(left), &e),
        // Should not happen (called only when at least one is Complex)
        (None, None) => "Float64".to_string(),
    };

    LatticeType::Concrete(ConcreteType::Struct {
        name: format!("Complex{{{}}}", result_elem),
        type_id: 0, // Type ID will be resolved later
    })
}

/// Transfer function for `gcd` / `lcm`.
///
/// Type rules (Issue #5922, mirrors the legacy expression-inference gate):
/// - any `BigInt` argument → `BigInt` (Issue #2383)
/// - all-`Int64` arguments → `Int64`
/// - anything else → Top (the expression adapter falls back to `Int64`)
pub fn tfunc_gcd(args: &[LatticeType]) -> LatticeType {
    let arg_concrete = |arg: &LatticeType| match arg {
        LatticeType::Concrete(ct) => Some(ct.clone()),
        LatticeType::Const(cv) => Some(cv.to_concrete_type()),
        _ => None,
    };
    if args.iter().any(|arg| {
        matches!(
            arg_concrete(arg),
            Some(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt
            )))
        )
    }) {
        return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt,
        )));
    }
    if !args.is_empty()
        && args.iter().all(|arg| {
            matches!(
                arg_concrete(arg),
                Some(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                )))
            )
        })
    {
        return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
    }
    LatticeType::Top
}

/// Transfer function for `fld` / `cld` (floored / ceiling integer division).
///
/// Type rules (Issue #5922): claim `Int64` only for the all-`Int64` case;
/// other shapes stay Top so the shared engine makes no unsound claim while
/// the expression adapter keeps the legacy `Int64` fallback.
pub fn tfunc_fld(args: &[LatticeType]) -> LatticeType {
    let all_int64 = args.len() == 2
        && args.iter().all(|arg| match arg {
            LatticeType::Concrete(ct) => matches!(
                ct,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            ),
            LatticeType::Const(cv) => matches!(
                cv.to_concrete_type(),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            ),
            _ => false,
        });
    if all_int64 {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )))
    } else {
        LatticeType::Top
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConstValue;

    #[test]
    fn test_add_int_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_add(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_add_float_float() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_add(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_add_int_float() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_add(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_add_const_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Const(ConstValue::Int64(1)),
        ];
        let result = tfunc_add(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_div_always_returns_float() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    /// Issue #8900: BigInt `/` BigInt returns BigFloat (float division), matching
    /// upstream Julia where `big(a) / big(b)` always yields BigFloat.
    /// Only `÷` (IntDiv) stays as integer division returning BigInt.
    #[test]
    fn test_div_bigint_bigint_returns_bigfloat() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat
            )))
        );
    }

    #[test]
    fn test_div_bigint_int64_returns_bigfloat() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat
            )))
        );
    }

    #[test]
    fn test_div_int64_bigint_returns_bigfloat() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat
            )))
        );
    }

    #[test]
    fn test_div_bigfloat_bigfloat_returns_bigfloat() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat
            )))
        );
    }

    #[test]
    fn test_div_bigfloat_float64_returns_bigfloat() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat
            )))
        );
    }

    #[test]
    fn test_eq_returns_bool() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_eq(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_lt_numeric_returns_bool() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_lt(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_not_bool() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Bool),
        ))];
        let result = tfunc_not(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_add_wrong_arity() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_add(&args);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_lt_string_string() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        ];
        let result = tfunc_lt(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_lt_char_char() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))),
        ];
        let result = tfunc_lt(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_not_concrete_type() {
        // `!` is logical negation: its result type is always `Bool` (matching the
        // legacy pre-scan, which types `UnaryOp::Not` as `Bool` unconditionally).
        // Previously the engine widened a non-`Bool` operand to `Top`, leaving the
        // `Assign`-RHS slot `Any`; that divergence blocked routing `UnaryOp`
        // through the shared engine (Issue #6601, superseding Issue #3476).
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_not(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    // Bottom propagation tests (Issue #1717 prevention)
    // When either operand is Bottom, the result should be Bottom
    // to correctly represent unreachable code paths.

    #[test]
    fn test_add_bottom_left() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_add(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_add_bottom_right() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            LatticeType::Bottom,
        ];
        let result = tfunc_add(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_add_bottom_both() {
        let args = vec![LatticeType::Bottom, LatticeType::Bottom];
        let result = tfunc_add(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_sub_bottom() {
        // tfunc_sub delegates to tfunc_add
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_sub(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_mul_bottom() {
        // tfunc_mul delegates to tfunc_add
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            LatticeType::Bottom,
        ];
        let result = tfunc_mul(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_div_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_eq_bottom() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            LatticeType::Bottom,
        ];
        let result = tfunc_eq(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_lt_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_lt(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_le_bottom() {
        // tfunc_le delegates to tfunc_lt
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            LatticeType::Bottom,
        ];
        let result = tfunc_le(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_gt_bottom() {
        // tfunc_gt delegates to tfunc_lt
        let args = vec![LatticeType::Bottom, LatticeType::Bottom];
        let result = tfunc_gt(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_ge_bottom() {
        // tfunc_ge delegates to tfunc_lt
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))),
        ];
        let result = tfunc_ge(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    // =========================================================================
    // Unary negation tests for tfunc_sub (Issue #1774 prevention)
    // These tests ensure tfunc_sub correctly handles unary negation (-x)
    // for all numeric types, preserving the operand type.
    // =========================================================================

    #[test]
    fn test_sub_unary_negation_float32() {
        // Unary negation: -x::Float32 → Float32
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_float64() {
        // Unary negation: -x::Float64 → Float64
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_int64() {
        // Unary negation: -x::Int64 → Int64
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_int32() {
        // Unary negation: -x::Int32 → Int32
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int32),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_int16() {
        // Unary negation: -x::Int16 → Int16
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int16),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int16
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_int8() {
        // Unary negation: -x::Int8 → Int8
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int8),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        );
    }

    #[test]
    fn test_sub_unary_negation_int128() {
        // Unary negation: -x::Int128 → Int128
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int128),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_uint64() {
        // Unary negation: -x::UInt64 → UInt64 (bit pattern negation)
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt64),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_const_value() {
        // Unary negation with const value: -1::Int64 → Int64
        let args = vec![LatticeType::Const(ConstValue::Int64(1))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_const_float() {
        // Unary negation with const value: -1.0::Float64 → Float64
        let args = vec![LatticeType::Const(ConstValue::Float64(1.0))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_sub_unary_negation_bottom() {
        // Unary negation with Bottom → Bottom (unreachable code)
        let args = vec![LatticeType::Bottom];
        let result = tfunc_sub(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_sub_unary_negation_concrete_preserved() {
        // Unary negation preserves the concrete operand type (Issue #6601):
        // negation is type-preserving for numeric structs like `Complex{T}`
        // (`typeof(-(1.0+2.0im)) === ComplexF64`), and the empty-table engine
        // cannot prove `Complex{Float64} <: Number`, so we no longer gate on
        // `is_numeric()` and instead preserve any concrete operand — matching
        // the legacy pre-scan (`Neg → infer_value_type(operand)`).
        let complex = LatticeType::Concrete(ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        });
        assert_eq!(tfunc_sub(std::slice::from_ref(&complex)), complex);
    }

    #[test]
    fn test_sub_unary_negation_bool() {
        // Unary negation of Bool → Bool (Bool is numeric in Julia, subtype of Integer)
        // Note: In Julia, -true == -1 (Int64), but for type inference purposes
        // we preserve Bool type. The runtime handles actual value conversion.
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Bool),
        ))];
        let result = tfunc_sub(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }
}
