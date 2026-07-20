//! Dict/Set local-variable instruction handlers.
//!
//! Both `Value::Dict` (Issue #6731) and `Value::Set` (Issue #6732) were retired;
//! `Dict` and `Set` are pure-Julia structs operated on through method dispatch.
//! These instructions (`DictSet`/`DictLen`/`LoadDict`/`StoreDict`/`ReturnDict`)
//! are emitted only on stale `ValueType::Dict`/`ValueType::Set` paths that no
//! value reaches at runtime; they are kept decodable but unreachable, so each
//! surfaces a loud error if executed.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute dict/set local instructions (all unreachable after carrier removal).
    #[inline]
    pub(super) fn execute_dict(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::DictSet => {
                let _value = self.stack.pop_value()?;
                let _key = self.stack.pop_value()?;
                let other = self.stack.pop_value()?;
                Err(VmError::InternalError(format!(
                    "DictSet is unreachable after Value::Dict removal, got {:?}",
                    crate::vm::util::value_type_name(&other)
                )))
            }

            Instr::DictLen => {
                let other = self.stack.pop_value()?;
                Err(VmError::InternalError(format!(
                    "DictLen is unreachable after Dict/Set carrier removal, got {:?}",
                    crate::vm::util::value_type_name(&other)
                )))
            }

            Instr::LoadDict(name) => Err(VmError::InternalError(format!(
                "LoadDict is unreachable after Dict/Set carrier removal: '{}'",
                name
            ))),

            Instr::StoreDict(_) => {
                let other = self.stack.pop_value()?;
                Err(VmError::InternalError(format!(
                    "StoreDict is unreachable after Dict/Set carrier removal, got {:?}",
                    crate::vm::util::value_type_name(&other)
                )))
            }

            Instr::ReturnDict => {
                let val = self.stack.pop().unwrap_or(Value::Nothing);
                // Route through the shared continuation machinery (kept for the
                // generator/HOF driver shape; the value is no longer a carrier).
                match self.route_value_return(val)? {
                    super::return_ops::ValueReturnRouting::Handled => Ok(DispatchAction::Continue),
                    super::return_ops::ValueReturnRouting::Exit(v) => Ok(DispatchAction::Exit(v)),
                }
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
