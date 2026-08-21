use std::collections::HashMap;

use crate::aot::ir::{ArraySelector, Instruction, VarRef};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};
use wasm_encoder::{Function, Instruction as W};

use super::super::types::descriptor_layout;
use super::allocator::{ALLOC_NAME, FREE_NAME};
use super::conversion::emit_checked_conversion;
use super::descriptor::trap_on_stack;
use super::instruction::emit_array_load;
use super::locals::LocalLayout;

pub(super) fn emit(
    body: &mut Function,
    instruction: &Instruction,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let Instruction::ArraySliceAssign {
        array,
        selectors,
        value,
    } = instruction
    else {
        return Err(AotError::InvalidIR("expected slice assignment".to_string()));
    };
    let destination = descriptor_layout(&array.ty)?;
    super::array_slice_validate::destination(body, array, selectors, layout)?;
    super::array_slice_validate::selection_count(body, selectors, layout)?;
    match &value.ty {
        StaticType::Array { .. } => {
            super::array_slice_validate::source(body, value, selectors, layout)?;
            copy_source_to_temporary(body, value, destination.element_size, layout, functions)?;
            copy_temporary_to_destination(body, array, selectors, value, layout)?;
            body.instruction(&W::LocalGet(layout.slice.temporary));
            body.instruction(&W::I32WrapI64);
            body.instruction(&W::Call(functions[FREE_NAME]));
        }
        _ => fill_destination(body, array, selectors, value, layout)?,
    }
    Ok(())
}

fn copy_source_to_temporary(
    body: &mut Function,
    source: &VarRef,
    element_size: i32,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    body.instruction(&W::LocalGet(layout.slice.count));
    body.instruction(&W::I64Const(i64::from(element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I32Const(element_size));
    body.instruction(&W::Call(functions[ALLOC_NAME]));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalTee(layout.slice.temporary));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::LocalGet(layout.slice.count));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::I32Eqz);
    body.instruction(&W::I32And);
    trap_on_stack(body);
    super::array_slice_copy::linear_to_temporary(body, source, element_size, layout)
}

fn copy_temporary_to_destination(
    body: &mut Function,
    array: &VarRef,
    selectors: &[ArraySelector],
    source: &VarRef,
    layout: &LocalLayout,
) -> AotResult<()> {
    super::array_slice_copy::destination_loop(body, array, selectors, layout, |body| {
        body.instruction(&W::LocalGet(layout.slice.temporary));
        body.instruction(&W::LocalGet(layout.memory.product));
        body.instruction(&W::I64Const(i64::from(
            descriptor_layout(&array.ty)?.element_size,
        )));
        body.instruction(&W::I64Mul);
        body.instruction(&W::I64Add);
        body.instruction(&W::I32WrapI64);
        emit_array_load(body, element_type(&source.ty)?)
    })
}

fn fill_destination(
    body: &mut Function,
    array: &VarRef,
    selectors: &[ArraySelector],
    value: &VarRef,
    layout: &LocalLayout,
) -> AotResult<()> {
    super::array_slice_copy::destination_loop(body, array, selectors, layout, |body| {
        emit_checked_conversion(body, value, element_type(&array.ty)?, &layout.locals)
    })
}

fn element_type(ty: &StaticType) -> AotResult<&StaticType> {
    match ty {
        StaticType::Array { element, .. } => Ok(element),
        _ => Err(AotError::InvalidIR(
            "slice assignment value is not an array".to_string(),
        )),
    }
}
