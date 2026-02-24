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
use super::value::ArrayValue;

/// Enum to represent either an array or a scalar for broadcasting.
///
/// Only Array and ScalarF64 variants are currently constructed by callers
/// (dynamic_ops.rs). Other variants are retained for match completeness in
/// broadcast_op_f64 / broadcast_op_complex but are unreachable in practice.
/// This is intentionally broader than current runtime construction sites.
#[allow(dead_code)]
pub(crate) enum Broadcastable {
    Array(ArrayValue),
    ScalarF64(f64),
}

impl Broadcastable {
    /// Check if any operand involves complex numbers
    pub(crate) fn is_complex(&self) -> bool {
        match self {
            Broadcastable::Array(arr) => {
                // Check if this is an interleaved complex array
                // Interleaved arrays have data.len() == element_count() * 2
                let element_count = arr.element_count();
                element_count > 0 && arr.len() == element_count * 2
            }
            _ => false,
        }
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
        (Broadcastable::Array(arr_a), Broadcastable::Array(arr_b)) => {
            // Compute result shape using Julia broadcasting rules
            let result_shape = compute_broadcast_shape(&arr_a.shape, &arr_b.shape)?;
            let result_size: usize = result_shape.iter().product();

            // Fast path: same shape, no broadcasting needed
            if arr_a.shape == arr_b.shape {
                let data: Vec<f64> = arr_a
                    .try_data_f64()?
                    .iter()
                    .zip(arr_b.try_data_f64()?.iter())
                    .map(|(&x, &y)| op(x, y))
                    .collect();
                return Ok(ArrayValue::from_f64(data, arr_a.shape.clone()));
            }

            // Get expanded shapes for Julia-style broadcasting
            let (a_expanded, b_expanded) = expand_shapes_for_julia(&arr_a.shape, &arr_b.shape);

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
                data.push(op(
                    arr_a.try_data_f64()?[a_idx],
                    arr_b.try_data_f64()?[b_idx],
                ));
            }

            Ok(ArrayValue::from_f64(data, result_shape))
        }
        // Array .op ScalarF64
        (Broadcastable::Array(arr), Broadcastable::ScalarF64(s)) => {
            let data: Vec<f64> = arr.try_data_f64()?.iter().map(|&x| op(x, *s)).collect();
            Ok(ArrayValue::from_f64(data, arr.shape.clone()))
        }
        // ScalarF64 .op Array
        (Broadcastable::ScalarF64(s), Broadcastable::Array(arr)) => {
            let data: Vec<f64> = arr.try_data_f64()?.iter().map(|&x| op(*s, x)).collect();
            Ok(ArrayValue::from_f64(data, arr.shape.clone()))
        }
        // ScalarF64 .op ScalarF64 - return 1-element array
        (Broadcastable::ScalarF64(a_val), Broadcastable::ScalarF64(b_val)) => {
            Ok(ArrayValue::from_f64(vec![op(*a_val, *b_val)], vec![1]))
        }
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
pub(crate) fn broadcast_op_complex<F>(
    a: &Broadcastable,
    b: &Broadcastable,
    op: F,
) -> Result<ArrayValue, VmError>
where
    F: Fn((f64, f64), (f64, f64)) -> (f64, f64),
{
    // Helper to get shape
    let get_shape = |bc: &Broadcastable| -> Vec<usize> {
        match bc {
            Broadcastable::Array(arr) => arr.shape.clone(),
            Broadcastable::ScalarF64(_) => vec![1],
        }
    };

    let a_shape = get_shape(a);
    let b_shape = get_shape(b);

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

                // Check if this is an interleaved complex array
                let element_count = arr.element_count();
                if arr.len() == element_count * 2 {
                    // Interleaved complex: [re0, im0, re1, im1, ...]
                    Ok((
                        arr.try_data_f64()?[src_idx * 2],
                        arr.try_data_f64()?[src_idx * 2 + 1],
                    ))
                } else {
                    // Regular F64 array - treat as real part, imaginary part is 0
                    Ok((arr.try_data_f64()?[src_idx], 0.0))
                }
            }
        }
    };

    // Check if operand is scalar (element count == 1)
    let is_scalar = |bc: &Broadcastable| -> bool {
        match bc {
            Broadcastable::ScalarF64(_) => true,
            Broadcastable::Array(arr) => arr.element_count() == 1,
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

    Ok(ArrayValue::from_f64(result_data, result_shape))
}

#[cfg(test)]
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
