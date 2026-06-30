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
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;
use super::value_to_string;
use super::DispatchAction;

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
                    let s = super::super::formatting::format_value_print(&resolved);
                    self.stack.push(Value::Str(s));
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
                let mut parts: Vec<String> = Vec::with_capacity(*count);
                for _ in 0..*count {
                    if let Some(v) = self.stack.pop() {
                        let resolved = super::super::formatting::resolve_struct_refs_for_format(
                            &v,
                            &self.struct_heap,
                        );
                        // Issue #4741: interpolation should use print-form
                        // for Symbols ("$(:foo)" → "foo"), not show-form.
                        parts.push(super::super::formatting::format_value_print(&resolved));
                    }
                }
                // Reverse because we popped in reverse order
                parts.reverse();
                let result = parts.join("");
                self.stack.push(Value::Str(result));
                Ok(DispatchAction::Continue)
            }

            Instr::ConcatStrings(n) => {
                // Pop n values, convert to strings, concatenate. See
                // Issue #4727 / #4725 — same StructRef leak path as
                // StringConcat above.
                let mut parts: Vec<String> = Vec::with_capacity(*n);
                for _ in 0..*n {
                    let val = self.stack.pop_value()?;
                    let resolved = super::super::formatting::resolve_struct_refs_for_format(
                        &val,
                        &self.struct_heap,
                    );
                    parts.push(value_to_string(&resolved));
                }
                // Reverse because we popped in reverse order
                parts.reverse();
                let result = parts.join("");
                self.stack.push(Value::Str(result));
                Ok(DispatchAction::Continue)
            }

            Instr::ToStr => {
                let val = self.stack.pop_value()?;
                let resolved = super::super::formatting::resolve_struct_refs_for_format(
                    &val,
                    &self.struct_heap,
                );
                self.stack.push(Value::Str(value_to_string(&resolved)));
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
