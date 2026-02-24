//! Set builtin functions for the VM.
//!
//! Set operations: union, intersect, setdiff, push!, delete!, etc.
//! These operations support both Set and Array arguments.

mod intrinsics;
mod set_ops;
mod shared;

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute Set builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a Set builtin.
    pub(super) fn execute_builtin_sets(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        if self.execute_set_ops(builtin, argc)? {
            return Ok(Some(()));
        }
        if self.execute_set_intrinsics(builtin, argc)? {
            return Ok(Some(()));
        }
        Ok(None)
    }
}
