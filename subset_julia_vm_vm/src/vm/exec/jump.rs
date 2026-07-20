//! Jump/control flow operations for the VM.
//!
//! This module handles jump instructions:
//! - Jump: Unconditional jump
//! - JumpIfZero: Jump if condition is zero/false
//! - JumpIfNeI64, JumpIfEqI64: Jump based on I64 equality
//! - JumpIfLtI64, JumpIfGtI64, JumpIfLeI64, JumpIfGeI64: Jump based on I64 comparison
//! - JumpIfGtI64Slots: Jump based on direct I64 slot comparison
//! - AddConstI64SlotAndJumpIfLe: Add a constant to an I64 loop slot and branch on the updated value
//! - JumpIfEqF64, JumpIfNeF64, JumpIfNot*F64: fused F64 compare false branches

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use std::cmp::Ordering;

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::util::value_type_name;
use super::super::value::Value;
use super::super::Vm;
use subset_julia_vm_bytecode::I64Cmp;

impl<R: RngLike> Vm<R> {
    /// Execute jump instructions.
    /// Returns the execution result.
    // Hot dispatch handler: front-loaded in `dispatch_instr` (Issue #5175).
    #[inline(always)]
    pub(super) fn execute_jump(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::Jump(target) => self.jump_to(*target),

            Instr::JumpIfZero(target) => {
                // Use explicit match instead of `?` so TypeError from a non-boolean
                // condition (e.g., `if 42`) can be caught by a surrounding try/catch.
                match self.execute_jump_if_zero(*target) {
                    Ok(Some(new_ip)) => self.jump_to(new_ip),
                    Ok(None) => Ok(DispatchAction::Continue),
                    Err(err) => {
                        self.raise(err)?;
                        // handle_error has already set self.ip to the catch handler.
                        Ok(DispatchAction::Continue)
                    }
                }
            }

            Instr::JumpIfNeI64(target) => {
                // Compare top 2 I64s, jump if not equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a != b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfEqI64(target) => {
                // Compare top 2 I64s, jump if equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a == b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfLtI64(target) => {
                // Compare top 2 I64s, jump if less than
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a < b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfGtI64(target) => {
                // Compare top 2 I64s, jump if greater than
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a > b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, target) => {
                if let Some(frame) = self.frames.last() {
                    if let (Some(lhs), Some(rhs)) =
                        (frame.slot_i64(*lhs_slot), frame.slot_i64(*rhs_slot))
                    {
                        return if lhs > rhs {
                            self.jump_to(*target)
                        } else {
                            Ok(DispatchAction::Continue)
                        };
                    }
                }
                let Some(lhs) = self.load_i64_slot_for_jump(*lhs_slot, "JumpIfGtI64Slots")? else {
                    return Ok(DispatchAction::Continue);
                };
                let Some(rhs) = self.load_i64_slot_for_jump(*rhs_slot, "JumpIfGtI64Slots")? else {
                    return Ok(DispatchAction::Continue);
                };
                if lhs > rhs {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, target) => {
                if slot != stop_slot {
                    if let Some(frame) = self.frames.last_mut() {
                        let stop = frame.slot_i64(*stop_slot);
                        if let Some(stop) = stop {
                            if let Some(Some(Value::I64(current))) =
                                frame.locals_slots.get_mut(*slot)
                            {
                                let updated = current.wrapping_add(*delta);
                                *current = updated;
                                return if updated <= stop {
                                    self.jump_to(*target)
                                } else {
                                    Ok(DispatchAction::Continue)
                                };
                            }
                        }
                    }
                }

                let update_result: Result<Option<i64>, VmError> = {
                    let Some(frame) = self.frames.last_mut() else {
                        return Ok(DispatchAction::Continue);
                    };
                    let current = match frame.locals_slots.get(*slot) {
                        Some(Some(Value::I64(value))) => Some(*value),
                        Some(Some(_)) => {
                            return Err(VmError::InternalError(
                                "AddConstI64SlotAndJumpIfLe: expected I64".to_string(),
                            ));
                        }
                        Some(None) => None,
                        None => {
                            return Err(super::slot_out_of_bounds(
                                "AddConstI64SlotAndJumpIfLe",
                                slot,
                            ));
                        }
                    };
                    if let Some(current) = current {
                        let updated = current.wrapping_add(*delta);
                        if !frame.set_slot_i64(*slot, updated) {
                            return Err(super::slot_out_of_bounds(
                                "AddConstI64SlotAndJumpIfLe",
                                slot,
                            ));
                        }
                        Ok(Some(updated))
                    } else {
                        Ok(None)
                    }
                };
                let updated = match update_result? {
                    Some(updated) => updated,
                    None => {
                        let slot_name = self
                            .frames
                            .last()
                            .map(|frame| self.slot_name_for_frame(frame, *slot))
                            .unwrap_or_else(|| format!("slot {}", slot));
                        self.raise(VmError::UndefVarError(slot_name))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let Some(stop) =
                    self.load_i64_slot_for_jump(*stop_slot, "AddConstI64SlotAndJumpIfLe")?
                else {
                    return Ok(DispatchAction::Continue);
                };
                if updated <= stop {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfLeI64(target) => {
                // Compare top 2 I64s, jump if less or equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a <= b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfGeI64(target) => {
                // Compare top 2 I64s, jump if greater or equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a >= b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfEqF64(target) => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                if a == b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfNeF64(target) => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                if a != b {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfNotLtF64(target) => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                if !matches!(a.partial_cmp(&b), Some(Ordering::Less)) {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfNotGtF64(target) => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                if !matches!(a.partial_cmp(&b), Some(Ordering::Greater)) {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfNotLeF64(target) => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                if !matches!(a.partial_cmp(&b), Some(Ordering::Less | Ordering::Equal)) {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfNotGeF64(target) => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                if !matches!(a.partial_cmp(&b), Some(Ordering::Greater | Ordering::Equal)) {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::JumpIfCmpI64SlotConst(slot, konst, cmp, target) => {
                // Fused `LoadSlotI64(slot); PushI64(konst); <cmp>I64; JumpIfZero`
                // (Issue #10105). `load_i64_slot_for_jump` reproduces the exact
                // `LoadSlotI64`+`pop_i64` widening / `UndefVarError` / `TypeError`
                // semantics; `cmp` is already the (inverted) branch predicate the
                // peephole pass folded, so the comparison here is applied directly.
                let Some(a) = self.load_i64_slot_for_jump(*slot, "JumpIfCmpI64SlotConst")? else {
                    return Ok(DispatchAction::Continue);
                };
                let b = *konst;
                let take = match cmp {
                    I64Cmp::Lt => a < b,
                    I64Cmp::Gt => a > b,
                    I64Cmp::Le => a <= b,
                    I64Cmp::Ge => a >= b,
                    I64Cmp::Eq => a == b,
                    I64Cmp::Ne => a != b,
                };
                if take {
                    self.jump_to(*target)
                } else {
                    Ok(DispatchAction::Continue)
                }
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    #[inline(always)]
    fn load_i64_slot_for_jump(
        &mut self,
        slot: usize,
        instr_name: &'static str,
    ) -> Result<Option<i64>, VmError> {
        let Some(frame) = self.frames.last() else {
            self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
            return Ok(None);
        };

        if let Some(value) = frame.slot_i64(slot) {
            return Ok(Some(value));
        }

        match frame.locals_slots.get(slot) {
            Some(Some(Value::I64(value))) => Ok(Some(*value)),
            Some(Some(Value::Bool(value))) => Ok(Some(if *value { 1 } else { 0 })),
            Some(Some(Value::I32(value))) => Ok(Some(i64::from(*value))),
            Some(Some(Value::I16(value))) => Ok(Some(i64::from(*value))),
            Some(Some(Value::I8(value))) => Ok(Some(i64::from(*value))),
            Some(Some(Value::I128(value))) => Ok(Some(*value as i64)),
            Some(Some(Value::U8(value))) => Ok(Some(i64::from(*value))),
            Some(Some(Value::U16(value))) => Ok(Some(i64::from(*value))),
            Some(Some(Value::U32(value))) => Ok(Some(i64::from(*value))),
            Some(Some(Value::U64(value))) => Ok(Some(*value as i64)),
            Some(Some(Value::U128(value))) => Ok(Some(*value as i64)),
            Some(Some(value @ (Value::F16(_) | Value::F32(_) | Value::F64(_)))) => Err(
                VmError::TypeError(format!("expected I64, got {:?}", value_type_name(value))),
            ),
            Some(Some(value)) => {
                let name = self.slot_name_for_frame(frame, slot);
                Err(VmError::InternalError(format!(
                    "{}: expected numeric in {}, got {:?}",
                    instr_name, name, value
                )))
            }
            Some(None) => {
                let name = self.slot_name_for_frame(frame, slot);
                self.raise(VmError::UndefVarError(name))?;
                Ok(None)
            }
            None => Err(super::slot_out_of_bounds(instr_name, slot)),
        }
    }
}
