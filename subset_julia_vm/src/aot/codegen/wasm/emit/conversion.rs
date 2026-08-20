use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::unsupported;
use super::ops::{emit_conversion, normalize_u8};

fn emit_trap_if(body: &mut Function) {
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
}

fn emit_float_exactness_check(body: &mut Function, source: &VarRef, local: u32) -> AotResult<()> {
    body.instruction(&W::LocalGet(local));
    body.instruction(&W::LocalGet(local));
    match &source.ty {
        StaticType::F32 => {
            body.instruction(&W::F32Trunc);
            body.instruction(&W::F32Ne);
        }
        StaticType::F64 => {
            body.instruction(&W::F64Trunc);
            body.instruction(&W::F64Ne);
        }
        other => {
            return Err(unsupported(format!(
                "Wasm AoT cannot check exactness for `{}`",
                other.julia_type_name()
            )))
        }
    }
    emit_trap_if(body);
    Ok(())
}

fn emit_checked_bool_conversion(body: &mut Function, source: &VarRef, local: u32) -> AotResult<()> {
    body.instruction(&W::LocalGet(local));
    match &source.ty {
        StaticType::I64 => {
            body.instruction(&W::I64Eqz);
        }
        StaticType::F32 => {
            body.instruction(&W::F32Const(0.0.into()));
            body.instruction(&W::F32Eq);
        }
        StaticType::F64 => {
            body.instruction(&W::F64Const(0.0.into()));
            body.instruction(&W::F64Eq);
        }
        StaticType::I32 | StaticType::U8 | StaticType::Bool => {
            body.instruction(&W::I32Eqz);
        }
        other => {
            return Err(unsupported(format!(
                "Wasm AoT cannot convert `{}` to `Bool`",
                other.julia_type_name()
            )))
        }
    }
    body.instruction(&W::LocalGet(local));
    match &source.ty {
        StaticType::I64 => {
            body.instruction(&W::I64Const(1));
            body.instruction(&W::I64Eq);
        }
        StaticType::F32 => {
            body.instruction(&W::F32Const(1.0.into()));
            body.instruction(&W::F32Eq);
        }
        StaticType::F64 => {
            body.instruction(&W::F64Const(1.0.into()));
            body.instruction(&W::F64Eq);
        }
        StaticType::I32 | StaticType::U8 | StaticType::Bool => {
            body.instruction(&W::I32Const(1));
            body.instruction(&W::I32Eq);
        }
        other => {
            return Err(unsupported(format!(
                "Wasm AoT cannot convert `{}` to `Bool`",
                other.julia_type_name()
            )))
        }
    }
    body.instruction(&W::I32Or);
    body.instruction(&W::I32Eqz);
    emit_trap_if(body);
    body.instruction(&W::LocalGet(local));
    match &source.ty {
        StaticType::I64 => {
            body.instruction(&W::I32WrapI64);
        }
        StaticType::F32 => {
            body.instruction(&W::I32TruncF32U);
        }
        StaticType::F64 => {
            body.instruction(&W::I32TruncF64U);
        }
        StaticType::I32 | StaticType::U8 | StaticType::Bool => {}
        _ => {}
    }
    Ok(())
}

fn emit_u8_range_check(body: &mut Function, source: &VarRef, local: u32) -> AotResult<()> {
    body.instruction(&W::LocalGet(local));
    match &source.ty {
        StaticType::F32 => {
            body.instruction(&W::F32Const(0.0.into()));
        }
        StaticType::F64 => {
            body.instruction(&W::F64Const(0.0.into()));
        }
        other => {
            return Err(unsupported(format!(
                "Wasm AoT cannot range-check `{}` for `UInt8`",
                other.julia_type_name()
            )))
        }
    }
    match &source.ty {
        StaticType::F32 => {
            body.instruction(&W::F32Lt);
        }
        StaticType::F64 => {
            body.instruction(&W::F64Lt);
        }
        _ => {}
    }
    body.instruction(&W::LocalGet(local));
    match &source.ty {
        StaticType::F32 => {
            body.instruction(&W::F32Const(255.0.into()));
        }
        StaticType::F64 => {
            body.instruction(&W::F64Const(255.0.into()));
        }
        _ => {}
    }
    match &source.ty {
        StaticType::F32 => {
            body.instruction(&W::F32Gt);
        }
        StaticType::F64 => {
            body.instruction(&W::F64Gt);
        }
        _ => {}
    }
    body.instruction(&W::I32Or);
    emit_trap_if(body);
    Ok(())
}

pub(super) fn emit_checked_conversion(
    body: &mut Function,
    source: &VarRef,
    target: &StaticType,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    let local = *locals
        .get(&source.name)
        .ok_or_else(|| AotError::InvalidIR(format!("unknown Wasm local `{}`", source.name)))?;
    if &source.ty == target {
        body.instruction(&W::LocalGet(local));
        return Ok(());
    }
    if *target == StaticType::Bool {
        return emit_checked_bool_conversion(body, source, local);
    }
    if matches!(source.ty, StaticType::F32 | StaticType::F64)
        && matches!(target, StaticType::I32 | StaticType::I64 | StaticType::U8)
    {
        emit_float_exactness_check(body, source, local)?;
        if *target == StaticType::U8 {
            emit_u8_range_check(body, source, local)?;
        }
    }
    body.instruction(&W::LocalGet(local));
    match (&source.ty, target) {
        (StaticType::F32, StaticType::F64) => {
            body.instruction(&W::F64PromoteF32);
        }
        (StaticType::F64, StaticType::F32) => {
            body.instruction(&W::F32DemoteF64);
        }
        (StaticType::F32, StaticType::I32) => {
            body.instruction(&W::I32TruncF32S);
        }
        (StaticType::F64, StaticType::I32) => {
            body.instruction(&W::I32TruncF64S);
        }
        (StaticType::F32, StaticType::I64) => {
            body.instruction(&W::I64TruncF32S);
        }
        (StaticType::F64, StaticType::I64) => {
            body.instruction(&W::I64TruncF64S);
        }
        (StaticType::F32, StaticType::U8) => {
            body.instruction(&W::I32TruncF32U);
            normalize_u8(body);
        }
        (StaticType::F64, StaticType::U8) => {
            body.instruction(&W::I32TruncF64U);
            normalize_u8(body);
        }
        (StaticType::I32, StaticType::F32) => {
            body.instruction(&W::F32ConvertI32S);
        }
        (StaticType::I64, StaticType::F32) => {
            body.instruction(&W::F32ConvertI64S);
        }
        (StaticType::U8 | StaticType::Bool, StaticType::F32) => {
            if source.ty == StaticType::U8 {
                normalize_u8(body);
            }
            body.instruction(&W::F32ConvertI32U);
        }
        (StaticType::I32, StaticType::F64) => {
            body.instruction(&W::F64ConvertI32S);
        }
        (StaticType::I64, StaticType::F64) => {
            body.instruction(&W::F64ConvertI64S);
        }
        (StaticType::U8 | StaticType::Bool, StaticType::F64) => {
            if source.ty == StaticType::U8 {
                normalize_u8(body);
            }
            body.instruction(&W::F64ConvertI32U);
        }
        _ => return emit_conversion(body, &source.ty, target),
    }
    Ok(())
}
