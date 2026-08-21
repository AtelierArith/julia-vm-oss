use crate::aot::ir::{AotExpr, ArraySelector, Instruction, IrFunction};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn array_slice_assign(
        &mut self,
        array: &AotExpr,
        indices: &[AotExpr],
        value: &AotExpr,
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let array = self.expr(array, function)?;
        let rank = match &array.ty {
            StaticType::Array {
                ndims: Some(rank), ..
            } => *rank,
            _ => return Err(unsupported("slice assignment requires a static array rank")),
        };
        if indices.len() != rank {
            return Err(unsupported(
                "slice assignment index count must match array rank",
            ));
        }
        let mut selectors = Vec::with_capacity(rank);
        for index in indices {
            selectors.push(match index {
                AotExpr::Range {
                    start,
                    stop,
                    step: None,
                    elem_ty: StaticType::I64,
                } => ArraySelector::UnitRange {
                    start: self.expr(start, function)?,
                    stop: self.expr(stop, function)?,
                },
                AotExpr::Range {
                    step: Some(step), ..
                } => {
                    return Err(unsupported(format!(
                    "Wasm slice assignment requires unit-step ranges; unsupported step `{step:?}`"
                )))
                }
                AotExpr::Range { .. } => {
                    return Err(unsupported("Wasm slice ranges require Int64 bounds"))
                }
                scalar => ArraySelector::Scalar(self.expr(scalar, function)?),
            });
        }
        let value = self.expr(value, function)?;
        self.current_block_mut(function)?
            .push(Instruction::ArraySliceAssign {
                array,
                selectors,
                value,
            });
        Ok(())
    }
}
