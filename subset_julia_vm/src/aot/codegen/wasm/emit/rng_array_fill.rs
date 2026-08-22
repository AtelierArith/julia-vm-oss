use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::{descriptor_layout, unsupported, DESCRIPTOR_ELEMENT_COUNT_OFFSET};
use super::descriptor::{emit_i64_load, trap_if};
use super::locals::LocalLayout;
use super::memory::memarg;
use super::ops::get;
use super::rng::NEXT_NAME;

const RANDN_NAME: &str = "__sjulia_rng_randn";

pub(super) fn emit_fill_uniform(
    body: &mut Function,
    array: &VarRef,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    emit_fill(body, array, layout, functions, false)
}

pub(super) fn emit_fill_normal(
    body: &mut Function,
    array: &VarRef,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    emit_fill(body, array, layout, functions, true)
}

fn emit_fill(
    body: &mut Function,
    array: &VarRef,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
    normal: bool,
) -> AotResult<()> {
    let descriptor = descriptor_layout(&array.ty)?;

    get(body, &layout.locals, array)?;
    body.instruction(&W::I32Const(24));
    body.instruction(&W::I32Add);
    body.instruction(&W::I32Load(memarg(0)));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalSet(layout.memory.data_start));

    emit_i64_load(body, array, &layout.locals, DESCRIPTOR_ELEMENT_COUNT_OFFSET)?;
    body.instruction(&W::LocalSet(layout.memory.product));

    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64Const(i64::from(descriptor.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalGet(layout.memory.data_start));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(layout.memory.data_end));

    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.term));
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));

    if normal {
        body.instruction(&W::Call(functions[RANDN_NAME]));
    } else {
        body.instruction(&W::Call(functions[NEXT_NAME]));
        body.instruction(&W::I64Const(11));
        body.instruction(&W::I64ShrU);
        body.instruction(&W::F64ConvertI64U);
        body.instruction(&W::F64Const((1.0 / 9_007_199_254_740_992.0).into()));
        body.instruction(&W::F64Mul);
    }
    body.instruction(&W::LocalSet(layout.math.x));

    emit_store_address(body, layout, i64::from(descriptor.element_size));
    body.instruction(&W::LocalGet(layout.math.x));
    match &array.ty {
        StaticType::Array { element, .. } => match element.as_ref() {
            StaticType::F32 => {
                body.instruction(&W::F32DemoteF64);
                body.instruction(&W::F32Store(memarg(0)));
            }
            StaticType::F64 => {
                body.instruction(&W::F64Store(memarg(0)));
            }
            _ => {
                return Err(unsupported(
                    "Array RNG requires Float32 or Float64 elements",
                ))
            }
        },
        _ => return Err(unsupported("Array RNG requires array type")),
    }

    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(layout.memory.term));
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);
    Ok(())
}

fn emit_store_address(body: &mut Function, layout: &LocalLayout, element_size: i64) {
    body.instruction(&W::LocalGet(layout.memory.data_start));
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::I64Const(element_size));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalTee(layout.memory.max_offset));
    body.instruction(&W::LocalGet(layout.memory.data_start));
    trap_if(body, W::I64LtU);
    body.instruction(&W::LocalGet(layout.memory.max_offset));
    body.instruction(&W::LocalGet(layout.memory.data_end));
    trap_if(body, W::I64GeU);
    body.instruction(&W::LocalGet(layout.memory.data_end));
    body.instruction(&W::LocalGet(layout.memory.max_offset));
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64Const(element_size));
    trap_if(body, W::I64LtU);
    body.instruction(&W::LocalGet(layout.memory.max_offset));
    body.instruction(&W::I32WrapI64);
}
