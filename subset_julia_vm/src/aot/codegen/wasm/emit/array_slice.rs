use std::collections::HashMap;

use crate::aot::ir::{ArrayInit, ArraySelector, Instruction, VarRef};
use crate::aot::{AotError, AotResult};
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{
    descriptor_layout, DESCRIPTOR_AXIS_SIZE, DESCRIPTOR_DATA_PTR_OFFSET, DESCRIPTOR_DIM_OFFSET,
    DESCRIPTOR_ELEMENT_COUNT_OFFSET, DESCRIPTOR_STRIDE_OFFSET,
};
use super::descriptor::{
    emit_descriptor_validation, emit_i64_load, trap_if, DescriptorAccess, DescriptorContext,
};
use super::instruction::{emit_array_load, emit_array_store};
use super::locals::LocalLayout;
use super::memory::memarg;
use super::ops::get;

pub(super) fn emit(
    body: &mut Function,
    instruction: &Instruction,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let Instruction::ArraySlice {
        dest,
        source,
        selectors,
        dims,
    } = instruction
    else {
        return Err(AotError::InvalidIR("expected array slice".to_string()));
    };
    let source_layout = descriptor_layout(&source.ty)?;
    if selectors.len() != source_layout.rank {
        return Err(AotError::InvalidIR("array slice rank mismatch".to_string()));
    }
    validate_source(body, source, selectors, layout)?;
    super::array::emit_new(
        body,
        &Instruction::ArrayNew {
            dest: dest.clone(),
            dims: dims.clone(),
            init: ArrayInit::Zero,
        },
        layout,
        functions,
    )?;
    copy_elements(body, dest, source, selectors, layout)
}

fn validate_source(
    body: &mut Function,
    source: &VarRef,
    selectors: &[ArraySelector],
    layout: &LocalLayout,
) -> AotResult<()> {
    let descriptor = descriptor_layout(&source.ty)?;
    emit_descriptor_validation(
        body,
        source,
        descriptor,
        &DescriptorContext {
            locals: &layout.locals,
            scratch: &layout.memory,
        },
        DescriptorAccess::Read,
    )?;
    for (axis, selector) in selectors.iter().enumerate() {
        let dim_offset = axis_offset(DESCRIPTOR_DIM_OFFSET, axis)?;
        match selector {
            ArraySelector::Scalar(index) => bounds_check(body, source, index, dim_offset, layout)?,
            ArraySelector::UnitRange { start, stop } => {
                get(body, &layout.locals, stop)?;
                get(body, &layout.locals, start)?;
                body.instruction(&W::I64LtS);
                body.instruction(&W::If(BlockType::Empty));
                body.instruction(&W::Else);
                bounds_check(body, source, start, dim_offset, layout)?;
                bounds_check(body, source, stop, dim_offset, layout)?;
                body.instruction(&W::End);
            }
        }
    }
    Ok(())
}

fn bounds_check(
    body: &mut Function,
    source: &VarRef,
    index: &VarRef,
    dim_offset: u64,
    layout: &LocalLayout,
) -> AotResult<()> {
    get(body, &layout.locals, index)?;
    body.instruction(&W::I64Const(1));
    trap_if(body, W::I64LtS);
    get(body, &layout.locals, index)?;
    emit_i64_load(body, source, &layout.locals, dim_offset)?;
    trap_if(body, W::I64GtU);
    Ok(())
}

fn copy_elements(
    body: &mut Function,
    dest: &VarRef,
    source: &VarRef,
    selectors: &[ArraySelector],
    layout: &LocalLayout,
) -> AotResult<()> {
    let destination = descriptor_layout(&dest.ty)?;
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.product));
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    body.instruction(&W::LocalGet(layout.memory.product));
    emit_i64_load(body, dest, &layout.locals, DESCRIPTOR_ELEMENT_COUNT_OFFSET)?;
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));

    get(body, &layout.locals, dest)?;
    body.instruction(&W::I32Load(memarg(DESCRIPTOR_DATA_PTR_OFFSET)));
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64Const(i64::from(destination.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Add);

    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::LocalSet(layout.memory.term));
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.max_offset));
    let mut result_axis = 0;
    for (source_axis, selector) in selectors.iter().enumerate() {
        let stride_offset = axis_offset(DESCRIPTOR_STRIDE_OFFSET, source_axis)?;
        match selector {
            ArraySelector::Scalar(index) => {
                get(body, &layout.locals, index)?;
                body.instruction(&W::I64Const(1));
                body.instruction(&W::I64Sub);
            }
            ArraySelector::UnitRange { start, .. } => {
                let result_dim = axis_offset(DESCRIPTOR_DIM_OFFSET, result_axis)?;
                body.instruction(&W::LocalGet(layout.memory.term));
                emit_i64_load(body, dest, &layout.locals, result_dim)?;
                body.instruction(&W::I64RemU);
                get(body, &layout.locals, start)?;
                body.instruction(&W::I64Add);
                body.instruction(&W::I64Const(1));
                body.instruction(&W::I64Sub);
                body.instruction(&W::LocalGet(layout.memory.term));
                emit_i64_load(body, dest, &layout.locals, result_dim)?;
                body.instruction(&W::I64DivU);
                body.instruction(&W::LocalSet(layout.memory.term));
                result_axis += 1;
            }
        }
        emit_i64_load(body, source, &layout.locals, stride_offset)?;
        body.instruction(&W::I64Mul);
        body.instruction(&W::LocalGet(layout.memory.max_offset));
        body.instruction(&W::I64Add);
        body.instruction(&W::LocalSet(layout.memory.max_offset));
    }
    get(body, &layout.locals, source)?;
    body.instruction(&W::I32Load(memarg(DESCRIPTOR_DATA_PTR_OFFSET)));
    body.instruction(&W::LocalGet(layout.memory.max_offset));
    body.instruction(&W::I64Const(i64::from(destination.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Add);
    emit_array_load(body, element_type(&source.ty)?)?;
    emit_array_store(body, element_type(&dest.ty)?)?;

    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(layout.memory.product));
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);
    Ok(())
}

fn axis_offset(base: u64, axis: usize) -> AotResult<u64> {
    let axis = u64::try_from(axis)
        .map_err(|_| AotError::CodegenError("array slice axis overflow".to_string()))?;
    let bytes = axis
        .checked_mul(
            u64::try_from(DESCRIPTOR_AXIS_SIZE)
                .map_err(|_| AotError::CodegenError("negative descriptor axis size".to_string()))?,
        )
        .ok_or_else(|| AotError::CodegenError("array slice axis overflow".to_string()))?;
    base.checked_add(bytes)
        .ok_or_else(|| AotError::CodegenError("array slice offset overflow".to_string()))
}

fn element_type(ty: &crate::aot::types::StaticType) -> AotResult<&crate::aot::types::StaticType> {
    match ty {
        crate::aot::types::StaticType::Array { element, .. } => Ok(element),
        _ => Err(AotError::InvalidIR(
            "array slice value is not an array".to_string(),
        )),
    }
}
