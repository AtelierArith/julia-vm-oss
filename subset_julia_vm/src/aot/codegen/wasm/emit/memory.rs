use crate::aot::ir::VarRef;
use crate::aot::{AotError, AotResult};
use wasm_encoder::{Function, Instruction as W, MemArg};

use super::super::types::{
    descriptor_layout, DESCRIPTOR_AXIS_SIZE, DESCRIPTOR_DIM_OFFSET, DESCRIPTOR_STRIDE_OFFSET,
};
use super::descriptor::{
    emit_descriptor_validation, emit_i64_load, trap_if, DescriptorAccess, DescriptorContext,
};
use super::locals::LocalLayout;
use super::ops::get;

pub(super) fn memarg(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 0,
        memory_index: 0,
    }
}

pub(super) fn emit_u8_address(
    body: &mut Function,
    descriptor: &VarRef,
    indices: &[VarRef],
    layout: &LocalLayout,
    access: DescriptorAccess,
) -> AotResult<()> {
    let descriptor_layout = descriptor_layout(&descriptor.ty)?;
    if indices.len() != descriptor_layout.rank {
        return Err(AotError::InvalidIR(format!(
            "Wasm descriptor rank {} received {} indices",
            descriptor_layout.rank,
            indices.len()
        )));
    }
    let context = DescriptorContext {
        locals: &layout.locals,
        scratch: &layout.memory,
    };
    emit_descriptor_validation(body, descriptor, descriptor_layout, &context, access)?;
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.max_offset));

    for (axis, index) in indices.iter().enumerate() {
        let axis_bytes = i64::try_from(axis)
            .map_err(|_| AotError::CodegenError("Wasm rank exceeds i64".to_string()))?
            * DESCRIPTOR_AXIS_SIZE;
        let dim_offset = axis_offset(DESCRIPTOR_DIM_OFFSET, axis_bytes)?;
        let stride_offset = axis_offset(DESCRIPTOR_STRIDE_OFFSET, axis_bytes)?;
        get(body, &layout.locals, index)?;
        body.instruction(&W::I64Const(1));
        body.instruction(&W::I64Sub);
        body.instruction(&W::LocalSet(layout.memory.term));
        body.instruction(&W::LocalGet(layout.memory.term));
        body.instruction(&W::I64Const(0));
        trap_if(body, W::I64LtS);
        body.instruction(&W::LocalGet(layout.memory.term));
        emit_i64_load(body, descriptor, &layout.locals, dim_offset)?;
        trap_if(body, W::I64GeU);
        body.instruction(&W::LocalGet(layout.memory.term));
        emit_i64_load(body, descriptor, &layout.locals, stride_offset)?;
        body.instruction(&W::I64Mul);
        body.instruction(&W::LocalSet(layout.memory.term));
        get(body, &layout.locals, index)?;
        body.instruction(&W::I64Const(1));
        body.instruction(&W::I64Sub);
        body.instruction(&W::I64Eqz);
        body.instruction(&W::If(wasm_encoder::BlockType::Empty));
        body.instruction(&W::Else);
        body.instruction(&W::LocalGet(layout.memory.term));
        get(body, &layout.locals, index)?;
        body.instruction(&W::I64Const(1));
        body.instruction(&W::I64Sub);
        body.instruction(&W::I64DivU);
        emit_i64_load(body, descriptor, &layout.locals, stride_offset)?;
        trap_if(body, W::I64Ne);
        body.instruction(&W::End);
        body.instruction(&W::LocalGet(layout.memory.max_offset));
        body.instruction(&W::LocalGet(layout.memory.term));
        body.instruction(&W::I64Add);
        body.instruction(&W::LocalTee(layout.memory.term));
        body.instruction(&W::LocalGet(layout.memory.max_offset));
        trap_if(body, W::I64LtU);
        body.instruction(&W::LocalGet(layout.memory.term));
        body.instruction(&W::LocalSet(layout.memory.max_offset));
    }

    body.instruction(&W::LocalGet(layout.memory.data_start));
    body.instruction(&W::LocalGet(layout.memory.max_offset));
    body.instruction(&W::I64Const(i64::from(descriptor_layout.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalTee(layout.memory.term));
    body.instruction(&W::LocalGet(layout.memory.data_start));
    trap_if(body, W::I64LtU);
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::LocalGet(layout.memory.data_end));
    trap_if(body, W::I64GeU);
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::I32WrapI64);
    Ok(())
}

fn axis_offset(base: u64, added: i64) -> AotResult<u64> {
    let added = u64::try_from(added)
        .map_err(|_| AotError::CodegenError("negative Wasm axis offset".to_string()))?;
    base.checked_add(added)
        .ok_or_else(|| AotError::CodegenError("Wasm axis offset overflow".to_string()))
}
