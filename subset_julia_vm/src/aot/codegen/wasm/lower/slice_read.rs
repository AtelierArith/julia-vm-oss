use crate::aot::ir::{Instruction, IrFunction, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn unit_range_length(
        &mut self,
        start: &VarRef,
        stop: &VarRef,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let length = self.temporary(StaticType::I64);
        self.current_block_mut(function)?
            .push(Instruction::UnitRangeLength {
                dest: length.clone(),
                start: start.clone(),
                stop: stop.clone(),
            });
        Ok(length)
    }
}
