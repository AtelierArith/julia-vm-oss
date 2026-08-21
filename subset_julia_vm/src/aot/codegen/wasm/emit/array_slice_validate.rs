use crate::aot::ir::{ArraySelector, VarRef};
use crate::aot::{AotError, AotResult};
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{
    descriptor_layout, DESCRIPTOR_AXIS_SIZE, DESCRIPTOR_DIM_OFFSET,
    DESCRIPTOR_ELEMENT_COUNT_OFFSET,
};
use super::descriptor::{
    emit_descriptor_validation, emit_i64_load, trap_if, DescriptorAccess, DescriptorContext,
};
use super::locals::LocalLayout;
use super::ops::get;

pub(super) fn destination(
    body: &mut Function,
    array: &VarRef,
    selectors: &[ArraySelector],
    layout: &LocalLayout,
) -> AotResult<()> {
    let descriptor = descriptor_layout(&array.ty)?;
    if selectors.len() != descriptor.rank {
        return Err(AotError::InvalidIR("slice assignment rank mismatch".to_string()));
    }
    descriptor_check(body, array, descriptor, layout, DescriptorAccess::Write)?;
    for (axis, selector) in selectors.iter().enumerate() {
        let dim = axis_offset(DESCRIPTOR_DIM_OFFSET, axis)?;
        match selector {
            ArraySelector::Scalar(index) => bound(body, array, index, dim, layout)?,
            ArraySelector::UnitRange { start, stop } => {
                get(body, &layout.locals, stop)?;
                get(body, &layout.locals, start)?;
                body.instruction(&W::I64LtS);
                body.instruction(&W::If(BlockType::Empty));
                body.instruction(&W::Else);
                bound(body, array, start, dim, layout)?;
                bound(body, array, stop, dim, layout)?;
                body.instruction(&W::End);
            }
        }
    }
    Ok(())
}

pub(super) fn source(
    body: &mut Function,
    source: &VarRef,
    selectors: &[ArraySelector],
    layout: &LocalLayout,
) -> AotResult<()> {
    let descriptor = descriptor_layout(&source.ty)?;
    descriptor_check(body, source, descriptor, layout, DescriptorAccess::Read)?;
    let range_count = selectors
        .iter()
        .filter(|selector| matches!(selector, ArraySelector::UnitRange { .. }))
        .count();
    if descriptor.rank != range_count {
        body.instruction(&W::Unreachable);
        return Ok(());
    }
    emit_i64_load(body, source, &layout.locals, DESCRIPTOR_ELEMENT_COUNT_OFFSET)?;
    body.instruction(&W::LocalGet(layout.slice.count));
    trap_if(body, W::I64Ne);
    let mut source_axis = 0;
    for selector in selectors {
        if let ArraySelector::UnitRange { start, stop } = selector {
            emit_i64_load(
                body,
                source,
                &layout.locals,
                axis_offset(DESCRIPTOR_DIM_OFFSET, source_axis)?,
            )?;
            range_length(body, start, stop, layout)?;
            trap_if(body, W::I64Ne);
            source_axis += 1;
        }
    }
    Ok(())
}

pub(super) fn selection_count(
    body: &mut Function,
    selectors: &[ArraySelector],
    layout: &LocalLayout,
) -> AotResult<()> {
    body.instruction(&W::I64Const(1));
    body.instruction(&W::LocalSet(layout.slice.count));
    for selector in selectors {
        if let ArraySelector::UnitRange { start, stop } = selector {
            range_length(body, start, stop, layout)?;
            body.instruction(&W::LocalSet(layout.memory.term));
            body.instruction(&W::LocalGet(layout.slice.count));
            body.instruction(&W::LocalGet(layout.memory.term));
            body.instruction(&W::I64Mul);
            body.instruction(&W::LocalSet(layout.slice.count));
        }
    }
    Ok(())
}

fn descriptor_check(
    body: &mut Function,
    value: &VarRef,
    descriptor: super::super::types::DescriptorLayout,
    layout: &LocalLayout,
    access: DescriptorAccess,
) -> AotResult<()> {
    emit_descriptor_validation(
        body,
        value,
        descriptor,
        &DescriptorContext {
            locals: &layout.locals,
            scratch: &layout.memory,
        },
        access,
    )
}

fn range_length(
    body: &mut Function,
    start: &VarRef,
    stop: &VarRef,
    layout: &LocalLayout,
) -> AotResult<()> {
    get(body, &layout.locals, stop)?;
    get(body, &layout.locals, start)?;
    body.instruction(&W::I64LtS);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::I64)));
    body.instruction(&W::I64Const(0));
    body.instruction(&W::Else);
    get(body, &layout.locals, stop)?;
    get(body, &layout.locals, start)?;
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::End);
    Ok(())
}

fn bound(
    body: &mut Function,
    array: &VarRef,
    index: &VarRef,
    dim: u64,
    layout: &LocalLayout,
) -> AotResult<()> {
    get(body, &layout.locals, index)?;
    body.instruction(&W::I64Const(1));
    trap_if(body, W::I64LtS);
    get(body, &layout.locals, index)?;
    emit_i64_load(body, array, &layout.locals, dim)?;
    trap_if(body, W::I64GtU);
    Ok(())
}

fn axis_offset(base: u64, axis: usize) -> AotResult<u64> {
    let axis = u64::try_from(axis)
        .map_err(|_| AotError::CodegenError("slice assignment axis overflow".to_string()))?;
    let width = u64::try_from(DESCRIPTOR_AXIS_SIZE)
        .map_err(|_| AotError::CodegenError("negative descriptor axis size".to_string()))?;
    base.checked_add(axis.checked_mul(width).ok_or_else(|| {
        AotError::CodegenError("slice assignment axis overflow".to_string())
    })?)
    .ok_or_else(|| AotError::CodegenError("slice assignment offset overflow".to_string()))
}
