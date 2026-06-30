//! Print/output operations for the VM.
//!
//! This module handles print instructions:
//! - PrintStr, PrintI64, PrintF64: Print with newline
//! - Print*NoNewline: Print without newline
//! - PrintAny, PrintAnyNoNewline: Print any value
//! - PrintNewline: Print just a newline

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::util::{bind_value_to_slot, format_float_julia, format_value};
use super::super::value::{self, Value};
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute print instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_print(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::PrintStr => {
                let s = self.stack.pop_str()?;
                self.emit_output(&s, true);
                Ok(DispatchAction::Continue)
            }

            Instr::PrintI64 => {
                let x = self.stack.pop_i64()?;
                let s = x.to_string();
                self.emit_output(&s, true);
                Ok(DispatchAction::Continue)
            }

            Instr::PrintF64 => {
                let x = self.pop_f64_or_i64()?;
                let s = format_float_julia(x);
                self.emit_output(&s, true);
                Ok(DispatchAction::Continue)
            }

            Instr::PrintStrNoNewline => {
                let s = self.stack.pop_str()?;
                self.emit_output(&s, false);
                Ok(DispatchAction::Continue)
            }

            Instr::PrintI64NoNewline => {
                let x = self.stack.pop_i64()?;
                let s = x.to_string();
                self.emit_output(&s, false);
                Ok(DispatchAction::Continue)
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
                            // AUDIT(#4766): heap index already missing; the
                            // resolved Struct is unavailable so we fall back to
                            // the bare value's own formatting (no leak — the
                            // index is dangling, not a live heap struct).
                            format_value(&val)
                        }
                    }
                    // AUDIT(#4766): reached only for non-StructRef `val` (the
                    // StructRef arm above resolves via self.struct_heap.get), so
                    // no `StructRef(heap_idx=N)` repr can leak here.
                    _ => format_value(&val),
                };
                self.emit_output(&s, false);
                Ok(DispatchAction::Continue)
            }

            Instr::PrintAny => {
                if let Some(v) = self.stack.pop() {
                    // Issue #5234: deep-resolve nested `Value::StructRef`
                    // against the struct heap (not just a top-level deref) so
                    // arrays whose elements are heap-allocated structs
                    // (`[1 => 2, 3 => 4]`, `[Foo(1), Foo(2)]`) render each
                    // element via its show form instead of leaking the Rust
                    // debug `StructRef(heap_idx=N)` repr from inside
                    // `format_array_element`. A top-level StructRef still
                    // resolves to `Value::Struct`, matching the previous
                    // shallow behavior for scalars.
                    let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
                        &v,
                        &self.struct_heap,
                    );
                    // Issue #7893: an array whose struct elements have a
                    // registered `Base.show(io, ::T)` (e.g. `Matrix{Num}`)
                    // renders each element via that method, matching upstream's
                    // per-element `show` in array display.
                    let s = self
                        .render_array_via_user_show(&resolved)
                        // Issue #4741: use print-form for Symbols (bare name,
                        // no `:` prefix). show retains the `:`.
                        .unwrap_or_else(|| crate::vm::formatting::format_value_print(&resolved));
                    self.emit_output(&s, true);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PrintAnyNoNewline => {
                if let Some(v) = self.stack.pop() {
                    // Issue #5234: deep-resolve nested `Value::StructRef` (see
                    // the PrintAny arm above) so heap-struct array elements
                    // render via their show form rather than leaking
                    // `StructRef(heap_idx=N)`. A top-level StructRef still
                    // resolves to `Value::Struct`, so the custom-`show`-method
                    // dispatch below continues to fire for scalar structs.
                    let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
                        &v,
                        &self.struct_heap,
                    );

                    // Check if this is a struct with a custom show method. Uses the
                    // shared lookup so module-qualified type names (e.g.
                    // "Primes.Factorization") still match a `Base.show(io, ::Factorization)`
                    // registered under the bare name (Issue #7171/#7172), in addition to
                    // the parametric base-name fallback ("Complex{Float64}" -> "Complex").
                    let show_func_index = self.user_show_method_for(&resolved);

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
                                self.acquire_frame(func.local_slot_count, Some(func_index));
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
                            self.try_push_call_frame(frame)?;
                            self.ip = func.entry;
                            return Ok(DispatchAction::Continue); // Continue to execute the show function
                        }
                    }

                    // Issue #7893: array of structs with a registered
                    // `Base.show` renders each element via that method.
                    // Default: format value directly. Issue #4741: use
                    // print-form so Symbols emit bare names.
                    let s = self
                        .render_array_via_user_show(&resolved)
                        .unwrap_or_else(|| crate::vm::formatting::format_value_print(&resolved));
                    self.emit_output(&s, false);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PrintNewline => {
                self.emit_output("", true);
                Ok(DispatchAction::Continue)
            }

            Instr::IOPrintlnNewline => {
                // Issue #4853 follow-up: pop one value and write only a trailing
                // newline to it as a sink. For an `IO` value route the newline to
                // the resolved sink (mirroring `BuiltinId::Println`'s IO branch);
                // for any non-IO value (the `println(non_io, x)` case) emit the
                // newline to stdout. This never re-prints the value itself, so it
                // composes after the `IOPrint(2); Pop` sequence without
                // double-printing the first argument.
                let val = self.stack.pop_value()?;
                if let Value::IO(io_ref) = &val {
                    let io_kind = io_ref.borrow().kind.clone();
                    if self.sprint_state.is_some() || io_kind == crate::vm::IOKind::Stdout {
                        self.emit_output("\n", false);
                    } else if io_kind == crate::vm::IOKind::Stderr {
                        self.emit_stderr("\n", false);
                    } else if io_kind == crate::vm::IOKind::Devnull {
                        // discard
                    } else {
                        io_ref.borrow_mut().buffer.push('\n');
                    }
                } else {
                    self.emit_output("\n", false);
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
