//! Matrix operation instructions.
//!
//! Handles: MatMul
//! Note: Adjoint and Transpose have been migrated to Pure Julia

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::matmul::{is_complex_array, matmul, matmul_complex};
use super::super::*;
use super::stack_ops::StackOps;
use super::DispatchAction;
use crate::rng::RngLike;
use crate::vm::builtins_linalg::linalg_value_to_array_value;

impl<R: RngLike> Vm<R> {
    /// Execute matrix operation instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not a matrix operation.
    #[inline]
    pub(super) fn execute_matrix(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::MatMul => {
                let b_value = self.stack.pop_value()?;
                let a_value = self.stack.pop_value()?;
                // Issue #7964 Phase 2+3: Rust-level StaticArray matvec / matmat.
                // Inline variant is zero-allocation (pure stack copy).
                {
                    use crate::vm::value::{static_matmat, static_matvec};
                    let handled = match (&a_value, &b_value) {
                        (Value::StaticArrayInline(a), Value::StaticArrayInline(b))
                            if !a.is_vector() =>
                        {
                            if b.is_vector() {
                                a.inline_matvec(b)
                            } else {
                                a.inline_matmat(b)
                            }
                        }
                        (Value::StaticArray(a), Value::StaticArray(b)) if !a.is_vector() => {
                            if b.is_vector() {
                                static_matvec(a, b)
                            } else {
                                static_matmat(a, b)
                            }
                        }
                        _ => None,
                    };
                    if let Some(result) = handled {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                }
                if let Some(result) =
                    super::binary_both::try_matrix_diagonal_mul(self, &a_value, &b_value)?
                {
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }
                self.stack.push(a_value);
                self.stack.push(b_value);

                let b_result = linalg_value_to_array_value(
                    self.stack.pop_value()?,
                    &self.struct_heap,
                    "*",
                    Some("right operand"),
                );
                let b = match self.try_or_handle(b_result)? {
                    Some(arr) => arr,
                    None => return Ok(DispatchAction::Continue),
                };
                let a_result = linalg_value_to_array_value(
                    self.stack.pop_value()?,
                    &self.struct_heap,
                    "*",
                    Some("left operand"),
                );
                let a = match self.try_or_handle(a_result)? {
                    Some(arr) => arr,
                    None => return Ok(DispatchAction::Continue),
                };

                // Check if either array contains complex numbers
                let a_is_complex = is_complex_array(&a);
                let b_is_complex = is_complex_array(&b);

                let mul_result = if a_is_complex || b_is_complex {
                    // Use complex-aware matmul with access to struct_heap
                    matmul_complex(&a, &b, &self.struct_heap)
                } else {
                    // Use standard real matmul
                    matmul(&a, &b)
                };

                let mut result = match self.try_or_handle(mul_result)? {
                    Some(result) => result,
                    None => return Ok(DispatchAction::Continue),
                };
                // Store correct Complex type_id for complex array results (Issue #1804)
                if result
                    .element_type_override
                    .as_ref()
                    .is_some_and(|e| e.is_complex())
                {
                    result.struct_type_id = Some(self.get_complex_type_id());
                }
                self.push_array_value_as_wrapper(result)?;
                Ok(DispatchAction::Continue)
            }

            // Note: Instr::Adjoint and Instr::Transpose have been removed
            // They are now implemented in Pure Julia:
            // - subset_julia_vm/src/julia/base/array.jl (for arrays)
            // - subset_julia_vm/src/julia/base/number.jl (for scalars)
            // - subset_julia_vm/src/julia/base/complex.jl (for Complex numbers)
            _ => Err(super::unhandled(instr)),
        }
    }
}
