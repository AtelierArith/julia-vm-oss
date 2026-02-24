use crate::vm::error::VmError;
use crate::vm::value::{ArrayValue, StructInstance};

use super::complex::Complex64;
use super::helpers::{as_f64_data, extract_complex_data, is_complex_array};

#[inline]
fn decode_matrix_dims(shape: &[usize], as_lhs: bool) -> Option<(usize, usize)> {
    match shape.len() {
        1 => {
            if as_lhs {
                Some((1, shape[0]))
            } else {
                Some((shape[0], 1))
            }
        }
        2 => Some((shape[0], shape[1])),
        _ => None,
    }
}

#[inline]
fn result_shape(a_rank: usize, b_rank: usize, a_rows: usize, b_cols: usize) -> Vec<usize> {
    match (a_rank, b_rank) {
        (1, 1) => vec![1],
        (1, 2) => vec![b_cols],
        (2, 1) => vec![a_rows],
        (2, 2) => vec![a_rows, b_cols],
        _ => vec![],
    }
}

/// Matrix multiplication: A * B.
pub(crate) fn matmul(a: &ArrayValue, b: &ArrayValue) -> Result<ArrayValue, VmError> {
    if is_complex_array(a) || is_complex_array(b) {
        return Err(VmError::TypeError(
            "matmul: complex arrays require struct_heap access, use matmul_complex instead"
                .to_string(),
        ));
    }

    let (a_rows, a_cols) =
        decode_matrix_dims(&a.shape, true).ok_or_else(|| VmError::MatMulDimensionMismatch {
            a_shape: a.shape.clone(),
            b_shape: b.shape.clone(),
        })?;
    let (b_rows, b_cols) =
        decode_matrix_dims(&b.shape, false).ok_or_else(|| VmError::MatMulDimensionMismatch {
            a_shape: a.shape.clone(),
            b_shape: b.shape.clone(),
        })?;

    if a_cols != b_rows {
        return Err(VmError::MatMulDimensionMismatch {
            a_shape: a.shape.clone(),
            b_shape: b.shape.clone(),
        });
    }

    let a_data = as_f64_data(a)?;
    let b_data = as_f64_data(b)?;

    let mut out = vec![0.0; a_rows * b_cols];
    for i in 0..a_rows {
        for j in 0..b_cols {
            let mut sum = 0.0;
            for k in 0..a_cols {
                let a_idx = if a.shape.len() == 1 {
                    k
                } else {
                    i + k * a_rows
                };
                let b_idx = if b.shape.len() == 1 {
                    k
                } else {
                    k + j * b_rows
                };
                sum += a_data[a_idx] * b_data[b_idx];
            }
            let out_idx = if b_cols == 1 { i } else { i + j * a_rows };
            out[out_idx] = sum;
        }
    }

    let out_shape = result_shape(a.shape.len(), b.shape.len(), a_rows, b_cols);
    if out_shape.is_empty() {
        return Err(VmError::InternalError(format!(
            "matmul: unexpected shape ranks ({}, {})",
            a.shape.len(),
            b.shape.len()
        )));
    }

    Ok(ArrayValue::from_f64(out, out_shape))
}

/// Complex matrix multiplication: A * B.
pub(crate) fn matmul_complex(
    a: &ArrayValue,
    b: &ArrayValue,
    struct_heap: &[StructInstance],
) -> Result<ArrayValue, VmError> {
    let (a_rows, a_cols) =
        decode_matrix_dims(&a.shape, true).ok_or_else(|| VmError::MatMulDimensionMismatch {
            a_shape: a.shape.clone(),
            b_shape: b.shape.clone(),
        })?;
    let (b_rows, b_cols) =
        decode_matrix_dims(&b.shape, false).ok_or_else(|| VmError::MatMulDimensionMismatch {
            a_shape: a.shape.clone(),
            b_shape: b.shape.clone(),
        })?;

    if a_cols != b_rows {
        return Err(VmError::MatMulDimensionMismatch {
            a_shape: a.shape.clone(),
            b_shape: b.shape.clone(),
        });
    }

    let a_data = extract_complex_data(a, struct_heap)?;
    let b_data = extract_complex_data(b, struct_heap)?;

    let mut out = vec![Complex64::new(0.0, 0.0); a_rows * b_cols];
    for i in 0..a_rows {
        for j in 0..b_cols {
            let mut sum = Complex64::new(0.0, 0.0);
            for k in 0..a_cols {
                let a_idx = if a.shape.len() == 1 {
                    k
                } else {
                    i + k * a_rows
                };
                let b_idx = if b.shape.len() == 1 {
                    k
                } else {
                    k + j * b_rows
                };
                sum = sum.add(a_data[a_idx].mul(b_data[b_idx]));
            }
            let out_idx = if b_cols == 1 { i } else { i + j * a_rows };
            out[out_idx] = sum;
        }
    }

    let out_shape = result_shape(a.shape.len(), b.shape.len(), a_rows, b_cols);
    if out_shape.is_empty() {
        return Err(VmError::InternalError(format!(
            "matmul_complex: unexpected shape ranks ({}, {})",
            a.shape.len(),
            b.shape.len()
        )));
    }

    let interleaved: Vec<f64> = out.iter().flat_map(|c| [c.re, c.im]).collect();
    Ok(ArrayValue::complex_f64(interleaved, out_shape))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::ArrayValue;

    #[test]
    fn test_decode_matrix_dims_1d_lhs() {
        // 1-D as LHS: treated as row vector (1, n)
        assert_eq!(decode_matrix_dims(&[5], true), Some((1, 5)));
    }

    #[test]
    fn test_decode_matrix_dims_1d_rhs() {
        // 1-D as RHS: treated as column vector (n, 1)
        assert_eq!(decode_matrix_dims(&[5], false), Some((5, 1)));
    }

    #[test]
    fn test_decode_matrix_dims_2d() {
        assert_eq!(decode_matrix_dims(&[3, 4], true), Some((3, 4)));
        assert_eq!(decode_matrix_dims(&[3, 4], false), Some((3, 4)));
    }

    #[test]
    fn test_decode_matrix_dims_3d_returns_none() {
        assert_eq!(decode_matrix_dims(&[2, 3, 4], true), None);
    }

    #[test]
    fn test_result_shape_1d_dot_product() {
        // vector dot vector → scalar (shape [1])
        assert_eq!(result_shape(1, 1, 1, 1), vec![1]);
    }

    #[test]
    fn test_result_shape_matrix_vector() {
        // matrix * column vector → vector (a_rows)
        assert_eq!(result_shape(2, 1, 4, 1), vec![4]);
    }

    #[test]
    fn test_result_shape_row_vector_matrix() {
        // row vector * matrix → row vector (b_cols)
        assert_eq!(result_shape(1, 2, 1, 6), vec![6]);
    }

    #[test]
    fn test_result_shape_matrix_matrix() {
        assert_eq!(result_shape(2, 2, 3, 4), vec![3, 4]);
    }

    #[test]
    fn test_matmul_2x2() {
        // [1,2; 3,4] * [5,6; 7,8] in column-major order
        // col-major: [1,3,2,4] and [5,7,6,8]
        let a = ArrayValue::from_f64(vec![1.0, 3.0, 2.0, 4.0], vec![2, 2]);
        let b = ArrayValue::from_f64(vec![5.0, 7.0, 6.0, 8.0], vec![2, 2]);
        let c = matmul(&a, &b).unwrap();
        // Result [19,43; 22,50] in column-major: [19,43,22,50]
        if let crate::vm::value::ArrayData::F64(data) = c.data {
            assert_eq!(data, vec![19.0, 43.0, 22.0, 50.0]);
        }
    }

    #[test]
    fn test_matmul_dimension_mismatch_returns_err() {
        let a = ArrayValue::from_f64(vec![1.0, 2.0], vec![1, 2]);
        let b = ArrayValue::from_f64(vec![1.0, 2.0], vec![1, 2]);
        assert!(matches!(
            matmul(&a, &b),
            Err(VmError::MatMulDimensionMismatch { .. })
        ));
    }
}
