//! Linear algebra builtin functions for the VM.
//!
//! Provides LU decomposition, determinant, and matrix inverse operations
//! using the nalgebra library.

use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use crate::vm::value::is_native_array_value;
use nalgebra::linalg::SymmetricEigen;
use nalgebra::{Complex, DMatrix, DVector};

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{
    array_wrapper_value_from_array_value, native_array_value_ref, ArrayElementType, ArrayValue,
    NamedTupleValue, StructInstance, TupleValue, Value,
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

    let data = arr.to_logical_f64_vec()?;

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

    ArrayValue::memory_first_from_f64(data, vec![nrows, ncols])
}

/// Convert already-computed Complex{Float64} values to the transitional
/// ArrayValue wrapper through Memory first, mirroring the array/memory boundary
/// used by Julia's Array wrapper over primitive storage.
fn complex_f64_values_to_array(
    values: &[Complex<f64>],
    shape: Vec<usize>,
    complex_type_id: usize,
) -> Result<ArrayValue, VmError> {
    let mut arr =
        ArrayValue::memory_first_with_capacity(ArrayElementType::ComplexF64, values.len());
    arr.struct_type_id = Some(complex_type_id);
    for value in values {
        arr.push(Value::complex_struct(complex_type_id, value.re, value.im))?;
    }
    arr.shape = shape;
    Ok(arr)
}

fn real_f64_values_to_complex_array(
    values: &[f64],
    shape: Vec<usize>,
    complex_type_id: usize,
) -> Result<ArrayValue, VmError> {
    let complex_values = values
        .iter()
        .map(|value| Complex::new(*value, 0.0))
        .collect::<Vec<_>>();
    complex_f64_values_to_array(&complex_values, shape, complex_type_id)
}

fn with_linalg_array<T>(
    val: Value,
    struct_heap: &[StructInstance],
    op_name: &str,
    role: Option<&str>,
    f: impl FnOnce(&ArrayValue) -> Result<T, VmError>,
) -> Result<T, VmError> {
    // Route the transitional native Array variant through
    // `native_array_value_ref` so the unwrap stays centralized while #3908
    // retires the variant. Non-Array values fall through to the by-value
    // match below.
    if let Some(arr_ref) = native_array_value_ref(&val) {
        let arr = arr_ref.borrow();
        return f(&arr);
    }
    match val {
        Value::StructRef(idx) => {
            let instance = struct_heap.get(idx).ok_or_else(|| {
                VmError::TypeError(format!("{op_name}: invalid StructRef({idx})"))
            })?;
            if let Some(arr) = linalg_array_wrapper_value(instance, struct_heap, op_name)? {
                f(&arr)
            } else {
                let target =
                    role.map_or_else(|| "Array".to_string(), |role| format!("Array for {role}"));
                Err(VmError::TypeError(format!(
                    "{op_name}: expected {target}, got StructRef({idx})"
                )))
            }
        }
        Value::Struct(instance) => {
            if let Some(arr) = linalg_array_wrapper_value(&instance, struct_heap, op_name)? {
                f(&arr)
            } else {
                let target =
                    role.map_or_else(|| "Array".to_string(), |role| format!("Array for {role}"));
                Err(VmError::TypeError(format!(
                    "{op_name}: expected {target}, got Struct({})",
                    instance.struct_name
                )))
            }
        }
        other => {
            let target =
                role.map_or_else(|| "Array".to_string(), |role| format!("Array for {role}"));
            Err(VmError::TypeError(format!(
                "{op_name}: expected {target}, got {other:?}"
            )))
        }
    }
}

pub fn linalg_value_to_array_value(
    val: Value,
    struct_heap: &[StructInstance],
    op_name: &str,
    role: Option<&str>,
) -> Result<ArrayValue, VmError> {
    if let Some(arr_ref) = native_array_value_ref(&val) {
        return Ok(arr_ref.borrow().clone());
    }

    match val {
        Value::StructRef(idx) => {
            let instance = struct_heap.get(idx).ok_or_else(|| {
                VmError::TypeError(format!("{op_name}: invalid StructRef({idx})"))
            })?;
            if let Some(arr) = linalg_array_wrapper_value(instance, struct_heap, op_name)? {
                Ok(arr)
            } else {
                let target =
                    role.map_or_else(|| "Array".to_string(), |role| format!("Array for {role}"));
                Err(VmError::TypeError(format!(
                    "{op_name}: expected {target}, got StructRef({idx})"
                )))
            }
        }
        Value::Struct(instance) => {
            if let Some(arr) = linalg_array_wrapper_value(&instance, struct_heap, op_name)? {
                Ok(arr)
            } else {
                let target =
                    role.map_or_else(|| "Array".to_string(), |role| format!("Array for {role}"));
                Err(VmError::TypeError(format!(
                    "{op_name}: expected {target}, got Struct({})",
                    instance.struct_name
                )))
            }
        }
        other => {
            let target =
                role.map_or_else(|| "Array".to_string(), |role| format!("Array for {role}"));
            Err(VmError::TypeError(format!(
                "{op_name}: expected {target}, got {other:?}"
            )))
        }
    }
}

fn value_to_nalgebra_mat(
    val: Value,
    struct_heap: &[StructInstance],
    op_name: &str,
) -> Result<DMatrix<f64>, VmError> {
    with_linalg_array(val, struct_heap, op_name, None, array_to_nalgebra_mat)
}

fn is_array_wrapper_name(name: &str) -> bool {
    // Strip parameters before module qualification: a qualified type parameter
    // (`Array{Symbolics.Num, 2}`) may itself contain dots, so doing `rsplit('.')`
    // first mistakes `Num, 2}` for the wrapper family (Issue #11216).
    let unparameterized = name.split('{').next().unwrap_or(name);
    let base = unparameterized
        .rsplit('.')
        .next()
        .unwrap_or(unparameterized);
    matches!(base, "Array" | "Vector" | "Matrix")
}

fn is_subarray_wrapper_name(name: &str) -> bool {
    let unparameterized = name.split('{').next().unwrap_or(name);
    let base = unparameterized
        .rsplit('.')
        .next()
        .unwrap_or(unparameterized);
    base == "SubArray"
}

fn linalg_array_wrapper_value(
    instance: &StructInstance,
    struct_heap: &[StructInstance],
    op_name: &str,
) -> Result<Option<ArrayValue>, VmError> {
    if is_subarray_wrapper_name(&instance.struct_name) {
        return linalg_subarray_wrapper_value(instance, struct_heap, op_name);
    }
    if !is_array_wrapper_name(&instance.struct_name) {
        return Ok(None);
    }
    let Some(storage) = instance.values.first() else {
        return Err(VmError::TypeError(format!(
            "{op_name}: Array wrapper missing storage field"
        )));
    };
    let Some(size) = instance.values.get(1) else {
        return Err(VmError::TypeError(format!(
            "{op_name}: Array wrapper missing size field"
        )));
    };
    let (shape, offset) = match storage {
        Value::MemoryRef(memref) => {
            let shape = array_wrapper_shape_from_value(size, op_name)?;
            (shape, memref.memory_index())
        }
        _ => array_wrapper_shape_and_offset(size, op_name)?,
    };
    let len: usize = shape.iter().product();

    match storage {
        Value::MemoryRef(memref) => {
            let parent = memref.parent();
            let mem_borrow = parent.borrow();
            let mut values = Vec::with_capacity(len);
            for linear in 0..len {
                values.push(mem_borrow.get(offset + linear)?);
            }
            let mut arr =
                ArrayValue::memory_first_collect_values(values, mem_borrow.element_type.clone())?;
            arr.shape = shape;
            Ok(Some(arr))
        }
        Value::Memory(mem_ref) => {
            let mem_borrow = mem_ref.borrow();
            let mut values = Vec::with_capacity(len);
            for linear in 0..len {
                values.push(mem_borrow.get(offset + linear)?);
            }
            let mut arr =
                ArrayValue::memory_first_collect_values(values, mem_borrow.element_type.clone())?;
            arr.shape = shape;
            Ok(Some(arr))
        }
        // Route the transitional native Array `_mem` arm through
        // `native_array_value_ref` so the unwrap stays centralized while
        // #3908 migrates Array wrapper storage to Memory-first dispatch.
        // The surrounding `other =>` arm preserves exhaustiveness.
        _ if is_native_array_value(storage) => {
            let Some(array_ref) = native_array_value_ref(storage) else {
                return Err(VmError::TypeError(format!(
                    "{op_name}: Array wrapper storage must be MemoryRef, Memory, or Array, got {:?}",
                    storage.value_type()
                )));
            };
            let array_borrow = array_ref.borrow();
            let mut values = Vec::with_capacity(len);
            for linear in 0..len {
                values.push(array_borrow.get_linear(offset - 1 + linear)?);
            }
            Ok(Some(ArrayValue::memory_first_slice_from_values(
                &array_borrow,
                values,
                shape,
            )?))
        }
        Value::StructRef(idx) => {
            let nested = struct_heap.get(*idx).ok_or_else(|| {
                VmError::TypeError(format!("{op_name}: invalid nested Array StructRef({idx})"))
            })?;
            if let Some(nested_array) = linalg_array_wrapper_value(nested, struct_heap, op_name)? {
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(nested_array.get_linear(offset - 1 + linear)?);
                }
                Ok(Some(ArrayValue::memory_first_slice_from_values(
                    &nested_array,
                    values,
                    shape,
                )?))
            } else {
                Err(VmError::TypeError(format!(
                    "{op_name}: Array wrapper storage must be MemoryRef, Memory, or Array, got StructRef({idx})"
                )))
            }
        }
        Value::Struct(nested) => {
            if let Some(nested_array) = linalg_array_wrapper_value(nested, struct_heap, op_name)? {
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(nested_array.get_linear(offset - 1 + linear)?);
                }
                Ok(Some(ArrayValue::memory_first_slice_from_values(
                    &nested_array,
                    values,
                    shape,
                )?))
            } else {
                Err(VmError::TypeError(format!(
                    "{op_name}: Array wrapper storage must be MemoryRef, Memory, or Array, got Struct({})",
                    nested.struct_name
                )))
            }
        }
        other => Err(VmError::TypeError(format!(
            "{op_name}: Array wrapper storage must be MemoryRef, Memory, or Array, got {:?}",
            other.value_type()
        ))),
    }
}

fn linalg_subarray_wrapper_value(
    instance: &StructInstance,
    struct_heap: &[StructInstance],
    op_name: &str,
) -> Result<Option<ArrayValue>, VmError> {
    let Some(parent) = instance.values.first() else {
        return Err(VmError::TypeError(format!(
            "{op_name}: SubArray wrapper missing parent field"
        )));
    };
    let Some(Value::Tuple(indices)) = instance.values.get(1) else {
        return Err(VmError::TypeError(format!(
            "{op_name}: SubArray wrapper missing indices tuple"
        )));
    };
    let Some(Value::I64(offset)) = instance.values.get(2) else {
        return Err(VmError::TypeError(format!(
            "{op_name}: SubArray wrapper missing offset field"
        )));
    };
    let Some(Value::I64(len)) = instance.values.get(3) else {
        return Err(VmError::TypeError(format!(
            "{op_name}: SubArray wrapper missing len field"
        )));
    };
    if *offset < 0 || *len < 0 {
        return Err(VmError::TypeError(format!(
            "{op_name}: SubArray offset and len must be non-negative"
        )));
    }

    // The compact SubArray representation stores 1-D range views as a
    // contiguous parent slice: `offset` is 0-based and `len` is the logical
    // view length. Higher-dimensional views need index mapping through each
    // stored parent index and remain on the generic Julia getindex paths.
    if indices.elements.len() != 1 {
        return Ok(None);
    }

    let offset = usize::try_from(*offset).map_err(|_| {
        VmError::TypeError(format!(
            "{op_name}: SubArray offset must fit usize, got {offset}"
        ))
    })?;
    let len = usize::try_from(*len).map_err(|_| {
        VmError::TypeError(format!("{op_name}: SubArray len must fit usize, got {len}"))
    })?;
    let parent_arr = linalg_value_to_array_value(
        parent.clone(),
        struct_heap,
        op_name,
        Some("SubArray parent"),
    )?;
    let end = offset.checked_add(len).ok_or_else(|| {
        VmError::TypeError(format!("{op_name}: SubArray offset + len overflowed"))
    })?;
    if end > parent_arr.element_count() {
        return Err(VmError::IndexOutOfBounds {
            indices: vec![i64::try_from(end).unwrap_or(i64::MAX)],
            shape: parent_arr.shape.clone(),
        });
    }
    let mut values = Vec::with_capacity(len);
    for linear in 0..len {
        values.push(parent_arr.get_linear(offset + linear)?);
    }
    Ok(Some(ArrayValue::memory_first_slice_from_values(
        &parent_arr,
        values,
        vec![len],
    )?))
}

fn array_wrapper_shape_from_value(size: &Value, op_name: &str) -> Result<Vec<usize>, VmError> {
    let Value::Tuple(size_tuple) = size else {
        return Err(VmError::TypeError(format!(
            "{op_name}: Array wrapper size must be Tuple"
        )));
    };
    array_wrapper_shape_from_tuple(size_tuple, op_name)
}

fn array_wrapper_shape_and_offset(
    size: &Value,
    op_name: &str,
) -> Result<(Vec<usize>, usize), VmError> {
    let Value::Tuple(size_tuple) = size else {
        return Err(VmError::TypeError(format!(
            "{op_name}: Array wrapper _size must be Tuple"
        )));
    };

    if let Some(Value::Tuple(dims_tuple)) = size_tuple.elements.first() {
        let shape = array_wrapper_shape_from_tuple(dims_tuple, op_name)?;
        let offset = match size_tuple.elements.get(1) {
            Some(Value::I64(i)) if *i >= 1 => usize::try_from(*i).map_err(|_| {
                VmError::TypeError(format!(
                    "{op_name}: Array wrapper offset must fit usize, got {i}"
                ))
            })?,
            Some(other) => {
                return Err(VmError::TypeError(format!(
                    "{op_name}: Array wrapper offset must be positive Int64, got {:?}",
                    other.value_type()
                )))
            }
            None => {
                return Err(VmError::TypeError(format!(
                    "{op_name}: Array wrapper offset-encoded _size missing offset"
                )))
            }
        };
        return Ok((shape, offset));
    }

    Ok((array_wrapper_shape_from_tuple(size_tuple, op_name)?, 1))
}

fn array_wrapper_shape_from_tuple(
    dims_tuple: &TupleValue,
    op_name: &str,
) -> Result<Vec<usize>, VmError> {
    dims_tuple
        .elements
        .iter()
        .map(|dim| match dim {
            Value::I64(i) if *i >= 0 => usize::try_from(*i).map_err(|_| {
                VmError::TypeError(format!(
                    "{op_name}: Array wrapper dimension must fit usize, got {i}"
                ))
            }),
            other => Err(VmError::TypeError(format!(
                "{op_name}: Array wrapper dimensions must be non-negative Int64 values, got {:?}",
                other.value_type()
            ))),
        })
        .collect()
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
    /// Convert a freshly-computed linear-algebra result `ArrayValue` into the
    /// MemoryRef-backed `Array{T,N}` wrapper, matching the public array
    /// constructors that already return wrappers (`zeros`/`collect`, Issue
    /// #6653). Replaces the former native-array carrier producer used across the
    /// decomposition builtins (lu/inv/svd/qr/eigen/...) as part of retiring the
    /// carrier (Issue #6807). The decomposition inputs are consumed into
    /// nalgebra matrices before any result is produced, so `self` is free here.
    fn linalg_wrapper(&mut self, arr: ArrayValue) -> Result<Value, VmError> {
        let type_id = self.get_array_type_id();
        array_wrapper_value_from_array_value(arr, type_id, &mut self.struct_heap)
    }

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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "lu")?;

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
                let p_arr = ArrayValue::memory_first_from_i64(p_data, vec![nrows]);
                let result = Value::Tuple(TupleValue {
                    elements: vec![
                        self.linalg_wrapper(l_arr)?,
                        self.linalg_wrapper(u_arr)?,
                        self.linalg_wrapper(p_arr)?,
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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "det")?;

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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "inv")?;

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

                let wrapper = self.linalg_wrapper(inv_arr)?;
                self.stack.push(wrapper);
            }

            // =================================================================
            // Left Division (Solve Linear System)
            // =================================================================
            BuiltinId::Ldiv => {
                // A \ b - solve Ax = b for x using LU decomposition
                // Stack: [A, b] -> pop b first, then A
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;

                let a_mat = with_linalg_array(
                    a_val,
                    &self.struct_heap,
                    "\\",
                    Some("first argument"),
                    array_to_nalgebra_mat,
                )?;

                let nrows = a_mat.nrows();
                let ncols = a_mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError(
                        "\\: matrix A must be square".to_string(),
                    ));
                }

                // Convert b to nalgebra column vector/matrix
                let (b_data, b_shape) = with_linalg_array(
                    b_val,
                    &self.struct_heap,
                    "\\",
                    Some("second argument"),
                    |arr| Ok((arr.to_logical_f64_vec()?, arr.shape.clone())),
                )?;

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
                    let result_arr = ArrayValue::memory_first_from_f64(x_data, vec![b_rows]);
                    let wrapper = self.linalg_wrapper(result_arr)?;
                    self.stack.push(wrapper);
                } else {
                    // b is a matrix - solve AX = B for each column
                    let b_cols = b_shape[1];
                    let b_mat = DMatrix::from_column_slice(b_rows, b_cols, &b_data);
                    let x = lu
                        .solve(&b_mat)
                        .ok_or_else(|| VmError::TypeError("\\: matrix is singular".to_string()))?;

                    let x_arr = nalgebra_mat_to_array(&x);
                    let wrapper = self.linalg_wrapper(x_arr)?;
                    self.stack.push(wrapper);
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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "svd")?;

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
                let s_arr = ArrayValue::memory_first_from_f64(s_data, vec![s_len]);

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
                        self.linalg_wrapper(u_arr)?,
                        self.linalg_wrapper(s_arr)?,
                        self.linalg_wrapper(v_arr)?,
                        self.linalg_wrapper(vt_arr)?,
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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "qr")?;

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
                    vec![self.linalg_wrapper(q_arr)?, self.linalg_wrapper(r_arr)?],
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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "eigen")?;

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
                    let values_arr = ArrayValue::memory_first_from_f64(values_data, vec![nrows]);

                    let vectors_arr = nalgebra_mat_to_array(&eigen.eigenvectors);

                    let result = NamedTupleValue::new(
                        vec!["values".to_string(), "vectors".to_string()],
                        vec![
                            self.linalg_wrapper(values_arr)?,
                            self.linalg_wrapper(vectors_arr)?,
                        ],
                    )?;
                    self.stack.push(Value::NamedTuple(result));
                } else {
                    // For non-symmetric matrices, compute complex eigenvalues and eigenvectors
                    let eigenvalues: Vec<Complex<f64>> =
                        mat.complex_eigenvalues().as_slice().to_vec();

                    // Compute eigenvectors for each eigenvalue
                    let eigenvectors = compute_general_eigenvectors(&mat, &eigenvalues);

                    let complex_type_id = self.get_complex_type_id();
                    let values_arr =
                        complex_f64_values_to_array(&eigenvalues, vec![nrows], complex_type_id)?;

                    // Convert eigenvectors to interleaved Complex{Float64} matrix
                    // Each column is an eigenvector, stored in column-major order
                    let mut vectors_data = Vec::with_capacity(nrows * nrows);
                    for column in eigenvectors.iter().take(nrows) {
                        for value in column.iter().take(nrows) {
                            vectors_data.push(*value);
                        }
                    }
                    let vectors_arr = complex_f64_values_to_array(
                        &vectors_data,
                        vec![nrows, nrows],
                        complex_type_id,
                    )?;

                    let result = NamedTupleValue::new(
                        vec!["values".to_string(), "vectors".to_string()],
                        vec![
                            self.linalg_wrapper(values_arr)?,
                            self.linalg_wrapper(vectors_arr)?,
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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "eigvals")?;

                let nrows = mat.nrows();
                let ncols = mat.ncols();

                if nrows != ncols {
                    return Err(VmError::TypeError(
                        "eigvals: matrix must be square".to_string(),
                    ));
                }

                let result_arr = if is_symmetric(&mat, 1e-10) {
                    let eigen = SymmetricEigen::new(mat.clone());
                    let mut values = eigen.eigenvalues.as_slice().to_vec();
                    values.sort_by(|a, b| a.total_cmp(b));
                    real_f64_values_to_complex_array(
                        &values,
                        vec![nrows],
                        self.get_complex_type_id(),
                    )?
                } else {
                    let eigenvalues = mat.complex_eigenvalues();
                    complex_f64_values_to_array(
                        eigenvalues.as_slice(),
                        vec![nrows],
                        self.get_complex_type_id(),
                    )?
                };
                let wrapper = self.linalg_wrapper(result_arr)?;
                self.stack.push(wrapper);
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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "cholesky")?;

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
                    vec![self.linalg_wrapper(l_arr)?, self.linalg_wrapper(u_arr)?],
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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "rank")?;

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
                let mat = value_to_nalgebra_mat(val, &self.struct_heap, "cond")?;

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
