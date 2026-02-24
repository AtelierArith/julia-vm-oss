//! Random number generation operations for the VM.
//!
//! This module handles RNG instructions including:
//! - RandF64: single random float
//! - RandArray: array of random floats
//! - RandIntArray: array of random integers

// SAFETY: i64→u64 cast for RNG seed reinterprets the bit pattern; negative seeds
// are valid and result in a different (but well-defined) seed value.
#![allow(clippy::cast_sign_loss)]
//! - RandnF64: single standard normal value
//! - RandnArray: array of standard normal values
//! - SeedGlobalRng: reseed the global RNG

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{new_array_ref, ArrayValue, Value};
use super::super::Vm;
use super::randn;

impl<R: RngLike> Vm<R> {
    /// Execute RNG instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_rng(&mut self, instr: &Instr) -> Result<Option<()>, VmError> {
        match instr {
            Instr::RandF64 => {
                self.stack.push(Value::F64(self.rng.next_f64()));
                Ok(Some(()))
            }

            Instr::RandArray(n) => {
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let size: usize = dims.iter().product();
                let data: Vec<f64> = (0..size).map(|_| self.rng.next_f64()).collect();
                let arr = ArrayValue::from_f64(data, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
                Ok(Some(()))
            }

            Instr::RandIntArray(n) => {
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let size: usize = dims.iter().product();
                // Generate random integers in a reasonable range (0 to i64::MAX)
                let data: Vec<f64> = (0..size)
                    .map(|_| (self.rng.next_u64() as i64).abs() as f64)
                    .collect();
                let arr = ArrayValue::from_f64(data, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
                Ok(Some(()))
            }

            Instr::RandnF64 => {
                // randn() - standard normal distribution using global RNG
                let val = randn(&mut self.rng);
                self.stack.push(Value::F64(val));
                Ok(Some(()))
            }

            Instr::RandnArray(n) => {
                // randn(dims...) - array of standard normal values using global RNG
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let size: usize = dims.iter().product();
                let data: Vec<f64> = (0..size).map(|_| randn(&mut self.rng)).collect();
                let arr = ArrayValue::from_f64(data, dims);
                self.stack.push(Value::Array(new_array_ref(arr)));
                Ok(Some(()))
            }

            Instr::SeedGlobalRng => {
                // seed!(n) - reseed global RNG
                let seed = self.stack.pop_i64()? as u64;
                self.rng.reseed(seed);
                self.stack.push(Value::Nothing);
                Ok(Some(()))
            }

            _ => Ok(None),
        }
    }
}
