use std::collections::HashMap;

use crate::aot::ir::{Instruction, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::unsupported;
use super::allocator::ALLOC_NAME;
use super::memory::memarg;
use super::ops::{get, set};

pub(super) fn emit_new(
    body: &mut Function,
    instruction: &Instruction,
    locals: &HashMap<String, u32>,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let Instruction::StructNew {
        dest,
        layout_id,
        size,
        align,
        fields,
    } = instruction
    else {
        return Err(unsupported("expected aggregate construction instruction"));
    };
    body.instruction(&W::I64Const(i64::from(*size) + 4));
    body.instruction(&W::I32Const(i32::from((*align).max(4))));
    body.instruction(&W::Call(functions[ALLOC_NAME]));
    body.instruction(&W::LocalTee(locals[&dest.name]));
    body.instruction(&W::I32Eqz);
    trap_on_stack(body);
    get(body, locals, dest)?;
    body.instruction(&W::I32Const(layout_i32(*layout_id)?));
    body.instruction(&W::I32Store(memarg(0)));
    for field in fields {
        get(body, locals, dest)?;
        get(body, locals, &field.value)?;
        emit_store(body, &field.value.ty, field_offset(field.offset)?)?;
    }
    Ok(())
}

pub(super) fn emit_get(
    body: &mut Function,
    dest: &VarRef,
    object: &VarRef,
    layout_id: u32,
    offset: i32,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    validate_handle(body, object, layout_id, offset, &dest.ty, locals)?;
    emit_load(body, &dest.ty, field_offset(offset)?)?;
    set(body, locals, dest)
}

fn validate_handle(
    body: &mut Function,
    object: &VarRef,
    layout_id: u32,
    offset: i32,
    field_ty: &StaticType,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    get(body, locals, object)?;
    body.instruction(&W::I32Const(3));
    body.instruction(&W::I32And);
    body.instruction(&W::I32Eqz);
    body.instruction(&W::I32Eqz);
    trap_on_stack(body);
    get(body, locals, object)?;
    body.instruction(&W::I32Load(memarg(0)));
    body.instruction(&W::I32Const(layout_i32(layout_id)?));
    body.instruction(&W::I32Ne);
    trap_on_stack(body);
    get(body, locals, object)?;
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Const(
        i64::from(offset) + 4 + i64::from(field_size(field_ty)?),
    ));
    body.instruction(&W::I64Add);
    body.instruction(&W::MemorySize(0));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Const(65_536));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I64GtU);
    trap_on_stack(body);
    get(body, locals, object)?;
    Ok(())
}

fn trap_on_stack(body: &mut Function) {
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
}

fn field_offset(offset: i32) -> AotResult<u64> {
    u64::try_from(offset + 4).map_err(|_| unsupported("Wasm aggregate field offset is negative"))
}

fn layout_i32(layout_id: u32) -> AotResult<i32> {
    i32::try_from(layout_id)
        .map_err(|_| unsupported("Wasm aggregate layout ID exceeds signed instruction range"))
}

fn field_size(ty: &StaticType) -> AotResult<u32> {
    Ok(match ty {
        StaticType::Bool | StaticType::I8 | StaticType::U8 => 1,
        StaticType::I16 | StaticType::U16 | StaticType::F16 => 2,
        StaticType::I32
        | StaticType::U32
        | StaticType::F32
        | StaticType::Char
        | StaticType::Tuple(_)
        | StaticType::NamedTuple(_)
        | StaticType::Struct { .. } => 4,
        StaticType::I64 | StaticType::U64 | StaticType::F64 => 8,
        other => {
            return Err(unsupported(format!(
                "unsupported aggregate field `{other}`"
            )))
        }
    })
}

fn emit_load(body: &mut Function, ty: &StaticType, offset: u64) -> AotResult<()> {
    match ty {
        StaticType::Bool | StaticType::U8 => body.instruction(&W::I32Load8U(memarg(offset))),
        StaticType::I32
        | StaticType::Tuple(_)
        | StaticType::NamedTuple(_)
        | StaticType::Struct { .. } => body.instruction(&W::I32Load(memarg(offset))),
        StaticType::F32 => body.instruction(&W::F32Load(memarg(offset))),
        StaticType::I64 => body.instruction(&W::I64Load(memarg(offset))),
        StaticType::F64 => body.instruction(&W::F64Load(memarg(offset))),
        other => {
            return Err(unsupported(format!(
                "unsupported aggregate field `{other}`"
            )))
        }
    };
    Ok(())
}

fn emit_store(body: &mut Function, ty: &StaticType, offset: u64) -> AotResult<()> {
    match ty {
        StaticType::Bool | StaticType::U8 => body.instruction(&W::I32Store8(memarg(offset))),
        StaticType::I32
        | StaticType::Tuple(_)
        | StaticType::NamedTuple(_)
        | StaticType::Struct { .. } => body.instruction(&W::I32Store(memarg(offset))),
        StaticType::F32 => body.instruction(&W::F32Store(memarg(offset))),
        StaticType::I64 => body.instruction(&W::I64Store(memarg(offset))),
        StaticType::F64 => body.instruction(&W::F64Store(memarg(offset))),
        other => {
            return Err(unsupported(format!(
                "unsupported aggregate field `{other}`"
            )))
        }
    };
    Ok(())
}
