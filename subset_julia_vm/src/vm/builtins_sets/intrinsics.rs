use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::shared::pop_set;
use crate::vm::error::VmError;
use crate::vm::stack_ops::StackOps;
use crate::vm::value::{DictKey, Value};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_set_intrinsics(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<bool, VmError> {
        match builtin {
            BuiltinId::_SetPush => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "_set_push! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let elem = self.stack.pop_value()?;
                let key = DictKey::from_value(&elem)?;
                let mut set = pop_set(&mut self.stack)?;
                set.insert(key);
                self.stack.push(Value::Set(set));
                Ok(true)
            }
            BuiltinId::_SetDelete => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "_set_delete! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let elem = self.stack.pop_value()?;
                let key = DictKey::from_value(&elem)?;
                let mut set = pop_set(&mut self.stack)?;
                set.remove(&key);
                self.stack.push(Value::Set(set));
                Ok(true)
            }
            BuiltinId::_SetIn => {
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "_set_in requires 2 arguments, got {}",
                        argc
                    )));
                }
                let set = pop_set(&mut self.stack)?;
                let elem = self.stack.pop_value()?;
                let key = DictKey::from_value(&elem)?;
                self.stack.push(Value::Bool(set.contains(&key)));
                Ok(true)
            }
            BuiltinId::_SetEmpty => {
                if argc != 1 {
                    return Err(VmError::TypeError(format!(
                        "_set_empty! requires 1 argument, got {}",
                        argc
                    )));
                }
                let _ = pop_set(&mut self.stack)?;
                self.stack
                    .push(Value::Set(crate::vm::value::SetValue::new()));
                Ok(true)
            }
            BuiltinId::_SetLength => {
                if argc != 1 {
                    return Err(VmError::TypeError(format!(
                        "_set_length requires 1 argument, got {}",
                        argc
                    )));
                }
                let set = pop_set(&mut self.stack)?;
                self.stack.push(Value::I64(set.len() as i64));
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
