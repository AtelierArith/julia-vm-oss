use std::collections::HashMap;

use crate::aot::ir::Instruction;
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{Function, Instruction as W};

use super::super::types::unsupported;
use super::locals::LocalLayout;
use super::ops::{get, set};

pub(super) fn emit_array_load(body: &mut Function, ty: &StaticType) -> AotResult<()> {
    let instruction = match ty {
        StaticType::U8 | StaticType::Bool => W::I32Load8U(super::memory::memarg(0)),
        StaticType::I32 => W::I32Load(super::memory::memarg(0)),
        StaticType::I64 => W::I64Load(super::memory::memarg(0)),
        StaticType::F32 => W::F32Load(super::memory::memarg(0)),
        StaticType::F64 => W::F64Load(super::memory::memarg(0)),
        other => {
            return Err(unsupported(format!(
                "unsupported Wasm array load `{other}`"
            )))
        }
    };
    body.instruction(&instruction);
    Ok(())
}

pub(super) fn emit_array_store(body: &mut Function, ty: &StaticType) -> AotResult<()> {
    let instruction = match ty {
        StaticType::U8 | StaticType::Bool => W::I32Store8(super::memory::memarg(0)),
        StaticType::I32 => W::I32Store(super::memory::memarg(0)),
        StaticType::I64 => W::I64Store(super::memory::memarg(0)),
        StaticType::F32 => W::F32Store(super::memory::memarg(0)),
        StaticType::F64 => W::F64Store(super::memory::memarg(0)),
        other => {
            return Err(unsupported(format!(
                "unsupported Wasm array store `{other}`"
            )))
        }
    };
    body.instruction(&instruction);
    Ok(())
}

pub(super) fn normalize_bool(body: &mut Function) {
    body.instruction(&W::I32Eqz);
    body.instruction(&W::I32Eqz);
}

pub(super) fn emit(
    body: &mut Function,
    instruction: &Instruction,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    match instruction {
        Instruction::ArraySlice { .. } => {
            super::array_slice::emit(body, instruction, layout, functions)
        }
        Instruction::ArraySliceAssign { .. } => {
            super::array_slice_assign::emit(body, instruction, layout, functions)
        }
        Instruction::UnitRangeLength { dest, start, stop } => {
            let locals = &layout.locals;
            get(body, locals, stop)?;
            get(body, locals, start)?;
            body.instruction(&W::I64LtS);
            body.instruction(&W::If(wasm_encoder::BlockType::Result(
                wasm_encoder::ValType::I64,
            )));
            body.instruction(&W::I64Const(0));
            body.instruction(&W::Else);
            get(body, locals, stop)?;
            get(body, locals, start)?;
            body.instruction(&W::I64Sub);
            body.instruction(&W::I64Const(1));
            body.instruction(&W::I64Add);
            body.instruction(&W::End);
            set(body, locals, dest)
        }
        _ => Err(unsupported("non-slice instruction routed to slice emitter")),
    }
}
