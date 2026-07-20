//! Backtrace and stacktrace VM builtins.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::{VmError, VmStackFrame};
use super::stack_ops::StackOps;
use super::value::ArrayValue;
use super::value::StrRef;
use super::Vm;

fn render_stack_frame(frame: &VmStackFrame) -> String {
    if let Some(span) = frame.span {
        if span.start_line > 0 {
            return format!(
                "{} at line {}:{}",
                frame.function, span.start_line, span.start_column
            );
        }
    }
    frame.function.clone()
}

impl<R: RngLike> Vm<R> {
    fn push_stack_frames(&mut self, frames: Vec<VmStackFrame>) -> Result<(), VmError> {
        let rendered = frames
            .iter()
            .map(render_stack_frame)
            .map(StrRef::from)
            .collect::<Vec<StrRef>>();
        let len = rendered.len();
        let arr = ArrayValue::memory_first_from_strings(rendered, vec![len]);
        self.push_array_value_as_wrapper(arr)
    }

    pub(super) fn execute_builtin_stacktrace(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::Backtrace => {
                if argc != 0 {
                    return Err(VmError::MethodError(
                        "backtrace expects no arguments".to_string(),
                    ));
                }
                self.push_stack_frames(self.runtime_stack_trace())?;
                Ok(Some(()))
            }
            BuiltinId::CatchBacktrace => {
                if argc != 0 {
                    return Err(VmError::MethodError(
                        "catch_backtrace expects no arguments".to_string(),
                    ));
                }
                let frames = self
                    .caught_exceptions
                    .last()
                    .map(|(_, _, backtrace)| backtrace.clone())
                    .or_else(|| self.pending_backtrace.clone())
                    .unwrap_or_default();
                self.push_stack_frames(frames)?;
                Ok(Some(()))
            }
            BuiltinId::Stacktrace => {
                if argc == 0 {
                    self.push_stack_frames(self.runtime_stack_trace())?;
                } else if argc == 1 {
                    let trace = self.stack.pop_value()?;
                    self.stack.push(trace);
                } else {
                    return Err(VmError::MethodError(
                        "stacktrace expects zero or one argument".to_string(),
                    ));
                }
                Ok(Some(()))
            }
            _ => Ok(None),
        }
    }
}
