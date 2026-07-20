// Broadcast helper utilities for element-wise array operations.
//
// This module provides low-level broadcast primitives used by:
// - dynamic_ops.rs: DynamicAdd/Sub/Mul/Div for Array operands
// - hof_exec.rs: HOF state machine iteration (broadcast_get_index, compute_strides)
//

// SAFETY: isize→usize casts are guarded by `if a_idx >= 0` / `if b_idx >= 0` checks.
#![allow(clippy::cast_sign_loss)]
// The broadcast *instructions* (BroadcastBinOp, BroadcastUnaryOp, etc.) have been
// removed (Issue #2680). Broadcasting is now handled by the Pure Julia pipeline
// (broadcast.jl / Broadcast.jl).

use super::error::VmError;
use super::value::{ArrayElementType, ArrayValue, MemoryRef, Value};

/// Enum to represent either an array or a scalar for broadcasting.
///
/// Only Array and ScalarF64 variants are currently constructed by callers
/// (dynamic_ops.rs). Other variants are retained for match completeness in
/// broadcast_op_f64 / broadcast_op_complex but are unreachable in practice.
/// This is intentionally broader than current runtime construction sites.
#[allow(dead_code)]
pub(crate) enum Broadcastable {
    Array(ArrayValue),
    Memory(MemoryRef),
    ScalarF64(f64),
}

impl Broadcastable {
    pub(crate) fn shape(&self) -> Vec<usize> {
        match self {
            Broadcastable::Array(arr) => arr.shape.clone(),
            Broadcastable::Memory(mem) => vec![mem.borrow().len()],
            Broadcastable::ScalarF64(_) => vec![1],
        }
    }

    fn element_count(&self) -> usize {
        match self {
            Broadcastable::Array(arr) => arr.element_count(),
            Broadcastable::Memory(mem) => mem.borrow().len(),
            Broadcastable::ScalarF64(_) => 1,
        }
    }

    fn get_linear_f64(&self, linear: usize) -> Result<f64, VmError> {
        match self {
            Broadcastable::Array(arr) => arr.get_linear_f64(linear),
            Broadcastable::Memory(mem) => {
                let value = mem.borrow().get(linear + 1)?;
                value_to_f64(value)
            }
            Broadcastable::ScalarF64(v) => Ok(*v),
        }
    }

    /// Check if any operand involves complex numbers
    pub(crate) fn is_complex(&self) -> bool {
        match self {
            Broadcastable::Array(arr) => {
                if arr.element_type().is_complex() {
                    return true;
                }
                // Legacy interleaved complex arrays may not carry the logical
                // element tag; keep the raw-length sentinel as a compatibility
                // fallback while Memory-backed arrays migrate to type tags.
                let element_count = arr.element_count();
                element_count > 0 && arr.len() == element_count * 2
            }
            Broadcastable::Memory(_) => false,
            _ => false,
        }
    }
}

fn value_to_f64(value: Value) -> Result<f64, VmError> {
    match value {
        Value::F64(v) => Ok(v),
        Value::F32(v) => Ok(v as f64),
        Value::F16(v) => Ok(v.to_f64()),
        Value::I64(v) => Ok(v as f64),
        Value::I32(v) => Ok(v as f64),
        Value::I16(v) => Ok(v as f64),
        Value::I8(v) => Ok(v as f64),
        Value::U64(v) => Ok(v as f64),
        Value::U32(v) => Ok(v as f64),
        Value::U16(v) => Ok(v as f64),
        Value::U8(v) => Ok(v as f64),
        Value::Bool(v) => Ok(if v { 1.0 } else { 0.0 }),
        other => Err(VmError::TypeError(format!(
            "expected numeric broadcast element, got {:?}",
            other.value_type()
        ))),
    }
}

/// Compute the result shape for Julia-style broadcasting.
///
/// Julia treats 1D arrays as column vectors in 2D contexts:
/// - [n] is conceptually [n, 1] when broadcast with 2D arrays
///
/// Examples:
/// - [1, 9] .* [9] → [9, 9]  (outer product: row .* col)
/// - [5, 1] .* [1, 3] → [5, 3]
/// - [3] .+ [3] → [3]
/// - [2, 3] .* [3] → [2, 3]
pub(crate) fn compute_broadcast_shape(
    shape_a: &[usize],
    shape_b: &[usize],
) -> Result<Vec<usize>, VmError> {
    // Julia-specific: 1D arrays are treated as column vectors in 2D+ contexts
    // [n] becomes [n, 1] when broadcast with [m, k]
    let (a_expanded, b_expanded) = expand_shapes_for_julia(shape_a, shape_b);

    let max_dims = a_expanded.len().max(b_expanded.len());
    let mut result = Vec::with_capacity(max_dims);

    // Align from the right (trailing dimensions)
    for i in 0..max_dims {
        let a_idx = a_expanded.len() as isize - max_dims as isize + i as isize;
        let b_idx = b_expanded.len() as isize - max_dims as isize + i as isize;

        let a_dim = if a_idx >= 0 {
            a_expanded[a_idx as usize]
        } else {
            1
        };
        let b_dim = if b_idx >= 0 {
            b_expanded[b_idx as usize]
        } else {
            1
        };

        // Check compatibility: dimensions must be equal or one of them is 1
        if a_dim != b_dim && a_dim != 1 && b_dim != 1 {
            return Err(VmError::BroadcastDimensionMismatch {
                a_shape: shape_a.to_vec(),
                b_shape: shape_b.to_vec(),
            });
        }

        result.push(a_dim.max(b_dim));
    }

    Ok(result)
}

/// Expand shapes for Julia-style broadcasting.
/// In Julia, 1D arrays are column vectors, so [n] becomes [n, 1] in 2D contexts.
pub(crate) fn expand_shapes_for_julia(
    shape_a: &[usize],
    shape_b: &[usize],
) -> (Vec<usize>, Vec<usize>) {
    let ndims_a = shape_a.len();
    let ndims_b = shape_b.len();

    // If both are 1D, no expansion needed
    if ndims_a <= 1 && ndims_b <= 1 {
        return (shape_a.to_vec(), shape_b.to_vec());
    }

    // If one is 1D and the other is 2D+, expand the 1D to be a column [n] → [n, 1]
    let a_expanded = if ndims_a == 1 && ndims_b >= 2 {
        let mut expanded = shape_a.to_vec();
        expanded.push(1); // [n] → [n, 1]
        expanded
    } else {
        shape_a.to_vec()
    };

    let b_expanded = if ndims_b == 1 && ndims_a >= 2 {
        let mut expanded = shape_b.to_vec();
        expanded.push(1); // [n] → [n, 1]
        expanded
    } else {
        shape_b.to_vec()
    };

    (a_expanded, b_expanded)
}

/// Compute strides for column-major (Julia-style) array indexing.
pub(crate) fn compute_strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = Vec::with_capacity(shape.len());
    let mut stride = 1;
    for &dim in shape {
        strides.push(stride);
        stride *= dim;
    }
    strides
}

/// Compute the source array index for a given result index during broadcast.
/// For dimensions where the original size is 1, the index is always 0 (broadcast).
/// Uses column-major ordering (Julia convention).
pub(crate) fn broadcast_get_index(
    linear_idx: usize,
    result_shape: &[usize],
    result_strides: &[usize],
    orig_shape: &[usize],
    orig_strides: &[usize],
    ndims_diff: usize, // result.ndims - orig.ndims
) -> usize {
    let mut orig_idx = 0;
    let mut remaining = linear_idx;

    // Decompose linear index into multi-dimensional indices (column-major)
    // and compute the original array index
    for i in (0..result_shape.len()).rev() {
        let dim_idx = remaining / result_strides[i];
        remaining %= result_strides[i];

        // Map to original array dimension (offset by ndims_diff)
        if i >= ndims_diff {
            let orig_dim_idx = i - ndims_diff;
            if orig_dim_idx < orig_shape.len() {
                // If original dimension is 1, always use index 0 (broadcast)
                // Otherwise, use the dimension index from the result
                let mapped_idx = if orig_shape[orig_dim_idx] == 1 {
                    0
                } else {
                    // Ensure dim_idx is within bounds of original dimension
                    dim_idx.min(orig_shape[orig_dim_idx] - 1)
                };
                orig_idx += mapped_idx * orig_strides[orig_dim_idx];
            }
        }
        // If i < ndims_diff, this dimension doesn't exist in original (implicit 1)
        // For implicit dimensions, we don't add anything to orig_idx (they're broadcast)
    }

    orig_idx
}

/// Perform element-wise broadcast operation (f64 only)
/// Supports Julia-style broadcasting:
/// - Array .op Array (compatible shapes, broadcasts size-1 dimensions)
/// - Array .op Scalar (scalar broadcast to all elements)
/// - Scalar .op Array (scalar broadcast to all elements)
pub(crate) fn broadcast_op_f64<F>(
    a: &Broadcastable,
    b: &Broadcastable,
    op: F,
) -> Result<ArrayValue, VmError>
where
    F: Fn(f64, f64) -> f64,
{
    match (a, b) {
        // Array .op Array - Julia-style broadcasting
        (
            Broadcastable::Array(_) | Broadcastable::Memory(_),
            Broadcastable::Array(_) | Broadcastable::Memory(_),
        ) => {
            // Compute result shape using Julia broadcasting rules
            let a_shape = a.shape();
            let b_shape = b.shape();
            let result_shape = compute_broadcast_shape(&a_shape, &b_shape)?;
            let result_size: usize = result_shape.iter().product();

            // Fast path: same shape, no broadcasting needed
            if a_shape == b_shape {
                let data: Vec<f64> = (0..a.element_count())
                    .map(|i| Ok(op(a.get_linear_f64(i)?, b.get_linear_f64(i)?)))
                    .collect::<Result<_, VmError>>()?;
                return Ok(ArrayValue::memory_first_from_f64(data, a_shape));
            }

            // Get expanded shapes for Julia-style broadcasting
            let (a_expanded, b_expanded) = expand_shapes_for_julia(&a_shape, &b_shape);

            // Compute strides using expanded shapes
            let result_strides = compute_strides(&result_shape);
            let a_strides = compute_strides(&a_expanded);
            let b_strides = compute_strides(&b_expanded);
            let a_ndims_diff = result_shape.len() - a_expanded.len();
            let b_ndims_diff = result_shape.len() - b_expanded.len();

            // Build result array with broadcasting
            let mut data = Vec::with_capacity(result_size);
            for i in 0..result_size {
                let a_idx = broadcast_get_index(
                    i,
                    &result_shape,
                    &result_strides,
                    &a_expanded,
                    &a_strides,
                    a_ndims_diff,
                );
                let b_idx = broadcast_get_index(
                    i,
                    &result_shape,
                    &result_strides,
                    &b_expanded,
                    &b_strides,
                    b_ndims_diff,
                );
                data.push(op(a.get_linear_f64(a_idx)?, b.get_linear_f64(b_idx)?));
            }

            Ok(ArrayValue::memory_first_from_f64(data, result_shape))
        }
        // Array .op ScalarF64
        (Broadcastable::Array(_) | Broadcastable::Memory(_), Broadcastable::ScalarF64(s)) => {
            let data: Vec<f64> = (0..a.element_count())
                .map(|i| Ok(op(a.get_linear_f64(i)?, *s)))
                .collect::<Result<_, VmError>>()?;
            Ok(ArrayValue::memory_first_from_f64(data, a.shape()))
        }
        // ScalarF64 .op Array
        (Broadcastable::ScalarF64(s), Broadcastable::Array(_) | Broadcastable::Memory(_)) => {
            let data: Vec<f64> = (0..b.element_count())
                .map(|i| Ok(op(*s, b.get_linear_f64(i)?)))
                .collect::<Result<_, VmError>>()?;
            Ok(ArrayValue::memory_first_from_f64(data, b.shape()))
        }
        // ScalarF64 .op ScalarF64 - return 1-element array
        (Broadcastable::ScalarF64(a_val), Broadcastable::ScalarF64(b_val)) => Ok(
            ArrayValue::memory_first_from_f64(vec![op(*a_val, *b_val)], vec![1]),
        ),
    }
}

/// Complex number operations as inline helpers
#[inline]
pub(crate) fn complex_add(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}

#[inline]
pub(crate) fn complex_sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

#[inline]
pub(crate) fn complex_mul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

#[inline]
pub(crate) fn complex_div(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let denom = b.0 * b.0 + b.1 * b.1;
    (
        (a.0 * b.0 + a.1 * b.1) / denom,
        (a.1 * b.0 - a.0 * b.1) / denom,
    )
}

/// Perform element-wise broadcast operation with complex number support
/// Automatically promotes to complex when either operand is complex
/// Uses Julia-style broadcasting for arrays with compatible shapes
/// Elementwise binary arithmetic op for the upstream-exact broadcast kernel
/// (Issues #8797/#9659 parity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryArithOp {
    Add,
    Sub,
    Mul,
}

/// One fetched element: a real operand stays real (it must NOT become
/// `(x, 0.0)` — the Base mixed real/complex methods pass the complex side's
/// imaginary part through untouched, which differs from `0.0 + im` for
/// signed zeros).
#[derive(Clone, Copy)]
enum ArithElem {
    R(f64),
    C(f64, f64),
}

/// Upstream-exact elementwise `+`/`-`/`*` over two broadcastable numeric
/// operands (Issues #8797/#9659): shape expansion per Julia broadcasting
/// rules, element formulas mirroring the dispatched Base methods
/// (complex.jl) bit-for-bit, including signed zeros. Supports `Array` (f64 /
/// int-backed real, or interleaved ComplexF64) and `ScalarF64` operands;
/// anything else returns `Ok(None)` so the caller keeps the generic path.
pub(crate) fn broadcast_binary_arith_exact(
    a: &Broadcastable,
    b: &Broadcastable,
    op: BinaryArithOp,
) -> Result<Option<ArrayValue>, VmError> {
    let a_complex = a.is_complex();
    let b_complex = b.is_complex();
    let out_complex = a_complex || b_complex;

    // Guard operand storage kinds: real sides must be f64-backed unless the
    // output is complex (an int-backed real side promotes to f64 exactly);
    // complex sides must be interleaved-f64 ComplexF64.
    fn side_supported(bc: &Broadcastable, is_complex: bool, out_complex: bool) -> bool {
        match bc {
            Broadcastable::ScalarF64(_) => true,
            Broadcastable::Array(arr) => {
                if is_complex {
                    arr.complex_interleaved_f64().is_ok()
                } else {
                    match arr.element_type() {
                        super::value::ArrayElementType::F64 => true,
                        super::value::ArrayElementType::I64 => out_complex,
                        _ => false,
                    }
                }
            }
            Broadcastable::Memory(_) => false,
        }
    }
    if !side_supported(a, a_complex, out_complex) || !side_supported(b, b_complex, out_complex) {
        return Ok(None);
    }

    let a_shape = a.shape();
    let b_shape = b.shape();
    let result_shape = compute_broadcast_shape(&a_shape, &b_shape)?;
    let result_size: usize = result_shape.iter().product();
    if result_size == 0 {
        return Ok(None);
    }
    let (a_expanded, b_expanded) = expand_shapes_for_julia(&a_shape, &b_shape);
    let result_strides = compute_strides(&result_shape);
    let a_strides = compute_strides(&a_expanded);
    let b_strides = compute_strides(&b_expanded);
    let a_ndims_diff = result_shape.len() - a_expanded.len();
    let b_ndims_diff = result_shape.len() - b_expanded.len();

    let fetch = |bc: &Broadcastable,
                 is_complex: bool,
                 idx: usize,
                 shape: &[usize],
                 strides: &[usize],
                 diff: usize|
     -> Result<ArithElem, VmError> {
        match bc {
            Broadcastable::ScalarF64(v) => Ok(ArithElem::R(*v)),
            Broadcastable::Array(arr) => {
                let src_idx =
                    broadcast_get_index(idx, &result_shape, &result_strides, shape, strides, diff);
                if is_complex {
                    let buf = arr.complex_interleaved_f64()?;
                    Ok(ArithElem::C(buf[src_idx * 2], buf[src_idx * 2 + 1]))
                } else {
                    Ok(ArithElem::R(arr.get_linear_f64(src_idx)?))
                }
            }
            Broadcastable::Memory(_) => Err(VmError::InternalError(
                "broadcast_binary_arith_exact: unsupported operand".to_string(),
            )),
        }
    };

    // Combine per the dispatched Base method formulas (base/complex.jl):
    // real operands pass the complex side's imaginary part through (negated
    // for `x - z`), so signed zeros match the generic path bit-for-bit.
    let combine = |av: ArithElem, bv: ArithElem| -> ArithElem {
        use ArithElem::{C, R};
        use BinaryArithOp::{Add, Mul, Sub};
        match (op, av, bv) {
            (Add, R(x), R(y)) => R(x + y),
            (Add, R(x), C(cr, ci)) => C(x + cr, ci),
            (Add, C(cr, ci), R(x)) => C(cr + x, ci),
            (Add, C(ar, ai), C(br, bi)) => C(ar + br, ai + bi),
            (Sub, R(x), R(y)) => R(x - y),
            (Sub, R(x), C(cr, ci)) => C(x - cr, -ci),
            (Sub, C(cr, ci), R(x)) => C(cr - x, ci),
            (Sub, C(ar, ai), C(br, bi)) => C(ar - br, ai - bi),
            (Mul, R(x), R(y)) => R(x * y),
            (Mul, R(x), C(cr, ci)) => C(x * cr, x * ci),
            (Mul, C(cr, ci), R(x)) => C(cr * x, ci * x),
            (Mul, C(ar, ai), C(br, bi)) => C(ar * br - ai * bi, ar * bi + ai * br),
        }
    };

    if out_complex {
        let mut out: Vec<f64> = Vec::with_capacity(result_size * 2);
        for idx in 0..result_size {
            let av = fetch(a, a_complex, idx, &a_expanded, &a_strides, a_ndims_diff)?;
            let bv = fetch(b, b_complex, idx, &b_expanded, &b_strides, b_ndims_diff)?;
            match combine(av, bv) {
                ArithElem::C(re, im) => {
                    out.push(re);
                    out.push(im);
                }
                ArithElem::R(_) => {
                    return Err(VmError::InternalError(
                        "broadcast_binary_arith_exact: real result in complex output".to_string(),
                    ))
                }
            }
        }
        Ok(Some(
            ArrayValue::memory_first_from_array_data_with_element_type(
                super::value::ArrayData::StructF64(out),
                result_shape,
                super::value::ArrayElementType::ComplexF64,
            ),
        ))
    } else {
        let mut out: Vec<f64> = Vec::with_capacity(result_size);
        for idx in 0..result_size {
            let av = fetch(a, a_complex, idx, &a_expanded, &a_strides, a_ndims_diff)?;
            let bv = fetch(b, b_complex, idx, &b_expanded, &b_strides, b_ndims_diff)?;
            match combine(av, bv) {
                ArithElem::R(v) => out.push(v),
                ArithElem::C(..) => {
                    return Err(VmError::InternalError(
                        "broadcast_binary_arith_exact: complex result in real output".to_string(),
                    ))
                }
            }
        }
        Ok(Some(
            ArrayValue::memory_first_from_array_data_with_element_type(
                super::value::ArrayData::F64(out),
                result_shape,
                super::value::ArrayElementType::F64,
            ),
        ))
    }
}

pub(crate) fn broadcast_op_complex<F>(
    a: &Broadcastable,
    b: &Broadcastable,
    op: F,
) -> Result<ArrayValue, VmError>
where
    F: Fn((f64, f64), (f64, f64)) -> (f64, f64),
{
    // Helper to get shape
    let a_shape = a.shape();
    let b_shape = b.shape();

    // Compute result shape using Julia broadcasting rules
    let result_shape = compute_broadcast_shape(&a_shape, &b_shape)?;
    let result_size: usize = result_shape.iter().product();

    // Compute expanded shapes and strides for broadcasting
    let (a_expanded, b_expanded) = expand_shapes_for_julia(&a_shape, &b_shape);
    let result_strides = compute_strides(&result_shape);
    let a_strides = compute_strides(&a_expanded);
    let b_strides = compute_strides(&b_expanded);
    let a_ndims_diff = result_shape.len() - a_expanded.len();
    let b_ndims_diff = result_shape.len() - b_expanded.len();

    // Extract complex values from each operand at a given index
    let get_complex_at = |bc: &Broadcastable,
                          idx: usize,
                          orig_shape: Option<&[usize]>,
                          orig_strides: Option<&[usize]>,
                          ndims_diff: Option<usize>|
     -> Result<(f64, f64), VmError> {
        match bc {
            Broadcastable::ScalarF64(v) => Ok((*v, 0.0)),
            Broadcastable::Array(arr) => {
                // Compute the correct source index for broadcasting
                let src_idx = if let (Some(shape), Some(strides), Some(diff)) =
                    (orig_shape, orig_strides, ndims_diff)
                {
                    broadcast_get_index(idx, &result_shape, &result_strides, shape, strides, diff)
                } else {
                    idx
                };

                // Check if this is an interleaved complex array. Prefer the
                // logical element type tag when present; retain the raw-length
                // sentinel for older native carriers that predate the tag.
                let element_count = arr.element_count();
                match arr.element_type() {
                    ArrayElementType::ComplexF64 | ArrayElementType::ComplexF32 => {
                        let value = arr.get_linear(src_idx)?;
                        match value.as_complex_parts() {
                            Some(parts) => Ok(parts),
                            None => Err(VmError::TypeError(format!(
                                "expected complex array element, got {:?}",
                                value.value_type()
                            ))),
                        }
                    }
                    _ if arr.len() == element_count * 2 => {
                        // ArrayDataAudit: typed interleaved ComplexF64 fast path.
                        // The real-valued public broadcast path above uses logical
                        // get_linear_f64 so reshape shared backing remains visible.
                        // Interleaved complex: [re0, im0, re1, im1, ...]. Issue #9198
                        // S5: the buffer is the contiguous-isbits `StructF64` variant,
                        // read via the storage-variant-agnostic accessor.
                        let buf = arr.complex_interleaved_f64()?;
                        Ok((buf[src_idx * 2], buf[src_idx * 2 + 1]))
                    }
                    _ => {
                        // Regular F64 array - treat as real part, imaginary part is 0
                        Ok((arr.get_linear_f64(src_idx)?, 0.0))
                    }
                }
            }
            Broadcastable::Memory(_) => {
                let src_idx = if let (Some(shape), Some(strides), Some(diff)) =
                    (orig_shape, orig_strides, ndims_diff)
                {
                    broadcast_get_index(idx, &result_shape, &result_strides, shape, strides, diff)
                } else {
                    idx
                };
                Ok((bc.get_linear_f64(src_idx)?, 0.0))
            }
        }
    };

    // Check if operand is scalar (element count == 1)
    let is_scalar = |bc: &Broadcastable| -> bool {
        match bc {
            Broadcastable::ScalarF64(_) => true,
            Broadcastable::Array(_) | Broadcastable::Memory(_) => bc.element_count() == 1,
        }
    };

    let a_is_scalar = is_scalar(a);
    let b_is_scalar = is_scalar(b);

    // Build result data (interleaved for complex)
    let mut result_data = Vec::with_capacity(result_size * 2);

    if a_is_scalar && b_is_scalar {
        let a_val = get_complex_at(a, 0, None, None, None)?;
        let b_val = get_complex_at(b, 0, None, None, None)?;
        let (re, im) = op(a_val, b_val);
        result_data.push(re);
        result_data.push(im);
    } else if a_is_scalar {
        let a_val = get_complex_at(a, 0, None, None, None)?;
        for i in 0..result_size {
            let b_val = get_complex_at(
                b,
                i,
                Some(&b_expanded),
                Some(&b_strides),
                Some(b_ndims_diff),
            )?;
            let (re, im) = op(a_val, b_val);
            result_data.push(re);
            result_data.push(im);
        }
    } else if b_is_scalar {
        let b_val = get_complex_at(b, 0, None, None, None)?;
        for i in 0..result_size {
            let a_val = get_complex_at(
                a,
                i,
                Some(&a_expanded),
                Some(&a_strides),
                Some(a_ndims_diff),
            )?;
            let (re, im) = op(a_val, b_val);
            result_data.push(re);
            result_data.push(im);
        }
    } else {
        // Array .op Array with Julia-style broadcasting
        if a_shape == b_shape {
            for i in 0..result_size {
                let a_val = get_complex_at(a, i, None, None, None)?;
                let b_val = get_complex_at(b, i, None, None, None)?;
                let (re, im) = op(a_val, b_val);
                result_data.push(re);
                result_data.push(im);
            }
        } else {
            for i in 0..result_size {
                let a_val = get_complex_at(
                    a,
                    i,
                    Some(&a_expanded),
                    Some(&a_strides),
                    Some(a_ndims_diff),
                )?;
                let b_val = get_complex_at(
                    b,
                    i,
                    Some(&b_expanded),
                    Some(&b_strides),
                    Some(b_ndims_diff),
                )?;
                let (re, im) = op(a_val, b_val);
                result_data.push(re);
                result_data.push(im);
            }
        }
    }

    Ok(ArrayValue::complex_f64(result_data, result_shape))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── compute_broadcast_shape ───────────────────────────────────────────────

    #[test]
    fn test_broadcast_shape_same_1d_shapes() {
        // [3] .+ [3] → [3]
        let result = compute_broadcast_shape(&[3], &[3]).unwrap();
        assert_eq!(result, vec![3]);
    }

    #[test]
    fn test_broadcast_shape_scalar_broadcast() {
        // [1] .+ [5] → [5]
        let result = compute_broadcast_shape(&[1], &[5]).unwrap();
        assert_eq!(result, vec![5]);
    }

    #[test]
    fn test_broadcast_shape_2d_with_size_one_dim() {
        // [5, 1] .* [1, 3] → [5, 3]
        let result = compute_broadcast_shape(&[5, 1], &[1, 3]).unwrap();
        assert_eq!(result, vec![5, 3]);
    }

    #[test]
    fn test_broadcast_shape_incompatible_dims_returns_error() {
        // [3] .+ [4] → error (neither is 1)
        let result = compute_broadcast_shape(&[3], &[4]);
        assert!(result.is_err(), "Incompatible shapes should return error");
    }

    #[test]
    fn test_broadcast_shape_1d_with_2d_as_column_vector() {
        // [3] with [3, 2] → 1D is treated as [3, 1] → result [3, 2]
        let result = compute_broadcast_shape(&[3], &[3, 2]).unwrap();
        assert_eq!(result, vec![3, 2]);
    }

    // ── expand_shapes_for_julia ───────────────────────────────────────────────

    #[test]
    fn test_expand_shapes_both_1d_unchanged() {
        // Both 1D: no expansion
        let (a, b) = expand_shapes_for_julia(&[5], &[5]);
        assert_eq!(a, vec![5]);
        assert_eq!(b, vec![5]);
    }

    #[test]
    fn test_expand_shapes_1d_with_2d_gets_column() {
        // [n] with [m, k] → [n, 1] with [m, k]
        let (a, b) = expand_shapes_for_julia(&[3], &[3, 2]);
        assert_eq!(a, vec![3, 1]); // 1D expanded to column vector
        assert_eq!(b, vec![3, 2]); // 2D unchanged
    }

    #[test]
    fn test_expand_shapes_2d_with_1d_gets_column() {
        // [m, k] with [n] → [m, k] with [n, 1]
        let (a, b) = expand_shapes_for_julia(&[3, 2], &[3]);
        assert_eq!(a, vec![3, 2]); // 2D unchanged
        assert_eq!(b, vec![3, 1]); // 1D expanded to column vector
    }

    // ── compute_strides ───────────────────────────────────────────────────────

    #[test]
    fn test_strides_empty_shape() {
        let strides = compute_strides(&[]);
        assert_eq!(strides, Vec::<usize>::new());
    }

    #[test]
    fn test_strides_1d_always_one() {
        // 1D array: stride is always [1]
        let strides = compute_strides(&[5]);
        assert_eq!(strides, vec![1]);
    }

    #[test]
    fn test_strides_2d_column_major() {
        // 2D [rows, cols]: strides = [1, rows] (column-major)
        let strides = compute_strides(&[3, 4]);
        assert_eq!(strides, vec![1, 3]);
    }

    #[test]
    fn test_strides_3d_column_major() {
        // 3D [a, b, c]: strides = [1, a, a*b]
        let strides = compute_strides(&[2, 3, 4]);
        assert_eq!(strides, vec![1, 2, 6]);
    }

    // ── complex_add / complex_sub / complex_mul / complex_div ─────────────────

    #[test]
    fn test_complex_add() {
        let result = complex_add((1.0, 2.0), (3.0, 4.0));
        assert_eq!(result, (4.0, 6.0));
    }

    #[test]
    fn test_complex_sub() {
        let result = complex_sub((5.0, 6.0), (1.0, 2.0));
        assert_eq!(result, (4.0, 4.0));
    }

    #[test]
    fn test_complex_mul() {
        // (1 + 2i)(3 + 4i) = 3 + 4i + 6i + 8i² = (3-8) + (4+6)i = -5 + 10i
        let result = complex_mul((1.0, 2.0), (3.0, 4.0));
        assert_eq!(result, (-5.0, 10.0));
    }

    #[test]
    fn test_complex_div() {
        // (2 + 0i) / (1 + 0i) = 2 + 0i
        let result = complex_div((2.0, 0.0), (1.0, 0.0));
        assert!((result.0 - 2.0).abs() < 1e-10);
        assert!(result.1.abs() < 1e-10);
    }

    #[test]
    fn test_complex_mul_pure_imaginary() {
        // i * i = -1: (0,1) * (0,1) = (0*0 - 1*1, 0*1 + 1*0) = (-1, 0)
        let result = complex_mul((0.0, 1.0), (0.0, 1.0));
        assert!((result.0 - (-1.0)).abs() < 1e-10);
        assert!(result.1.abs() < 1e-10);
    }
}
