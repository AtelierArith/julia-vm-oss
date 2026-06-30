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
use super::super::formatting::resolve_struct_refs_for_format;
use super::super::frame::Handler;
use super::super::instr::Instr;
use super::super::util::{format_value, format_value_print};
use super::super::value::Value;
use super::super::Vm;
use super::DispatchAction;

/// Reconstruct the `(func::Symbol, T, val)` fields of the Julia `InexactError`
/// struct from the `"{T}({val})"` message carried by [`VmError::InexactError`]
/// (Issue #8212). The message shape is produced uniformly by the conversion
/// paths (`vm/convert.rs`), e.g. `"Int64(1.5)"`. Unparsable values fall back to
/// a `Str`, and a message without the `(...)` shape falls back to placeholder
/// fields — in both cases the reconstructed struct still has the correct type so
/// `typeof(e) == InexactError` / `isa(e, InexactError)` hold.
fn inexact_error_fields(msg: &str) -> Vec<Value> {
    use crate::vm::value::SymbolValue;
    match msg.find('(') {
        Some(open) => {
            let type_name = &msg[..open];
            let inner = msg
                .get(open + 1..)
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or("");
            let func = Value::Symbol(SymbolValue::new(type_name));
            let t = Value::DataType(Box::new(crate::types::JuliaType::from_name_or_struct(
                type_name,
            )));
            let val = if let Ok(i) = inner.parse::<i64>() {
                Value::I64(i)
            } else if let Ok(f) = inner.parse::<f64>() {
                Value::F64(f)
            } else {
                Value::Str(inner.to_string())
            };
            vec![func, t, val]
        }
        None => vec![Value::Nothing, Value::Nothing, Value::Str(msg.to_string())],
    }
}

impl<R: RngLike> Vm<R> {
    fn error_exception_value(&mut self, msg: &str) -> Option<Value> {
        use crate::vm::value::StructInstance;
        let type_id = self
            .struct_defs
            .iter()
            .position(|d| d.name == "ErrorException")?;
        let instance = StructInstance::with_name(
            type_id,
            "ErrorException".to_string(),
            vec![Value::Str(msg.to_string())],
        );
        let idx = self.struct_heap.len();
        self.struct_heap.push(instance);
        Some(Value::StructRef(idx))
    }

    /// Reconstruct the Julia `Exception` struct for a Rust-raised [`VmError`] so a
    /// `catch` binds a value with the correct type (Issue #5648). Returns `None`
    /// for variants without a registered exception struct (caller falls back to a
    /// `String`). The struct is heap-allocated and returned as a `StructRef`,
    /// matching how a pure-Julia `throw` stores its exception. Field values that
    /// the `VmError` does not carry are filled with placeholders — `typeof(e)`,
    /// `e isa Exception`, and the message field are what callers rely on.
    fn vm_error_to_exception_value(&mut self, err: &VmError) -> Option<Value> {
        use crate::vm::value::StructInstance;
        let (name, fields): (&str, Vec<Value>) = match err {
            VmError::DomainError(msg) => {
                ("DomainError", vec![Value::Nothing, Value::Str(msg.clone())])
            }
            VmError::OverflowError(msg) => ("OverflowError", vec![Value::Str(msg.clone())]),
            VmError::DivisionByZero => ("DivideError", vec![]),
            // Runaway recursion guard (Issue #5969): a caught stack overflow
            // binds `e` to a `StackOverflowError()` so `e isa StackOverflowError`
            // / `typeof(e)` behave like upstream. The struct is field-less
            // (julia/base/boot.jl: `struct StackOverflowError <: Exception end`).
            VmError::StackOverflow => ("StackOverflowError", vec![]),
            VmError::IndexOutOfBounds { indices, .. } => (
                "BoundsError",
                vec![
                    Value::Nothing,
                    Value::I64(indices.first().copied().unwrap_or(0)),
                ],
            ),
            VmError::RangeIndexOutOfBounds { index, .. }
            | VmError::TupleIndexOutOfBounds { index, .. } => {
                ("BoundsError", vec![Value::Nothing, Value::I64(*index)])
            }
            VmError::MethodError(msg) => {
                ("MethodError", vec![Value::Str(msg.clone()), Value::Nothing])
            }
            VmError::DictKeyNotFound(key) | VmError::InvalidDictKey(key) => {
                ("KeyError", vec![Value::Str(key.clone())])
            }
            // Issue #8212: a failed `convert(T, x)` (direct call, typed local, or
            // `for i::T in itr`) raised `InexactError` whose caught value was a
            // bare `String`, so `typeof(e)` / `isa(e, InexactError)` diverged from
            // upstream. Reconstruct the `InexactError(func, T, val)` struct from
            // the carried `"{T}({val})"` message so the exception type is correct.
            VmError::InexactError(msg) => ("InexactError", inexact_error_fields(msg)),
            _ => return None,
        };
        let type_id = self.struct_defs.iter().position(|d| d.name == name)?;
        let instance = StructInstance::with_name(type_id, name.to_string(), fields);
        let idx = self.struct_heap.len();
        self.struct_heap.push(instance);
        Some(Value::StructRef(idx))
    }

    /// Execute error handling and test instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_error_handling(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::ThrowError => {
                let msg = match self.stack.pop() {
                    Some(Value::Str(s)) => s,
                    Some(v) => format!("{:?}", v),
                    None => "error".to_string(),
                };
                self.raise(VmError::ErrorException(msg))?;
                Ok(DispatchAction::Continue)
            }

            Instr::ThrowMethodError(msg) => {
                // Ambiguous-dispatch MethodError raised at runtime (Issue #5071).
                // The message is carried inline (known at compile time), so this
                // raises a catchable `VmError::MethodError` directly — matching
                // upstream Julia, which raises an ambiguity `MethodError` at
                // runtime rather than aborting compilation.
                self.raise(VmError::MethodError(msg.clone()))?;
                Ok(DispatchAction::Continue)
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

                // Create an error message from the value for the VmError.
                // Recognized exception structs are formatted with their
                // upstream `showerror` message rather than the raw struct
                // constructor repr (Issue #5146).
                let msg = match &resolved {
                    Value::Struct(_) => self
                        .format_exception_struct(&resolved)
                        .unwrap_or_else(|| format_value(&resolved)),
                    Value::Str(s) => s.clone(),
                    _ => format!("{:?}", resolved),
                };

                self.raise(VmError::ErrorException(msg))?;
                Ok(DispatchAction::Continue)
            }

            Instr::PushExceptionValue => {
                // Push the pending exception value onto the stack so catch blocks
                // can access the original exception. A pure-Julia `throw(E(...))`
                // already stored the struct in `pending_exception_value`. A
                // Rust-raised `VmError` (e.g. `sqrt(-1)`'s DomainError, an out-of-
                // bounds BoundsError, a DivideError) did not — those previously
                // surfaced as a bare `String`, losing `typeof(e)` (Issue #5648).
                // Reconstruct the matching `Exception` struct so `typeof(e)` /
                // `e isa Exception` / field access behave like upstream.
                let value = if let Some(v) = self.pending_exception_value.clone() {
                    v
                } else if let Some(err) = self.pending_error.clone() {
                    self.vm_error_to_exception_value(&err)
                        .unwrap_or_else(|| Value::Str(err.to_string()))
                } else {
                    Value::Str("Unknown error".to_string())
                };
                self.stack.push(value);
                Ok(DispatchAction::Continue)
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
                    self.any_test_failed = true; // Issue #8191: drives non-zero CLI exit.
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
                Ok(DispatchAction::Continue)
            }

            Instr::TestSetBegin(name) => {
                self.current_testset = Some(name.clone());
                self.test_pass_count = 0;
                self.test_fail_count = 0;
                self.emit_output(&format!("Test Set: {}", name), true);
                self.emit_output(&"=".repeat(40), true);
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
            }

            Instr::TestThrowsBegin(expected_type) => {
                // Initialize test_throws state - we expect this exception type
                self.test_throws_state = Some((expected_type.clone(), false));
                Ok(DispatchAction::Continue)
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
                        self.any_test_failed = true; // Issue #8191: drives non-zero CLI exit.
                        self.emit_output(
                            &format!(
                                "  Test Failed: @test_throws {} (no exception was thrown)",
                                expected_type
                            ),
                            true,
                        );
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PushHandler(catch_ip, finally_ip) => {
                let handler = Handler {
                    catch_ip: *catch_ip,
                    finally_ip: *finally_ip,
                    stack_len: self.stack.len(),
                    frame_len: self.frames.len(),
                    return_ip_len: self.return_ips.len(),
                    caught_exception_len: self.caught_exceptions.len(),
                };
                self.handlers.push(handler);
                Ok(DispatchAction::Continue)
            }

            Instr::PopHandler => {
                self.handlers.pop();
                Ok(DispatchAction::Continue)
            }

            Instr::ClearError => {
                if let Some(err) = self.pending_error.take() {
                    let value = self.pending_exception_value.take();
                    self.caught_exceptions.push((err, value));
                } else {
                    self.pending_exception_value = None;
                }
                self.rethrow_on_finally = false;
                // Mark exception as thrown for @test_throws context
                if let Some((_, ref mut was_thrown)) = self.test_throws_state {
                    *was_thrown = true;
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PopCaughtException => {
                self.caught_exceptions.pop();
                Ok(DispatchAction::Continue)
            }

            Instr::PushErrorCode => {
                let code = self
                    .pending_error
                    .as_ref()
                    .map(Self::error_code)
                    .unwrap_or(0);
                self.stack.push(Value::I64(code));
                Ok(DispatchAction::Continue)
            }

            Instr::PushErrorMessage => {
                let message = self
                    .pending_error
                    .as_ref()
                    .map(|err| err.to_string())
                    .unwrap_or_default();
                self.stack.push(Value::Str(message));
                Ok(DispatchAction::Continue)
            }

            Instr::Rethrow => {
                if self.rethrow_on_finally {
                    if let Some(err) = self.pending_error.take() {
                        self.rethrow_on_finally = false;
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                    self.rethrow_on_finally = false;
                }
                Ok(DispatchAction::Continue)
            }

            Instr::RethrowCurrent => {
                // Julia's rethrow() - rethrow the current pending exception
                if let Some((err, value)) = self.caught_exceptions.pop() {
                    self.pending_exception_value = value;
                    self.raise(err)?;
                    Ok(DispatchAction::Continue)
                } else if let Some(err) = self.pending_error.take() {
                    // Preserve the exception value for later catch blocks
                    self.raise(err)?;
                    Ok(DispatchAction::Continue)
                } else {
                    // No pending exception - this is an error in Julia, and it
                    // is catchable by an enclosing try block.
                    let msg = "rethrow() not allowed outside a catch block";
                    self.pending_exception_value = self.error_exception_value(msg);
                    self.raise(VmError::ErrorException(msg.to_string()))?;
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::RethrowOther => {
                // Julia's rethrow(e) - rethrow with a different exception value
                let value = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                if self.caught_exceptions.pop().is_none() && self.pending_error.is_none() {
                    let msg = "rethrow(exc) not allowed outside a catch block";
                    self.pending_exception_value = self.error_exception_value(msg);
                    self.raise(VmError::ErrorException(msg.to_string()))?;
                    return Ok(DispatchAction::Continue);
                }

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

                // Create an error message from the value (Issue #5146).
                let msg = match &resolved {
                    Value::Struct(_) => self
                        .format_exception_struct(&resolved)
                        .unwrap_or_else(|| format_value(&resolved)),
                    Value::Str(s) => s.clone(),
                    _ => format!("{:?}", resolved),
                };

                self.raise(VmError::ErrorException(msg))?;
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    /// Format a recognized exception struct using its upstream `showerror`
    /// message, returning `None` for structs we do not special-case (so the
    /// caller falls back to the raw `format_value` repr).
    ///
    /// Currently handles `TypeError` (Issue #5146), mirroring
    /// `julia/base/errorshow.jl`'s `showerror(io, ex::TypeError)` and the pure
    /// Julia `_showerror_str(ex::TypeError)` in `base/errorshow.jl`. `TypeError`
    /// fields are `(func, context, expected, got)`; `got` holds the offending
    /// VALUE, formatted as "a value of type $(typeof(got))".
    fn format_exception_struct(&self, value: &Value) -> Option<String> {
        let s = match value {
            Value::Struct(s) => s,
            _ => return None,
        };
        if &*s.struct_name != "TypeError" || s.values.len() != 4 {
            return None;
        }
        // `func`/`context` interpolate as bare names (e.g. `typeassert`, not
        // `:typeassert`); `format_value_print` strips the leading `:` for
        // Symbol, matching upstream `string(ex.func)`.
        let resolved = resolve_struct_refs_for_format(&s.values[0], &self.struct_heap);
        let func = format_value_print(&resolved);
        let resolved = resolve_struct_refs_for_format(&s.values[1], &self.struct_heap);
        let context = match &resolved {
            Value::Str(c) => c.clone(),
            // AUDIT(#6421): `other` is borrowed from the heap-resolved field above.
            other => format_value_print(other),
        };
        let resolved = resolve_struct_refs_for_format(&s.values[2], &self.struct_heap);
        let expected = format_value(&resolved);
        let got = resolve_struct_refs_for_format(&s.values[3], &self.struct_heap);

        // `expected === Bool` => non-boolean message.
        if expected == "Bool" {
            return Some(format!(
                "TypeError: non-boolean ({}) used in boolean context",
                self.get_type_name(&got)
            ));
        }

        // A value that is itself a type prints as `Type{T}`; otherwise as
        // "a value of type $(typeof(got))".
        let targ = match &got {
            // AUDIT(#6421): `got` was heap-resolved before this match.
            Value::DataType(_) => format!("Type{{{}}}", format_value(&got)),
            _ => format!("a value of type {}", self.get_type_name(&got)),
        };
        let ctx = if context.is_empty() {
            format!("in {}", func)
        } else {
            format!("in {}, in {}", func, context)
        };
        Some(format!(
            "TypeError: {}, expected {}, got {}",
            ctx, expected, targ
        ))
    }
}
