//! VM-instruction effects for production bytecode optimizers (Issue #9494).
//!
//! This table is deliberately fail-closed: only instructions understood by the
//! straight-line CSE consumer are classified more precisely than [`Barrier`].
//! Adding an `Instr` variant therefore cannot silently make it movable or
//! reusable. Extend the table only together with a consumer regression.

use crate::Instr;

/// The effects relevant to local bytecode value numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionEffects {
    /// No externally observable effect and no local-slot access.
    Pure,
    /// Reads one local slot without mutating it.
    ReadsSlot(usize),
    /// Writes one local slot.
    WritesSlot(usize),
    /// Unknown, control-flow, allocating, throwing, dispatching, or otherwise
    /// observable. Local CSE must discard all available expressions here.
    Barrier,
}

/// Return the conservative effect summary for one VM instruction.
pub fn instruction_effects(instr: &Instr) -> InstructionEffects {
    match instr {
        Instr::Nop | Instr::PushI64(_) | Instr::AddI64 | Instr::SubI64 | Instr::MulI64 => {
            InstructionEffects::Pure
        }
        Instr::LoadSlotI64(slot)
        | Instr::LoadAddI64Slot(slot)
        | Instr::LoadSubI64Slot(slot)
        | Instr::LoadMulI64Slot(slot) => InstructionEffects::ReadsSlot(*slot),
        Instr::StoreSlotI64(slot) => InstructionEffects::WritesSlot(*slot),
        _ => InstructionEffects::Barrier,
    }
}

/// Whether an instruction is safe inside a local CSE expression.
pub fn instruction_can_cse(instr: &Instr) -> bool {
    matches!(
        instruction_effects(instr),
        InstructionEffects::Pure | InstructionEffects::ReadsSlot(_)
    )
}

/// Return the written slot when the instruction has a modeled local write.
pub fn instruction_written_slot(instr: &Instr) -> Option<usize> {
    match instruction_effects(instr) {
        InstructionEffects::WritesSlot(slot) => Some(slot),
        _ => None,
    }
}

/// Whether value numbering must stop before this instruction.
pub fn instruction_is_cse_barrier(instr: &Instr) -> bool {
    matches!(instruction_effects(instr), InstructionEffects::Barrier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_table_is_precise_for_the_cse_consumer_and_fail_closed_elsewhere() {
        assert_eq!(
            instruction_effects(&Instr::LoadSlotI64(3)),
            InstructionEffects::ReadsSlot(3)
        );
        assert_eq!(
            instruction_effects(&Instr::StoreSlotI64(4)),
            InstructionEffects::WritesSlot(4)
        );
        assert!(instruction_can_cse(&Instr::LoadAddI64Slot(2)));
        assert!(instruction_is_cse_barrier(&Instr::CallTypeConstructor));
        assert!(instruction_is_cse_barrier(&Instr::Jump(0)));
    }
}
