use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{
    DescriptorLayout, ABI_VERSION, ALLOWED_FLAGS, DESCRIPTOR_AXIS_SIZE,
    DESCRIPTOR_ELEMENT_SIZE_OFFSET, DESCRIPTOR_ELEMENT_TAG_OFFSET, DESCRIPTOR_FLAGS_OFFSET,
    DESCRIPTOR_HEADER_SIZE, DESCRIPTOR_LAYOUT_OFFSET, DESCRIPTOR_RANK_OFFSET,
    DESCRIPTOR_RESERVED_OFFSET, FLAG_READONLY,
};
use super::descriptor_data::emit_data_validation;
use super::descriptor_shape::emit_shape_validation;
use super::locals::MemoryLocals;
use super::memory::memarg;
use super::ops::get;

#[derive(Clone, Copy)]
pub(super) enum DescriptorAccess {
    Read,
    Write,
}

pub(super) struct DescriptorContext<'a> {
    pub(super) locals: &'a HashMap<String, u32>,
    pub(super) scratch: &'a MemoryLocals,
}

pub(super) fn emit_descriptor_validation(
    body: &mut Function,
    descriptor: &VarRef,
    layout: DescriptorLayout,
    context: &DescriptorContext<'_>,
    access: DescriptorAccess,
) -> AotResult<()> {
    get(body, context.locals, descriptor)?;
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalTee(context.scratch.descriptor_start));
    body.instruction(&W::I64Eqz);
    trap_on_stack(body);
    body.instruction(&W::LocalGet(context.scratch.descriptor_start));
    body.instruction(&W::I64Const(7));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Const(0));
    trap_if(body, W::I64Ne);

    refresh_memory_size(body, context.scratch);
    body.instruction(&W::LocalGet(context.scratch.descriptor_start));
    body.instruction(&W::I64Const(DESCRIPTOR_HEADER_SIZE));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalGet(context.scratch.memory_bytes));
    trap_if(body, W::I64GtU);

    emit_i32_field_mismatch(body, descriptor, context.locals, 0, ABI_VERSION)?;
    emit_unknown_flags_trap(body, descriptor, context.locals)?;
    emit_i32_field_mismatch(
        body,
        descriptor,
        context.locals,
        DESCRIPTOR_RESERVED_OFFSET,
        0,
    )?;
    emit_rank_and_metadata_extent(body, descriptor, layout, context)?;
    emit_i32_field_mismatch(
        body,
        descriptor,
        context.locals,
        DESCRIPTOR_ELEMENT_TAG_OFFSET,
        i32::try_from(layout.element_tag).map_err(|_| {
            crate::aot::AotError::CodegenError("Wasm element tag exceeds i32".to_string())
        })?,
    )?;
    emit_i32_field_mismatch(
        body,
        descriptor,
        context.locals,
        DESCRIPTOR_ELEMENT_SIZE_OFFSET,
        layout.element_size,
    )?;
    emit_i32_field_mismatch(
        body,
        descriptor,
        context.locals,
        DESCRIPTOR_LAYOUT_OFFSET,
        0,
    )?;
    emit_shape_validation(body, descriptor, layout, context)?;
    emit_data_validation(body, descriptor, layout, context)?;

    if matches!(access, DescriptorAccess::Write) {
        emit_i32_load(body, descriptor, context.locals, DESCRIPTOR_FLAGS_OFFSET)?;
        body.instruction(&W::I32Const(FLAG_READONLY));
        body.instruction(&W::I32And);
        body.instruction(&W::I32Const(0));
        trap_if(body, W::I32Ne);
    }
    Ok(())
}

fn emit_rank_and_metadata_extent(
    body: &mut Function,
    descriptor: &VarRef,
    layout: DescriptorLayout,
    context: &DescriptorContext<'_>,
) -> AotResult<()> {
    let expected_rank = i32::try_from(layout.rank)
        .map_err(|_| crate::aot::AotError::CodegenError("Wasm rank exceeds i32".to_string()))?;
    emit_i32_load(body, descriptor, context.locals, DESCRIPTOR_RANK_OFFSET)?;
    body.instruction(&W::I32Const(expected_rank));
    trap_if(body, W::I32Ne);
    body.instruction(&W::LocalGet(context.scratch.descriptor_start));
    body.instruction(&W::I64Const(DESCRIPTOR_HEADER_SIZE));
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(DESCRIPTOR_AXIS_SIZE));
    body.instruction(&W::I64Const(i64::from(expected_rank)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalTee(context.scratch.metadata_end));
    body.instruction(&W::LocalGet(context.scratch.memory_bytes));
    trap_if(body, W::I64GtU);
    Ok(())
}

fn emit_unknown_flags_trap(
    body: &mut Function,
    descriptor: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    emit_i32_load(body, descriptor, locals, DESCRIPTOR_FLAGS_OFFSET)?;
    body.instruction(&W::I32Const(!ALLOWED_FLAGS));
    body.instruction(&W::I32And);
    body.instruction(&W::I32Const(0));
    trap_if(body, W::I32Ne);
    Ok(())
}

pub(super) fn refresh_memory_size(body: &mut Function, scratch: &MemoryLocals) {
    body.instruction(&W::MemorySize(0));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Const(65_536));
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalSet(scratch.memory_bytes));
}

fn emit_i32_field_mismatch(
    body: &mut Function,
    descriptor: &VarRef,
    locals: &HashMap<String, u32>,
    offset: u64,
    expected: i32,
) -> AotResult<()> {
    emit_i32_load(body, descriptor, locals, offset)?;
    body.instruction(&W::I32Const(expected));
    trap_if(body, W::I32Ne);
    Ok(())
}

pub(super) fn emit_i32_load(
    body: &mut Function,
    descriptor: &VarRef,
    locals: &HashMap<String, u32>,
    offset: u64,
) -> AotResult<()> {
    get(body, locals, descriptor)?;
    body.instruction(&W::I32Load(memarg(offset)));
    Ok(())
}

pub(super) fn emit_i64_load(
    body: &mut Function,
    descriptor: &VarRef,
    locals: &HashMap<String, u32>,
    offset: u64,
) -> AotResult<()> {
    get(body, locals, descriptor)?;
    body.instruction(&W::I64Load(memarg(offset)));
    Ok(())
}

pub(super) fn trap_if(body: &mut Function, condition: W<'_>) {
    body.instruction(&condition);
    trap_on_stack(body);
}

pub(super) fn trap_on_stack(body: &mut Function) {
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
}
