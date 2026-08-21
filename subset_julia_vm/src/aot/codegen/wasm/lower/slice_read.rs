use crate::aot::ir::{AotExpr, Instruction, IrFunction, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn slice_read(
        &mut self,
        expr: &AotExpr,
        function: &mut IrFunction,
    ) -> AotResult<Option<VarRef>> {
        match expr {
            AotExpr::Index {
                elem_ty,
                is_tuple: false,
                ..
            } if matches!(elem_ty, StaticType::Array { .. }) => {
                self.array_slice(expr, elem_ty, function).map(Some)
            }
            AotExpr::Convert { value, target_ty }
                if matches!(
                    value.as_ref(),
                    AotExpr::Index {
                        is_tuple: false,
                        ..
                    }
                ) && matches!(target_ty, StaticType::Array { .. }) =>
            {
                self.array_slice(value, target_ty, function).map(Some)
            }
            _ => Ok(None),
        }
    }

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
