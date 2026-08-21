use std::collections::HashMap;

use crate::aot::ir::{Instruction, VarRef};
use crate::aot::{AotError, AotResult};
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{
    descriptor_layout, ABI_VERSION, DESCRIPTOR_AXIS_SIZE, DESCRIPTOR_DIM_OFFSET,
    DESCRIPTOR_ELEMENT_COUNT_OFFSET, DESCRIPTOR_HEADER_SIZE, DESCRIPTOR_STRIDE_OFFSET,
    FLAG_MODULE_OWNED, MAX_DIMENSION,
};
use super::allocator::{ALLOC_NAME, FREE_NAME};
use super::descriptor::{trap_if, trap_on_stack};
use super::locals::LocalLayout;
use super::memory::memarg;
use super::ops::get;

pub(super) fn emit_new(
    body: &mut Function,
    instruction: &Instruction,
    locals: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let Instruction::ArrayNew { dest, dims, init } = instruction else {
        return Err(AotError::InvalidIR(
            "expected array construction".to_string(),
        ));
    };
    let descriptor = descriptor_layout(&dest.ty)?;
    if dims.len() != descriptor.rank {
        return Err(AotError::InvalidIR(
            "array constructor rank mismatch".to_string(),
        ));
    }
    let scratch = &locals.memory;
    body.instruction(&W::I64Const(1));
    body.instruction(&W::LocalSet(scratch.product));
    for dim in dims {
        get(body, &locals.locals, dim)?;
        body.instruction(&W::LocalTee(scratch.term));
        body.instruction(&W::I64Const(0));
        trap_if(body, W::I64LtS);
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::I64Const(MAX_DIMENSION));
        trap_if(body, W::I64GtU);
        checked_mul_local(body, scratch.product, scratch.term, scratch.element_count);
    }
    body.instruction(&W::LocalGet(scratch.product));
    body.instruction(&W::I64Const(i64::from(descriptor.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalTee(scratch.term));
    body.instruction(&W::LocalGet(scratch.product));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.term));
    body.instruction(&W::I64Const(i64::from(descriptor.element_size)));
    body.instruction(&W::I64DivU);
    body.instruction(&W::LocalGet(scratch.product));
    trap_if(body, W::I64Ne);
    body.instruction(&W::End);
    allocate_data(body, descriptor.element_size, scratch, functions);
    allocate_descriptor(body, dest, descriptor.rank, locals, functions)?;
    write_header(body, dest, descriptor, locals)?;
    write_shape(body, dest, dims, locals)?;
    super::array_init::initialize(body, *init, &dest.ty, locals)?;
    Ok(())
}

fn checked_mul_local(body: &mut Function, product: u32, factor: u32, previous: u32) {
    body.instruction(&W::LocalGet(product));
    body.instruction(&W::LocalSet(previous));
    body.instruction(&W::LocalGet(product));
    body.instruction(&W::LocalGet(factor));
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalSet(product));
    body.instruction(&W::LocalGet(factor));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(product));
    body.instruction(&W::LocalGet(factor));
    body.instruction(&W::I64DivU);
    body.instruction(&W::LocalGet(previous));
    trap_if(body, W::I64Ne);
    body.instruction(&W::End);
}

fn allocate_data(
    body: &mut Function,
    alignment: i32,
    scratch: &super::locals::MemoryLocals,
    functions: &HashMap<String, u32>,
) {
    body.instruction(&W::LocalGet(scratch.product));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::I64)));
    body.instruction(&W::I64Const(0));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.term));
    body.instruction(&W::I32Const(alignment));
    body.instruction(&W::Call(functions[ALLOC_NAME]));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalTee(scratch.data_start));
    body.instruction(&W::I64Eqz);
    trap_on_stack(body);
    body.instruction(&W::LocalGet(scratch.data_start));
    body.instruction(&W::End);
    body.instruction(&W::LocalSet(scratch.data_start));
}

fn allocate_descriptor(
    body: &mut Function,
    dest: &VarRef,
    rank: usize,
    locals: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let rank = i64::try_from(rank).map_err(|_| AotError::CodegenError("rank overflow".into()))?;
    body.instruction(&W::I64Const(
        DESCRIPTOR_HEADER_SIZE + DESCRIPTOR_AXIS_SIZE * rank,
    ));
    body.instruction(&W::I32Const(8));
    body.instruction(&W::Call(functions[ALLOC_NAME]));
    body.instruction(&W::LocalTee(locals.locals[&dest.name]));
    body.instruction(&W::I32Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(locals.memory.data_start));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(locals.memory.data_start));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::Call(functions[FREE_NAME]));
    body.instruction(&W::End);
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    Ok(())
}

fn write_header(
    body: &mut Function,
    dest: &VarRef,
    descriptor: super::super::types::DescriptorLayout,
    locals: &LocalLayout,
) -> AotResult<()> {
    let fields = [
        (0, ABI_VERSION),
        (4, FLAG_MODULE_OWNED),
        (
            8,
            i32::try_from(descriptor.element_tag)
                .map_err(|_| AotError::CodegenError("tag overflow".into()))?,
        ),
        (12, descriptor.element_size),
        (16, 0),
        (
            20,
            i32::try_from(descriptor.rank)
                .map_err(|_| AotError::CodegenError("rank overflow".into()))?,
        ),
        (28, 0),
    ];
    for (offset, value) in fields {
        get(body, &locals.locals, dest)?;
        body.instruction(&W::I32Const(value));
        body.instruction(&W::I32Store(memarg(offset)));
    }
    get(body, &locals.locals, dest)?;
    body.instruction(&W::LocalGet(locals.memory.data_start));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Store(memarg(24)));
    get(body, &locals.locals, dest)?;
    body.instruction(&W::LocalGet(locals.memory.product));
    body.instruction(&W::I64Store(memarg(DESCRIPTOR_ELEMENT_COUNT_OFFSET)));
    Ok(())
}

fn write_shape(
    body: &mut Function,
    dest: &VarRef,
    dims: &[VarRef],
    locals: &LocalLayout,
) -> AotResult<()> {
    body.instruction(&W::I64Const(1));
    body.instruction(&W::LocalSet(locals.memory.term));
    for dim in dims {
        get(body, &locals.locals, dim)?;
        body.instruction(&W::LocalSet(locals.memory.data_end));
        checked_mul_local(
            body,
            locals.memory.term,
            locals.memory.data_end,
            locals.memory.element_count,
        );
    }
    body.instruction(&W::I64Const(1));
    body.instruction(&W::LocalSet(locals.memory.term));
    for (axis, dim) in dims.iter().enumerate() {
        let axis =
            u64::try_from(axis).map_err(|_| AotError::CodegenError("axis overflow".into()))?;
        get(body, &locals.locals, dest)?;
        get(body, &locals.locals, dim)?;
        body.instruction(&W::I64Store(memarg(DESCRIPTOR_DIM_OFFSET + axis * 16)));
        get(body, &locals.locals, dest)?;
        body.instruction(&W::LocalGet(locals.memory.term));
        body.instruction(&W::I64Store(memarg(DESCRIPTOR_STRIDE_OFFSET + axis * 16)));
        get(body, &locals.locals, dim)?;
        body.instruction(&W::LocalSet(locals.memory.data_end));
        checked_mul_local(
            body,
            locals.memory.term,
            locals.memory.data_end,
            locals.memory.element_count,
        );
    }
    Ok(())
}
