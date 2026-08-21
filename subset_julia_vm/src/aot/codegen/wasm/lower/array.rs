use crate::aot::ir::{
    AotExpr, ArrayInit, ConstValue, Instruction, IrFunction, StructFieldInit, VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn array_index(
        &mut self,
        array: &AotExpr,
        indices: &[AotExpr],
        elem_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let array = self.expr(array, function)?;
        let indices = indices
            .iter()
            .map(|index| self.expr(index, function))
            .collect::<AotResult<Vec<_>>>()?;
        let dest = self.temporary(elem_ty.clone());
        self.current_block_mut(function)?
            .push(Instruction::GetIndex {
                dest: dest.clone(),
                array,
                indices,
            });
        Ok(dest)
    }

    pub(super) fn array_length(
        &mut self,
        array: &AotExpr,
        return_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let array = self.expr(array, function)?;
        let dest = self.temporary(return_ty.clone());
        self.current_block_mut(function)?.push(Instruction::Call {
            dest: Some(dest.clone()),
            func: "__sjulia_array_len".to_string(),
            args: vec![array],
        });
        Ok(dest)
    }

    pub(super) fn array_ndims(
        &mut self,
        array: &AotExpr,
        return_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let array = self.expr(array, function)?;
        let rank = match &array.ty {
            StaticType::Array {
                ndims: Some(rank), ..
            } => *rank,
            _ => return Err(unsupported("ndims requires a statically ranked array")),
        };
        let rank =
            i64::try_from(rank).map_err(|_| unsupported("array rank exceeds Julia Int64"))?;
        self.constant(ConstValue::Int64(rank), return_ty.clone(), function)
    }

    pub(super) fn array_size_axis(
        &mut self,
        args: &[AotExpr],
        return_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let args = args
            .iter()
            .map(|arg| self.expr(arg, function))
            .collect::<AotResult<Vec<_>>>()?;
        let dest = self.temporary(return_ty.clone());
        self.current_block_mut(function)?.push(Instruction::Call {
            dest: Some(dest.clone()),
            func: "__sjulia_array_size_axis".to_string(),
            args,
        });
        Ok(dest)
    }

    pub(super) fn array_size(
        &mut self,
        array: &AotExpr,
        return_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let array = self.expr(array, function)?;
        let rank = match &array.ty {
            StaticType::Array {
                ndims: Some(rank), ..
            } => *rank,
            _ => return Err(unsupported("size requires a statically ranked array")),
        };
        let layout = self.layouts.layout(return_ty)?.clone();
        if layout.fields.len() != rank {
            return Err(unsupported("size tuple arity does not match array rank"));
        }
        let mut fields = Vec::with_capacity(rank);
        for (axis, field) in layout.fields.iter().enumerate() {
            let axis = i64::try_from(axis + 1)
                .map_err(|_| unsupported("array axis exceeds Julia Int64"))?;
            let axis = self.constant(ConstValue::Int64(axis), StaticType::I64, function)?;
            let value = self.temporary(StaticType::I64);
            self.current_block_mut(function)?.push(Instruction::Call {
                dest: Some(value.clone()),
                func: "__sjulia_array_size_axis".to_string(),
                args: vec![array.clone(), axis],
            });
            fields.push(StructFieldInit {
                offset: i32::try_from(field.offset)
                    .map_err(|_| unsupported("size tuple field offset overflow"))?,
                value,
            });
        }
        let dest = self.temporary(return_ty.clone());
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

    pub(super) fn array_new(
        &mut self,
        args: &[AotExpr],
        return_ty: &StaticType,
        init: ArrayInit,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let dims = args
            .iter()
            .map(|arg| self.expr(arg, function))
            .collect::<AotResult<Vec<_>>>()?;
        let dest = self.temporary(return_ty.clone());
        self.current_block_mut(function)?
            .push(Instruction::ArrayNew {
                dest: dest.clone(),
                dims,
                init,
            });
        Ok(dest)
    }
}
