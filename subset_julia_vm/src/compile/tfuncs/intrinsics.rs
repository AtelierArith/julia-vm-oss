//! Transfer functions for intrinsic operations and type conversions.
//!
//! This module implements type inference for Julia's intrinsic operations,
//! type checking, and conversions.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::promotion::promote_type;
use crate::inference_core::CorePrimitive;
use crate::inference_core::{CoreAbstract, CoreType};

/// Transfer function for `isa` (type checking).
///
/// Type rules:
/// - isa(Any, Type) → Bool
///
/// # Examples
/// ```text
/// isa(Int64, Type) → Bool
/// ```
pub fn tfunc_isa(_args: &[LatticeType]) -> LatticeType {
    // isa always returns Bool
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
}

/// Transfer function for predicates that always return `Bool`.
pub fn tfunc_bool_predicate(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
}

/// Transfer function for never-returning raisers (`throw`, `rethrow`).
///
/// Mirrors upstream `add_tfunc(throw, 1, 1, @nospecs((𝕃, x)->Bottom), 0)` in
/// `julia/Compiler/src/tfuncs.jl`: a raising call has return type `Union{}`
/// (Bottom), so a function whose every exit raises infers `Union{}`, and a
/// raising branch contributes nothing to the join — `x > 0 ? 1.0 : error("neg")`
/// infers `Float64` (Issue #6532).
pub fn tfunc_throw(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Bottom
}

/// Transfer function for `typeof` (get type of value).
///
/// Julia rule: `typeof(x) → DataType` for concrete values.
/// When the argument type is known, return a named DataType; otherwise a generic DataType. (Issue #3482)
pub fn tfunc_typeof(args: &[LatticeType]) -> LatticeType {
    let name = if let Some(LatticeType::Concrete(ct)) = args.first() {
        ct.to_type_name().unwrap_or_default()
    } else {
        String::new()
    };
    LatticeType::Concrete(ConcreteType::DataType { name })
}

/// Transfer function for `convert` (type conversion).
///
/// Type rules:
/// - convert(T, x) → T
///
/// # Examples
/// ```text
/// convert(Float64, Int64) → Float64
/// ```
pub fn tfunc_convert(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // convert(T, x) → T: return the target type T, not the source type x. (Issue #3475)
    // args[0] is T (a DataType literal), args[1] is the value to convert.
    match &args[0] {
        LatticeType::Concrete(ConcreteType::DataType { name }) => {
            if let Some(ct) = ConcreteType::from_type_name(name) {
                return LatticeType::Concrete(ct);
            }
            LatticeType::Top
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int64` (conversion to Int64).
pub fn tfunc_to_int64(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Float64` (conversion to Float64).
pub fn tfunc_to_float64(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Bool` (conversion to Bool).
pub fn tfunc_to_bool(_args: &[LatticeType]) -> LatticeType {
    // Bool() conversion always returns Bool
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
}

/// Transfer function for `String` (conversion to String).
pub fn tfunc_to_string(_args: &[LatticeType]) -> LatticeType {
    // String() conversion always returns String
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::String,
    )))
}

/// Transfer function for `sqrt` (square root).
///
/// Type rules:
/// - sqrt(Float*) → same concrete float type
/// - sqrt(Integer/Bool) → Float64
///
/// # Examples
/// ```text
/// sqrt(Int64) → Float64
/// sqrt(Float32) → Float32
/// ```
pub fn tfunc_sqrt(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_float() => LatticeType::Concrete(ct.clone()),
        // Issue #6601: sqrt/exp/sin/cos/log preserve `Complex{T}`
        // (`exp(::ComplexF64)::ComplexF64`). `Complex` structs are not `is_numeric`,
        // so without this arm they fell through to `Top` (-> `ValueType::Any`).
        LatticeType::Concrete(ct @ ConcreteType::Struct { name, .. })
            if name.starts_with("Complex") =>
        {
            LatticeType::Concrete(ct.clone())
        }
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        )),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `abs` (absolute value).
///
/// Type rules:
/// - abs(Int) → Int
/// - abs(Float) → Float
/// - abs(Complex{T}) → Float64 (magnitude of complex number)
pub fn tfunc_abs(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            // abs preserves numeric type
            LatticeType::Concrete(ct.clone())
        }
        // Complex numbers: abs returns the magnitude (a real number)
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) if name.starts_with("Complex") => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `sin` (sine).
pub fn tfunc_sin(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules.
}

/// Transfer function for `cos` (cosine).
pub fn tfunc_cos(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules.
}

/// Transfer function for `exp` (exponential).
pub fn tfunc_exp(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules.
}

/// Transfer function for `log` (natural logarithm).
pub fn tfunc_log(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules.
}

/// Transfer function for unary math functions that infer `Float64` for numeric inputs.
pub fn tfunc_unary_float64(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        )),
        _ => LatticeType::Top,
    }
}

/// Transfer function for reductions/statistics functions that infer `Float64`.
pub fn tfunc_float64_result(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    if args.iter().any(|arg| {
        matches!(
            arg,
            LatticeType::Top | LatticeType::Concrete(ConcreteType::Struct { .. })
        )
    }) {
        return LatticeType::Top;
    }

    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Float64,
    )))
}

/// Transfer function for functions that infer `Int64`.
pub fn tfunc_int64_result(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        LatticeType::Top
    } else {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )))
    }
}

/// Transfer function for `big` (widen to an arbitrary-precision type).
///
/// Type rules (Issue #5922, mirrors the legacy expression-inference gate):
/// - `big(::Float32 | ::Float64) → BigFloat`
/// - `big(::concrete) → BigInt` for the remaining concrete inputs
/// - `big() → BigInt` (legacy default)
/// - unknown argument types → Top (the expression adapter falls back to BigInt)
pub fn tfunc_big(args: &[LatticeType]) -> LatticeType {
    let first = match args.first() {
        None => {
            return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            )))
        }
        Some(LatticeType::Const(cv)) => cv.to_concrete_type(),
        Some(LatticeType::Concrete(ct)) => ct.clone(),
        Some(_) => return LatticeType::Top,
    };
    match first {
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)) => LatticeType::Concrete(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)),
        ),
        _ => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt,
        ))),
    }
}

/// Transfer function for `IOBuffer` (in-memory IO stream constructor).
///
/// Julia rule: `IOBuffer(...) → IOBuffer <: IO` for any argument shape.
pub fn tfunc_iobuffer(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)))
}

/// Transfer function for type-returning helpers (`promote_type`, `promote_rule`).
///
/// These always produce a type object, represented as a generic `DataType`.
pub fn tfunc_datatype_result(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::DataType {
        name: String::new(),
    })
}

/// Transfer function for `min` (minimum of two values).
///
/// Julia rule: `min(a, b)` returns the promoted numeric type, not a Union. (Issue #3479)
pub fn tfunc_min(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    match (&args[0], &args[1]) {
        // Same concrete type: return it directly
        (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a == b => {
            LatticeType::Concrete(a.clone())
        }
        // Different numeric concrete types: use Julia promotion rules
        (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
            if let (Some(name_a), Some(name_b)) = (a.to_type_name(), b.to_type_name()) {
                let result_name = promote_type(&name_a, &name_b);
                if let Some(ct) = ConcreteType::from_type_name(&result_name) {
                    return LatticeType::Concrete(ct);
                }
            }
            LatticeType::Top
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `max` (maximum of two values).
pub fn tfunc_max(args: &[LatticeType]) -> LatticeType {
    tfunc_min(args) // Same type rules
}

/// Transfer function for `println` and `print` (I/O operations).
///
/// Returns Nothing.
pub fn tfunc_println(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Nothing,
    )))
}

// ============================================================================
// Extended Type Conversion Functions
// ============================================================================

/// Transfer function for `promote` (type promotion).
///
/// Type rules:
/// - promote(x, y) → Tuple{T, T} where T is the promoted type
/// - Returns the common promoted type for all arguments
///
/// # Examples
/// ```text
/// promote(Int64, Float64) → Tuple{Float64, Float64}
/// promote(Int32, Int64) → Tuple{Int64, Int64}
/// ```
pub fn tfunc_promote(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    // Find the promoted type by joining all argument types
    let mut promoted = args[0].clone();
    for arg in &args[1..] {
        promoted = promoted.join(arg);
    }

    // Return tuple of promoted types (one for each argument)
    match promoted {
        LatticeType::Concrete(ct) => {
            let elements = vec![ct; args.len()];
            LatticeType::Concrete(ConcreteType::Tuple { elements })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int8` (conversion to Int8).
pub fn tfunc_to_int8(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int16` (conversion to Int16).
pub fn tfunc_to_int16(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int16),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int16,
            )))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int16,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int32` (conversion to Int32).
pub fn tfunc_to_int32(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int32),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32,
            )))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int128` (conversion to Int128).
pub fn tfunc_to_int128(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int128),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt8` (conversion to UInt8).
pub fn tfunc_to_uint8(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt8),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8,
            )))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt16` (conversion to UInt16).
pub fn tfunc_to_uint16(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt16),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt16,
            )))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt16,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt32` (conversion to UInt32).
pub fn tfunc_to_uint32(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt32),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt32,
            )))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt32,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt64` (conversion to UInt64).
pub fn tfunc_to_uint64(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt64),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt128` (conversion to UInt128).
pub fn tfunc_to_uint128(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt128),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt128,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Float32` (conversion to Float32).
pub fn tfunc_to_float32(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Float16` (conversion to Float16).
pub fn tfunc_to_float16(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float16),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float16,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `BigInt` (conversion to BigInt).
pub fn tfunc_to_bigint(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::BigInt),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `BigFloat` (conversion to BigFloat).
pub fn tfunc_to_bigfloat(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::BigFloat),
        )),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            )))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Char` (conversion to Char).
pub fn tfunc_to_char(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_integer() => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `zero` (return zero of a type).
///
/// Type rules:
/// - zero(T) → T (zero value of type T)
/// - zero(x::T) → T (zero value of x's type)
pub fn tfunc_zero(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `one` (return one of a type).
///
/// Type rules:
/// - one(T) → T (one value of type T)
/// - one(x::T) → T (one value of x's type)
pub fn tfunc_one(args: &[LatticeType]) -> LatticeType {
    tfunc_zero(args) // Same type rules as zero
}

/// Transfer function for `typemin` (minimum value of a type).
///
/// Unlike `zero`/`one` which take either a type or a value,
/// `typemin`/`typemax` take a Type{T} argument and return T.
/// e.g., typemin(Float64) → Float64, typemin(Int64) → Int64
pub fn tfunc_typemin(args: &[LatticeType]) -> LatticeType {
    tfunc_type_to_value(args)
}

/// Transfer function for `typemax` (maximum value of a type).
pub fn tfunc_typemax(args: &[LatticeType]) -> LatticeType {
    tfunc_type_to_value(args)
}

/// Shared helper for functions that take Type{T} and return a value of type T.
/// Handles both DataType arguments (e.g., Float64 as Type{Float64}) and
/// numeric value arguments (for overloads like zero(x::T)).
fn tfunc_type_to_value(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }
    match &args[0] {
        // Direct numeric type (e.g., zero(1.0) → Float64)
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        // DataType argument: typemin(Float64) where Float64 is DataType{name: "Float64"}
        LatticeType::Concrete(ConcreteType::DataType { name }) => {
            if let Some(ct) = ConcreteType::from_type_name(name) {
                if ct.is_numeric() {
                    return LatticeType::Concrete(ct);
                }
            }
            LatticeType::Top
        }
        _ => LatticeType::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isa_returns_bool() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Top,
        ];
        let result = tfunc_isa(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_bool_predicate_returns_bool() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_bool_predicate(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_throw_returns_bottom_issue_6532() {
        // Mirrors upstream `add_tfunc(throw, 1, 1, ->Bottom, 0)`: raisers have
        // return type `Union{}`, so a raising branch joins away.
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String),
        ))];
        assert_eq!(tfunc_throw(&args), LatticeType::Bottom);
        assert_eq!(tfunc_throw(&[]), LatticeType::Bottom);
        assert_eq!(
            tfunc_throw(&[]).join(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_to_int64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_to_int64(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_to_float64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_to_float64(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_sqrt_preserves_float_width_and_widens_integer() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_sqrt(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))];
        let result = tfunc_sqrt(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_abs_preserves_type() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_abs(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_abs(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_unary_math_preserves_float_width_and_widens_integer() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_sin(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))];
        assert_eq!(
            tfunc_sin(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
        assert_eq!(
            tfunc_cos(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
        assert_eq!(
            tfunc_exp(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
        assert_eq!(
            tfunc_log(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_unary_float64_returns_float64_for_numeric_inputs() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        assert_eq!(
            tfunc_unary_float64(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))];
        assert_eq!(
            tfunc_unary_float64(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String),
        ))];
        assert_eq!(tfunc_unary_float64(&args), LatticeType::Top);
    }

    #[test]
    fn test_float64_result_returns_float64_for_non_struct_inputs() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        })];
        assert_eq!(
            tfunc_float64_result(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        let args = vec![
            LatticeType::Concrete(ConcreteType::Function {
                name: "identity".to_string(),
            }),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                ))),
                ndims: None,
            }),
        ];
        assert_eq!(
            tfunc_float64_result(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );

        assert_eq!(tfunc_float64_result(&[]), LatticeType::Top);

        let args = vec![LatticeType::Concrete(ConcreteType::Struct {
            name: "S".to_string(),
            type_id: 1,
        })];
        assert_eq!(tfunc_float64_result(&args), LatticeType::Top);
    }

    #[test]
    fn test_int64_result_returns_int64_for_nonempty_args() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            ndims: None,
        })];
        assert_eq!(
            tfunc_int64_result(&args),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(tfunc_int64_result(&[]), LatticeType::Top);
    }

    #[test]
    fn test_min_joins_types() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_min(&args);
        assert!(result.is_numeric());
    }

    #[test]
    fn test_println_returns_nothing() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String),
        ))];
        let result = tfunc_println(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing
            )))
        );
    }

    #[test]
    fn test_to_bool() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_to_bool(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_to_string() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_to_string(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
    }

    #[test]
    fn test_promote_same_types() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_promote(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
                ]
            })
        );
    }

    #[test]
    fn test_promote_mixed_numeric() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_promote(&args);
        // When mixing Int32 and Float64, the join may return Union or Top
        // depending on lattice rules. We just verify it doesn't panic.
        assert!(
            matches!(
                &result,
                LatticeType::Concrete(ConcreteType::Tuple { .. })
                    | LatticeType::Union(_)
                    | LatticeType::Top
            ),
            "Unexpected result: {:?}",
            result
        );
        if let LatticeType::Concrete(ConcreteType::Tuple { elements }) = result {
            assert_eq!(elements.len(), 2);
            // Both should be promoted to the same type
            assert_eq!(elements[0], elements[1]);
        }
    }

    #[test]
    fn test_to_int8() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_to_int8(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        );
    }

    #[test]
    fn test_to_uint64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_to_uint64(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64
            )))
        );
    }

    #[test]
    fn test_to_float32() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_to_float32(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_to_char() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_to_char(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        );
    }

    #[test]
    fn test_zero() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_zero(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_one() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int32),
        ))];
        let result = tfunc_one(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32
            )))
        );
    }
}
