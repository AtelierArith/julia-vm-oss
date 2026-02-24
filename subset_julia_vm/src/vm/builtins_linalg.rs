//! Linear algebra builtin functions for the VM.
//!
//! Provides LU decomposition, determinant, and matrix inverse operations
//! using the nalgebra library.

use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use nalgebra::linalg::SymmetricEigen;
use nalgebra::{Complex, DMatrix, DVector};

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{
    new_array_ref, ArrayData, ArrayElementType, ArrayValue, NamedTupleValue, TupleValue, Value,
};
use super::Vm;

const EIGENVECTOR_INVERSE_ITERATION_LIMIT: usize = 20;

/// Convert VM ArrayValue to nalgebra DMatrix<f64>
fn array_to_nalgebra_mat(arr: &ArrayValue) -> Result<DMatrix<f64>, VmError> {
    if arr.shape.len() != 2 {
        return Err(VmError::TypeError(format!(
            "Expected 2D matrix, got {}D array",
            arr.shape.len()
        )));
    }

    let nrows = arr.shape[0];
    let ncols = arr.shape[1];

    // Get data as f64 vector
    let data = arr.try_as_f64_vec()?;

    // Create nalgebra matrix from column-major data (same as Julia)
    let mat = DMatrix::from_column_slice(nrows, ncols, &data);

    Ok(mat)
}

/// Convert nalgebra DMatrix<f64> to VM ArrayValue
fn nalgebra_mat_to_array(mat: &DMatrix<f64>) -> ArrayValue {
    let nrows = mat.nrows();
    let ncols = mat.ncols();

    // Extract data in column-major order (same as Julia)
    // nalgebra stores data in column-major order, so we can use as_slice()
    let data: Vec<f64> = mat.as_slice().to_vec();

    ArrayValue {
        data: ArrayData::F64(data),
        shape: vec![nrows, ncols],
        struct_type_id: None,
        element_type_override: None,
    }
}

/// Check if a matrix is approximately symmetric within a tolerance
fn is_symmetric(mat: &DMatrix<f64>, tol: f64) -> bool {
    let nrows = mat.nrows();
    let ncols = mat.ncols();
    if nrows != ncols {
        return false;
    }
    for i in 0..nrows {
        for j in (i + 1)..ncols {
            if (mat[(i, j)] - mat[(j, i)]).abs() > tol {
                return false;
            }
        }
    }
    true
}

/// Compute eigenvectors for a general (non-symmetric) matrix using Schur decomposition
/// Returns a complex matrix where each column is an eigenvector
fn compute_general_eigenvectors(
    mat: &DMatrix<f64>,
    eigenvalues: &[Complex<f64>],
) -> Vec<Vec<Complex<f64>>> {
    let n = mat.nrows();
    let mut eigenvectors: Vec<Vec<Complex<f64>>> = Vec::with_capacity(n);

    for &lambda in eigenvalues {
        // Solve (A - λI)v = 0 using inverse iteration with shifts
        // For numerical stability, we use a slightly perturbed solve
        let mut v = vec![Complex::new(1.0, 0.0); n];

        // Normalize initial guess
        let norm: f64 = v.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt();
        for c in &mut v {
            *c /= norm;
        }

        // Perform a few iterations of inverse iteration
        for _ in 0..EIGENVECTOR_INVERSE_ITERATION_LIMIT {
            // Build (A - λI) as a complex matrix
            let mut a_minus_lambda: Vec<Vec<Complex<f64>>> = Vec::with_capacity(n);
            for i in 0..n {
                let mut row = Vec::with_capacity(n);
                for j in 0..n {
                    let val = mat[(i, j)];
                    if i == j {
                        row.push(Complex::new(val, 0.0) - lambda);
                    } else {
                        row.push(Complex::new(val, 0.0));
                    }
                }
                a_minus_lambda.push(row);
            }

            // Solve (A - λI)w = v using Gaussian elimination with partial pivoting
            let w = solve_complex_system(&a_minus_lambda, &v);

            // Normalize
            let w_norm: f64 = w.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt();
            if w_norm > 1e-10 {
                v = w.iter().map(|c| *c / w_norm).collect();
            } else {
                break;
            }
        }

        eigenvectors.push(v);
    }

    eigenvectors
}

/// Solve a complex linear system Ax = b using Gaussian elimination with partial pivoting
fn solve_complex_system(a: &[Vec<Complex<f64>>], b: &[Complex<f64>]) -> Vec<Complex<f64>> {
    let n = b.len();
    if n == 0 {
        return vec![];
    }

    // Create augmented matrix [A|b]
    let mut aug: Vec<Vec<Complex<f64>>> = a
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut new_row = row.clone();
            new_row.push(b[i]);
            new_row
        })
        .collect();

    // Forward elimination with partial pivoting
    for col in 0..n {
        // Find pivot
        let mut max_row = col;
        let mut max_val = aug[col][col].norm();
        for (row, aug_row) in aug.iter().enumerate().take(n).skip(col + 1) {
            let val = aug_row[col].norm();
            if val > max_val {
                max_val = val;
                max_row = row;
            }
        }

        // Swap rows
        if max_row != col {
            aug.swap(col, max_row);
        }

        // Check for near-singular
        if aug[col][col].norm() < 1e-14 {
            // Add small perturbation to avoid division by zero
            aug[col][col] += Complex::new(1e-10, 1e-10);
        }

        // Eliminate below
        for row in (col + 1)..n {
            let factor = aug[row][col] / aug[col][col];
            let pivot_segment: Vec<Complex<f64>> = aug[col][col..=n].to_vec();
            for (target, pivot) in aug[row][col..=n].iter_mut().zip(pivot_segment.iter()) {
                *target -= factor * *pivot;
            }
        }
    }

    // Back substitution
    let mut x = vec![Complex::new(0.0, 0.0); n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        if aug[i][i].norm() > 1e-14 {
            x[i] = sum / aug[i][i];
        } else {
            x[i] = Complex::new(1.0, 0.0); // Default for singular case
        }
    }

    x
}

impl<R: RngLike> Vm<R> {
    /// Execute linear algebra builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a linalg builtin.
    pub(super) fn execute_builtin_linalg(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            // =================================================================
            // LU Decomposition with Partial Pivoting
            // =================================================================
            BuiltinId::Lu => {
                // lu(A) -> (L, U, p)
                // Returns lower triangular L, upper triangular U, and permutation vector p
                // such that A[p, :] = L * U
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "lu: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                // Convert to nalgebra matrix
                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError("lu: matrix must be square".to_string()));
                }

                // Perform LU decomposition with partial pivoting
                let lu = mat.lu();

                // Extract L (unit lower triangular)
                let l_mat = lu.l();
                let l_arr = nalgebra_mat_to_array(&l_mat);

                // Extract U (upper triangular)
                let u_mat = lu.u();
                let u_arr = nalgebra_mat_to_array(&u_mat);

                // Extract permutation as 1-based indices (Julia convention)
                // Create a column vector with row indices and apply the permutation
                let perm = lu.p();
                let mut indices: DMatrix<f64> = DMatrix::from_fn(nrows, 1, |i, _| i as f64);
                perm.inv_permute_rows(&mut indices);
                let p_data: Vec<i64> = indices.as_slice().iter().map(|&x| (x as i64) + 1).collect();

                // Return (L, U, p) tuple
                let result = Value::Tuple(TupleValue {
                    elements: vec![
                        Value::Array(new_array_ref(l_arr)),
                        Value::Array(new_array_ref(u_arr)),
                        Value::Array(new_array_ref(ArrayValue {
                            data: ArrayData::I64(p_data),
                            shape: vec![nrows],
                            struct_type_id: None,
                            element_type_override: None,
                        })),
                    ],
                });
                self.stack.push(result);
            }

            // =================================================================
            // Determinant
            // =================================================================
            BuiltinId::Det => {
                // det(A) -> scalar
                // Computes matrix determinant using LU decomposition
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "det: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError("det: matrix must be square".to_string()));
                }

                // Compute determinant via LU decomposition
                let det = mat.determinant();
                self.stack.push(Value::F64(det));
            }

            // =================================================================
            // Matrix Inverse
            // =================================================================
            BuiltinId::Inv => {
                // inv(A) -> A^(-1)
                // Computes matrix inverse using LU decomposition
                // Note: This is type-dispatched at compile time:
                //   - Array types route here (nalgebra-based builtin)
                //   - Rational types route to Pure Julia inv(::Rational{T})
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "inv: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError("inv: matrix must be square".to_string()));
                }

                // Compute inverse
                let inv_mat = mat
                    .try_inverse()
                    .ok_or_else(|| VmError::TypeError("inv: matrix is singular".to_string()))?;
                let inv_arr = nalgebra_mat_to_array(&inv_mat);

                self.stack.push(Value::Array(new_array_ref(inv_arr)));
            }

            // =================================================================
            // Left Division (Solve Linear System)
            // =================================================================
            BuiltinId::Ldiv => {
                // A \ b - solve Ax = b for x using LU decomposition
                // Stack: [A, b] -> pop b first, then A
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;

                let a_arr = match &a_val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "\\: expected Array for first argument, got {:?}",
                            a_val
                        )))
                    }
                };

                let b_arr = match &b_val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "\\: expected Array for second argument, got {:?}",
                            b_val
                        )))
                    }
                };

                // Convert A to nalgebra matrix
                let a_mat = array_to_nalgebra_mat(&a_arr)?;
                drop(a_arr);

                let nrows = a_mat.nrows();
                let ncols = a_mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError(
                        "\\: matrix A must be square".to_string(),
                    ));
                }

                // Convert b to nalgebra column vector/matrix
                let b_data = b_arr.try_as_f64_vec()?;
                let b_shape = b_arr.shape.clone();
                drop(b_arr);

                // Check dimensions match
                let b_rows = if b_shape.len() == 1 || b_shape.len() == 2 {
                    b_shape[0]
                } else {
                    return Err(VmError::TypeError(
                        "\\: b must be 1D vector or 2D matrix".to_string(),
                    ));
                };

                if b_rows != nrows {
                    return Err(VmError::TypeError(format!(
                        "\\: dimension mismatch - A is {}x{} but b has {} rows",
                        nrows, ncols, b_rows
                    )));
                }

                // Perform LU decomposition
                let lu = a_mat.lu();

                // Solve based on b's shape
                if b_shape.len() == 1 {
                    // b is a vector - solve Ax = b
                    let b_vec = DVector::from_column_slice(&b_data);
                    let x = lu
                        .solve(&b_vec)
                        .ok_or_else(|| VmError::TypeError("\\: matrix is singular".to_string()))?;

                    // Extract result as 1D vector
                    let x_data: Vec<f64> = x.as_slice().to_vec();
                    let result_arr = ArrayValue {
                        data: ArrayData::F64(x_data),
                        shape: vec![b_rows],
                        struct_type_id: None,
                        element_type_override: None,
                    };
                    self.stack.push(Value::Array(new_array_ref(result_arr)));
                } else {
                    // b is a matrix - solve AX = B for each column
                    let b_cols = b_shape[1];
                    let b_mat = DMatrix::from_column_slice(b_rows, b_cols, &b_data);
                    let x = lu
                        .solve(&b_mat)
                        .ok_or_else(|| VmError::TypeError("\\: matrix is singular".to_string()))?;

                    let x_arr = nalgebra_mat_to_array(&x);
                    self.stack.push(Value::Array(new_array_ref(x_arr)));
                }
            }

            // =================================================================
            // Singular Value Decomposition (SVD)
            // =================================================================
            BuiltinId::Svd => {
                // svd(A) -> (U=..., S=..., V=..., Vt=...)
                // Returns a named tuple with:
                //   - U: left singular vectors (m x min(m,n))
                //   - S: singular values as 1D vector (min(m,n))
                //   - V: right singular vectors (n x min(m,n))
                //   - Vt: transposed right singular vectors (min(m,n) x n)
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "svd: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                // Convert to nalgebra matrix
                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                // Perform SVD (compute_u=true, compute_v=true)
                let svd = mat.svd(true, true);

                // Extract U (left singular vectors): m x min(m,n)
                let u_mat = svd
                    .u
                    .ok_or_else(|| VmError::TypeError("svd: failed to compute U".to_string()))?;
                let u_arr = nalgebra_mat_to_array(&u_mat);

                // Extract S (singular values): return as 1D vector
                let s_data: Vec<f64> = svd.singular_values.as_slice().to_vec();
                let s_len = s_data.len();
                let s_arr = ArrayValue {
                    data: ArrayData::F64(s_data),
                    shape: vec![s_len],
                    struct_type_id: None,
                    element_type_override: None,
                };

                // Extract V (right singular vectors): n x min(m,n)
                let v_mat = svd
                    .v_t
                    .ok_or_else(|| VmError::TypeError("svd: failed to compute V".to_string()))?
                    .transpose();
                let v_arr = nalgebra_mat_to_array(&v_mat);

                // Compute Vt (transposed V): min(m,n) x n
                let vt_mat = v_mat.transpose();
                let vt_arr = nalgebra_mat_to_array(&vt_mat);

                // Return as named tuple (U=..., S=..., V=..., Vt=...)
                // This matches Julia's SVD result structure
                let result = NamedTupleValue::new(
                    vec![
                        "U".to_string(),
                        "S".to_string(),
                        "V".to_string(),
                        "Vt".to_string(),
                    ],
                    vec![
                        Value::Array(new_array_ref(u_arr)),
                        Value::Array(new_array_ref(s_arr)),
                        Value::Array(new_array_ref(v_arr)),
                        Value::Array(new_array_ref(vt_arr)),
                    ],
                )?;
                self.stack.push(Value::NamedTuple(result));
            }

            // =================================================================
            // QR Decomposition
            // =================================================================
            BuiltinId::Qr => {
                // qr(A) -> (Q=..., R=...)
                // Returns a named tuple with:
                //   - Q: orthogonal matrix (m x min(m,n))
                //   - R: upper triangular matrix (min(m,n) x n)
                // such that A = Q * R
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "qr: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                // Convert to nalgebra matrix
                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                // Perform QR decomposition
                let qr = mat.qr();

                // Extract Q (orthogonal matrix)
                let q_mat = qr.q();
                let q_arr = nalgebra_mat_to_array(&q_mat);

                // Extract R (upper triangular)
                let r_mat = qr.r();
                let r_arr = nalgebra_mat_to_array(&r_mat);

                // Return as named tuple (Q=..., R=...)
                // This matches Julia's QR result structure
                let result = NamedTupleValue::new(
                    vec!["Q".to_string(), "R".to_string()],
                    vec![
                        Value::Array(new_array_ref(q_arr)),
                        Value::Array(new_array_ref(r_arr)),
                    ],
                )?;
                self.stack.push(Value::NamedTuple(result));
            }

            // =================================================================
            // Eigenvalue Decomposition
            // =================================================================
            BuiltinId::Eigen => {
                // eigen(A) -> (values=..., vectors=...)
                // Returns a named tuple with:
                //   - values: real eigenvalues (length n)
                //   - vectors: eigenvectors as columns (n x n)
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "eigen: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError(
                        "eigen: matrix must be square".to_string(),
                    ));
                }

                // Check if matrix is symmetric to determine which algorithm to use
                let symmetric = is_symmetric(&mat, 1e-10);

                if symmetric {
                    // For symmetric matrices, use SymmetricEigen which gives real eigenvalues/vectors
                    let eigen = SymmetricEigen::new(mat.clone());

                    let values_data = eigen.eigenvalues.as_slice().to_vec();
                    let values_arr = ArrayValue {
                        data: ArrayData::F64(values_data),
                        shape: vec![nrows],
                        struct_type_id: None,
                        element_type_override: None,
                    };

                    let vectors_arr = nalgebra_mat_to_array(&eigen.eigenvectors);

                    let result = NamedTupleValue::new(
                        vec!["values".to_string(), "vectors".to_string()],
                        vec![
                            Value::Array(new_array_ref(values_arr)),
                            Value::Array(new_array_ref(vectors_arr)),
                        ],
                    )?;
                    self.stack.push(Value::NamedTuple(result));
                } else {
                    // For non-symmetric matrices, compute complex eigenvalues and eigenvectors
                    let eigenvalues: Vec<Complex<f64>> =
                        mat.complex_eigenvalues().as_slice().to_vec();

                    // Compute eigenvectors for each eigenvalue
                    let eigenvectors = compute_general_eigenvectors(&mat, &eigenvalues);

                    // Convert eigenvalues to interleaved Complex{Float64} array
                    let mut values_data = Vec::with_capacity(nrows * 2);
                    for ev in &eigenvalues {
                        values_data.push(ev.re);
                        values_data.push(ev.im);
                    }
                    let complex_type_id = Some(self.get_complex_type_id());
                    let values_arr = ArrayValue {
                        data: ArrayData::F64(values_data),
                        shape: vec![nrows],
                        struct_type_id: complex_type_id,
                        element_type_override: Some(ArrayElementType::ComplexF64),
                    };

                    // Convert eigenvectors to interleaved Complex{Float64} matrix
                    // Each column is an eigenvector, stored in column-major order
                    let mut vectors_data = Vec::with_capacity(nrows * nrows * 2);
                    for column in eigenvectors.iter().take(nrows) {
                        for value in column.iter().take(nrows) {
                            vectors_data.push(value.re);
                            vectors_data.push(value.im);
                        }
                    }
                    let vectors_arr = ArrayValue {
                        data: ArrayData::F64(vectors_data),
                        shape: vec![nrows, nrows],
                        struct_type_id: complex_type_id,
                        element_type_override: Some(ArrayElementType::ComplexF64),
                    };

                    let result = NamedTupleValue::new(
                        vec!["values".to_string(), "vectors".to_string()],
                        vec![
                            Value::Array(new_array_ref(values_arr)),
                            Value::Array(new_array_ref(vectors_arr)),
                        ],
                    )?;
                    self.stack.push(Value::NamedTuple(result));
                }
            }

            // =================================================================
            // Eigenvalue Decomposition
            // =================================================================
            BuiltinId::Eigvals => {
                // eigvals(A) -> Vector{Complex{Float64}}
                // Returns eigenvalues of matrix A as complex numbers
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "eigvals: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                // Convert to nalgebra matrix
                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError(
                        "eigvals: matrix must be square".to_string(),
                    ));
                }

                // Compute eigenvalues using nalgebra
                // complex_eigenvalues() returns Vec<Complex<f64>>
                let eigenvalues = mat.complex_eigenvalues();

                // Convert to interleaved F64 array for Complex{Float64}
                // Each complex number is stored as (re, im) pair
                let mut data = Vec::with_capacity(nrows * 2);
                for ev in &eigenvalues {
                    data.push(ev.re);
                    data.push(ev.im);
                }

                // Return as 1D array of Complex{Float64}
                // Uses interleaved storage format with element_type_override
                let result_arr = ArrayValue {
                    data: ArrayData::F64(data),
                    shape: vec![nrows],
                    struct_type_id: Some(self.get_complex_type_id()),
                    element_type_override: Some(ArrayElementType::ComplexF64),
                };
                self.stack.push(Value::Array(new_array_ref(result_arr)));
            }

            // =================================================================
            // Cholesky Decomposition
            // =================================================================
            BuiltinId::Cholesky => {
                // cholesky(A) -> (L=..., U=...)
                // Returns a named tuple with:
                //   - L: lower triangular factor (n x n)
                //   - U: upper triangular factor (n x n), where U = L'
                // such that A = L * L' for symmetric positive definite A
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "cholesky: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                // Convert to nalgebra matrix
                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError(
                        "cholesky: matrix must be square".to_string(),
                    ));
                }

                // Perform Cholesky decomposition
                let chol = mat.cholesky().ok_or_else(|| {
                    VmError::TypeError(
                        "cholesky: decomposition failed (matrix may not be positive definite)"
                            .to_string(),
                    )
                })?;

                // Extract L (lower triangular factor)
                let l_mat = chol.l();
                let l_arr = nalgebra_mat_to_array(&l_mat);

                // Compute U = L' (upper triangular factor)
                let u_mat = l_mat.transpose();
                let u_arr = nalgebra_mat_to_array(&u_mat);

                // Return as named tuple (L=..., U=...)
                // This matches Julia's Cholesky result structure
                let result = NamedTupleValue::new(
                    vec!["L".to_string(), "U".to_string()],
                    vec![
                        Value::Array(new_array_ref(l_arr)),
                        Value::Array(new_array_ref(u_arr)),
                    ],
                )?;
                self.stack.push(Value::NamedTuple(result));
            }

            // =================================================================
            // Matrix Rank
            // =================================================================
            BuiltinId::Rank => {
                // rank(A) -> Int
                // Returns the rank of matrix A (number of singular values above tolerance)
                // Default tolerance: max(m,n) * eps * max(singular values)
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "rank: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                // Convert to nalgebra matrix
                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                // Compute singular values using nalgebra
                let singular_values = mat.singular_values();

                // Default tolerance: max(m,n) * eps * max(singular values)
                // eps for f64 is approximately 2.220446049250313e-16
                let eps = f64::EPSILON;
                let max_sv = singular_values.iter().cloned().fold(0.0_f64, f64::max);
                let tol = (nrows.max(ncols) as f64) * eps * max_sv;

                // Count singular values above tolerance
                let rank = singular_values.iter().filter(|&&sv| sv > tol).count() as i64;

                self.stack.push(Value::I64(rank));
            }

            // =================================================================
            // Condition Number
            // =================================================================
            BuiltinId::Cond => {
                // cond(A) -> Float64
                // Returns the condition number of matrix A (2-norm condition number)
                // Computed as: max(singular values) / min(singular values)
                // For singular matrices, returns Inf
                let val = self.stack.pop_value()?;

                let arr = match &val {
                    Value::Array(arr_ref) => arr_ref.borrow(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "cond: expected Array, got {:?}",
                            val
                        )))
                    }
                };

                // Convert to nalgebra matrix
                let mat = array_to_nalgebra_mat(&arr)?;
                drop(arr);

                // Compute singular values using nalgebra
                let singular_values = mat.singular_values();

                // Condition number = max(sv) / min(sv)
                // If min_sv is 0 (or very small), matrix is singular and cond = Inf
                let max_sv = singular_values.iter().cloned().fold(0.0_f64, f64::max);
                let min_sv = singular_values
                    .iter()
                    .cloned()
                    .fold(f64::INFINITY, f64::min);

                let condition_number = if min_sv == 0.0 {
                    f64::INFINITY
                } else {
                    max_sv / min_sv
                };

                self.stack.push(Value::F64(condition_number));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
