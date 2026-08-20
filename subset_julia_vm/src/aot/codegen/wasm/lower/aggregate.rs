use crate::aot::ir::{AotExpr, Instruction, IrFunction, StructFieldInit, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn tuple_index(
        &mut self,
        array: &AotExpr,
        indices: &[AotExpr],
        elem_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let [AotExpr::LitI64(index)] = indices else {
            return Err(unsupported("Wasm tuple indexing requires a constant index"));
        };
        let layout = self.layouts.layout(&array.get_type())?;
        let zero_based = usize::try_from(*index - 1)
            .map_err(|_| unsupported("Wasm tuple index is out of bounds"))?;
        let field = layout
            .fields
            .get(zero_based)
            .ok_or_else(|| unsupported("Wasm tuple index is out of bounds"))?;
        let offset = i32::try_from(field.offset)
            .map_err(|_| unsupported("Wasm aggregate field offset overflow"))?;
        let layout_id = layout.id;
        let object = self.expr(array, function)?;
        let dest = self.temporary(elem_ty.clone());
        self.current_block_mut(function)?
            .push(Instruction::GetFieldOffset {
                dest: dest.clone(),
                object,
                layout_id,
                offset,
            });
        Ok(dest)
    }

    pub(super) fn field_access(
        &mut self,
        object: &AotExpr,
        field: &str,
        field_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let (layout_id, offset) = self.layouts.field(&object.get_type(), field)?;
        let object = self.expr(object, function)?;
        let dest = self.temporary(field_ty.clone());
        self.current_block_mut(function)?
            .push(Instruction::GetFieldOffset {
                dest: dest.clone(),
                object,
                layout_id,
                offset: i32::try_from(offset)
                    .map_err(|_| unsupported("Wasm aggregate field offset overflow"))?,
            });
        Ok(dest)
    }

    pub(super) fn aggregate(
        &mut self,
        ty: StaticType,
        values: &[AotExpr],
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let layout = self.layouts.layout(&ty)?.clone();
        if layout.fields.len() != values.len() {
            return Err(unsupported("Wasm aggregate constructor arity mismatch"));
        }
        let fields = values
            .iter()
            .zip(&layout.fields)
            .map(|(value, field)| {
                Ok(StructFieldInit {
                    offset: i32::try_from(field.offset)
                        .map_err(|_| unsupported("Wasm aggregate field offset overflow"))?,
                    value: self.expr(value, function)?,
                })
            })
            .collect::<AotResult<Vec<_>>>()?;
        let dest = self.temporary(ty);
        self.current_block_mut(function)?
            .push(Instruction::StructNew {
                dest: dest.clone(),
                layout_id: layout.id,
                size: layout.size,
                align: layout.align,
                fields,
            });
        Ok(dest)
    }
}
