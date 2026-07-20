//! String operations for the VM.
//!
//! This module handles string-related instructions including:
//! - ToString: Convert value to string using format_value
//! - StringConcat: Concatenate multiple values using format_value
//! - ConcatStrings: Concatenate multiple values using value_to_string
//! - ToStr: Convert a value to string using value_to_string

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::formatting::Resolved;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;
use super::value_to_string;
use super::DispatchAction;

/// Raw byte payload for direct string-carrier concat parts (Issue #8995):
/// String/StrBytes contribute their exact bytes and malformed Chars their
/// pattern bytes, so invalid UTF-8 survives `*` / `string(...)` instead of
/// being replaced through a lossy render. All other values render through
/// the usual show/print pipeline.
fn concat_part_bytes(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Str(s) => Some(s.as_bytes().to_vec()),
        Value::StrBytes(b) => Some(b.as_ref().to_vec()),
        Value::CharMalformed(bits) => {
            let (bytes, len) = super::super::value::julia_char_pattern_bytes(*bits);
            Some(bytes[..len].to_vec())
        }
        _ => None,
    }
}

impl<R: RngLike> Vm<R> {
    /// Execute string operation instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_string_ops(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::ToString => {
                if let Some(v) = self.stack.pop() {
                    // Issue #4741: ToString uses print-form for Symbols.
                    // Issue #4766: resolve `Value::StructRef` against the
                    // struct heap before formatting so heap-allocated structs
                    // (`Pair(1, 2)`, user structs) render via their show form
                    // instead of leaking the Rust debug `StructRef(heap_idx=N)`
                    // repr. Although the compiler does not currently emit
                    // `Instr::ToString`, keep this display entry point on the
                    // same heap-resolution contract as the live string paths so
                    // it cannot reintroduce the leak if it is ever wired up.
                    let resolved = super::super::formatting::resolve_struct_refs_for_format(
                        &v,
                        &self.struct_heap,
                    );
                    let s = self
                        .render_value_via_user_show_for_print(&resolved)
                        .or_else(|| self.render_array_via_user_show(&resolved))
                        .unwrap_or_else(|| {
                            super::super::formatting::format_value_print(&Resolved::trivial(
                                &resolved,
                            ))
                        });
                    self.stack.push(Value::str_new(s));
                }
                Ok(DispatchAction::Continue)
            }

            Instr::StringConcat(count) => {
                // Pop count values, convert each to string, concatenate.
                // Issue #4727: pre-resolve StructRef values against the
                // struct heap so interpolated heap-allocated structs like
                // `Pair(1, 2)` render as `"1 => 2"` instead of leaking the
                // Rust `StructRef(heap_idx=N)` debug repr. This is the same
                // helper StringNew (#4725) uses; string interpolation lowers
                // to Instr::StringConcat which has its own heap-less
                // format_value call site.
                let mut parts: Vec<Vec<u8>> = Vec::with_capacity(*count);
                for _ in 0..*count {
                    if let Some(v) = self.stack.pop() {
                        if let Some(bytes) = concat_part_bytes(&v) {
                            parts.push(bytes);
                            continue;
                        }
                        let resolved = super::super::formatting::resolve_struct_refs_for_format(
                            &v,
                            &self.struct_heap,
                        );
                        // Issue #4741: interpolation should use print-form
                        // for Symbols ("$(:foo)" → "foo"), not show-form.
                        parts.push(
                            self.render_value_via_user_show_for_print(&resolved)
                                .or_else(|| self.render_array_via_user_show(&resolved))
                                .unwrap_or_else(|| {
                                    super::super::formatting::format_value_print(
                                        &Resolved::trivial(&resolved),
                                    )
                                })
                                .into_bytes(),
                        );
                    }
                }
                // Reverse because we popped in reverse order
                parts.reverse();
                let Some(result_len) = parts
                    .iter()
                    .try_fold(0usize, |acc, part| acc.checked_add(part.len()))
                else {
                    self.raise(VmError::OutOfMemory)?;
                    return Ok(DispatchAction::Continue);
                };
                if self.byte_allocation_exceeds_budget(result_len) {
                    self.raise(VmError::OutOfMemory)?;
                    return Ok(DispatchAction::Continue);
                }
                let result: Vec<u8> = parts.concat();
                self.stack.push(Value::str_from_bytes(result));
                Ok(DispatchAction::Continue)
            }

            Instr::ConcatStrings(n) => {
                // Pop n values, convert to strings, concatenate. See
                // Issue #4727 / #4725 — same StructRef leak path as
                // StringConcat above.
                let mut parts: Vec<Vec<u8>> = Vec::with_capacity(*n);
                for _ in 0..*n {
                    let val = self.stack.pop_value()?;
                    if let Some(bytes) = concat_part_bytes(&val) {
                        parts.push(bytes);
                        continue;
                    }
                    let resolved = super::super::formatting::resolve_struct_refs_for_format(
                        &val,
                        &self.struct_heap,
                    );
                    parts.push(
                        self.render_value_via_user_show_for_print(&resolved)
                            .or_else(|| self.render_array_via_user_show(&resolved))
                            .unwrap_or_else(|| value_to_string(&Resolved::trivial(&resolved)))
                            .into_bytes(),
                    );
                }
                // Reverse because we popped in reverse order
                parts.reverse();
                let Some(result_len) = parts
                    .iter()
                    .try_fold(0usize, |acc, part| acc.checked_add(part.len()))
                else {
                    self.raise(VmError::OutOfMemory)?;
                    return Ok(DispatchAction::Continue);
                };
                if self.byte_allocation_exceeds_budget(result_len) {
                    self.raise(VmError::OutOfMemory)?;
                    return Ok(DispatchAction::Continue);
                }
                let result: Vec<u8> = parts.concat();
                self.stack.push(Value::str_from_bytes(result));
                Ok(DispatchAction::Continue)
            }

            Instr::ToStr => {
                let val = self.stack.pop_value()?;
                let resolved = super::super::formatting::resolve_struct_refs_for_format(
                    &val,
                    &self.struct_heap,
                );
                let rendered = self
                    .render_value_via_user_show_for_print(&resolved)
                    .or_else(|| self.render_array_via_user_show(&resolved))
                    .unwrap_or_else(|| value_to_string(&Resolved::trivial(&resolved)));
                self.stack.push(Value::str_new(rendered));
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
