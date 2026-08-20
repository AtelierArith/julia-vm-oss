use crate::aot::ir::VarRef;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{DescriptorLayout, DESCRIPTOR_DATA_PTR_OFFSET};
use super::descriptor::{
    emit_i32_load, refresh_memory_size, trap_if, trap_on_stack, DescriptorContext,
};

pub(super) fn emit_data_validation(
    body: &mut Function,
    descriptor: &VarRef,
    layout: DescriptorLayout,
    context: &DescriptorContext<'_>,
) -> AotResult<()> {
    emit_i32_load(body, descriptor, context.locals, DESCRIPTOR_DATA_PTR_OFFSET)?;
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalSet(context.scratch.data_start));
    body.instruction(&W::LocalGet(context.scratch.element_count));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(context.scratch.data_start));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(context.scratch.data_start));
    body.instruction(&W::LocalSet(context.scratch.data_end));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(context.scratch.data_start));
    body.instruction(&W::I64Eqz);
    trap_on_stack(body);
    body.instruction(&W::LocalGet(context.scratch.data_start));
    body.instruction(&W::I64Const(layout.element_alignment));
    body.instruction(&W::I64RemU);
    body.instruction(&W::I64Const(0));
    trap_if(body, W::I64Ne);
    body.instruction(&W::LocalGet(context.scratch.max_offset));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(context.scratch.term));
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::LocalGet(context.scratch.max_offset));
    trap_if(body, W::I64LeU);
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::I64Const(i64::from(layout.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalSet(context.scratch.term));
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::I64Const(i64::from(layout.element_size)));
    body.instruction(&W::I64DivU);
    body.instruction(&W::LocalGet(context.scratch.max_offset));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    trap_if(body, W::I64Ne);
    body.instruction(&W::LocalGet(context.scratch.data_start));
    body.instruction(&W::LocalGet(context.scratch.term));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(context.scratch.data_end));
    body.instruction(&W::LocalGet(context.scratch.data_end));
    body.instruction(&W::LocalGet(context.scratch.data_start));
    trap_if(body, W::I64LtU);
    body.instruction(&W::End);

    refresh_memory_size(body, context.scratch);
    body.instruction(&W::LocalGet(context.scratch.data_end));
    body.instruction(&W::LocalGet(context.scratch.memory_bytes));
    trap_if(body, W::I64GtU);
    body.instruction(&W::LocalGet(context.scratch.element_count));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(context.scratch.data_start));
    body.instruction(&W::LocalGet(context.scratch.metadata_end));
    body.instruction(&W::I64LtU);
    body.instruction(&W::LocalGet(context.scratch.data_end));
    body.instruction(&W::LocalGet(context.scratch.descriptor_start));
    body.instruction(&W::I64GtU);
    body.instruction(&W::I32And);
    trap_on_stack(body);
    body.instruction(&W::End);
    Ok(())
}
