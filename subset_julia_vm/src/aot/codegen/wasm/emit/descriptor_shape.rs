use crate::aot::ir::VarRef;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{
    DescriptorLayout, DESCRIPTOR_AXIS_SIZE, DESCRIPTOR_DIM_OFFSET, DESCRIPTOR_ELEMENT_COUNT_OFFSET,
    DESCRIPTOR_STRIDE_OFFSET, MAX_DIMENSION,
};
use super::descriptor::{emit_i64_load, trap_if, DescriptorContext};

pub(super) fn emit_shape_validation(
    body: &mut Function,
    descriptor: &VarRef,
    layout: DescriptorLayout,
    context: &DescriptorContext<'_>,
) -> AotResult<()> {
    emit_i64_load(
        body,
        descriptor,
        context.locals,
        DESCRIPTOR_ELEMENT_COUNT_OFFSET,
    )?;
    body.instruction(&W::LocalSet(context.scratch.element_count));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::LocalSet(context.scratch.product));
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(context.scratch.max_offset));

    for axis in 0..layout.rank {
        let axis_offset = i64::try_from(axis)
            .map_err(|_| crate::aot::AotError::CodegenError("Wasm rank exceeds i64".to_string()))?
            * DESCRIPTOR_AXIS_SIZE;
        let dim_offset = checked_offset(DESCRIPTOR_DIM_OFFSET, axis_offset)?;
        let stride_offset = checked_offset(DESCRIPTOR_STRIDE_OFFSET, axis_offset)?;
        emit_i64_load(body, descriptor, context.locals, dim_offset)?;
        body.instruction(&W::I64Const(MAX_DIMENSION));
        trap_if(body, W::I64GtU);
        emit_i64_load(body, descriptor, context.locals, stride_offset)?;
        body.instruction(&W::I64Const(0));
        trap_if(body, W::I64LtS);
        emit_product_step(body, descriptor, context, dim_offset)?;
        emit_max_offset_step(body, descriptor, context, dim_offset, stride_offset)?;
    }

    body.instruction(&W::LocalGet(context.scratch.product));
    body.instruction(&W::LocalGet(context.scratch.element_count));
    trap_if(body, W::I64Ne);
    Ok(())
}

fn emit_product_step(
    body: &mut Function,
    descriptor: &VarRef,
    context: &DescriptorContext<'_>,
    dim_offset: u64,
) -> AotResult<()> {
    body.instruction(&W::LocalGet(context.scratch.product));
    emit_i64_load(body, descriptor, context.locals, dim_offset)?;
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalSet(context.scratch.term));
    emit_i64_load(body, descriptor, context.locals, dim_offset)?;
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(context.scratch.term));
    emit_i64_load(body, descriptor, context.locals, dim_offset)?;
    body.instruction(&W::I64DivU);
    body.instruction(&W::LocalGet(context.scratch.product));
    trap_if(body, W::I64Ne);
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::LocalSet(context.scratch.product));
    Ok(())
}

fn emit_max_offset_step(
    body: &mut Function,
    descriptor: &VarRef,
    context: &DescriptorContext<'_>,
    dim_offset: u64,
    stride_offset: u64,
) -> AotResult<()> {
    emit_i64_load(body, descriptor, context.locals, dim_offset)?;
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    emit_i64_load(body, descriptor, context.locals, dim_offset)?;
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Sub);
    emit_i64_load(body, descriptor, context.locals, stride_offset)?;
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalSet(context.scratch.term));
    emit_i64_load(body, descriptor, context.locals, dim_offset)?;
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(context.scratch.term));
    emit_i64_load(body, descriptor, context.locals, dim_offset)?;
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64DivU);
    emit_i64_load(body, descriptor, context.locals, stride_offset)?;
    trap_if(body, W::I64Ne);
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(context.scratch.max_offset));
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(context.scratch.term));
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::LocalGet(context.scratch.max_offset));
    trap_if(body, W::I64LtU);
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::LocalSet(context.scratch.max_offset));
    body.instruction(&W::End);
    Ok(())
}

fn checked_offset(base: u64, added: i64) -> AotResult<u64> {
    let added = u64::try_from(added)
        .map_err(|_| crate::aot::AotError::CodegenError("negative Wasm offset".to_string()))?;
    base.checked_add(added)
        .ok_or_else(|| crate::aot::AotError::CodegenError("Wasm offset overflow".to_string()))
}
