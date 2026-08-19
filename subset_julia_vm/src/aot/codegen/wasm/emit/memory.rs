use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W, MemArg};

use super::super::types::{
    ABI_VERSION, DESCRIPTOR_ELEMENT_OFFSET, DESCRIPTOR_LEN_OFFSET, DESCRIPTOR_PTR_OFFSET,
    DESCRIPTOR_STRIDE_OFFSET, U8_ELEMENT_TYPE,
};
use super::ops::get;

pub(super) fn memarg(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 0,
        memory_index: 0,
    }
}

pub(super) fn emit_descriptor_check(
    body: &mut Function,
    descriptor: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    emit_i32_field_mismatch(body, descriptor, locals, 0, ABI_VERSION)?;
    emit_i32_field_mismatch(
        body,
        descriptor,
        locals,
        DESCRIPTOR_ELEMENT_OFFSET,
        U8_ELEMENT_TYPE,
    )?;
    emit_i32_field_mismatch(body, descriptor, locals, DESCRIPTOR_STRIDE_OFFSET, 1)?;
    get(body, locals, descriptor)?;
    body.instruction(&W::I32Load(memarg(DESCRIPTOR_PTR_OFFSET)));
    body.instruction(&W::I64ExtendI32U);
    get(body, locals, descriptor)?;
    body.instruction(&W::I32Load(memarg(DESCRIPTOR_LEN_OFFSET)));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Add);
    body.instruction(&W::MemorySize(0));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Const(65_536));
    body.instruction(&W::I64Mul);
    trap_if(body, W::I64GtU);
    Ok(())
}

fn emit_i32_field_mismatch(
    body: &mut Function,
    descriptor: &VarRef,
    locals: &HashMap<String, u32>,
    offset: u64,
    expected: i32,
) -> AotResult<()> {
    get(body, locals, descriptor)?;
    body.instruction(&W::I32Load(memarg(offset)));
    body.instruction(&W::I32Const(expected));
    trap_if(body, W::I32Ne);
    Ok(())
}

fn trap_if(body: &mut Function, condition: W<'_>) {
    body.instruction(&condition);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
}

pub(super) fn emit_u8_address(
    body: &mut Function,
    descriptor: &VarRef,
    index: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    emit_descriptor_check(body, descriptor, locals)?;
    emit_zero_based_index(body, index, locals)?;
    body.instruction(&W::I64Const(0));
    trap_if(body, W::I64LtS);
    emit_zero_based_index(body, index, locals)?;
    get(body, locals, descriptor)?;
    body.instruction(&W::I32Load(memarg(DESCRIPTOR_LEN_OFFSET)));
    body.instruction(&W::I64ExtendI32U);
    trap_if(body, W::I64GeU);
    get(body, locals, descriptor)?;
    body.instruction(&W::I32Load(memarg(DESCRIPTOR_PTR_OFFSET)));
    get(body, locals, index)?;
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Const(1));
    body.instruction(&W::I32Sub);
    body.instruction(&W::I32Add);
    Ok(())
}

fn emit_zero_based_index(
    body: &mut Function,
    index: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    get(body, locals, index)?;
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Sub);
    Ok(())
}
