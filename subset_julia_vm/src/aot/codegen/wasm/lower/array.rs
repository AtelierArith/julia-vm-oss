use crate::aot::ir::{
    AotExpr, ArrayInit, ArraySelector, ConstValue, Instruction, IrFunction, StructFieldInit, VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn array_slice(
        &mut self,
        value: &AotExpr,
        result_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let AotExpr::Index { array, indices, .. } = value else {
            return Err(unsupported("array slice requires an index expression"));
        };
        let source = self.expr(array, function)?;
        let source_rank = match &source.ty {
            StaticType::Array {
                ndims: Some(rank), ..
            } => *rank,
            _ => {
                return Err(unsupported(
                    "array slice requires a statically ranked array",
                ))
            }
        };
        if indices.len() != source_rank {
            return Err(unsupported("array slice index count must match array rank"));
        }
        let mut selectors = Vec::with_capacity(indices.len());
        let mut dims = Vec::new();
        for index in indices {
            match index {
                AotExpr::Range {
                    start,
                    stop,
                    step: None,
                    elem_ty: StaticType::I64,
                } => {
                    let start = self.expr(start, function)?;
                    let stop = self.expr(stop, function)?;
                    let length = self.unit_range_length(&start, &stop, function)?;
                    selectors.push(ArraySelector::UnitRange { start, stop });
                    dims.push(length);
                }
                AotExpr::Range {
                    step: Some(step), ..
                } => {
                    return Err(unsupported(format!(
                        "Wasm array slices require unit-step ranges; unsupported step `{step:?}`"
                    )))
                }
                AotExpr::Range { .. } => {
                    return Err(unsupported("Wasm array slice ranges require Int64 bounds"))
                }
                scalar => selectors.push(ArraySelector::Scalar(self.expr(scalar, function)?)),
            }
        }
        let result_rank = match result_ty {
            StaticType::Array {
                ndims: Some(rank), ..
            } => *rank,
            _ => return Err(unsupported("array slice result requires a static rank")),
        };
        if dims.len() != result_rank {
            return Err(unsupported(
                "array slice result rank does not match range axes",
            ));
        }
        let dest = self.temporary(result_ty.clone());
        self.current_block_mut(function)?
            .push(Instruction::ArraySlice {
                dest: dest.clone(),
                source,
                selectors,
                dims,
            });
        Ok(dest)
    }

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

    pub(super) fn array_size_conversion(
        &mut self,
        value: &AotExpr,
        target_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let AotExpr::CallBuiltin { builtin, args, .. } = value else {
            unreachable!()
        };
        self.expr(
            &AotExpr::CallBuiltin {
                builtin: *builtin,
                args: args.clone(),
                return_ty: target_ty.clone(),
            },
            function,
        )
    }
}
