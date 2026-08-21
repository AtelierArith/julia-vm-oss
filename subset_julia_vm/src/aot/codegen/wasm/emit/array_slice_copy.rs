use crate::aot::ir::{ArraySelector, VarRef};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{
    descriptor_layout, DESCRIPTOR_AXIS_SIZE, DESCRIPTOR_DATA_PTR_OFFSET, DESCRIPTOR_DIM_OFFSET,
    DESCRIPTOR_STRIDE_OFFSET,
};
use super::array_slice_dispatch::{emit_array_load, emit_array_store};
use super::descriptor::emit_i64_load;
use super::locals::LocalLayout;
use super::memory::memarg;
use super::ops::get;

pub(super) fn linear_to_temporary(
    body: &mut Function,
    source: &VarRef,
    element_size: i32,
    layout: &LocalLayout,
) -> AotResult<()> {
    loop_start(body, layout);
    body.instruction(&W::LocalGet(layout.slice.temporary));
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64Const(i64::from(element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I64Add);
    body.instruction(&W::I32WrapI64);
    linear_address(body, source, layout)?;
    emit_array_load(body, element_type(&source.ty)?)?;
    emit_array_store(body, element_type(&source.ty)?)?;
    loop_end(body, layout);
    Ok(())
}

pub(super) fn destination_loop<F>(
    body: &mut Function,
    array: &VarRef,
    selectors: &[ArraySelector],
    layout: &LocalLayout,
    mut value: F,
) -> AotResult<()>
where
    F: FnMut(&mut Function) -> AotResult<()>,
{
    loop_start(body, layout);
    selection_address(body, array, selectors, layout)?;
    value(body)?;
    emit_array_store(body, element_type(&array.ty)?)?;
    loop_end(body, layout);
    Ok(())
}

fn loop_start(body: &mut Function, layout: &LocalLayout) {
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.product));
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::LocalGet(layout.slice.count));
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));
}

fn loop_end(body: &mut Function, layout: &LocalLayout) {
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(layout.memory.product));
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);
}

fn linear_address(body: &mut Function, array: &VarRef, layout: &LocalLayout) -> AotResult<()> {
    let descriptor = descriptor_layout(&array.ty)?;
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::LocalSet(layout.memory.term));
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.max_offset));
    for axis in 0..descriptor.rank {
        body.instruction(&W::LocalGet(layout.memory.term));
        emit_i64_load(
            body,
            array,
            &layout.locals,
            axis_offset(DESCRIPTOR_DIM_OFFSET, axis)?,
        )?;
        body.instruction(&W::I64RemU);
        add_stride(body, array, axis, layout)?;
        body.instruction(&W::LocalGet(layout.memory.term));
        emit_i64_load(
            body,
            array,
            &layout.locals,
            axis_offset(DESCRIPTOR_DIM_OFFSET, axis)?,
        )?;
        body.instruction(&W::I64DivU);
        body.instruction(&W::LocalSet(layout.memory.term));
    }
    data_address(body, array, descriptor.element_size, layout)
}

fn selection_address(
    body: &mut Function,
    array: &VarRef,
    selectors: &[ArraySelector],
    layout: &LocalLayout,
) -> AotResult<()> {
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::LocalSet(layout.memory.term));
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.max_offset));
    for (axis, selector) in selectors.iter().enumerate() {
        match selector {
            ArraySelector::Scalar(index) => get(body, &layout.locals, index)?,
            ArraySelector::UnitRange { start, stop } => {
                body.instruction(&W::LocalGet(layout.memory.term));
                range_length(body, start, stop, layout)?;
                body.instruction(&W::I64RemU);
                get(body, &layout.locals, start)?;
                body.instruction(&W::I64Add);
                body.instruction(&W::LocalGet(layout.memory.term));
                range_length(body, start, stop, layout)?;
                body.instruction(&W::I64DivU);
                body.instruction(&W::LocalSet(layout.memory.term));
            }
        }
        body.instruction(&W::I64Const(1));
        body.instruction(&W::I64Sub);
        add_stride(body, array, axis, layout)?;
    }
    data_address(
        body,
        array,
        descriptor_layout(&array.ty)?.element_size,
        layout,
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
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    Ok(())
}

fn add_stride(
    body: &mut Function,
    array: &VarRef,
    axis: usize,
    layout: &LocalLayout,
) -> AotResult<()> {
    emit_i64_load(
        body,
        array,
        &layout.locals,
        axis_offset(DESCRIPTOR_STRIDE_OFFSET, axis)?,
    )?;
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalGet(layout.memory.max_offset));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(layout.memory.max_offset));
    Ok(())
}

fn data_address(
    body: &mut Function,
    array: &VarRef,
    element_size: i32,
    layout: &LocalLayout,
) -> AotResult<()> {
    get(body, &layout.locals, array)?;
    body.instruction(&W::I32Load(memarg(DESCRIPTOR_DATA_PTR_OFFSET)));
    body.instruction(&W::LocalGet(layout.memory.max_offset));
    body.instruction(&W::I64Const(i64::from(element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Add);
    Ok(())
}

fn axis_offset(base: u64, axis: usize) -> AotResult<u64> {
    let axis = u64::try_from(axis)
        .map_err(|_| AotError::CodegenError("slice assignment axis overflow".to_string()))?;
    let width = u64::try_from(DESCRIPTOR_AXIS_SIZE)
        .map_err(|_| AotError::CodegenError("negative descriptor axis size".to_string()))?;
    base.checked_add(
        axis.checked_mul(width)
            .ok_or_else(|| AotError::CodegenError("slice assignment axis overflow".to_string()))?,
    )
    .ok_or_else(|| AotError::CodegenError("slice assignment offset overflow".to_string()))
}

fn element_type(ty: &StaticType) -> AotResult<&StaticType> {
    match ty {
        StaticType::Array { element, .. } => Ok(element),
        _ => Err(AotError::InvalidIR(
            "slice assignment value is not an array".to_string(),
        )),
    }
}
