//! Set instruction handlers.
//!
//! `Value::Set` was retired (Issue #6732): every `Set` is a pure-Julia `Set{T}`
//! struct over `Dict{T,Nothing}` (`base/set.jl`), constructed and operated on
//! through ordinary method dispatch. The legacy `NewSet`/`NewSetTyped`/`SetAdd`/
//! `LoadSet`/`StoreSet`/`ReturnSet` instructions are emitted only on stale
//! `ValueType::Set` paths that no value ever reaches at runtime; they are kept
//! decodable but unreachable, so each surfaces a loud InternalError if executed.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute set instructions (all unreachable after `Value::Set` removal).
    #[inline]
    pub(super) fn execute_set(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::NewSet | Instr::NewSetTyped(_) => Err(VmError::InternalError(
                "NewSet is unreachable after Value::Set removal (Issue #6732)".to_string(),
            )),

            Instr::SetAdd => {
                let _element = self.stack.pop_value()?;
                let other = self.stack.pop_value()?;
                Err(VmError::TypeError(format!(
                    "expected Set for SetAdd, got {:?}",
                    crate::vm::util::value_type_name(&other)
                )))
            }

            Instr::StoreSet(_) => {
                let other = self.stack.pop_value()?;
                Err(VmError::TypeError(format!(
                    "expected Set for StoreSet, got {:?}",
                    crate::vm::util::value_type_name(&other)
                )))
            }

            Instr::LoadSet(name) => Err(VmError::TypeError(format!(
                "Set variable '{}' not found",
                name
            ))),

            Instr::ReturnSet => {
                let other = self.stack.pop_value()?;
                Err(VmError::TypeError(format!(
                    "expected Set for ReturnSet, got {:?}",
                    crate::vm::util::value_type_name(&other)
                )))
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
