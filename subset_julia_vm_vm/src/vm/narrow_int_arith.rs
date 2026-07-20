use crate::intrinsics::Intrinsic;

use super::value::Value;

/// Narrow integer types whose same-type arithmetic preserves the operand type
/// with modular wrapping, matching upstream Julia.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::vm) enum NarrowIntKind {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
}

impl NarrowIntKind {
    pub(in crate::vm) fn of(value: &Value) -> Option<Self> {
        match value {
            Value::I8(_) => Some(Self::I8),
            Value::I16(_) => Some(Self::I16),
            Value::I32(_) => Some(Self::I32),
            Value::U8(_) => Some(Self::U8),
            Value::U16(_) => Some(Self::U16),
            Value::U32(_) => Some(Self::U32),
            _ => None,
        }
    }

    /// Wrap an I64 arithmetic result back into this narrow integer type.
    #[allow(clippy::cast_sign_loss)]
    pub(in crate::vm) fn wrap_i64(self, value: i64) -> Value {
        match self {
            Self::I8 => Value::I8(value as i8),
            Self::I16 => Value::I16(value as i16),
            Self::I32 => Value::I32(value as i32),
            Self::U8 => Value::U8(value as u8),
            Self::U16 => Value::U16(value as u16),
            Self::U32 => Value::U32(value as u32),
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::vm) enum NarrowIntArithOp {
    Add,
    Sub,
    Mul,
}

/// When both operands share the same narrow integer type (Int8/Int16/Int32 or
/// the unsigned counterparts) and the operation is a type-preserving arithmetic
/// op (`+`, `-`, `*`, `%`), upstream Julia keeps the result in that narrow type
/// using native wrapping arithmetic. Runtime fallback paths normalize such
/// operands to I64, so callers use this helper to wrap the I64 result back into
/// the original narrow type (Issue #5205, #6512).
///
/// `Bool` is treated as the identity for mixed-type narrow int arithmetic:
/// `rem(true, UInt8(5))` → `UInt8(1)` matching upstream Julia (Issue #8883).
/// Bool promotes to whatever narrow integer type the other operand is.
pub(in crate::vm) fn narrow_int_arith_result_kind(
    left: &Value,
    right: &Value,
    fallback_intrinsic: &Intrinsic,
) -> Option<NarrowIntKind> {
    // Type-preserving ops: + - * and % (rem). Division (/) returns Float64
    // and comparisons return Bool, so they are excluded.
    if !matches!(
        fallback_intrinsic,
        Intrinsic::DynamicAdd
            | Intrinsic::DynamicSub
            | Intrinsic::DynamicMul
            | Intrinsic::AddInt
            | Intrinsic::SubInt
            | Intrinsic::MulInt
            | Intrinsic::SremInt // Issue #8883: rem preserves narrow int type
    ) {
        return None;
    }
    narrow_promoted_int_kind(left, right)
}

pub(in crate::vm) fn same_type_narrow_int_arith(
    left: &Value,
    right: &Value,
    op: NarrowIntArithOp,
) -> Option<Value> {
    let kind = same_narrow_int_kind(left, right)?;
    let left = narrow_int_i64(left)?;
    let right = narrow_int_i64(right)?;
    let result = match op {
        NarrowIntArithOp::Add => left.wrapping_add(right),
        NarrowIntArithOp::Sub => left.wrapping_sub(right),
        NarrowIntArithOp::Mul => left.wrapping_mul(right),
    };
    Some(kind.wrap_i64(result))
}

/// Returns the promoted narrow int kind for a pair of values, treating `Bool`
/// as the identity that promotes to the other operand's narrow type.
///
/// `same_narrow_int_kind` (same-type only) is used by `same_type_narrow_int_arith`
/// for the fast-path `+/-/*` ops. This variant also covers `(Bool, narrow_int)`
/// for the `%` operator (Issue #8883).
fn narrow_promoted_int_kind(left: &Value, right: &Value) -> Option<NarrowIntKind> {
    match (left, right) {
        // Bool mixed with a narrow int: Bool promotes to the other type.
        // e.g. rem(true, UInt8(5)) → UInt8(1), rem(true, Int8(5)) → Int8(0)
        (Value::Bool(_), _) => NarrowIntKind::of(right),
        (_, Value::Bool(_)) => NarrowIntKind::of(left),
        // Same narrow int type: keep it as-is.
        _ => same_narrow_int_kind(left, right),
    }
}

fn same_narrow_int_kind(left: &Value, right: &Value) -> Option<NarrowIntKind> {
    let left_kind = NarrowIntKind::of(left)?;
    let right_kind = NarrowIntKind::of(right)?;
    (left_kind == right_kind).then_some(left_kind)
}

fn narrow_int_i64(value: &Value) -> Option<i64> {
    match value {
        Value::I8(value) => Some(i64::from(*value)),
        Value::I16(value) => Some(i64::from(*value)),
        Value::I32(value) => Some(i64::from(*value)),
        Value::U8(value) => Some(i64::from(*value)),
        Value::U16(value) => Some(i64::from(*value)),
        Value::U32(value) => Some(i64::from(*value)),
        _ => None,
    }
}
