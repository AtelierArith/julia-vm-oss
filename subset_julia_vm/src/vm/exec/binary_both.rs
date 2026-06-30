//! Handler for `CallDynamicBinaryBoth` instruction.
//!
//! Extracted from `call_dynamic_binary.rs` to reduce function length (Issue #2935).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// SAFETY: i64→u32 casts for Char arithmetic codepoints; values are used with
// char::from_u32 which safely handles invalid codepoints via unwrap_or.
#![allow(clippy::cast_sign_loss)]

use super::super::value::{array_wrapper_value_to_array_value, ArrayRef, ArrayValue, MemoryRef};
use super::super::*;
#[cfg(debug_assertions)]
use super::call_dynamic::dispatch_debug_log;
use super::call_dynamic_binary::try_string_char_concat;
use super::util::{
    bind_value_to_slot, is_rust_dict_parametric_mismatch, is_struct_dict_bare_mismatch,
};
use super::DispatchAction;
use crate::inference_core::dispatch_resolver::{
    resolve_runtime_core_signature_candidates, RuntimeCoreCandidate,
};
use crate::rng::RngLike;
use crate::vm::narrow_int_arith::narrow_int_arith_result_kind;
#[cfg(test)]
use crate::vm::narrow_int_arith::NarrowIntKind;
use crate::vm::native_array_compat::{
    is_native_array_value, native_array_ref_from_borrowed_value as native_array_ref_from_value,
};
use crate::vm::type_utils::type_objects_equal;

#[cfg(debug_assertions)]
use super::call_dynamic::dispatch_debug_enabled;

fn is_integer_or_bigint_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Bool(_)
            | Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::BigInt(_)
    )
}

fn is_float_value(value: &Value) -> bool {
    matches!(value, Value::F16(_) | Value::F32(_) | Value::F64(_))
}

fn is_numeric_or_big_value(value: &Value) -> bool {
    is_integer_or_bigint_value(value)
        || is_float_value(value)
        || matches!(value, Value::BigFloat(_))
}

fn is_array_like_value(value: &Value, struct_heap: &[StructInstance]) -> bool {
    if matches!(value, Value::Memory(_)) {
        return true;
    }
    if array_wrapper_value_to_array_value(value, struct_heap)
        .ok()
        .flatten()
        .is_some()
    {
        return true;
    }
    match value.runtime_type() {
        crate::types::JuliaType::VectorOf(_) | crate::types::JuliaType::MatrixOf(_) => true,
        crate::types::JuliaType::Struct(name) => name.starts_with("Array{"),
        _ => false,
    }
}

fn is_diagonal_value(value: &Value, struct_heap: &[StructInstance]) -> bool {
    let Some(name) = (match value {
        Value::Struct(s) => Some(&*s.struct_name),
        Value::StructRef(idx) => struct_heap.get(*idx).map(|s| &*s.struct_name),
        _ => None,
    }) else {
        return false;
    };
    let base = name
        .rsplit('.')
        .next()
        .unwrap_or(name)
        .split('{')
        .next()
        .unwrap_or(name);
    base == "Diagonal"
}

fn diagonal_entries(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<Vec<f64>>, VmError> {
    use crate::vm::builtins_linalg::linalg_value_to_array_value;

    if !is_diagonal_value(value, struct_heap) {
        return Ok(None);
    }
    let instance = match value {
        Value::Struct(s) => s,
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .ok_or_else(|| VmError::TypeError(format!("invalid Diagonal StructRef({idx})")))?,
        _ => return Ok(None),
    };
    let Some(diag_field) = instance.values.first() else {
        return Ok(None);
    };
    let arr =
        linalg_value_to_array_value(diag_field.clone(), struct_heap, "*", Some("Diagonal.diag"))?;
    Ok(Some(arr.to_logical_f64_vec()?))
}

/// Intercept binary ops for StaticArray operands (Issue #7964 Phase 2+3).
/// Returns `Some(result)` when the operation is fully handled in Rust.
fn try_static_array_binary_op(left: &Value, right: &Value, op: &Intrinsic) -> Option<Value> {
    use crate::vm::value::{
        static_add, static_matmat, static_matvec, static_scalar_mul, static_sub,
    };
    match (left, right) {
        // Phase 3: inline variant — zero allocation.
        (Value::StaticArrayInline(a), Value::StaticArrayInline(b)) => match op {
            Intrinsic::AddFloat | Intrinsic::AddInt => a.inline_add(b),
            Intrinsic::SubFloat | Intrinsic::SubInt => a.inline_sub(b),
            Intrinsic::MulFloat | Intrinsic::MulInt => {
                if !a.is_vector() && b.is_vector() {
                    a.inline_matvec(b)
                } else if !a.is_vector() && !b.is_vector() {
                    a.inline_matmat(b)
                } else {
                    None
                }
            }
            _ => None,
        },
        (scalar, Value::StaticArrayInline(sv))
            if matches!(op, Intrinsic::MulFloat | Intrinsic::MulInt) =>
        {
            sv.inline_scalar_mul(scalar)
        }
        (Value::StaticArrayInline(sv), scalar)
            if matches!(op, Intrinsic::MulFloat | Intrinsic::MulInt) =>
        {
            sv.inline_scalar_mul(scalar)
        }
        // Phase 2: boxed variant fallback.
        (Value::StaticArray(a), Value::StaticArray(b)) => match op {
            Intrinsic::AddFloat | Intrinsic::AddInt => static_add(a, b),
            Intrinsic::SubFloat | Intrinsic::SubInt => static_sub(a, b),
            Intrinsic::MulFloat | Intrinsic::MulInt => {
                if !a.is_vector() && b.is_vector() {
                    static_matvec(a, b)
                } else if !a.is_vector() && !b.is_vector() {
                    static_matmat(a, b)
                } else {
                    None
                }
            }
            _ => None,
        },
        (scalar, Value::StaticArray(sv))
            if matches!(op, Intrinsic::MulFloat | Intrinsic::MulInt) =>
        {
            static_scalar_mul(scalar, sv)
        }
        (Value::StaticArray(sv), scalar)
            if matches!(op, Intrinsic::MulFloat | Intrinsic::MulInt) =>
        {
            static_scalar_mul(scalar, sv)
        }
        _ => None,
    }
}

pub(super) fn try_matrix_diagonal_mul<R: RngLike>(
    vm: &mut Vm<R>,
    left: &Value,
    right: &Value,
) -> Result<Option<Value>, VmError> {
    use crate::vm::builtins_linalg::linalg_value_to_array_value;
    use crate::vm::value::ArrayValue;

    let left_is_diag = is_diagonal_value(left, &vm.struct_heap);
    let right_is_diag = is_diagonal_value(right, &vm.struct_heap);
    if !left_is_diag && !right_is_diag {
        return Ok(None);
    }

    let (matrix_val, diag_val, matrix_on_left) = if left_is_diag {
        (right, left, false)
    } else {
        (left, right, true)
    };

    let Some(diag) = diagonal_entries(diag_val, &vm.struct_heap)? else {
        return Ok(None);
    };
    let n = diag.len();

    let mat_arr =
        linalg_value_to_array_value(matrix_val.clone(), &vm.struct_heap, "*", Some("matrix"))?;
    let ndims = mat_arr.shape.len();

    if ndims == 1 {
        if mat_arr.shape[0] != n {
            return Err(VmError::ErrorException(format!(
                "DimensionMismatch: Diagonal matrix has {n} columns, but vector has {} elements",
                mat_arr.shape[0]
            )));
        }
        let mut result = vec![0.0; n];
        let data = mat_arr.to_logical_f64_vec()?;
        for j in 0..n {
            result[j] = data[j] * diag[j];
        }
        return Ok(Some(vm.array_value_to_wrapper(
            ArrayValue::memory_first_from_f64(result, vec![n]),
        )?));
    }

    if ndims != 2 {
        return Err(VmError::ErrorException(format!(
            "DimensionMismatch: Diagonal * A requires A to be 1D or 2D, got {ndims}D"
        )));
    }

    let nrows = mat_arr.shape[0];
    let ncols = mat_arr.shape[1];
    let data = mat_arr.to_logical_f64_vec()?;
    let mut result = vec![0.0; nrows * ncols];

    if matrix_on_left {
        if ncols != n {
            return Err(VmError::ErrorException(format!(
                "DimensionMismatch: Diagonal matrix has {n} columns, but A has {ncols} columns"
            )));
        }
        for (j, &dj) in diag.iter().enumerate().take(ncols) {
            for i in 0..nrows {
                let idx = i + j * nrows;
                result[idx] = data[idx] * dj;
            }
        }
    } else {
        if nrows != n {
            return Err(VmError::ErrorException(format!(
                "DimensionMismatch: Diagonal matrix has {n} rows, but A has {nrows} rows"
            )));
        }
        for j in 0..ncols {
            for (i, &di) in diag.iter().enumerate().take(nrows) {
                let idx = i + j * nrows;
                result[idx] = di * data[idx];
            }
        }
    }

    Ok(Some(vm.array_value_to_wrapper(
        ArrayValue::memory_first_from_f64(result, vec![nrows, ncols]),
    )?))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NumericInteger {
    NonNegative(u128),
    Negative(i128),
}

fn signed_integer_value(value: i128) -> NumericInteger {
    if value >= 0 {
        NumericInteger::NonNegative(value.cast_unsigned())
    } else {
        NumericInteger::Negative(value)
    }
}

fn numeric_integer_value(value: &Value) -> Option<NumericInteger> {
    match value {
        Value::Bool(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::I8(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I16(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I32(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I64(v) => Some(signed_integer_value(i128::from(*v))),
        Value::I128(v) => Some(signed_integer_value(*v)),
        Value::U8(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U16(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U32(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U64(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
        Value::U128(v) => Some(NumericInteger::NonNegative(*v)),
        _ => None,
    }
}

fn numeric_integer_values_equal(left: &Value, right: &Value) -> Option<bool> {
    Some(numeric_integer_value(left)? == numeric_integer_value(right)?)
}

fn is_supported_array_scalar_value(value: &Value) -> bool {
    matches!(
        value,
        Value::I64(_) | Value::F64(_) | Value::F32(_) | Value::Bool(_)
    )
}

fn binary_dispatch_type_name(value: &Value, fallback: &str) -> String {
    match value {
        Value::DataType(jt) => format!("Type{{{}}}", jt.name()),
        _ => fallback.to_string(),
    }
}

fn values_equal_for_memory_boundary(left: Option<Value>, right: Option<Value>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    values_equal_for_operator(&left, &right)
}

fn values_equal_for_operator(left: &Value, right: &Value) -> bool {
    if let Some(result) = numeric_integer_values_equal(left, right) {
        return result;
    }
    match (left, right) {
        (Value::F16(x), Value::F16(y)) => x == y,
        (Value::F32(x), Value::F32(y)) => x == y,
        (Value::F64(x), Value::F64(y)) => x == y,
        (Value::I64(x), Value::F64(y)) | (Value::F64(y), Value::I64(x)) => (*x as f64) == *y,
        (Value::I64(x), Value::F32(y)) | (Value::F32(y), Value::I64(x)) => (*x as f32) == *y,
        (Value::I64(x), Value::F16(y)) | (Value::F16(y), Value::I64(x)) => {
            half::f16::from_f64(*x as f64) == *y
        }
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::BigFloat(x), Value::BigFloat(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Nothing, Value::Nothing) => true,
        (Value::Missing, Value::Missing) => true,
        (Value::Symbol(x), Value::Symbol(y)) => x == y,
        (Value::DataType(x), Value::DataType(y)) => type_objects_equal(x, y),
        (Value::RuntimeTypeVar(x), Value::RuntimeTypeVar(y)) => x.id == y.id,
        // Tuple `==` and Core.SimpleVector `==` are both element-wise (Issue #4722).
        (Value::Tuple(x), Value::Tuple(y)) | (Value::SimpleVector(x), Value::SimpleVector(y)) => {
            x.elements.len() == y.elements.len()
                && x.elements
                    .iter()
                    .zip(y.elements.iter())
                    .all(|(xv, yv)| values_equal_for_operator(xv, yv))
        }
        _ if is_native_array_value(left) && is_native_array_value(right) => {
            native_array_values_equal(left, right).unwrap_or(false)
        }
        _ => format!("{:?}", left) == format!("{:?}", right),
    }
}

fn values_equal_for_tuple_operator(left: &Value, right: &Value) -> bool {
    values_equal_for_operator(left, right)
}

fn native_array_values_equal(left: &Value, right: &Value) -> Option<bool> {
    let left_ref = native_array_ref_from_value(left)?;
    let right_ref = native_array_ref_from_value(right)?;
    let left_arr = left_ref.borrow();
    let right_arr = right_ref.borrow();
    if left_arr.shape != right_arr.shape || left_arr.len() != right_arr.len() {
        return Some(false);
    }
    Some((0..left_arr.len()).all(|i| {
        values_equal_for_memory_boundary(left_arr.get_linear(i).ok(), right_arr.get_linear(i).ok())
    }))
}

/// Return the Memory/Array pair when one side of `(left, right)` is a
/// `Value::Memory` and the other is a legacy native array carrier (in either
/// order). Lets the matmul / scalar-array equality bridge below avoid
/// cross-carrier tuple-pattern arms by routing the native-array destructure
/// through the shared [`super::super::value::native_array_value_ref`] helper
/// while Issue #3908 retires the legacy carrier.
fn memory_array_pair<'a>(
    left: &'a Value,
    right: &'a Value,
) -> Option<(&'a MemoryRef, &'a ArrayRef)> {
    if let Value::Memory(mem) = left {
        if let Some(arr) = native_array_ref_from_value(right) {
            return Some((mem, arr));
        }
    }
    if let Value::Memory(mem) = right {
        if let Some(arr) = native_array_ref_from_value(left) {
            return Some((mem, arr));
        }
    }
    None
}

fn memory_array_values_equal(memory: &MemoryRef, array: &ArrayRef) -> bool {
    let memory = memory.borrow();
    let array = array.borrow();
    if array.shape.as_slice() != [memory.len()] || array.len() != memory.len() {
        return false;
    }

    for i in 0..memory.len() {
        // Issue #3908: route through Memory's 1-indexed public boundary
        // instead of touching MemoryValue::data directly so the equality
        // bridge keeps tracking the same logical reads as ArrayValue.
        let mem_value = memory.get(i + 1).ok();
        if !values_equal_for_memory_boundary(mem_value, array.get_linear(i).ok()) {
            return false;
        }
    }
    true
}

fn runtime_array_value_from_value(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<ArrayValue>, VmError> {
    if let Some(array_ref) = native_array_ref_from_value(value) {
        return Ok(Some(array_ref.borrow().clone()));
    }

    array_wrapper_value_to_array_value(value, struct_heap)
}

fn is_struct_like_value(value: &Value) -> bool {
    matches!(value, Value::Struct(_) | Value::StructRef(_))
}

fn struct_scalar_array_pair(
    left: &Value,
    right: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<(Value, ArrayValue)>, VmError> {
    if is_struct_like_value(left) && runtime_array_value_from_value(left, struct_heap)?.is_none() {
        if let Some(array) = runtime_array_value_from_value(right, struct_heap)? {
            return Ok(Some((left.clone(), array)));
        }
    }
    if is_struct_like_value(right) && runtime_array_value_from_value(right, struct_heap)?.is_none()
    {
        if let Some(array) = runtime_array_value_from_value(left, struct_heap)? {
            return Ok(Some((right.clone(), array)));
        }
    }
    Ok(None)
}

fn int_or_f64_scalar_value(value: &Value) -> bool {
    matches!(value, Value::I64(_) | Value::F64(_))
}

fn real_scalar_array_pair(
    left: &Value,
    right: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<(Value, ArrayValue)>, VmError> {
    if int_or_f64_scalar_value(left) {
        if let Some(array) = runtime_array_value_from_value(right, struct_heap)? {
            return Ok(Some((left.clone(), array)));
        }
    }
    if int_or_f64_scalar_value(right) {
        if let Some(array) = runtime_array_value_from_value(left, struct_heap)? {
            return Ok(Some((right.clone(), array)));
        }
    }
    Ok(None)
}

fn runtime_array_pair(
    left: &Value,
    right: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<(ArrayValue, ArrayValue)>, VmError> {
    let Some(left_array) = runtime_array_value_from_value(left, struct_heap)? else {
        return Ok(None);
    };
    let Some(right_array) = runtime_array_value_from_value(right, struct_heap)? else {
        return Ok(None);
    };
    Ok(Some((left_array, right_array)))
}

fn bigfloat_intrinsic_handles(left: &Value, right: &Value) -> bool {
    let has_bigfloat = matches!(
        (left, right),
        (Value::BigFloat(_), _) | (_, Value::BigFloat(_))
    );
    let mixed_bigint_float = (matches!(left, Value::BigInt(_)) && is_float_value(right))
        || (is_float_value(left) && matches!(right, Value::BigInt(_)));

    (has_bigfloat || mixed_bigint_float)
        && is_numeric_or_big_value(left)
        && is_numeric_or_big_value(right)
}

fn bigint_intrinsic_handles(left: &Value, right: &Value) -> bool {
    (matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)))
        && is_integer_or_bigint_value(left)
        && is_integer_or_bigint_value(right)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryBothFallbackPrecedence {
    SharedResolverFirst,
    PrimitiveFallbackFirst,
}

fn is_string_type_name(name: &str) -> bool {
    name == "String" || name == "AbstractString"
}

fn binary_both_fallback_precedence(left: &Value, right: &Value) -> BinaryBothFallbackPrecedence {
    // Issue #2437/#2492/#2498: primitive and BigInt/BigFloat pairs are handled
    // by VM intrinsics before generic `Number` methods to avoid promotion
    // recursion and preserve primitive arithmetic semantics. Everything else
    // reaches the shared resolver before fallback (Issue #3910).
    let both_primitive = super::super::util::is_builtin_numeric_value(left)
        && super::super::util::is_builtin_numeric_value(right);
    if both_primitive
        || bigint_intrinsic_handles(left, right)
        || bigfloat_intrinsic_handles(left, right)
    {
        BinaryBothFallbackPrecedence::PrimitiveFallbackFirst
    } else {
        BinaryBothFallbackPrecedence::SharedResolverFirst
    }
}

/// Issue #8168: only memoize the binary-both resolver decision for two
/// `Struct`/`StructRef` operands. Their matched method is determined purely by
/// the operand type names — none of the value-dependent guards in
/// `resolve_binary_both_candidate` (rust-`Dict`/`Memory` parametric mismatches)
/// can fire for two struct operands — so a `(left_type_hash, right_type_hash)`
/// key returns exactly what the resolver would compute. Every other operand
/// shape stays on the uncached path, keeping correctness identical.
fn binary_both_dispatch_cacheable(left: &Value, right: &Value) -> bool {
    matches!(left, Value::Struct(_) | Value::StructRef(_))
        && matches!(right, Value::Struct(_) | Value::StructRef(_))
}

fn fast_primitive_binary_both(
    left: &Value,
    right: &Value,
    fallback_intrinsic: &Intrinsic,
) -> Option<Result<Value, VmError>> {
    match (left, right) {
        (Value::F64(a), Value::F64(b)) => {
            let value = match fallback_intrinsic {
                Intrinsic::AddFloat => Value::F64(a + b),
                Intrinsic::SubFloat => Value::F64(a - b),
                Intrinsic::MulFloat => Value::F64(a * b),
                Intrinsic::DivFloat => Value::F64(a / b),
                Intrinsic::EqFloat => Value::Bool(a == b),
                Intrinsic::NeFloat => Value::Bool(a != b),
                Intrinsic::LtFloat => Value::Bool(a < b),
                Intrinsic::LeFloat => Value::Bool(a <= b),
                Intrinsic::GtFloat => Value::Bool(a > b),
                Intrinsic::GeFloat => Value::Bool(a >= b),
                _ => return None,
            };
            Some(Ok(value))
        }
        (Value::I64(a), Value::I64(b)) => {
            let value = match fallback_intrinsic {
                Intrinsic::AddFloat | Intrinsic::AddInt => Value::I64(a.wrapping_add(*b)),
                Intrinsic::SubFloat | Intrinsic::SubInt => Value::I64(a.wrapping_sub(*b)),
                Intrinsic::MulFloat | Intrinsic::MulInt => Value::I64(a.wrapping_mul(*b)),
                Intrinsic::DivFloat => Value::F64(*a as f64 / *b as f64),
                Intrinsic::SdivInt => {
                    if *b == 0 {
                        return Some(Err(VmError::DivisionByZero));
                    }
                    Value::I64(a / b)
                }
                Intrinsic::SremInt => {
                    if *b == 0 {
                        return Some(Err(VmError::DivisionByZero));
                    }
                    Value::I64(a % b)
                }
                Intrinsic::EqFloat | Intrinsic::EqInt => Value::Bool(a == b),
                Intrinsic::NeFloat | Intrinsic::NeInt => Value::Bool(a != b),
                Intrinsic::LtFloat | Intrinsic::SltInt => Value::Bool(a < b),
                Intrinsic::LeFloat | Intrinsic::SleInt => Value::Bool(a <= b),
                Intrinsic::GtFloat | Intrinsic::SgtInt => Value::Bool(a > b),
                Intrinsic::GeFloat | Intrinsic::SgeInt => Value::Bool(a >= b),
                _ => return None,
            };
            Some(Ok(value))
        }
        _ => None,
    }
}

/// Same-type numeric operation table (Issue #6338).
///
/// Upstream Julia implements binary numeric operators only for SAME-type
/// pairs (`+(x::T, y::T) where {T<:BitInteger} = add_int(x, y)` in
/// `julia/base/int.jl`); heterogeneous pairs are first promoted to a common
/// type by the generic fallback (`+(x::Number, y::Number) =
/// +(promote(x,y)...)` in `julia/base/promotion.jl`). This helper mirrors the
/// same-type layer: it delegates the hot raw pairs (Int64×Int64,
/// Float64×Float64) to [`fast_primitive_binary_both`] and extends the Float64
/// table with the floor-based `mod`/`div` semantics that the promoted
/// mixed-type pairs need (`a - floor(a/b)*b` / `floor(a/b)`, matching the
/// dedicated arms this path replaced). It also owns the Float32×Float32 and
/// Float16×Float16 tables (moved verbatim from the legacy
/// `float32-intrinsics` / `float16-intrinsics` arms; F16 computes in F64 to
/// leverage hardware FP, then narrows back — Issue #3621/#3750). Returns
/// `None` for ops the legacy fallback chain still owns.
fn same_type_fast_path(
    op: &Intrinsic,
    left: &Value,
    right: &Value,
) -> Option<Result<Value, VmError>> {
    if let Some(result) = fast_primitive_binary_both(left, right, op) {
        return Some(result);
    }
    match (left, right) {
        (Value::F64(a), Value::F64(b)) => match op {
            // Julia's mod for floats: a - floor(a/b) * b (always same sign as b).
            Intrinsic::SremInt => Some(Ok(Value::F64(a - (a / b).floor() * b))),
            // Julia's div for floats: floor division.
            Intrinsic::SdivInt => Some(Ok(Value::F64((a / b).floor()))),
            _ => None,
        },
        (Value::F32(a), Value::F32(b)) => {
            let value = match op {
                Intrinsic::AddFloat => Value::F32(a + b),
                Intrinsic::SubFloat => Value::F32(a - b),
                Intrinsic::MulFloat => Value::F32(a * b),
                Intrinsic::DivFloat => Value::F32(a / b),
                Intrinsic::EqFloat => Value::Bool(a == b),
                Intrinsic::NeFloat => Value::Bool(a != b),
                Intrinsic::LtFloat => Value::Bool(a < b),
                Intrinsic::LeFloat => Value::Bool(a <= b),
                Intrinsic::GtFloat => Value::Bool(a > b),
                Intrinsic::GeFloat => Value::Bool(a >= b),
                // Julia's mod: a - floor(a/b) * b (Issue #1776).
                Intrinsic::SremInt => Value::F32(a - (a / b).floor() * b),
                // Julia's div: floor division (Issue #1849).
                Intrinsic::SdivInt => Value::F32((a / b).floor()),
                _ => return None,
            };
            Some(Ok(value))
        }
        (Value::F16(a), Value::F16(b)) => {
            let (a, b) = (a.to_f64(), b.to_f64());
            let value = match op {
                Intrinsic::AddFloat => Value::F16(half::f16::from_f64(a + b)),
                Intrinsic::SubFloat => Value::F16(half::f16::from_f64(a - b)),
                Intrinsic::MulFloat => Value::F16(half::f16::from_f64(a * b)),
                Intrinsic::DivFloat => Value::F16(half::f16::from_f64(a / b)),
                Intrinsic::EqFloat => Value::Bool(a == b),
                Intrinsic::NeFloat => Value::Bool(a != b),
                Intrinsic::LtFloat => Value::Bool(a < b),
                Intrinsic::LeFloat => Value::Bool(a <= b),
                Intrinsic::GtFloat => Value::Bool(a > b),
                Intrinsic::GeFloat => Value::Bool(a >= b),
                Intrinsic::SremInt => Value::F16(half::f16::from_f64(a - (a / b).floor() * b)),
                Intrinsic::SdivInt => Value::F16(half::f16::from_f64((a / b).floor())),
                _ => return None,
            };
            Some(Ok(value))
        }
        _ => None,
    }
}

/// What to do when a promoted pair's operation has no entry in
/// [`same_type_fast_path`] (Issue #6338).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PromotedPairPolicy {
    /// The promote path fully replaced a dedicated mixed-type arm: ops
    /// without a same-type table entry raise the arm's original
    /// `unsupported_op` error. The label string is preserved verbatim so the
    /// observable error message is unchanged.
    RaiseUnsupported(&'static str),
    /// The legacy fallback chain still owns the remaining ops for this pair:
    /// fall through with the ORIGINAL (unpromoted) operands.
    FallThrough,
}

/// Promote a heterogeneous primitive numeric pair to its common type
/// (Issue #6338), mirroring `julia/base/promotion.jl` and the compile-time
/// rules in `compile/promotion.rs` / docs/vm/PROMOTION.md
/// (`promote_type(Int64, Float64) == promote_type(Float32, Float64) ==
/// promote_type(Float16, Float64) == Float64`; `promote_type(Float16,
/// Float32) == promote_type(Float32, Int64) == promote_type(Float32,
/// Int128) == Float32`).
///
/// Only pairs whose CURRENT dynamic-dispatch semantics are exactly
/// "promote, then same-type op" are listed here; behavior-exception pairs
/// (Bool operands, Float16×Int — which narrows the RESULT instead of the
/// int operand, unsigned widths, Int128×Int64/F64, BigInt/BigFloat, Char)
/// must keep their explicit arms (see Issue #5966 on why missing pairs must
/// never silently reach the Pure Julia promote fallback).
fn promote_numeric_pair(left: &Value, right: &Value) -> Option<(Value, Value, PromotedPairPolicy)> {
    use PromotedPairPolicy::{FallThrough, RaiseUnsupported};
    let f64p = |a: f64, b: f64, p: PromotedPairPolicy| Some((Value::F64(a), Value::F64(b), p));
    let f32p = |a: f32, b: f32, p: PromotedPairPolicy| Some((Value::F32(a), Value::F32(b), p));
    // Per-pair unhandled-op policies. Labels are preserved verbatim from the
    // dedicated mixed-type arms each promote entry replaced. The Int64×Float64
    // family instead falls through: the generic-primitive-intrinsic arm below
    // still owns its `÷`/`^`/int-named comparison intrinsics. `p_f32_int` is
    // the literal "Float32-Int64" even for Int128 (legacy arm quirk), and the
    // int operand is converted to f32 FIRST (true promote semantics).
    let p_f16_f64 = RaiseUnsupported("Float16-Float64");
    let p_f32_f64 = RaiseUnsupported("Float32-Float64");
    let p_f16_f32 = RaiseUnsupported("Float16-Float32");
    let p_f32_int = RaiseUnsupported("Float32-Int64");
    match (left, right) {
        (Value::F16(a), Value::F64(b)) => f64p(a.to_f64(), *b, p_f16_f64),
        (Value::F64(a), Value::F16(b)) => f64p(*a, b.to_f64(), p_f16_f64),
        (Value::F32(a), Value::F64(b)) => f64p(f64::from(*a), *b, p_f32_f64),
        (Value::F64(a), Value::F32(b)) => f64p(*a, f64::from(*b), p_f32_f64),
        (Value::I64(a), Value::F64(b)) => f64p(*a as f64, *b, FallThrough),
        (Value::F64(a), Value::I64(b)) => f64p(*a, *b as f64, FallThrough),
        (Value::F16(a), Value::F32(b)) => f32p(a.to_f32(), *b, p_f16_f32),
        (Value::F32(a), Value::F16(b)) => f32p(*a, b.to_f32(), p_f16_f32),
        (Value::F32(a), Value::I64(b)) => f32p(*a, *b as f32, p_f32_int),
        (Value::I64(a), Value::F32(b)) => f32p(*a as f32, *b, p_f32_int),
        (Value::F32(a), Value::I128(b)) => f32p(*a, *b as f32, p_f32_int),
        (Value::I128(a), Value::F32(b)) => f32p(*a as f32, *b, p_f32_int),
        _ => None,
    }
}

/// Exact mixed integer/float comparison (Issue #8187, generalized to all widths
/// in #8199).
///
/// [`promote_numeric_pair`] widens the integer operand to `f64`/`f32` before
/// applying the same-type op. That is correct for arithmetic but LOSSY for
/// comparisons: once `|i|` exceeds the float's exact-integer range (`2^53` for
/// `Float64`, `2^24` for `Float32`) the integer rounds, so e.g.
/// `9007199254740993 == 9.007199254740992e15` wrongly became `true`. For a
/// comparison intrinsic on any mixed fixed-width integer / fixed IEEE-float pair
/// (`Int*`/`UInt*` × `Float16`/`Float32`/`Float64`, either order) this returns
/// the value-based result (matching upstream `base/float.jl`), computed without
/// ever widening the integer. Returns `None` for non-comparison ops or non-mixed
/// pairs, leaving the promote/arithmetic handling untouched.
fn exact_int_float_comparison(left: &Value, right: &Value, op: &Intrinsic) -> Option<bool> {
    use crate::vm::numeric_identity::mixed_int_float_ordering;
    use std::cmp::Ordering;
    // Ordering of `left` vs `right` (inner NaN -> None: every relational op is
    // false, `!=` is true). Outer `None` -> not a mixed pair, fall through.
    let ord = mixed_int_float_ordering(left, right)?;
    Some(match op {
        Intrinsic::EqFloat | Intrinsic::EqInt => ord == Some(Ordering::Equal),
        Intrinsic::NeFloat | Intrinsic::NeInt => ord != Some(Ordering::Equal),
        Intrinsic::LtFloat | Intrinsic::SltInt => ord == Some(Ordering::Less),
        Intrinsic::LeFloat | Intrinsic::SleInt => {
            matches!(ord, Some(Ordering::Less | Ordering::Equal))
        }
        Intrinsic::GtFloat | Intrinsic::SgtInt => ord == Some(Ordering::Greater),
        Intrinsic::GeFloat | Intrinsic::SgeInt => {
            matches!(ord, Some(Ordering::Greater | Ordering::Equal))
        }
        _ => return None,
    })
}

impl<R: RngLike> Vm<R> {
    /// Derive and memoize the `(left, right)` expected-type-name pair for the
    /// given binary dispatch candidates (Issue #6496).
    ///
    /// The payload carries only function indices; expected name strings are
    /// reproduced from each candidate's `FunctionInfo` using the canonical
    /// projection rules, with the shared vararg expansion as fallback. Equality
    /// with the compile-time `MethodSig` projection is pinned by
    /// `base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495`
    /// in `compile/cache.rs`. Results are memoized in
    /// `Vm::binary_signature_cache` because `CallDynamicBinaryBoth` has no
    /// call-site dispatch cache and re-renders would run per dispatch.
    pub(super) fn ensure_binary_candidate_signatures(&mut self, candidates: &[usize]) {
        for &func_index in candidates {
            if self.binary_signature_cache.contains_key(&func_index) {
                continue;
            }
            let derived = self.functions.get(func_index).and_then(|func| {
                let param_types: Option<Vec<crate::types::JuliaType>> =
                    if func.param_julia_types.len() == 2 {
                        Some(func.param_julia_types.clone())
                    } else {
                        crate::vm::dispatch_binding::expanded_param_types_for_call(func, 2)
                            .filter(|types| types.len() == 2)
                    };
                param_types.map(|types| {
                    crate::vm::dispatch_binding::build_runtime_candidate_core_signature(
                        &types,
                        &func.type_params,
                    )
                })
            });
            self.binary_signature_cache.insert(func_index, derived);
        }
    }

    /// Memoized `(left, right)` expected type names for a binary dispatch
    /// candidate; `None` when the signature could not be derived (such
    /// candidates are excluded from scoring). Callers must run
    /// [`Self::ensure_binary_candidate_signatures`] first.
    pub(super) fn binary_candidate_signature(&self, func_index: usize) -> Option<(&str, &str)> {
        match self.binary_signature_cache.get(&func_index) {
            Some(Some(sig)) => match sig.rendered.as_slice() {
                [left, right] => Some((left.as_str(), right.as_str())),
                _ => None,
            },
            _ => None,
        }
    }

    /// Memoized structured candidate signature (per-slot cores + optional
    /// `core_signature` gate) for a binary dispatch candidate (Issue #6502
    /// slice 2). Callers must run
    /// [`Self::ensure_binary_candidate_signatures`] first.
    pub(super) fn binary_candidate_core_signature(
        &self,
        func_index: usize,
    ) -> Option<&crate::vm::dispatch_binding::RuntimeCandidateCoreSignature> {
        match self.binary_signature_cache.get(&func_index) {
            Some(Some(sig)) if sig.slots.len() == 2 => Some(sig),
            _ => None,
        }
    }

    /// Whether a user-defined method for this operator covers a
    /// `(String, String)` operand pair (e.g. `Base.:(==)(::String, ::String)`).
    /// Used to defer the built-in String fast path to the shared resolver so
    /// the user method is honored when String operands arrive via `Any`-typed
    /// dynamic dispatch (Issue #5435). `candidates` is already specific to the
    /// operator being dispatched, so any matching entry means the user
    /// overrode this operator.
    fn binary_candidates_have_string_user_override(&mut self, candidates: &[usize]) -> bool {
        self.ensure_binary_candidate_signatures(candidates);
        candidates.iter().any(|&func_index| {
            self.binary_candidate_signature(func_index)
                .is_some_and(|(left, right)| {
                    is_string_type_name(left) && is_string_type_name(right)
                })
        })
    }

    /// Whether any candidate is an exact `(Bool, Bool)` method, letting Bool
    /// equality resolve through the shared resolver before the primitive
    /// fallback.
    fn binary_candidates_have_exact_bool_equality(&mut self, candidates: &[usize]) -> bool {
        self.ensure_binary_candidate_signatures(candidates);
        candidates.iter().any(|&func_index| {
            self.binary_candidate_signature(func_index)
                .is_some_and(|(left, right)| left == "Bool" && right == "Bool")
        })
    }

    /// Handle `CallDynamicBinaryBoth` dispatch.
    ///
    /// Runtime dispatch for binary operators when both operands are `Any`.
    /// Falls back to intrinsic operations for primitive numeric types.
    /// Run the shared scored resolver for a `CallDynamicBinaryBoth` dispatch and
    /// return the matched candidate's function index. Extracted from
    /// `execute_binary_both` (Issue #8168) so the decision can be memoized per
    /// call site. The caller must have invoked
    /// [`Self::ensure_binary_candidate_signatures`] for `candidates` first.
    fn resolve_binary_both_candidate(
        &self,
        candidates: &[usize],
        _actual_type_names: &[&str; 2],
        left: &Value,
        right: &Value,
    ) -> Option<usize> {
        let actual_cores = [
            crate::vm::dispatch_binding::runtime_actual_core_type(
                &self.dispatch_julia_type_for_value(left),
            ),
            crate::vm::dispatch_binding::runtime_actual_core_type(
                &self.dispatch_julia_type_for_value(right),
            ),
        ];
        resolve_runtime_core_signature_candidates(
            &self.struct_hierarchy,
            candidates
                .iter()
                .enumerate()
                .filter_map(|(pos, &func_index)| {
                    let sig = self.binary_candidate_core_signature(func_index)?;
                    let (left_expected, right_expected) =
                        (sig.rendered[0].as_str(), sig.rendered[1].as_str());

                    // Value::Dict (Rust-backed) must not match parametric Dict{K,V}
                    // Pure Julia methods that expect StructRef (Issue #2748).
                    if is_rust_dict_parametric_mismatch(left, left_expected)
                        || is_rust_dict_parametric_mismatch(right, right_expected)
                        || is_struct_dict_bare_mismatch(left, left_expected, &self.struct_heap)
                        || is_struct_dict_bare_mismatch(right, right_expected, &self.struct_heap)
                    {
                        return None;
                    }

                    Some(RuntimeCoreCandidate {
                        idx: pos,
                        slots: [&sig.slots[0], &sig.slots[1]],
                        signature: sig.signature.as_ref(),
                    })
                }),
            &actual_cores,
            |actual, expected| self.check_subtype_core(actual, expected),
        )
        .map(|(pos, _)| candidates[pos])
    }

    pub(super) fn execute_binary_both(
        &mut self,
        fallback_intrinsic: &Intrinsic,
        candidates: &[usize],
    ) -> Result<DispatchAction, VmError> {
        // Pop both arguments
        let right = self.stack.pop_value()?;
        let left = self.stack.pop_value()?;

        // Issue #7964 Phase 2: Rust-level arithmetic for flat StaticArray values.
        // Intercept before the shared resolver so the call never traverses the Julia
        // arraymath.jl dispatch chain.
        if let Some(result) = try_static_array_binary_op(&left, &right, fallback_intrinsic) {
            self.stack.push(result);
            return Ok(DispatchAction::Continue);
        }

        if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
            if let Some(result) = try_matrix_diagonal_mul(self, &left, &right)? {
                self.stack.push(result);
                return Ok(DispatchAction::Continue);
            }
        }

        if let Some(result) = fast_primitive_binary_both(&left, &right, fallback_intrinsic) {
            crate::vm::profiler::record_event("BinaryBothPrimitiveFastHit");
            match result {
                Ok(value) => {
                    self.stack.push(value);
                }
                Err(err) => {
                    self.raise(err)?;
                    return Ok(DispatchAction::Continue);
                }
            }
            return Ok(DispatchAction::Continue);
        }
        crate::vm::profiler::record_event("BinaryBothPrimitiveFastMiss");

        // Issue #8187: a mixed Int64/Float64 *comparison* must be value-based,
        // not promote-then-compare (which rounds the integer for |i| > 2^53).
        // Intercept before the promote block below; arithmetic still promotes.
        if let Some(result) = exact_int_float_comparison(&left, &right, fallback_intrinsic) {
            crate::vm::profiler::record_event("BinaryBothExactIntFloatCmpHit");
            self.stack.push(Value::Bool(result));
            return Ok(DispatchAction::Continue);
        }

        // Issue #6338: heterogeneous numeric pairs whose semantics are exactly
        // "promote to the common type, then apply the same-type operation"
        // (upstream `+(x::Number, y::Number) = +(promote(x,y)...)` in
        // julia/base/promotion.jl). All pairs listed in `promote_numeric_pair`
        // are builtin-primitive pairs, i.e. `PrimitiveFallbackFirst`
        // preemption already skips the shared resolver for them today, so
        // intercepting before the resolver bypasses no user method.
        if let Some((promoted_left, promoted_right, policy)) = promote_numeric_pair(&left, &right) {
            let op_in_promote_scope = match policy {
                PromotedPairPolicy::RaiseUnsupported(_) => true,
                // (Int64, Float64): `÷` keeps its legacy route below
                // (`execute_intrinsic(SdivInt)` → `pop_i64` TypeError); only
                // ops proven identical to promote-then-same-type are folded.
                PromotedPairPolicy::FallThrough => {
                    !matches!(fallback_intrinsic, Intrinsic::SdivInt)
                }
            };
            if op_in_promote_scope {
                if let Some(result) =
                    same_type_fast_path(fallback_intrinsic, &promoted_left, &promoted_right)
                {
                    crate::vm::profiler::record_event("BinaryBothPromotePairHit");
                    match result {
                        Ok(value) => self.stack.push(value),
                        Err(err) => {
                            self.raise(err)?;
                        }
                    }
                    return Ok(DispatchAction::Continue);
                }
            }
            if let PromotedPairPolicy::RaiseUnsupported(pair_label) = policy {
                // Identical to the `_` branches of the dedicated mixed-type
                // arms this path replaced (same label, same error kind).
                self.raise(VmError::unsupported_op(pair_label, fallback_intrinsic))?;
                return Ok(DispatchAction::Continue);
            }
            // FallThrough: the legacy fallback chain below still owns this op.
        }

        // Issue #5435: skip the built-in String fast path when the user has
        // overridden this operator for `(String, String)`. The operands reached
        // here via `Any`-typed dynamic dispatch (e.g. `f(a::Any,b::Any) = a == b`
        // called with strings), so the user method — present in `candidates` —
        // must win, just as it does for a direct `"a" == "b"` call. Falling
        // through lets the shared resolver below dispatch to it. Mirrors the Bool
        // equality guard (`binary_candidates_have_exact_bool_equality`). The
        // override probe runs only for String operand pairs so non-String
        // dispatches never derive candidate signatures here (Issue #6496).
        if let (Value::Str(left_str), Value::Str(right_str)) = (&left, &right) {
            let result = match fallback_intrinsic {
                Intrinsic::EqFloat | Intrinsic::EqInt => Some(left_str == right_str),
                Intrinsic::NeFloat | Intrinsic::NeInt => Some(left_str != right_str),
                Intrinsic::LtFloat | Intrinsic::SltInt => Some(left_str < right_str),
                Intrinsic::LeFloat | Intrinsic::SleInt => Some(left_str <= right_str),
                Intrinsic::GtFloat | Intrinsic::SgtInt => Some(left_str > right_str),
                Intrinsic::GeFloat | Intrinsic::SgeInt => Some(left_str >= right_str),
                _ => None,
            };
            if let Some(result) = result {
                if !self.binary_candidates_have_string_user_override(candidates) {
                    self.stack.push(Value::Bool(result));
                    return Ok(DispatchAction::Continue);
                }
            }
        }

        if matches!(
            fallback_intrinsic,
            Intrinsic::AddFloat | Intrinsic::SubFloat | Intrinsic::MulFloat | Intrinsic::DivFloat
        ) && ((is_array_like_value(&left, &self.struct_heap)
            && is_supported_array_scalar_value(&right))
            || (is_supported_array_scalar_value(&left)
                && is_array_like_value(&right, &self.struct_heap)))
        {
            let result = match fallback_intrinsic {
                Intrinsic::AddFloat => self.dynamic_add(&left, &right)?,
                Intrinsic::SubFloat => self.dynamic_sub(&left, &right)?,
                Intrinsic::MulFloat => self.dynamic_mul(&left, &right)?,
                Intrinsic::DivFloat => self.dynamic_div(&left, &right)?,
                _ => unreachable!(),
            };
            self.stack.push(result);
            return Ok(DispatchAction::Continue);
        }

        // Get type names for both operands
        let left_type_name = self.get_type_name(&left);
        let right_type_name = self.get_type_name(&right);

        #[cfg(debug_assertions)]
        if dispatch_debug_enabled() {
            dispatch_debug_log(format_args!(
                "[DISPATCH] BinaryBoth: ({}, {}) intrinsic={:?}, candidates={}",
                left_type_name,
                right_type_name,
                fallback_intrinsic,
                candidates.len()
            ));
        }

        // BinaryBothFallback: primitive-dispatch-skip [bootstrap] (Issue #4262)
        // Find the best matching method using shared scored dispatch (Issue #3910).
        // Primitive fallback preemption is centralized in
        // `binary_both_fallback_precedence`; non-preempted pairs always go
        // through the resolver before fallback.
        let left_dispatch_type_name = binary_dispatch_type_name(&left, &left_type_name);
        let right_dispatch_type_name = binary_dispatch_type_name(&right, &right_type_name);
        let actual_type_names = [
            left_dispatch_type_name.as_str(),
            right_dispatch_type_name.as_str(),
        ];
        let bigint_fallback_handles = bigint_intrinsic_handles(&left, &right);
        let bigfloat_fallback_handles = bigfloat_intrinsic_handles(&left, &right);
        let fallback_precedence = binary_both_fallback_precedence(&left, &right);
        // The Bool-equality candidate probe (Issue #4262 / Issue #5435 lineage)
        // is gated on the operand/intrinsic shape first so signature
        // derivation only runs for actual `(Bool, Bool)` equality dispatches.
        let resolve_bool_equality_first =
            matches!((&left, &right), (Value::Bool(_), Value::Bool(_)))
                && matches!(fallback_intrinsic, Intrinsic::EqFloat | Intrinsic::NeFloat)
                && self.binary_candidates_have_exact_bool_equality(candidates);
        let primitive_fallback_preempts = fallback_precedence
            == BinaryBothFallbackPrecedence::PrimitiveFallbackFirst
            && !resolve_bool_equality_first;
        let matched = if primitive_fallback_preempts {
            None
        } else if binary_both_dispatch_cacheable(&left, &right) {
            // Issue #8168: for two struct operands the resolver decision is
            // fully determined by the operand type names (the value-dependent
            // Dict/Memory guards in `resolve_binary_both_candidate` cannot fire),
            // so memoize the matched function index per call site keyed by
            // `(left_type_hash, right_type_hash)` and skip the candidate scan /
            // subtype checks on repeat dispatches.
            let cache_key = (
                hash_type_name(actual_type_names[0]),
                hash_type_name(actual_type_names[1]),
            );
            let ip = self.ip;
            if let Some(&cached) = self
                .binary_both_dispatch_cache
                .get(&ip)
                .and_then(|by_types| by_types.get(&cache_key))
            {
                crate::vm::profiler::record_event("BinaryBothResolverCacheHit");
                cached
            } else {
                crate::vm::profiler::record_event("BinaryBothResolverLookup");
                self.ensure_binary_candidate_signatures(candidates);
                let resolved = self.resolve_binary_both_candidate(
                    candidates,
                    &actual_type_names,
                    &left,
                    &right,
                );
                self.binary_both_dispatch_cache
                    .entry(ip)
                    .or_default()
                    .insert(cache_key, resolved);
                resolved
            }
        } else {
            crate::vm::profiler::record_event("BinaryBothResolverLookup");
            // Issue #6496: the payload carries only candidate function
            // indices; the expected signatures consumed by the shared
            // resolver are derived from `FunctionInfo` and memoized per
            // function index. Issue #6502 slice 2: matching runs on the
            // structured `core_signature` projection (the rendered names
            // remain only for the VM representation fences).
            self.ensure_binary_candidate_signatures(candidates);
            self.resolve_binary_both_candidate(candidates, &actual_type_names, &left, &right)
        };

        if let Some(func_index) = matched {
            crate::vm::profiler::record_event("BinaryBothResolverMatch");
            #[cfg(debug_assertions)]
            if dispatch_debug_enabled() {
                if let Some((_left_exp, _right_exp)) = self.binary_candidate_signature(func_index) {
                    dispatch_debug_log(format_args!(
                        "[DISPATCH]   -> matched method #{}: ({}, {})",
                        func_index, _left_exp, _right_exp
                    ));
                }
            }
            // Call the user-defined method
            let func = match self.get_function_cloned_or_raise(func_index)? {
                Some(f) => f,
                None => return Ok(DispatchAction::Continue),
            };

            let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

            // Bind type parameters from where clauses (Issue #2468).
            // Only clone args when type params exist (common case: no type params).
            if !func.type_params.is_empty() {
                let args = [left.clone(), right.clone()];
                self.bind_type_params(&func, &args, &mut frame);
            }

            // Bind arguments directly to frame slots (Issue #3373: avoid double clone)
            if let Some(&slot) = func.param_slots.first() {
                bind_value_to_slot(&mut frame, slot, left, &mut self.struct_heap);
            }
            if let Some(&slot) = func.param_slots.get(1) {
                bind_value_to_slot(&mut frame, slot, right, &mut self.struct_heap);
            }

            for kwparam in &func.kwparams {
                if kwparam.required {
                    return Err(VmError::UndefKeywordError(kwparam.name.clone()));
                }
                bind_value_to_slot(
                    &mut frame,
                    kwparam.slot,
                    kwparam.default.clone(),
                    &mut self.struct_heap,
                );
            }

            self.return_ips.push(self.ip);
            self.try_push_call_frame(frame)?;
            self.ip = func.entry;
        } else {
            if !primitive_fallback_preempts {
                crate::vm::profiler::record_event("BinaryBothResolverMiss");
            }
            // No matching method - try fallback to intrinsic if both are primitives
            // Bool is also considered primitive (promotes to Int64 for arithmetic)
            // Float32 is also primitive and participates in type promotion
            #[cfg(debug_assertions)]
            if dispatch_debug_enabled() {
                dispatch_debug_log(format_args!(
                    "[DISPATCH]   -> no method match, trying primitive fallback"
                ));
            }
            let left_is_primitive = matches!(
                &left,
                Value::I64(_)
                    | Value::I128(_)
                    | Value::F64(_)
                    | Value::F32(_)
                    | Value::F16(_)
                    | Value::Bool(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
            );
            let right_is_primitive = matches!(
                &right,
                Value::I64(_)
                    | Value::I128(_)
                    | Value::F64(_)
                    | Value::F32(_)
                    | Value::F16(_)
                    | Value::Bool(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
            );
            let left_is_struct = matches!(&left, Value::Struct(_) | Value::StructRef(_));
            let right_is_struct = matches!(&right, Value::Struct(_) | Value::StructRef(_));

            // BinaryBothFallback: memory-operator-boundary [compatibility] (Issue #4262)
            // Memory participates in Julia's AbstractVector lattice, but this
            // dynamic Any fallback must not construct a temporary Array wrapper.
            // Keep the supported legacy operator cases on direct Memory-aware
            // paths and let unsupported combinations fall through to MethodError.
            // BinaryBothFallback: array-wrapper-equality [compatibility] (Issue #4262)
            if matches!(
                (&left, &right),
                (Value::Memory(_), _) | (_, Value::Memory(_))
            ) {
                let direct_result = match fallback_intrinsic {
                    Intrinsic::EqInt
                    | Intrinsic::EqFloat
                    | Intrinsic::NeInt
                    | Intrinsic::NeFloat
                        if is_array_like_value(&left, &self.struct_heap)
                            && is_array_like_value(&right, &self.struct_heap) =>
                    {
                        let is_equal = if let (Value::Memory(left_mem), Value::Memory(right_mem)) =
                            (&left, &right)
                        {
                            left_mem.borrow().isequal_contents(&right_mem.borrow())
                        } else if let Some((mem, arr)) = memory_array_pair(&left, &right) {
                            memory_array_values_equal(mem, arr)
                        } else {
                            false
                        };
                        let result = match fallback_intrinsic {
                            Intrinsic::EqInt | Intrinsic::EqFloat => is_equal,
                            Intrinsic::NeInt | Intrinsic::NeFloat => !is_equal,
                            _ => unreachable!(),
                        };
                        Some(Value::Bool(result))
                    }
                    Intrinsic::AddFloat | Intrinsic::SubFloat
                        if is_array_like_value(&left, &self.struct_heap)
                            && is_array_like_value(&right, &self.struct_heap) =>
                    {
                        Some(match fallback_intrinsic {
                            Intrinsic::AddFloat => self.dynamic_add(&left, &right)?,
                            Intrinsic::SubFloat => self.dynamic_sub(&left, &right)?,
                            _ => unreachable!(),
                        })
                    }
                    Intrinsic::DivFloat
                        if (matches!(&left, Value::Memory(_))
                            && is_supported_array_scalar_value(&right))
                            || (is_supported_array_scalar_value(&left)
                                && matches!(&right, Value::Memory(_))) =>
                    {
                        Some(self.dynamic_div(&left, &right)?)
                    }
                    Intrinsic::MulFloat
                        if (matches!(&left, Value::Memory(_))
                            && is_supported_array_scalar_value(&right))
                            || (is_supported_array_scalar_value(&left)
                                && matches!(&right, Value::Memory(_))) =>
                    {
                        Some(self.dynamic_mul(&left, &right)?)
                    }
                    _ => None,
                };

                if let Some(result) = direct_result {
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }
            }

            if matches!(
                fallback_intrinsic,
                Intrinsic::EqInt | Intrinsic::EqFloat | Intrinsic::NeInt | Intrinsic::NeFloat
            ) {
                if let Some(is_equal) = native_array_values_equal(&left, &right) {
                    let result = match fallback_intrinsic {
                        Intrinsic::EqInt | Intrinsic::EqFloat => is_equal,
                        Intrinsic::NeInt | Intrinsic::NeFloat => !is_equal,
                        _ => unreachable!(),
                    };
                    self.stack.push(Value::Bool(result));
                    return Ok(DispatchAction::Continue);
                }
                if let Some(is_equal) =
                    self.compare_array_wrapper_boundary_values_equal(&left, &right)
                {
                    let result = match fallback_intrinsic {
                        Intrinsic::EqInt | Intrinsic::EqFloat => is_equal,
                        Intrinsic::NeInt | Intrinsic::NeFloat => !is_equal,
                        _ => unreachable!(),
                    };
                    self.stack.push(Value::Bool(result));
                    return Ok(DispatchAction::Continue);
                }
            }

            // BinaryBothFallback: unsigned-comparison [bootstrap] (Issue #4262)
            // Issue #3566: Native UInt64 comparison path. When both operands are
            // UInt64 (or UInt64 paired with another unsigned that fits), comparing
            // by routing through Int64 wraps signed-ness and can raise OverflowError
            // for values > i64::MAX (e.g., typemax(UInt64) == 0xffffffffffffffff).
            // Handle these natively as u64 to preserve correctness for u64::MAX.
            // Only applies to comparison intrinsics; arithmetic falls through to the
            // existing widen-to-I64 path which is fine for non-extreme values.
            let is_cmp = matches!(
                fallback_intrinsic,
                Intrinsic::EqInt
                    | Intrinsic::NeInt
                    | Intrinsic::SltInt
                    | Intrinsic::SleInt
                    | Intrinsic::SgtInt
                    | Intrinsic::SgeInt
                    | Intrinsic::EqFloat
                    | Intrinsic::NeFloat
                    | Intrinsic::LtFloat
                    | Intrinsic::LeFloat
                    | Intrinsic::GtFloat
                    | Intrinsic::GeFloat
            );
            if is_cmp {
                // Issue #3696: native UInt128 comparison path. Handles every
                // (U128, U128) and (U128, smaller-unsigned/non-negative-signed)
                // combo without ever downcasting to i64, so values above
                // i64::MAX (which is the entire upper half of U128) compare
                // correctly.
                let has_u128 = matches!(&left, Value::U128(_)) || matches!(&right, Value::U128(_));
                if has_u128 {
                    let to_u128 = |v: &Value| -> Option<u128> {
                        match v {
                            Value::U128(x) => Some(*x),
                            Value::U64(x) => Some(u128::from(*x)),
                            Value::U32(x) => Some(u128::from(*x)),
                            Value::U16(x) => Some(u128::from(*x)),
                            Value::U8(x) => Some(u128::from(*x)),
                            Value::Bool(b) => Some(if *b { 1 } else { 0 }),
                            Value::I64(x) if *x >= 0 => Some(*x as u128),
                            Value::I32(x) if *x >= 0 => Some(*x as u128),
                            Value::I16(x) if *x >= 0 => Some(*x as u128),
                            Value::I8(x) if *x >= 0 => Some(*x as u128),
                            Value::I128(x) if *x >= 0 => Some(*x as u128),
                            _ => None,
                        }
                    };
                    if let (Some(a), Some(b)) = (to_u128(&left), to_u128(&right)) {
                        let result = match fallback_intrinsic {
                            Intrinsic::EqInt | Intrinsic::EqFloat => a == b,
                            Intrinsic::NeInt | Intrinsic::NeFloat => a != b,
                            Intrinsic::SltInt | Intrinsic::LtFloat => a < b,
                            Intrinsic::SleInt | Intrinsic::LeFloat => a <= b,
                            Intrinsic::SgtInt | Intrinsic::GtFloat => a > b,
                            Intrinsic::SgeInt | Intrinsic::GeFloat => a >= b,
                            _ => unreachable!("guarded by is_cmp"),
                        };
                        self.stack.push(Value::Bool(result));
                        return Ok(DispatchAction::Continue);
                    }
                    // Mixed UInt128 + negative signed: a negative signed value
                    // is always strictly less than any UInt128 value.
                    let left_neg = matches!(&left, Value::I128(x) if *x < 0)
                        || matches!(&left, Value::I64(x) if *x < 0)
                        || matches!(&left, Value::I32(x) if *x < 0)
                        || matches!(&left, Value::I16(x) if *x < 0)
                        || matches!(&left, Value::I8(x) if *x < 0);
                    let right_neg = matches!(&right, Value::I128(x) if *x < 0)
                        || matches!(&right, Value::I64(x) if *x < 0)
                        || matches!(&right, Value::I32(x) if *x < 0)
                        || matches!(&right, Value::I16(x) if *x < 0)
                        || matches!(&right, Value::I8(x) if *x < 0);
                    if (left_neg && matches!(&right, Value::U128(_)))
                        || (right_neg && matches!(&left, Value::U128(_)))
                    {
                        let result = match fallback_intrinsic {
                            Intrinsic::EqInt | Intrinsic::EqFloat => false,
                            Intrinsic::NeInt | Intrinsic::NeFloat => true,
                            Intrinsic::SltInt | Intrinsic::LtFloat => left_neg,
                            Intrinsic::SleInt | Intrinsic::LeFloat => left_neg,
                            Intrinsic::SgtInt | Intrinsic::GtFloat => right_neg,
                            Intrinsic::SgeInt | Intrinsic::GeFloat => right_neg,
                            _ => unreachable!("guarded by is_cmp"),
                        };
                        self.stack.push(Value::Bool(result));
                        return Ok(DispatchAction::Continue);
                    }
                }
                // Try to coerce both operands to u64 (when they're unsigned/non-negative).
                let to_u64 = |v: &Value| -> Option<u64> {
                    match v {
                        Value::U64(x) => Some(*x),
                        Value::U32(x) => Some(u64::from(*x)),
                        Value::U16(x) => Some(u64::from(*x)),
                        Value::U8(x) => Some(u64::from(*x)),
                        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
                        Value::I64(x) if *x >= 0 => Some(*x as u64),
                        Value::I32(x) if *x >= 0 => Some(*x as u64),
                        Value::I16(x) if *x >= 0 => Some(*x as u64),
                        Value::I8(x) if *x >= 0 => Some(*x as u64),
                        _ => None,
                    }
                };
                let has_u64 = matches!(&left, Value::U64(_)) || matches!(&right, Value::U64(_));
                if has_u64 {
                    if let (Some(a), Some(b)) = (to_u64(&left), to_u64(&right)) {
                        let result = match fallback_intrinsic {
                            Intrinsic::EqInt | Intrinsic::EqFloat => a == b,
                            Intrinsic::NeInt | Intrinsic::NeFloat => a != b,
                            Intrinsic::SltInt | Intrinsic::LtFloat => a < b,
                            Intrinsic::SleInt | Intrinsic::LeFloat => a <= b,
                            Intrinsic::SgtInt | Intrinsic::GtFloat => a > b,
                            Intrinsic::SgeInt | Intrinsic::GeFloat => a >= b,
                            _ => unreachable!("guarded by is_cmp"),
                        };
                        self.stack.push(Value::Bool(result));
                        return Ok(DispatchAction::Continue);
                    }
                    // Mixed UInt64 + negative signed: comparison still well-defined.
                    // A negative signed value is always < any UInt64 value.
                    let left_neg = matches!(&left, Value::I64(x) if *x < 0)
                        || matches!(&left, Value::I32(x) if *x < 0)
                        || matches!(&left, Value::I16(x) if *x < 0)
                        || matches!(&left, Value::I8(x) if *x < 0);
                    let right_neg = matches!(&right, Value::I64(x) if *x < 0)
                        || matches!(&right, Value::I32(x) if *x < 0)
                        || matches!(&right, Value::I16(x) if *x < 0)
                        || matches!(&right, Value::I8(x) if *x < 0);
                    if (left_neg && matches!(&right, Value::U64(_)))
                        || (right_neg && matches!(&left, Value::U64(_)))
                    {
                        // negative < unsigned always
                        let result = match fallback_intrinsic {
                            Intrinsic::EqInt | Intrinsic::EqFloat => false,
                            Intrinsic::NeInt | Intrinsic::NeFloat => true,
                            Intrinsic::SltInt | Intrinsic::LtFloat => left_neg, // neg < uint
                            Intrinsic::SleInt | Intrinsic::LeFloat => left_neg,
                            Intrinsic::SgtInt | Intrinsic::GtFloat => right_neg, // uint > neg
                            Intrinsic::SgeInt | Intrinsic::GeFloat => right_neg,
                            _ => unreachable!("guarded by is_cmp"),
                        };
                        self.stack.push(Value::Bool(result));
                        return Ok(DispatchAction::Continue);
                    }
                }
            }

            // BinaryBothFallback: uint128-arithmetic [bootstrap] (Issue #4262)
            // Issue #3697: Native UInt128 arithmetic path. Comparisons were already
            // handled in the is_cmp block above; this is for +, -, *, /, ÷, %.
            // Without this, the small-int prologue below would truncate U128 → I64
            // (raising OverflowError for any value > i64::MAX, or wrapping for *).
            let has_u128_arith =
                matches!(&left, Value::U128(_)) || matches!(&right, Value::U128(_));
            let has_float_arith = matches!(&left, Value::F64(_) | Value::F32(_) | Value::F16(_))
                || matches!(&right, Value::F64(_) | Value::F32(_) | Value::F16(_));
            if has_u128_arith && !has_float_arith {
                let to_u128 = |v: &Value| -> Option<u128> {
                    match v {
                        Value::U128(x) => Some(*x),
                        Value::U64(x) => Some(u128::from(*x)),
                        Value::U32(x) => Some(u128::from(*x)),
                        Value::U16(x) => Some(u128::from(*x)),
                        Value::U8(x) => Some(u128::from(*x)),
                        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
                        Value::I64(x) if *x >= 0 => Some(*x as u128),
                        Value::I32(x) if *x >= 0 => Some(*x as u128),
                        Value::I16(x) if *x >= 0 => Some(*x as u128),
                        Value::I8(x) if *x >= 0 => Some(*x as u128),
                        Value::I128(x) if *x >= 0 => Some(*x as u128),
                        _ => None,
                    }
                };
                if let (Some(a), Some(b)) = (to_u128(&left), to_u128(&right)) {
                    let result = match fallback_intrinsic {
                        Intrinsic::AddFloat => Value::U128(a.wrapping_add(b)),
                        Intrinsic::SubFloat => Value::U128(a.wrapping_sub(b)),
                        Intrinsic::MulFloat => Value::U128(a.wrapping_mul(b)),
                        Intrinsic::DivFloat => {
                            // Julia's `/` always returns Float64 even for integers
                            Value::F64(a as f64 / b as f64)
                        }
                        Intrinsic::SdivInt => {
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(DispatchAction::Continue);
                            }
                            Value::U128(a / b)
                        }
                        Intrinsic::SremInt => {
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(DispatchAction::Continue);
                            }
                            Value::U128(a % b)
                        }
                        Intrinsic::PowFloat => Value::F64((a as f64).powf(b as f64)),
                        _ => {
                            self.raise(VmError::unsupported_op("UInt128", fallback_intrinsic))?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }
                // Mixed with negative-signed: a UInt128 cannot meaningfully
                // accept a negative summand without first converting to BigInt
                // (which is the next dispatch path). Fall through; legacy
                // OverflowError below is the loud failure mode.
            }

            // BinaryBothFallback: uint64-arithmetic [bootstrap] (Issue #6755)
            // Native UInt64 arithmetic path. Comparisons were handled in the
            // is_cmp block above. Without this, the small-int prologue below
            // normalizes U64 → I64 (and `narrow_int_arith_result_kind` only
            // re-wraps U8/U16/U32, never U64), so `UInt64 op UInt64` produced a
            // `Value::I64` result even though `typeof` reports `UInt64`. The
            // mis-tagged value then routes the *next* arithmetic op to the
            // Int64 dispatch (e.g. `convert(UInt64, UInt32(5)) * UInt64(2)` was
            // Int64 instead of UInt64). Mirror the UInt128 block: keep the
            // result a `Value::U64` for the type-preserving integer ops, with a
            // negative signed operand bit-cast to u64 to match upstream's
            // convert-then-wrap behavior (`UInt64(10) + (-3) == UInt64(7)`).
            // `/` returns Float64 like upstream; `^`/other ops fall through to
            // their existing (already correct) paths.
            let has_u64_arith = matches!(&left, Value::U64(_)) || matches!(&right, Value::U64(_));
            if has_u64_arith && !has_u128_arith && !has_float_arith {
                let to_u64 = |v: &Value| -> Option<u64> {
                    match v {
                        Value::U64(x) => Some(*x),
                        Value::U32(x) => Some(u64::from(*x)),
                        Value::U16(x) => Some(u64::from(*x)),
                        Value::U8(x) => Some(u64::from(*x)),
                        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
                        // Signed operands (including negative) bit-cast to u64,
                        // matching `convert(UInt64, ::Signed)`-then-wrap.
                        Value::I64(x) => Some(*x as u64),
                        Value::I32(x) => Some(*x as i64 as u64),
                        Value::I16(x) => Some(*x as i64 as u64),
                        Value::I8(x) => Some(*x as i64 as u64),
                        _ => None,
                    }
                };
                if let (Some(a), Some(b)) = (to_u64(&left), to_u64(&right)) {
                    let result = match fallback_intrinsic {
                        Intrinsic::AddFloat | Intrinsic::AddInt => {
                            Some(Value::U64(a.wrapping_add(b)))
                        }
                        Intrinsic::SubFloat | Intrinsic::SubInt => {
                            Some(Value::U64(a.wrapping_sub(b)))
                        }
                        Intrinsic::MulFloat | Intrinsic::MulInt => {
                            Some(Value::U64(a.wrapping_mul(b)))
                        }
                        Intrinsic::DivFloat => {
                            // Julia's `/` always returns Float64 even for integers.
                            Some(Value::F64(a as f64 / b as f64))
                        }
                        Intrinsic::SdivInt => {
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(DispatchAction::Continue);
                            }
                            Some(Value::U64(a / b))
                        }
                        Intrinsic::SremInt => {
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(DispatchAction::Continue);
                            }
                            Some(Value::U64(a % b))
                        }
                        // `^` (PowFloat) and any other op are not handled here;
                        // fall through to the existing paths which already
                        // produce the correct UInt64 result.
                        _ => None,
                    };
                    if let Some(result) = result {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                }
                // Otherwise (unhandled op, or an operand that cannot coerce to
                // u64) fall through to the legacy path.
            }

            // Issue #5205: capture same-narrow-int arithmetic BEFORE normalizing
            // operands to I64 below, so the I64 result can be wrapped back into
            // the narrow type (Int8 + Int8 -> Int8, modular), matching upstream.
            let narrow_arith_result_kind =
                narrow_int_arith_result_kind(&left, &right, fallback_intrinsic);

            // BinaryBothFallback: small-int-normalization [bootstrap] (Issue #4262)
            // Convert Bool and small integer values to I64 for arithmetic operations
            // This enables mixed-type dispatch for UInt8/Int64 etc. (Issue #1853)
            // Convert Bool and small integer values to I64 for arithmetic operations
            let left = match left {
                Value::Bool(b) => Value::I64(if b { 1 } else { 0 }),
                Value::U8(v) => Value::I64(i64::from(v)),
                Value::U16(v) => Value::I64(i64::from(v)),
                Value::U32(v) => Value::I64(i64::from(v)),
                Value::U64(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt64 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::U128(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt128 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::I8(v) => Value::I64(v as i64),
                Value::I16(v) => Value::I64(v as i64),
                Value::I32(v) => Value::I64(v as i64),
                // Pass through I64, I128, F64, F32, F16, Complex, Rational, etc.
                // Guarded by left_is_primitive && right_is_primitive below
                other => other,
            };
            let right = match right {
                Value::Bool(b) => Value::I64(if b { 1 } else { 0 }),
                Value::U8(v) => Value::I64(i64::from(v)),
                Value::U16(v) => Value::I64(i64::from(v)),
                Value::U32(v) => Value::I64(i64::from(v)),
                Value::U64(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt64 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::U128(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt128 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::I8(v) => Value::I64(v as i64),
                Value::I16(v) => Value::I64(v as i64),
                Value::I32(v) => Value::I64(v as i64),
                // Pass through I64, I128, F64, F32, F16, Complex, Rational, etc.
                // Guarded by left_is_primitive && right_is_primitive below
                other => other,
            };

            // BinaryBothFallback: primitive-intrinsic-dispatch [bootstrap] (Issue #4262)
            if left_is_primitive && right_is_primitive {
                // Use the fallback intrinsic for primitive-only operations
                // Select Int version if both operands are I64, EXCEPT for:
                // - DivFloat: Julia's / always returns Float64, even for integers
                // - PowFloat: Power should use floating point for proper semantics
                let both_int = matches!((&left, &right), (Value::I64(_), Value::I64(_)));
                let has_i128 = matches!(&left, Value::I128(_)) || matches!(&right, Value::I128(_));
                let both_f32 = matches!((&left, &right), (Value::F32(_), Value::F32(_)));
                let has_f32 = matches!(&left, Value::F32(_)) || matches!(&right, Value::F32(_));
                let has_f64 = matches!(&left, Value::F64(_)) || matches!(&right, Value::F64(_));
                let both_f16 = matches!((&left, &right), (Value::F16(_), Value::F16(_)));
                let has_f16 = matches!(&left, Value::F16(_)) || matches!(&right, Value::F16(_));

                // BinaryBothFallback: int128-intrinsics [bootstrap] (Issue #4262)
                // Handle Int128 operations (Issue #1904)
                // I128 must be checked before float paths since I128+I64 should stay integer
                if has_i128 && !has_f64 && !has_f32 && !has_f16 {
                    // Both operands are integer (I128, I64, or Bool->I64)
                    // Promote both to i128
                    let a = match &left {
                        Value::I128(v) => *v,
                        Value::I64(v) => *v as i128,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "I128 path: unexpected left operand {:?}",
                                left
                            )))
                        }
                    };
                    let b = match &right {
                        Value::I128(v) => *v,
                        Value::I64(v) => *v as i128,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "I128 path: unexpected right operand {:?}",
                                right
                            )))
                        }
                    };
                    let result = match fallback_intrinsic {
                        Intrinsic::AddFloat => Value::I128(a.wrapping_add(b)),
                        Intrinsic::SubFloat => Value::I128(a.wrapping_sub(b)),
                        Intrinsic::MulFloat => Value::I128(a.wrapping_mul(b)),
                        Intrinsic::DivFloat => {
                            // Julia's / always returns Float64, even for integers
                            Value::F64(a as f64 / b as f64)
                        }
                        Intrinsic::PowFloat => Value::F64((a as f64).powf(b as f64)),
                        Intrinsic::SdivInt => {
                            // Integer division (÷)
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(DispatchAction::Continue);
                            }
                            Value::I128(a / b)
                        }
                        Intrinsic::SremInt => {
                            // Integer remainder (mod/rem)
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(DispatchAction::Continue);
                            }
                            // Julia's mod: result = a - floor(a/b) * b
                            Value::I128(((a % b) + b) % b)
                        }
                        Intrinsic::EqFloat | Intrinsic::EqInt => Value::Bool(a == b),
                        Intrinsic::NeFloat | Intrinsic::NeInt => Value::Bool(a != b),
                        Intrinsic::LtFloat | Intrinsic::SltInt => Value::Bool(a < b),
                        Intrinsic::LeFloat | Intrinsic::SleInt => Value::Bool(a <= b),
                        Intrinsic::GtFloat | Intrinsic::SgtInt => Value::Bool(a > b),
                        Intrinsic::GeFloat | Intrinsic::SgeInt => Value::Bool(a >= b),
                        _ => {
                            self.raise(VmError::unsupported_op("Int128", &fallback_intrinsic))?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(result);
                } else if both_f16 {
                    // BinaryBothFallback: float16-intrinsics [bootstrap] (Issue #4262)
                    // Float16 + Float16 -> Float16 (Issue #3621). The table
                    // lives in `same_type_fast_path` (Issue #6338): compute in
                    // F64 to leverage hardware FP, then narrow back to F16.
                    match same_type_fast_path(fallback_intrinsic, &left, &right) {
                        Some(Ok(value)) => self.stack.push(value),
                        Some(Err(err)) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                        None => {
                            self.raise(VmError::unsupported_op(
                                "Float16-Float16",
                                fallback_intrinsic,
                            ))?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                } else if has_f16 && !both_f16 {
                    // BinaryBothFallback: mixed-float16-intrinsics [bootstrap] (Issue #4262)
                    // Float16 mixed with other type. The (F16, F64) and
                    // (F16, F32) pairs are owned by `promote_numeric_pair`
                    // above (Issue #6338) — and the small-int normalization
                    // prologue can never produce an F32/F64 operand — so only
                    // F16×Int reaches this branch. It must stay explicit: the
                    // result is computed in f64 and narrowed to F16 at the
                    // END, which differs from true promote semantics
                    // (Int→Float16 FIRST would double-round, e.g.
                    // Float16(0.5) + 2049).
                    {
                        // F16 <-> Int: result is F16 (Julia semantics: Float16 + Int -> Float16)
                        let a = match &left {
                            Value::F16(v) => v.to_f64(),
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Int path: unexpected left operand {:?}",
                                    left
                                )))
                            }
                        };
                        let b = match &right {
                            Value::F16(v) => v.to_f64(),
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Int path: unexpected right operand {:?}",
                                    right
                                )))
                            }
                        };
                        let result = match fallback_intrinsic {
                            Intrinsic::AddFloat => Value::F16(half::f16::from_f64(a + b)),
                            Intrinsic::SubFloat => Value::F16(half::f16::from_f64(a - b)),
                            Intrinsic::MulFloat => Value::F16(half::f16::from_f64(a * b)),
                            Intrinsic::DivFloat => Value::F16(half::f16::from_f64(a / b)),
                            Intrinsic::EqFloat => Value::Bool(a == b),
                            Intrinsic::NeFloat => Value::Bool(a != b),
                            Intrinsic::LtFloat => Value::Bool(a < b),
                            Intrinsic::LeFloat => Value::Bool(a <= b),
                            Intrinsic::GtFloat => Value::Bool(a > b),
                            Intrinsic::GeFloat => Value::Bool(a >= b),
                            Intrinsic::SremInt => {
                                let result = a - (a / b).floor() * b;
                                Value::F16(half::f16::from_f64(result))
                            }
                            Intrinsic::SdivInt => {
                                let result = (a / b).floor();
                                Value::F16(half::f16::from_f64(result))
                            }
                            _ => {
                                self.raise(VmError::unsupported_op(
                                    "Float16-Int64",
                                    &fallback_intrinsic,
                                ))?;
                                return Ok(DispatchAction::Continue);
                            }
                        };
                        self.stack.push(result);
                    }
                } else if has_f32 {
                    // BinaryBothFallback: float32-intrinsics [bootstrap] (Issue #4262)
                    // Float32 operations, F32-preserving (Issue #3750). Raw
                    // F32×F64 / F32×I64 / F32×I128 pairs are owned by
                    // `promote_numeric_pair` BEFORE the resolver (Issue
                    // #6338); this arm still sees
                    // (a) F32×F32 — same-type, e.g. when +(::Number,::Number)
                    //     recurses after promote() (Float16+Float32 promotes
                    //     to F32+F32 and recurses into +), and
                    // (b) F32×I64 where the I64 came from the small-int
                    //     normalization prologue above (Bool/UInt8/...→I64),
                    //     which runs after the promote interception —
                    //     re-promoting here applies the identical F32 table.
                    let (promoted_left, promoted_right) = match promote_numeric_pair(&left, &right)
                    {
                        Some((l, r, _)) => (l, r),
                        None => (left.clone(), right.clone()),
                    };
                    match same_type_fast_path(fallback_intrinsic, &promoted_left, &promoted_right) {
                        Some(Ok(value)) => self.stack.push(value),
                        Some(Err(err)) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                        None => {
                            // Label preserved verbatim from the legacy arm
                            // (it said "Float32-Int64" even for F32×F32).
                            self.raise(VmError::unsupported_op(
                                "Float32-Int64",
                                fallback_intrinsic,
                            ))?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                } else if *fallback_intrinsic == Intrinsic::SremInt && !both_int {
                    // BinaryBothFallback: generic-float-rem [bootstrap] (Issue #4262)
                    // SremInt is the `%` / `rem` operator (truncated remainder), with
                    // at least one Float64/Float32/Float16 operand. Use fmod semantics.
                    let a = match &left {
                        Value::F64(v) => *v,
                        Value::F32(v) => *v as f64,
                        Value::F16(v) => v.to_f64(),
                        Value::I64(v) => *v as f64,
                        Value::I128(v) => *v as f64,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "SremInt float path: unexpected left operand {:?}",
                                left
                            )))
                        }
                    };
                    let b = match &right {
                        Value::F64(v) => *v,
                        Value::F32(v) => *v as f64,
                        Value::F16(v) => v.to_f64(),
                        Value::I64(v) => *v as f64,
                        Value::I128(v) => *v as f64,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "SremInt float path: unexpected right operand {:?}",
                                right
                            )))
                        }
                    };
                    // `%` / `rem` is the truncated remainder (sign of the dividend):
                    // rem(a, b) = a - trunc(a/b) * b. NOT floor — that is `mod`, which
                    // base/math.jl derives from `%` with its own sign adjustment (#6895).
                    let result = a - (a / b).trunc() * b;
                    // Preserve F32 type when both operands are F32 (Issue #1762)
                    if both_f32 {
                        self.stack.push(Value::F32(result as f32));
                    } else if both_f16 {
                        self.stack.push(Value::F16(half::f16::from_f64(result)));
                    } else {
                        self.stack.push(Value::F64(result));
                    }
                } else {
                    // BinaryBothFallback: generic-primitive-intrinsic [bootstrap] (Issue #4262)
                    let actual_intrinsic = if both_int {
                        match fallback_intrinsic {
                            Intrinsic::AddFloat => Intrinsic::AddInt,
                            Intrinsic::SubFloat => Intrinsic::SubInt,
                            Intrinsic::MulFloat => Intrinsic::MulInt,
                            // DivFloat stays as DivFloat - Julia's / always returns Float64
                            Intrinsic::DivFloat => Intrinsic::DivFloat,
                            // PowFloat stays as PowFloat for proper floating point semantics
                            Intrinsic::PowFloat => Intrinsic::PowFloat,
                            Intrinsic::LtFloat => Intrinsic::SltInt,
                            Intrinsic::LeFloat => Intrinsic::SleInt,
                            Intrinsic::GtFloat => Intrinsic::SgtInt,
                            Intrinsic::GeFloat => Intrinsic::SgeInt,
                            Intrinsic::EqFloat => Intrinsic::EqInt,
                            Intrinsic::NeFloat => Intrinsic::NeInt,
                            other => *other,
                        }
                    } else {
                        *fallback_intrinsic
                    };
                    // Convert operands to appropriate types
                    let (left_val, right_val) =
                        if matches!(actual_intrinsic, Intrinsic::DivFloat | Intrinsic::PowFloat)
                            && both_int
                        {
                            // For DivFloat and PowFloat with integers, convert both to F64
                            let l = match left {
                                Value::I64(v) => Value::F64(v as f64),
                                Value::I128(v) => Value::F64(v as f64),
                                Value::F32(v) => Value::F64(v as f64),
                                Value::F16(v) => Value::F64(v.to_f64()),
                                // F64 passes through unchanged; Complex/Rational handled elsewhere
                                other => other,
                            };
                            let r = match right {
                                Value::I64(v) => Value::F64(v as f64),
                                Value::I128(v) => Value::F64(v as f64),
                                Value::F32(v) => Value::F64(v as f64),
                                Value::F16(v) => Value::F64(v.to_f64()),
                                // F64 passes through unchanged; Complex/Rational handled elsewhere
                                other => other,
                            };
                            (l, r)
                        } else if matches!(
                            actual_intrinsic,
                            Intrinsic::EqFloat
                                | Intrinsic::NeFloat
                                | Intrinsic::LtFloat
                                | Intrinsic::LeFloat
                                | Intrinsic::GtFloat
                                | Intrinsic::GeFloat
                                | Intrinsic::AddFloat
                                | Intrinsic::SubFloat
                                | Intrinsic::MulFloat
                        ) && !both_int
                        {
                            // For Float comparisons/ops with mixed types, convert I64/I128/F32/F16 to F64
                            let l = match left {
                                Value::I64(v) => Value::F64(v as f64),
                                Value::I128(v) => Value::F64(v as f64),
                                Value::F32(v) => Value::F64(v as f64),
                                Value::F16(v) => Value::F64(v.to_f64()),
                                // F64 passes through unchanged; Complex/Rational handled elsewhere
                                other => other,
                            };
                            let r = match right {
                                Value::I64(v) => Value::F64(v as f64),
                                Value::I128(v) => Value::F64(v as f64),
                                Value::F32(v) => Value::F64(v as f64),
                                Value::F16(v) => Value::F64(v.to_f64()),
                                // F64 passes through unchanged; Complex/Rational handled elsewhere
                                other => other,
                            };
                            (l, r)
                        } else {
                            (left, right)
                        };
                    self.stack.push(left_val);
                    self.stack.push(right_val);
                    // Use raise() instead of ? to integrate with try-catch
                    if let Err(e) = self.execute_intrinsic(actual_intrinsic) {
                        self.raise(e)?;
                        return Ok(DispatchAction::Continue);
                    }
                    // Issue #5205: narrow same-narrow-int arithmetic results back
                    // to the operand type (wrapping), matching upstream Julia.
                    if let Some(kind) = narrow_arith_result_kind {
                        if let Some(Value::I64(v)) = self.stack.pop() {
                            self.stack.push(kind.wrap_i64(v));
                        } else {
                            return Err(VmError::InternalError(
                                "narrow-int arithmetic result was not I64".to_string(),
                            ));
                        }
                    }
                }
            } else if matches!(fallback_intrinsic, Intrinsic::MulFloat)
                && matches!(
                    (&left, &right),
                    (Value::Str(_), Value::Str(_))
                        | (Value::Str(_), Value::Char(_))
                        | (Value::Char(_), Value::Str(_))
                        | (Value::Char(_), Value::Char(_))
                )
            {
                // BinaryBothFallback: string-char-concat [candidate] (Issue #4262)
                // String/Char concatenation: "a" * "b", "a" * 'b', 'a' * "b", 'a' * 'b' (Issue #2127)
                if let Some(result) = try_string_char_concat(&left, &right) {
                    self.stack.push(result);
                } else {
                    return Err(VmError::InternalError(
                        "string/char concat match but helper returned None".to_string(),
                    ));
                }
            } else if matches!((&left, &right), (Value::Str(_), Value::Str(_)))
                && matches!(
                    fallback_intrinsic,
                    Intrinsic::LtFloat
                        | Intrinsic::LeFloat
                        | Intrinsic::GtFloat
                        | Intrinsic::GeFloat
                        | Intrinsic::EqFloat
                        | Intrinsic::NeFloat
                )
            {
                // BinaryBothFallback: string-comparison [candidate] (Issue #4262)
                // String comparison: lexicographic ordering (Issue #2025)
                let result = match (&left, &right) {
                    (Value::Str(a), Value::Str(b)) => match fallback_intrinsic {
                        Intrinsic::LtFloat => a < b,
                        Intrinsic::LeFloat => a <= b,
                        Intrinsic::GtFloat => a > b,
                        Intrinsic::GeFloat => a >= b,
                        Intrinsic::EqFloat => a == b,
                        Intrinsic::NeFloat => a != b,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "string comparison path: unexpected intrinsic {:?}",
                                fallback_intrinsic
                            )))
                        }
                    },
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "string comparison path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                self.stack.push(Value::Bool(result));
            } else if (left_is_struct && right_is_struct)
                && matches!(fallback_intrinsic, Intrinsic::EqFloat | Intrinsic::NeFloat)
            {
                // BinaryBothFallback: struct-equality [compatibility] (Issue #4262)
                // Struct-struct comparison: use field-by-field comparison
                let is_equal = self.compare_struct_fields(&left, &right);
                let result = if matches!(fallback_intrinsic, Intrinsic::EqFloat) {
                    is_equal
                } else {
                    !is_equal
                };
                self.stack.push(Value::Bool(result));
            } else if matches!((&left, &right), (Value::Symbol(_), Value::Symbol(_)))
                && matches!(
                    fallback_intrinsic,
                    Intrinsic::EqFloat
                        | Intrinsic::NeFloat
                        | Intrinsic::LtFloat
                        | Intrinsic::LeFloat
                        | Intrinsic::GtFloat
                        | Intrinsic::GeFloat
                )
            {
                // BinaryBothFallback: symbol comparison (Issue #4262, #5748).
                // Equality compares interned names; ordering is lexicographic by
                // name, matching upstream `isless(::Symbol, ::Symbol)`. This makes
                // sort (uses `<=`), cmp/max/min (use `</>`), and `<`/`<=`/`>`/`>=`
                // work on Symbols.
                let (a, b) = match (&left, &right) {
                    (Value::Symbol(a), Value::Symbol(b)) => (a, b),
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "symbol comparison path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = match fallback_intrinsic {
                    Intrinsic::EqFloat => a == b,
                    Intrinsic::NeFloat => a != b,
                    Intrinsic::LtFloat => a.as_str() < b.as_str(),
                    Intrinsic::LeFloat => a.as_str() <= b.as_str(),
                    Intrinsic::GtFloat => a.as_str() > b.as_str(),
                    Intrinsic::GeFloat => a.as_str() >= b.as_str(),
                    _ => unreachable!("guarded by the matches! above"),
                };
                self.stack.push(Value::Bool(result));
            } else if matches!((&left, &right), (Value::Bool(_), Value::Bool(_)))
                && matches!(fallback_intrinsic, Intrinsic::EqFloat | Intrinsic::NeFloat)
            {
                // BinaryBothFallback: bool-equality [bootstrap] (Issue #4262)
                // Bool-Bool comparison
                let is_equal = match (&left, &right) {
                    (Value::Bool(a), Value::Bool(b)) => a == b,
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "bool comparison path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = if matches!(fallback_intrinsic, Intrinsic::EqFloat) {
                    is_equal
                } else {
                    !is_equal
                };
                self.stack.push(Value::Bool(result));
            } else if matches!((&left, &right), (Value::Char(_), Value::Char(_))) {
                // BinaryBothFallback: char-char [candidate] (Issue #4262)
                // Char-Char operations (Issue #2122)
                let (a, b) = match (&left, &right) {
                    (Value::Char(a), Value::Char(b)) => (*a as i64, *b as i64),
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "char-char path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = match fallback_intrinsic {
                    // Char - Char → Int (difference of codepoints)
                    Intrinsic::SubFloat | Intrinsic::SubInt => Value::I64(a - b),
                    // Comparisons (both float and int intrinsic forms)
                    Intrinsic::EqFloat | Intrinsic::EqInt => Value::Bool(a == b),
                    Intrinsic::NeFloat | Intrinsic::NeInt => Value::Bool(a != b),
                    Intrinsic::LtFloat | Intrinsic::SltInt => Value::Bool(a < b),
                    Intrinsic::LeFloat | Intrinsic::SleInt => Value::Bool(a <= b),
                    Intrinsic::GtFloat | Intrinsic::SgtInt => Value::Bool(a > b),
                    Intrinsic::GeFloat | Intrinsic::SgeInt => Value::Bool(a >= b),
                    _ => {
                        return Err(VmError::unsupported_op("Char-Char", &fallback_intrinsic));
                    }
                };
                self.stack.push(result);
            } else if (matches!(&left, Value::Char(_)) && matches!(&right, Value::I64(_)))
                || (matches!(&left, Value::I64(_)) && matches!(&right, Value::Char(_)))
            {
                // BinaryBothFallback: char-int [candidate] (Issue #4262)
                // Issue #2122: Char+Int / Int+Char -> Char, Char-Int -> Char
                let (char_val, int_val) = match (&left, &right) {
                    (Value::Char(c), Value::I64(n)) => (*c as i64, *n),
                    (Value::I64(n), Value::Char(c)) => (*c as i64, *n),
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "char-int path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let left_is_char = matches!(&left, Value::Char(_));
                let result = match fallback_intrinsic {
                    Intrinsic::AddFloat | Intrinsic::AddInt => {
                        // Char + Int or Int + Char -> Char
                        let cp = char_val + int_val;
                        Value::Char(char::from_u32(cp as u32).unwrap_or('\0'))
                    }
                    Intrinsic::SubFloat | Intrinsic::SubInt if left_is_char => {
                        // Char - Int -> Char
                        let cp = char_val - int_val;
                        Value::Char(char::from_u32(cp as u32).unwrap_or('\0'))
                    }
                    Intrinsic::SubFloat | Intrinsic::SubInt => {
                        // Int - Char -> Int (unusual but handle it)
                        Value::I64(int_val - char_val)
                    }
                    Intrinsic::EqFloat | Intrinsic::EqInt => Value::Bool(char_val == int_val),
                    Intrinsic::NeFloat | Intrinsic::NeInt => Value::Bool(char_val != int_val),
                    Intrinsic::LtFloat | Intrinsic::SltInt => Value::Bool(if left_is_char {
                        char_val < int_val
                    } else {
                        int_val < char_val
                    }),
                    Intrinsic::LeFloat | Intrinsic::SleInt => Value::Bool(if left_is_char {
                        char_val <= int_val
                    } else {
                        int_val <= char_val
                    }),
                    Intrinsic::GtFloat | Intrinsic::SgtInt => Value::Bool(if left_is_char {
                        char_val > int_val
                    } else {
                        int_val > char_val
                    }),
                    Intrinsic::GeFloat | Intrinsic::SgeInt => Value::Bool(if left_is_char {
                        char_val >= int_val
                    } else {
                        int_val >= char_val
                    }),
                    _ => {
                        // INTERNAL: Char-Int comparison table covers all valid intrinsics; unsupported op is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "unsupported Char-Int operation: {:?}",
                            fallback_intrinsic
                        )));
                    }
                };
                self.stack.push(result);
            } else if bigint_fallback_handles {
                // BinaryBothFallback: bigint-intrinsics [bootstrap] (Issue #4262)
                // BigInt operations: at least one operand is BigInt, the other is integer-like.
                // `pop_bigint` performs the concrete coercion for all integer widths.
                let bigint_intrinsic = match fallback_intrinsic {
                    Intrinsic::AddFloat => Intrinsic::AddBigInt,
                    Intrinsic::SubFloat => Intrinsic::SubBigInt,
                    Intrinsic::MulFloat => Intrinsic::MulBigInt,
                    Intrinsic::DivFloat | Intrinsic::SdivInt => Intrinsic::DivBigInt, // ÷ and / both use DivBigInt for BigInt
                    Intrinsic::LtFloat => Intrinsic::LtBigInt,
                    Intrinsic::LeFloat => Intrinsic::LeBigInt,
                    Intrinsic::GtFloat => Intrinsic::GtBigInt,
                    Intrinsic::GeFloat => Intrinsic::GeBigInt,
                    Intrinsic::EqFloat => Intrinsic::EqBigInt,
                    Intrinsic::NeFloat => Intrinsic::NeBigInt,
                    Intrinsic::SremInt => Intrinsic::RemBigInt,
                    other => *other, // Keep other intrinsics as-is
                };
                self.stack.push(left);
                self.stack.push(right);
                if let Err(e) = self.execute_intrinsic(bigint_intrinsic) {
                    self.raise(e)?;
                    return Ok(DispatchAction::Continue);
                }
            } else if bigfloat_fallback_handles {
                // BinaryBothFallback: bigfloat-intrinsics [bootstrap] (Issue #4262)
                // BigFloat operations: BigFloat with any numeric operand, or BigInt with a
                // primitive float, promotes to BigFloat.
                let bigfloat_intrinsic = match fallback_intrinsic {
                    Intrinsic::AddFloat => Intrinsic::AddBigFloat,
                    Intrinsic::SubFloat => Intrinsic::SubBigFloat,
                    Intrinsic::MulFloat => Intrinsic::MulBigFloat,
                    Intrinsic::DivFloat | Intrinsic::SdivInt => Intrinsic::DivBigFloat,
                    Intrinsic::LtFloat => Intrinsic::LtBigFloat,
                    Intrinsic::LeFloat => Intrinsic::LeBigFloat,
                    Intrinsic::GtFloat => Intrinsic::GtBigFloat,
                    Intrinsic::GeFloat => Intrinsic::GeBigFloat,
                    Intrinsic::EqFloat => Intrinsic::EqBigFloat,
                    Intrinsic::NeFloat => Intrinsic::NeBigFloat,
                    // `%` / rem for BigFloat (Issue #6796).
                    Intrinsic::SremInt => Intrinsic::RemBigFloat,
                    other => *other, // Keep other intrinsics as-is
                };
                self.stack.push(left);
                self.stack.push(right);
                if let Err(e) = self.execute_intrinsic(bigfloat_intrinsic) {
                    self.raise(e)?;
                    return Ok(DispatchAction::Continue);
                }
            } else if let Some((scalar_val, arr_val)) =
                if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
                    struct_scalar_array_pair(&left, &right, &self.struct_heap)?
                } else {
                    None
                }
            {
                // BinaryBothFallback: complex-array-mul [compatibility] (Issue #4262)
                // Complex * Vector or Vector * Complex: scalar-vector multiplication.
                let scalar_struct = match &scalar_val {
                    Value::Struct(s) => Some(s.clone()),
                    Value::StructRef(idx) => self.struct_heap.get(*idx).cloned(),
                    _ => None,
                };

                if let Some(s) = &scalar_struct {
                    if self.is_complex(s) {
                        let (c_re, c_im) = s.as_complex_parts().unwrap_or((0.0, 0.0));
                        use crate::vm::matmul::{scalar_vector_mul_complex, Complex64};
                        let scalar = Complex64::new(c_re, c_im);
                        let mul_result =
                            scalar_vector_mul_complex(scalar, &arr_val, &self.struct_heap);

                        match mul_result {
                            Ok(mut result) => {
                                if result
                                    .element_type_override
                                    .as_ref()
                                    .is_some_and(|e| e.is_complex())
                                {
                                    result.struct_type_id = Some(self.get_complex_type_id());
                                }
                                self.push_array_value_as_wrapper(result)?;
                            }
                            Err(e) => {
                                self.raise(e)?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    } else {
                        self.raise(VmError::no_method_matching_op(
                            &left_type_name,
                            &right_type_name,
                        ))?;
                        return Ok(DispatchAction::Continue);
                    }
                } else {
                    self.raise(VmError::no_method_matching_op(
                        &left_type_name,
                        &right_type_name,
                    ))?;
                    return Ok(DispatchAction::Continue);
                }
            } else if let Some((a, b)) = if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
                runtime_array_pair(&left, &right, &self.struct_heap)?
            } else {
                None
            } {
                // BinaryBothFallback: array-array-matmul [compatibility] (Issue #4262)
                // Array * Array: use matrix multiplication (handles mixed real/complex).
                use crate::vm::matmul::{is_complex_array, matmul, matmul_complex};
                let a_is_complex = is_complex_array(&a);
                let b_is_complex = is_complex_array(&b);
                let mul_result = if a_is_complex || b_is_complex {
                    matmul_complex(&a, &b, &self.struct_heap)
                } else {
                    matmul(&a, &b)
                };
                match mul_result {
                    Ok(mut result) => {
                        if result
                            .element_type_override
                            .as_ref()
                            .is_some_and(|e| e.is_complex())
                        {
                            result.struct_type_id = Some(self.get_complex_type_id());
                        }
                        self.push_array_value_as_wrapper(result)?;
                    }
                    Err(e) => {
                        self.raise(e)?;
                        return Ok(DispatchAction::Continue);
                    }
                }
            } else if (left_is_primitive && right_is_struct)
                || (left_is_struct && right_is_primitive)
                || (left_is_struct && right_is_struct)
            {
                // BinaryBothFallback: primitive-struct-methoderror [compatibility] (Issue #4262)
                // Struct operations: handled by candidates-based dispatch above.
                // Complex arithmetic goes through Julia dispatch (Issue #2422 Phase 4).
                self.raise(VmError::no_method_matching_op(
                    &left_type_name,
                    &right_type_name,
                ))?;
                return Ok(DispatchAction::Continue);
            } else if *fallback_intrinsic == Intrinsic::SremInt {
                // BinaryBothFallback: late-float-rem [compatibility] (Issue #4262)
                // `%` / `rem` (truncated remainder) with a Float64 operand.
                let a = match &left {
                    Value::F64(v) => *v,
                    Value::I64(v) => *v as f64,
                    _ => {
                        self.raise(VmError::TypeError(format!(
                            "mod expects numeric, got {} and {}",
                            left_type_name, right_type_name
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let b = match &right {
                    Value::F64(v) => *v,
                    Value::I64(v) => *v as f64,
                    _ => {
                        self.raise(VmError::TypeError(format!(
                            "mod expects numeric, got {} and {}",
                            left_type_name, right_type_name
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                // `%` / `rem` is the truncated remainder (sign of the dividend):
                // rem(a, b) = a - trunc(a/b) * b. NOT floor — that is `mod`, which
                // base/math.jl derives from `%` with its own sign adjustment (#6895).
                let result = a - (a / b).trunc() * b;
                self.stack.push(Value::F64(result));
            } else {
                // Mixed struct + primitive or other non-primitive types
                // Check for String/Char operations before raising MethodError (Issue #2127)
                let is_str_or_char = |v: &Value| matches!(v, Value::Str(_) | Value::Char(_));
                if is_str_or_char(&left) && is_str_or_char(&right) {
                    // String/Char * concatenation
                    if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
                        if let Some(result) = try_string_char_concat(&left, &right) {
                            self.stack.push(result);
                        } else {
                            return Err(VmError::InternalError(
                                "string/char concat match but helper returned None".to_string(),
                            ));
                        }
                    } else if let (Value::Str(l), Value::Str(r)) = (&left, &right) {
                        // String comparison operations (only for Str-Str)
                        if matches!(fallback_intrinsic, Intrinsic::EqFloat) {
                            self.stack.push(Value::Bool(l == r));
                        } else if matches!(fallback_intrinsic, Intrinsic::NeFloat) {
                            self.stack.push(Value::Bool(l != r));
                        } else if matches!(fallback_intrinsic, Intrinsic::LtFloat) {
                            self.stack.push(Value::Bool(l < r));
                        } else if matches!(fallback_intrinsic, Intrinsic::LeFloat) {
                            self.stack.push(Value::Bool(l <= r));
                        } else if matches!(fallback_intrinsic, Intrinsic::GtFloat) {
                            self.stack.push(Value::Bool(l > r));
                        } else if matches!(fallback_intrinsic, Intrinsic::GeFloat) {
                            self.stack.push(Value::Bool(l >= r));
                        } else {
                            self.raise(VmError::MethodError(format!(
                                "no method matching operator(String, String) for {:?}",
                                fallback_intrinsic
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                    } else if matches!(
                        fallback_intrinsic,
                        Intrinsic::EqFloat
                            | Intrinsic::EqInt
                            | Intrinsic::NeFloat
                            | Intrinsic::NeInt
                    ) {
                        let is_equal = values_equal_for_operator(&left, &right);
                        let result = match fallback_intrinsic {
                            Intrinsic::EqFloat | Intrinsic::EqInt => is_equal,
                            Intrinsic::NeFloat | Intrinsic::NeInt => !is_equal,
                            _ => unreachable!(),
                        };
                        self.stack.push(Value::Bool(result));
                    } else {
                        // Char-involved comparison: not supported
                        self.raise(VmError::MethodError(format!(
                            "no method matching operator({}, {}) for {:?}",
                            left_type_name, right_type_name, fallback_intrinsic
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                } else if let (Value::Tuple(l), Value::Tuple(r)) = (&left, &right) {
                    // BinaryBothFallback: tuple-equality [candidate] (Issue #4262)
                    let is_equal = l.elements.len() == r.elements.len()
                        && l.elements
                            .iter()
                            .zip(r.elements.iter())
                            .all(|(lv, rv)| values_equal_for_tuple_operator(lv, rv));
                    let result = match fallback_intrinsic {
                        Intrinsic::EqFloat | Intrinsic::EqInt => is_equal,
                        Intrinsic::NeFloat | Intrinsic::NeInt => !is_equal,
                        _ => {
                            self.raise(VmError::MethodError(format!(
                                "comparison op {:?} not supported for Tuple",
                                fallback_intrinsic
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(Value::Bool(result));
                } else if let (Value::DataType(l), Value::DataType(r)) = (&left, &right) {
                    // BinaryBothFallback: datatype-equality [compatibility] (Issue #4262)
                    // DataType equality comparison (e.g., Int64 == Float64)
                    let result = match fallback_intrinsic {
                        Intrinsic::EqFloat | Intrinsic::EqInt => type_objects_equal(l, r),
                        Intrinsic::NeFloat | Intrinsic::NeInt => !type_objects_equal(l, r),
                        _ => {
                            self.raise(VmError::MethodError(format!(
                                "comparison op {:?} not supported for DataType",
                                fallback_intrinsic
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(Value::Bool(result));
                } else if let (Value::RuntimeTypeVar(l), Value::RuntimeTypeVar(r)) = (&left, &right)
                {
                    // BinaryBothFallback: typevar-equality [compatibility] (Issue #4262)
                    let same_identity = l.id == r.id;
                    let result = match fallback_intrinsic {
                        Intrinsic::EqFloat | Intrinsic::EqInt => same_identity,
                        Intrinsic::NeFloat | Intrinsic::NeInt => !same_identity,
                        _ => {
                            self.raise(VmError::MethodError(format!(
                                "comparison op {:?} not supported for TypeVar",
                                fallback_intrinsic
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    self.stack.push(Value::Bool(result));
                } else if let Some((scalar_val, arr_val)) =
                    if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
                        struct_scalar_array_pair(&left, &right, &self.struct_heap)?
                    } else {
                        None
                    }
                {
                    // BinaryBothFallback: complex-array-mul [compatibility] (Issue #4262)
                    // Complex * Vector or Vector * Complex: scalar-vector multiplication
                    // Check if scalar is Complex
                    let scalar_struct = match &scalar_val {
                        Value::Struct(s) => Some(s.clone()),
                        Value::StructRef(idx) => self.struct_heap.get(*idx).cloned(),
                        _ => None,
                    };

                    if let Some(s) = &scalar_struct {
                        if self.is_complex(s) {
                            // Get complex scalar components
                            let (c_re, c_im) = s.as_complex_parts().unwrap_or((0.0, 0.0));

                            // Use matmul helper for scalar-vector multiplication
                            use crate::vm::matmul::{scalar_vector_mul_complex, Complex64};
                            let scalar = Complex64::new(c_re, c_im);
                            let mul_result =
                                scalar_vector_mul_complex(scalar, &arr_val, &self.struct_heap);

                            match mul_result {
                                Ok(mut result) => {
                                    // Store correct Complex type_id (Issue #1804)
                                    if result
                                        .element_type_override
                                        .as_ref()
                                        .is_some_and(|e| e.is_complex())
                                    {
                                        result.struct_type_id = Some(self.get_complex_type_id());
                                    }
                                    self.push_array_value_as_wrapper(result)?;
                                }
                                Err(e) => {
                                    self.raise(e)?;
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                        } else {
                            // Non-Complex struct * Vector - not supported
                            self.raise(VmError::no_method_matching_op(
                                &left_type_name,
                                &right_type_name,
                            ))?;
                            return Ok(DispatchAction::Continue);
                        }
                    } else {
                        self.raise(VmError::no_method_matching_op(
                            &left_type_name,
                            &right_type_name,
                        ))?;
                        return Ok(DispatchAction::Continue);
                    }
                } else if let Some((scalar_val, arr)) =
                    if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
                        real_scalar_array_pair(&left, &right, &self.struct_heap)?
                    } else {
                        None
                    }
                {
                    // BinaryBothFallback: real-array-mul [compatibility] (Issue #4262)
                    // ============================================================
                    // Scalar-Array Multiplication Dispatch (Issue #1799)
                    // ============================================================
                    // This handles both `scalar * array` and `array * scalar` for
                    // all numeric types. The dispatch tree:
                    //
                    // 1. Real scalar × Complex array → scalar_vector_mul_complex
                    // 2. Real scalar × Real array    → scalar_vector_mul_real
                    //
                    // IMPORTANT: Always handle BOTH complex AND real arrays.
                    // Never make the else branch raise MethodError - that creates
                    // asymmetric dispatch where Complex works but Real doesn't.
                    //
                    // For a unified dispatcher, see: vm/matmul::scalar_vector_mul()
                    // ============================================================
                    use crate::vm::matmul::{
                        is_complex_array, scalar_vector_mul_complex, Complex64,
                    };

                    if is_complex_array(&arr) {
                        // Real scalar * Complex array: convert scalar to Complex(re, 0.0)
                        let scalar_f64 = match scalar_val {
                            Value::F64(v) => v,
                            Value::I64(v) => v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "real scalar-array path: unexpected scalar {:?}",
                                    scalar_val
                                )))
                            }
                        };
                        let scalar = Complex64::from_real(scalar_f64);
                        let mul_result = scalar_vector_mul_complex(scalar, &arr, &self.struct_heap);

                        match mul_result {
                            Ok(mut result) => {
                                // Store correct Complex type_id (Issue #1804)
                                if result
                                    .element_type_override
                                    .as_ref()
                                    .is_some_and(|e| e.is_complex())
                                {
                                    result.struct_type_id = Some(self.get_complex_type_id());
                                }
                                self.push_array_value_as_wrapper(result)?;
                            }
                            Err(e) => {
                                self.raise(e)?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    } else {
                        // Real scalar * Real array: element-wise multiplication
                        use crate::vm::matmul::scalar_vector_mul_real;
                        let scalar_f64 = match scalar_val {
                            Value::F64(v) => v,
                            Value::I64(v) => v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "real scalar-array path: unexpected scalar {:?}",
                                    scalar_val
                                )))
                            }
                        };
                        let mul_result = scalar_vector_mul_real(scalar_f64, &arr);

                        match mul_result {
                            Ok(result) => {
                                self.push_array_value_as_wrapper(result)?;
                            }
                            Err(e) => {
                                self.raise(e)?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    }
                } else if let Some((a, b)) = if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
                    runtime_array_pair(&left, &right, &self.struct_heap)?
                } else {
                    None
                } {
                    // BinaryBothFallback: array-array-matmul [compatibility] (Issue #4262)
                    // Array * Array: use matrix multiplication (handles mixed real/complex)
                    use crate::vm::matmul::{is_complex_array, matmul, matmul_complex};
                    let a_is_complex = is_complex_array(&a);
                    let b_is_complex = is_complex_array(&b);
                    let mul_result = if a_is_complex || b_is_complex {
                        matmul_complex(&a, &b, &self.struct_heap)
                    } else {
                        matmul(&a, &b)
                    };
                    match mul_result {
                        Ok(mut result) => {
                            // Store correct Complex type_id (Issue #1804)
                            if result
                                .element_type_override
                                .as_ref()
                                .is_some_and(|e| e.is_complex())
                            {
                                result.struct_type_id = Some(self.get_complex_type_id());
                            }
                            self.push_array_value_as_wrapper(result)?;
                        }
                        Err(e) => {
                            self.raise(e)?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                } else if {
                    let is_real_scalar =
                        |v: &Value| matches!(v, Value::I64(_) | Value::F64(_) | Value::F32(_));
                    (is_real_scalar(&left) && is_array_like_value(&right, &self.struct_heap))
                        || (is_array_like_value(&left, &self.struct_heap) && is_real_scalar(&right))
                } && matches!(
                    fallback_intrinsic,
                    Intrinsic::DivFloat
                        | Intrinsic::AddFloat
                        | Intrinsic::SubFloat
                        | Intrinsic::MulFloat
                ) {
                    // BinaryBothFallback: array-scalar-ops [compatibility] (Issue #4262)
                    // Array / Scalar, Scalar / Array, and other element-wise ops (Issue #1929)
                    // Use dynamic_ops which already handles Array/Scalar operations
                    let result = match fallback_intrinsic {
                        Intrinsic::AddFloat => self.dynamic_add(&left, &right)?,
                        Intrinsic::SubFloat => self.dynamic_sub(&left, &right)?,
                        Intrinsic::MulFloat => self.dynamic_mul(&left, &right)?,
                        Intrinsic::DivFloat => self.dynamic_div(&left, &right)?,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "array-scalar path: unexpected intrinsic {:?}",
                                fallback_intrinsic
                            )))
                        }
                    };
                    self.stack.push(result);
                } else if matches!(
                    fallback_intrinsic,
                    Intrinsic::EqFloat | Intrinsic::EqInt | Intrinsic::NeFloat | Intrinsic::NeInt
                ) {
                    let is_equal = values_equal_for_operator(&left, &right);
                    let result = match fallback_intrinsic {
                        Intrinsic::EqFloat | Intrinsic::EqInt => is_equal,
                        Intrinsic::NeFloat | Intrinsic::NeInt => !is_equal,
                        _ => unreachable!(),
                    };
                    self.stack.push(Value::Bool(result));
                } else {
                    // BinaryBothFallback: methoderror-fallback [compatibility] (Issue #4262)
                    // No matching method found - raise MethodError
                    self.raise(VmError::no_method_matching_op(
                        &left_type_name,
                        &right_type_name,
                    ))?;
                    return Ok(DispatchAction::Continue);
                }
            }
        }
        Ok(DispatchAction::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_pairs_preempt_shared_resolver() {
        assert_eq!(
            binary_both_fallback_precedence(&Value::I64(1), &Value::F64(2.0)),
            BinaryBothFallbackPrecedence::PrimitiveFallbackFirst
        );
        assert_eq!(
            binary_both_fallback_precedence(&Value::Bool(true), &Value::I64(2)),
            BinaryBothFallbackPrecedence::PrimitiveFallbackFirst
        );
    }

    #[test]
    fn big_number_intrinsics_preempt_shared_resolver() {
        assert_eq!(
            binary_both_fallback_precedence(&Value::bigint_from_i64(1), &Value::I64(2)),
            BinaryBothFallbackPrecedence::PrimitiveFallbackFirst
        );
        assert_eq!(
            binary_both_fallback_precedence(&Value::bigfloat_from_f64(1.0), &Value::F64(2.0)),
            BinaryBothFallbackPrecedence::PrimitiveFallbackFirst
        );
    }

    #[test]
    fn non_numeric_pairs_use_shared_resolver_first() {
        assert_eq!(
            binary_both_fallback_precedence(&Value::Str("x".to_string()), &Value::I64(2)),
            BinaryBothFallbackPrecedence::SharedResolverFirst
        );
    }

    #[test]
    fn fast_primitive_binary_both_handles_hot_f64_ops() {
        let result = match fast_primitive_binary_both(
            &Value::F64(2.0),
            &Value::F64(3.0),
            &Intrinsic::MulFloat,
        ) {
            Some(Ok(value)) => value,
            Some(Err(err)) => panic!("F64 * F64 should not throw: {err:?}"),
            None => panic!("F64 * F64 should use the primitive fast path"),
        };

        assert!(matches!(result, Value::F64(v) if v == 6.0));
    }

    #[test]
    fn fast_primitive_binary_both_handles_hot_i64_ops() {
        let result = match fast_primitive_binary_both(
            &Value::I64(2),
            &Value::I64(3),
            &Intrinsic::AddFloat,
        ) {
            Some(Ok(value)) => value,
            Some(Err(err)) => panic!("I64 + I64 should not throw: {err:?}"),
            None => panic!("I64 + I64 should use the primitive fast path"),
        };

        assert!(matches!(result, Value::I64(5)));
    }

    #[test]
    fn promote_numeric_pair_promotes_to_f64_with_expected_policy() {
        // Float16 × Float64 → F64 pair, replaced-arm policy (label preserved).
        let (l, r, policy) =
            promote_numeric_pair(&Value::F16(half::f16::from_f64(1.5)), &Value::F64(2.0))
                .unwrap_or_else(|| panic!("F16×F64 must promote"));
        assert!(matches!(l, Value::F64(v) if v == 1.5));
        assert!(matches!(r, Value::F64(v) if v == 2.0));
        assert_eq!(
            policy,
            PromotedPairPolicy::RaiseUnsupported("Float16-Float64")
        );

        // Float32 × Float64 → F64 pair (either order).
        let (_, _, policy) = promote_numeric_pair(&Value::F64(2.0), &Value::F32(1.0))
            .unwrap_or_else(|| panic!("F64×F32 must promote"));
        assert_eq!(
            policy,
            PromotedPairPolicy::RaiseUnsupported("Float32-Float64")
        );

        // Int64 × Float64 → F64 pair, legacy chain keeps unhandled ops.
        let (l, r, policy) = promote_numeric_pair(&Value::I64(3), &Value::F64(0.5))
            .unwrap_or_else(|| panic!("I64×F64 must promote"));
        assert!(matches!(l, Value::F64(v) if v == 3.0));
        assert!(matches!(r, Value::F64(v) if v == 0.5));
        assert_eq!(policy, PromotedPairPolicy::FallThrough);
    }

    #[test]
    fn promote_numeric_pair_promotes_to_f32_with_expected_policy() {
        // Float16 × Float32 → F32 pair, replaced-arm policy (label preserved).
        let (l, r, policy) =
            promote_numeric_pair(&Value::F16(half::f16::from_f64(1.5)), &Value::F32(2.0))
                .unwrap_or_else(|| panic!("F16×F32 must promote"));
        assert!(matches!(l, Value::F32(v) if v == 1.5));
        assert!(matches!(r, Value::F32(v) if v == 2.0));
        assert_eq!(
            policy,
            PromotedPairPolicy::RaiseUnsupported("Float16-Float32")
        );

        // Float32 × Int64 → F32 pair: the int operand is converted FIRST
        // (true promote semantics), result stays F32.
        let (l, r, policy) = promote_numeric_pair(&Value::I64(3), &Value::F32(0.5))
            .unwrap_or_else(|| panic!("I64×F32 must promote"));
        assert!(matches!(l, Value::F32(v) if v == 3.0));
        assert!(matches!(r, Value::F32(v) if v == 0.5));
        assert_eq!(
            policy,
            PromotedPairPolicy::RaiseUnsupported("Float32-Int64")
        );

        // Float32 × Int128 → F32 pair; legacy arm's literal "Float32-Int64"
        // label is preserved even for Int128.
        let (l, _, policy) = promote_numeric_pair(&Value::F32(1.0), &Value::I128(2))
            .unwrap_or_else(|| panic!("F32×I128 must promote"));
        assert!(matches!(l, Value::F32(v) if v == 1.0));
        assert_eq!(
            policy,
            PromotedPairPolicy::RaiseUnsupported("Float32-Int64")
        );
    }

    #[test]
    fn promote_numeric_pair_skips_same_type_and_exception_pairs() {
        // Same-type pairs are owned by the same-type fast path, not promotion.
        assert!(promote_numeric_pair(&Value::F64(1.0), &Value::F64(2.0)).is_none());
        assert!(promote_numeric_pair(&Value::I64(1), &Value::I64(2)).is_none());
        assert!(promote_numeric_pair(&Value::F32(1.0), &Value::F32(2.0)).is_none());
        assert!(promote_numeric_pair(
            &Value::F16(half::f16::from_f64(1.0)),
            &Value::F16(half::f16::from_f64(2.0))
        )
        .is_none());
        // Behavior-exception pairs must keep their explicit arms (Issue #5966):
        // Bool, Float16×Int (result-narrowing, not operand promotion),
        // unsigned, Int128×Int64/F64, BigInt.
        assert!(promote_numeric_pair(&Value::Bool(true), &Value::F64(2.0)).is_none());
        assert!(
            promote_numeric_pair(&Value::F16(half::f16::from_f64(1.0)), &Value::I64(2)).is_none()
        );
        assert!(promote_numeric_pair(&Value::U64(1), &Value::F64(2.0)).is_none());
        assert!(promote_numeric_pair(&Value::U64(1), &Value::F32(2.0)).is_none());
        assert!(promote_numeric_pair(&Value::I128(1), &Value::F64(2.0)).is_none());
        assert!(promote_numeric_pair(&Value::I128(1), &Value::I64(2)).is_none());
        // Non-numeric pairs never promote.
        assert!(promote_numeric_pair(&Value::Str("a".to_string()), &Value::F64(2.0)).is_none());
    }

    #[test]
    fn same_type_fast_path_f64_extends_floor_mod_and_div() {
        // mod: a - floor(a/b) * b (sign follows b, Julia semantics).
        let result =
            match same_type_fast_path(&Intrinsic::SremInt, &Value::F64(-7.0), &Value::F64(3.0)) {
                Some(Ok(value)) => value,
                other => panic!("F64 mod must hit the same-type table, got {other:?}"),
            };
        assert!(matches!(result, Value::F64(v) if v == 2.0));

        // div: floor(a/b).
        let result =
            match same_type_fast_path(&Intrinsic::SdivInt, &Value::F64(7.0), &Value::F64(2.0)) {
                Some(Ok(value)) => value,
                other => panic!("F64 div must hit the same-type table, got {other:?}"),
            };
        assert!(matches!(result, Value::F64(v) if v == 3.0));

        // Hot pairs still route through the primitive table.
        let result =
            match same_type_fast_path(&Intrinsic::AddFloat, &Value::F64(1.5), &Value::F64(2.0)) {
                Some(Ok(value)) => value,
                other => panic!("F64 + F64 must hit the fast path, got {other:?}"),
            };
        assert!(matches!(result, Value::F64(v) if v == 3.5));

        // Ops the legacy chain still owns return None (e.g. PowFloat).
        assert!(
            same_type_fast_path(&Intrinsic::PowFloat, &Value::F64(2.0), &Value::F64(3.0)).is_none()
        );
    }

    #[test]
    fn same_type_fast_path_handles_f32_and_f16_tables() {
        // F32 same-type table: F32-preserving arithmetic (Issue #3750).
        let result =
            match same_type_fast_path(&Intrinsic::AddFloat, &Value::F32(1.5), &Value::F32(2.0)) {
                Some(Ok(value)) => value,
                other => panic!("F32 + F32 must hit the same-type table, got {other:?}"),
            };
        assert!(matches!(result, Value::F32(v) if v == 3.5));

        // F32 floor-mod (Issue #1776) and floor-div (Issue #1849).
        let result =
            match same_type_fast_path(&Intrinsic::SremInt, &Value::F32(-7.0), &Value::F32(3.0)) {
                Some(Ok(value)) => value,
                other => panic!("F32 mod must hit the same-type table, got {other:?}"),
            };
        assert!(matches!(result, Value::F32(v) if v == 2.0));
        let result =
            match same_type_fast_path(&Intrinsic::SdivInt, &Value::F32(7.0), &Value::F32(2.0)) {
                Some(Ok(value)) => value,
                other => panic!("F32 div must hit the same-type table, got {other:?}"),
            };
        assert!(matches!(result, Value::F32(v) if v == 3.0));

        // F16 same-type table: compute in F64, narrow back to F16 (Issue #3621).
        let result = match same_type_fast_path(
            &Intrinsic::MulFloat,
            &Value::F16(half::f16::from_f64(1.5)),
            &Value::F16(half::f16::from_f64(2.0)),
        ) {
            Some(Ok(value)) => value,
            other => panic!("F16 * F16 must hit the same-type table, got {other:?}"),
        };
        assert!(matches!(result, Value::F16(v) if v.to_f64() == 3.0));

        // Comparisons return Bool.
        let result = match same_type_fast_path(
            &Intrinsic::LtFloat,
            &Value::F16(half::f16::from_f64(1.0)),
            &Value::F16(half::f16::from_f64(2.0)),
        ) {
            Some(Ok(value)) => value,
            other => panic!("F16 < F16 must hit the same-type table, got {other:?}"),
        };
        assert!(matches!(result, Value::Bool(true)));

        // Unsupported ops stay with the per-pair policy (PowFloat -> None).
        assert!(
            same_type_fast_path(&Intrinsic::PowFloat, &Value::F32(2.0), &Value::F32(3.0)).is_none()
        );
    }

    #[test]
    fn narrow_int_arith_result_kind_preserves_same_narrow_type() {
        // Same narrow type + arithmetic op -> wrap back to that narrow type.
        for op in [
            Intrinsic::AddFloat,
            Intrinsic::SubFloat,
            Intrinsic::MulFloat,
        ] {
            assert!(narrow_int_arith_result_kind(&Value::I8(1), &Value::I8(2), &op).is_some());
            assert!(narrow_int_arith_result_kind(&Value::U8(1), &Value::U8(2), &op).is_some());
            assert!(narrow_int_arith_result_kind(&Value::I16(1), &Value::I16(2), &op).is_some());
        }
    }

    #[test]
    fn narrow_int_arith_result_kind_skips_mixed_and_wide_and_nonarith() {
        // Mixed narrow types are left to promotion (no same-type narrowing).
        assert!(
            narrow_int_arith_result_kind(&Value::I8(1), &Value::I16(2), &Intrinsic::AddFloat)
                .is_none()
        );
        // I64 is not a narrow type.
        assert!(
            narrow_int_arith_result_kind(&Value::I64(1), &Value::I64(2), &Intrinsic::AddFloat)
                .is_none()
        );
        // Comparisons / division are not type-preserving arithmetic.
        assert!(
            narrow_int_arith_result_kind(&Value::I8(1), &Value::I8(2), &Intrinsic::LtFloat)
                .is_none()
        );
        assert!(
            narrow_int_arith_result_kind(&Value::I8(1), &Value::I8(2), &Intrinsic::DivFloat)
                .is_none()
        );
    }

    #[test]
    fn narrow_int_kind_wraps_i64_modularly() {
        // 132 wraps to -124 as Int8; 300 wraps to 44 as UInt8.
        assert!(matches!(NarrowIntKind::I8.wrap_i64(132), Value::I8(-124)));
        assert!(matches!(NarrowIntKind::U8.wrap_i64(300), Value::U8(44)));
        assert!(matches!(
            NarrowIntKind::I16.wrap_i64(60000),
            Value::I16(-5536)
        ));
    }
}
