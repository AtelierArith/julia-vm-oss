//! Error handling and testing operations for the VM.
//!
//! This module handles error and test instructions:
//! - ThrowError: Throw an error
//! - Test, TestSetBegin, TestSetEnd: Testing framework
//! - TestThrowsBegin, TestThrowsEnd: Test that code throws
//! - PushHandler, PopHandler: Exception handlers
//! - ClearError, PushErrorCode, PushErrorMessage: Error state
//! - Rethrow: Re-throw pending error

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::frame::Handler;
use super::super::instr::Instr;
use super::super::util::format_value;
use super::super::value::Value;
use super::super::Vm;

/// Result of executing an error handling instruction.
pub(super) enum ErrorResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute error handling and test instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_error_handling(&mut self, instr: &Instr) -> Result<ErrorResult, VmError> {
        match instr {
            Instr::ThrowError => {
                let msg = match self.stack.pop() {
                    Some(Value::Str(s)) => s,
                    Some(v) => format!("{:?}", v),
                    None => "error".to_string(),
                };
                self.raise(VmError::ErrorException(msg))?;
                Ok(ErrorResult::Continue)
            }

            Instr::ThrowValue => {
                // Pop any value (typically an exception struct) and throw it
                // This preserves the original value so it can be accessed in catch blocks
                let value = self.stack.pop().ok_or(VmError::StackUnderflow)?;

                // Store the exception value for later retrieval in catch blocks
                self.pending_exception_value = Some(value.clone());

                // Resolve StructRef to Struct for proper formatting
                let resolved = if let Value::StructRef(idx) = &value {
                    if let Some(s) = self.struct_heap.get(*idx) {
                        Value::Struct(s.clone())
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };

                // Create an error message from the value for the VmError
                let msg = match &resolved {
                    Value::Struct(_) => format_value(&resolved),
                    Value::Str(s) => s.clone(),
                    _ => format!("{:?}", resolved),
                };

                self.raise(VmError::ErrorException(msg))?;
                Ok(ErrorResult::Continue)
            }

            Instr::PushExceptionValue => {
                // Push the pending exception value onto the stack
                // This allows catch blocks to access the original exception struct
                let value = self.pending_exception_value.clone().unwrap_or_else(|| {
                    // If no exception value stored, create a string from the error
                    Value::Str(
                        self.pending_error
                            .as_ref()
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "Unknown error".to_string()),
                    )
                });
                self.stack.push(value);
                Ok(ErrorResult::Handled)
            }

            Instr::Test(msg) => {
                let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let cond = match v {
                    Value::Bool(b) => b,
                    _ => {
                        let type_name = self.get_type_name(&v);
                        // User-visible: user can pass a non-boolean expression to @test or as an if-condition
                        return Err(VmError::TypeError(format!(
                            "non-boolean ({}) used in boolean context",
                            type_name
                        )));
                    }
                };
                if cond {
                    self.test_pass_count += 1;
                    let prefix = if let Some(ref ts) = self.current_testset {
                        format!("  [{}] ", ts)
                    } else {
                        "  ".to_string()
                    };
                    if msg.is_empty() {
                        self.emit_output(&format!("{}Test Passed", prefix), true);
                    } else {
                        self.emit_output(&format!("{}Test Passed: {}", prefix, msg), true);
                    }
                } else {
                    self.test_fail_count += 1;
                    let prefix = if let Some(ref ts) = self.current_testset {
                        format!("  [{}] ", ts)
                    } else {
                        "  ".to_string()
                    };
                    if msg.is_empty() {
                        self.emit_output(&format!("{}Test Failed", prefix), true);
                    } else {
                        self.emit_output(&format!("{}Test Failed: {}", prefix, msg), true);
                    }
                }
                Ok(ErrorResult::Handled)
            }

            Instr::TestSetBegin(name) => {
                self.current_testset = Some(name.clone());
                self.test_pass_count = 0;
                self.test_fail_count = 0;
                self.emit_output(&format!("Test Set: {}", name), true);
                self.emit_output(&"=".repeat(40), true);
                Ok(ErrorResult::Handled)
            }

            Instr::TestSetEnd => {
                let total = self.test_pass_count + self.test_fail_count;
                self.emit_output(&"-".repeat(40), true);
                self.emit_output(
                    &format!(
                        "Results: {} passed, {} failed (total: {})",
                        self.test_pass_count, self.test_fail_count, total
                    ),
                    true,
                );
                if self.test_fail_count == 0 {
                    self.emit_output("All tests passed!", true);
                }
                self.emit_output("", true);
                self.current_testset = None;
                Ok(ErrorResult::Handled)
            }

            Instr::TestThrowsBegin(expected_type) => {
                // Initialize test_throws state - we expect this exception type
                self.test_throws_state = Some((expected_type.clone(), false));
                Ok(ErrorResult::Handled)
            }

            Instr::TestThrowsEnd => {
                // Check if exception was thrown as expected
                if let Some((expected_type, was_thrown)) = self.test_throws_state.take() {
                    if was_thrown {
                        // Pass: exception was thrown
                        self.test_pass_count += 1;
                        self.emit_output(
                            &format!(
                                "  Test Passed: @test_throws {} (exception was thrown)",
                                expected_type
                            ),
                            true,
                        );
                    } else {
                        // Fail: no exception was thrown
                        self.test_fail_count += 1;
                        self.emit_output(
                            &format!(
                                "  Test Failed: @test_throws {} (no exception was thrown)",
                                expected_type
                            ),
                            true,
                        );
                    }
                }
                Ok(ErrorResult::Handled)
            }

            Instr::PushHandler(catch_ip, finally_ip) => {
                let handler = Handler {
                    catch_ip: *catch_ip,
                    finally_ip: *finally_ip,
                    stack_len: self.stack.len(),
                    frame_len: self.frames.len(),
                    return_ip_len: self.return_ips.len(),
                };
                self.handlers.push(handler);
                Ok(ErrorResult::Handled)
            }

            Instr::PopHandler => {
                self.handlers.pop();
                Ok(ErrorResult::Handled)
            }

            Instr::ClearError => {
                self.pending_error = None;
                self.pending_exception_value = None;
                self.rethrow_on_finally = false;
                // Mark exception as thrown for @test_throws context
                if let Some((_, ref mut was_thrown)) = self.test_throws_state {
                    *was_thrown = true;
                }
                Ok(ErrorResult::Handled)
            }

            Instr::PushErrorCode => {
                let code = self
                    .pending_error
                    .as_ref()
                    .map(Self::error_code)
                    .unwrap_or(0);
                self.stack.push(Value::I64(code));
                Ok(ErrorResult::Handled)
            }

            Instr::PushErrorMessage => {
                let message = self
                    .pending_error
                    .as_ref()
                    .map(|err| err.to_string())
                    .unwrap_or_default();
                self.stack.push(Value::Str(message));
                Ok(ErrorResult::Handled)
            }

            Instr::Rethrow => {
                if self.rethrow_on_finally {
                    if let Some(err) = self.pending_error.take() {
                        self.rethrow_on_finally = false;
                        self.raise(err)?;
                        return Ok(ErrorResult::Continue);
                    }
                    self.rethrow_on_finally = false;
                }
                Ok(ErrorResult::Handled)
            }

            Instr::RethrowCurrent => {
                // Julia's rethrow() - rethrow the current pending exception
                if let Some(err) = self.pending_error.take() {
                    // Preserve the exception value for later catch blocks
                    self.raise(err)?;
                    Ok(ErrorResult::Continue)
                } else {
                    // No pending exception - this is an error in Julia
                    Err(VmError::ErrorException(
                        "rethrow() not allowed outside a catch block".to_string(),
                    ))
                }
            }

            Instr::RethrowOther => {
                // Julia's rethrow(e) - rethrow with a different exception value
                let value = self.stack.pop().ok_or(VmError::StackUnderflow)?;

                // Store the new exception value
                self.pending_exception_value = Some(value.clone());

                // Resolve StructRef to Struct for proper formatting
                let resolved = if let Value::StructRef(idx) = &value {
                    if let Some(s) = self.struct_heap.get(*idx) {
                        Value::Struct(s.clone())
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };

                // Create an error message from the value
                let msg = match &resolved {
                    Value::Struct(_) => format_value(&resolved),
                    Value::Str(s) => s.clone(),
                    _ => format!("{:?}", resolved),
                };

                self.raise(VmError::ErrorException(msg))?;
                Ok(ErrorResult::Continue)
            }

            _ => Ok(ErrorResult::NotHandled),
        }
    }
}
