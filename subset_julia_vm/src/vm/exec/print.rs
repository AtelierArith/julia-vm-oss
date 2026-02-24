//! Print/output operations for the VM.
//!
//! This module handles print instructions:
//! - PrintStr, PrintI64, PrintF64: Print with newline
//! - Print*NoNewline: Print without newline
//! - PrintAny, PrintAnyNoNewline: Print any value
//! - PrintNewline: Print just a newline

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::frame::Frame;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::util::{bind_value_to_slot, format_float_julia, format_value};
use super::super::value::{self, Value};
use super::super::Vm;

/// Result of executing a print instruction.
pub(super) enum PrintResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Instruction set up a function call, continue to execute it
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute print instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_print(&mut self, instr: &Instr) -> Result<PrintResult, VmError> {
        match instr {
            Instr::PrintStr => {
                let s = self.stack.pop_str()?;
                self.emit_output(&s, true);
                Ok(PrintResult::Handled)
            }

            Instr::PrintI64 => {
                let x = self.stack.pop_i64()?;
                let s = x.to_string();
                self.emit_output(&s, true);
                Ok(PrintResult::Handled)
            }

            Instr::PrintF64 => {
                let x = self.pop_f64_or_i64()?;
                let s = format_float_julia(x);
                self.emit_output(&s, true);
                Ok(PrintResult::Handled)
            }

            Instr::PrintStrNoNewline => {
                let s = self.stack.pop_str()?;
                self.emit_output(&s, false);
                Ok(PrintResult::Handled)
            }

            Instr::PrintI64NoNewline => {
                let x = self.stack.pop_i64()?;
                let s = x.to_string();
                self.emit_output(&s, false);
                Ok(PrintResult::Handled)
            }

            Instr::PrintF64NoNewline => {
                // Try F64/I64 first, but handle any value type gracefully
                let val = self.stack.pop_value()?;
                let s = match &val {
                    Value::F64(x) => format_float_julia(*x),
                    Value::I64(x) => format_float_julia(*x as f64),
                    Value::StructRef(idx) => {
                        // Resolve StructRef to Struct for proper formatting
                        if let Some(s) = self.struct_heap.get(*idx) {
                            format_value(&Value::Struct(s.clone()))
                        } else {
                            format_value(&val)
                        }
                    }
                    _ => format_value(&val),
                };
                self.emit_output(&s, false);
                Ok(PrintResult::Handled)
            }

            Instr::PrintAny => {
                if let Some(v) = self.stack.pop() {
                    // Resolve StructRef to Struct for proper formatting
                    let resolved = if let Value::StructRef(idx) = &v {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            Value::Struct(s.clone())
                        } else {
                            v
                        }
                    } else {
                        v
                    };
                    let s = format_value(&resolved);
                    self.emit_output(&s, true);
                }
                Ok(PrintResult::Handled)
            }

            Instr::PrintAnyNoNewline => {
                if let Some(v) = self.stack.pop() {
                    // Resolve StructRef to Struct for proper formatting
                    let resolved = if let Value::StructRef(idx) = &v {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            Value::Struct(s.clone())
                        } else {
                            v
                        }
                    } else {
                        v
                    };

                    // Check if this is a struct with a custom show method
                    // Try exact match first, then fall back to base name for parametric types
                    // e.g., "Complex{Float64}" -> "Complex"
                    let show_func_index = if let Value::Struct(ref s) = &resolved {
                        self.show_methods.get(&s.struct_name).copied().or_else(|| {
                            s.struct_name.find('{').and_then(|pos| {
                                let base = &s.struct_name[..pos];
                                self.show_methods.get(base).copied()
                            })
                        })
                    } else {
                        None
                    };

                    if let Some(func_index) = show_func_index {
                        // Get the show function
                        if let Some(func) = self.functions.get(func_index).cloned() {
                            // Create stdout IO value
                            let stdout = Value::IO(value::IOValue::stdout_ref());

                            // Push arguments in order: io, struct
                            self.stack.push(resolved.clone());
                            self.stack.push(stdout);

                            // Set up the function call frame
                            let args = [
                                self.stack.pop_value()?, // io
                                self.stack.pop_value()?, // struct
                            ];
                            // args is now [io, struct] - matches show(io, x) signature

                            let mut frame =
                                Frame::new_with_slots(func.local_slot_count, Some(func_index));
                            for (idx, slot) in func.param_slots.iter().enumerate() {
                                if let Some(val) = args.get(idx) {
                                    bind_value_to_slot(
                                        &mut frame,
                                        *slot,
                                        val.clone(),
                                        &mut self.struct_heap,
                                    );
                                }
                            }

                            // Push return IP (current ip, which is already past this instruction)
                            self.return_ips.push(self.ip);
                            self.frames.push(frame);
                            self.ip = func.entry;
                            return Ok(PrintResult::Continue); // Continue to execute the show function
                        }
                    }

                    // Default: format value directly
                    let s = format_value(&resolved);
                    self.emit_output(&s, false);
                }
                Ok(PrintResult::Handled)
            }

            Instr::PrintNewline => {
                self.emit_output("", true);
                Ok(PrintResult::Handled)
            }

            _ => Ok(PrintResult::NotHandled),
        }
    }
}
