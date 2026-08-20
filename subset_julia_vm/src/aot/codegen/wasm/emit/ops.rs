use std::collections::HashMap;

use crate::aot::ir::{BinOpKind, ConstValue, UnaryOpKind, VarRef};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};
use wasm_encoder::{Function, Instruction as W, ValType};

use super::super::types::{unsupported, value_type};

pub(super) fn required_type(ty: &StaticType) -> AotResult<ValType> {
    value_type(ty)?.ok_or_else(|| unsupported("Nothing cannot be stored in a Wasm local"))
}

pub(super) fn get(
    body: &mut Function,
    locals: &HashMap<String, u32>,
    var: &VarRef,
) -> AotResult<()> {
    let index = locals
        .get(&var.name)
        .ok_or_else(|| AotError::InvalidIR(format!("unknown Wasm local `{}`", var.name)))?;
    body.instruction(&W::LocalGet(*index));
    Ok(())
}

pub(super) fn set(
    body: &mut Function,
    locals: &HashMap<String, u32>,
    var: &VarRef,
) -> AotResult<()> {
    let index = locals
        .get(&var.name)
        .ok_or_else(|| AotError::InvalidIR(format!("unknown Wasm local `{}`", var.name)))?;
    body.instruction(&W::LocalSet(*index));
    Ok(())
}

pub(super) fn emit_const(
    body: &mut Function,
    value: &ConstValue,
    strings: &super::strings::StaticStrings,
) -> AotResult<()> {
    match value {
        ConstValue::Int64(value) => body.instruction(&W::I64Const(*value)),
        ConstValue::Int32(value) => body.instruction(&W::I32Const(*value)),
        ConstValue::Float32(value) => body.instruction(&W::F32Const((*value).into())),
        ConstValue::Float64(value) => body.instruction(&W::F64Const((*value).into())),
        ConstValue::Bool(value) => body.instruction(&W::I32Const(i32::from(*value))),
        ConstValue::String(value) => body.instruction(&W::I32Const(strings.descriptor(value)?)),
        other => {
            return Err(unsupported(format!(
                "Wasm AoT cannot emit constant `{other:?}`"
            )))
        }
    };
    Ok(())
}

pub(super) fn emit_conversion(
    body: &mut Function,
    source: &StaticType,
    target: &StaticType,
) -> AotResult<()> {
    if *target == StaticType::U8 {
        if *source == StaticType::I64 {
            body.instruction(&W::I32WrapI64);
        } else if !matches!(source, StaticType::I32 | StaticType::U8) {
            return Err(unsupported(format!(
                "Wasm AoT cannot convert `{}` to `{}`",
                source.julia_type_name(),
                target.julia_type_name()
            )));
        }
        normalize_u8(body);
        return Ok(());
    }
    if source == target {
        return Ok(());
    }
    match (source, target) {
        (StaticType::I64, StaticType::I32) => body.instruction(&W::I32WrapI64),
        (StaticType::I32, StaticType::I64) => body.instruction(&W::I64ExtendI32S),
        (StaticType::U8, StaticType::I64) => {
            normalize_u8(body);
            body.instruction(&W::I64ExtendI32U)
        }
        _ => {
            return Err(unsupported(format!(
                "Wasm AoT cannot convert `{}` to `{}`",
                source.julia_type_name(),
                target.julia_type_name()
            )))
        }
    };
    Ok(())
}

pub(super) fn normalize_u8(body: &mut Function) {
    body.instruction(&W::I32Const(0xff));
    body.instruction(&W::I32And);
}

pub(super) fn emit_binop(body: &mut Function, op: BinOpKind, ty: &StaticType) -> AotResult<()> {
    let instruction = match (ty, op) {
        (StaticType::I64, BinOpKind::Add) => W::I64Add,
        (StaticType::I64, BinOpKind::Sub) => W::I64Sub,
        (StaticType::I64, BinOpKind::Mul) => W::I64Mul,
        (StaticType::I64, BinOpKind::Div) => W::I64DivS,
        (StaticType::I64, BinOpKind::Rem) => W::I64RemS,
        (StaticType::I64, BinOpKind::Shl) => W::I64Shl,
        (StaticType::I64, BinOpKind::Shr) => W::I64ShrS,
        (StaticType::I64, BinOpKind::BitAnd) => W::I64And,
        (StaticType::I64, BinOpKind::BitOr) => W::I64Or,
        (StaticType::I64, BinOpKind::BitXor) => W::I64Xor,
        (StaticType::I64, BinOpKind::Eq) => W::I64Eq,
        (StaticType::I64, BinOpKind::Ne) => W::I64Ne,
        (StaticType::I64, BinOpKind::Lt) => W::I64LtS,
        (StaticType::I64, BinOpKind::Le) => W::I64LeS,
        (StaticType::I64, BinOpKind::Gt) => W::I64GtS,
        (StaticType::I64, BinOpKind::Ge) => W::I64GeS,
        (StaticType::F64, BinOpKind::Add) => W::F64Add,
        (StaticType::F64, BinOpKind::Sub) => W::F64Sub,
        (StaticType::F64, BinOpKind::Mul) => W::F64Mul,
        (StaticType::F64, BinOpKind::Div) => W::F64Div,
        (StaticType::F64, BinOpKind::Eq) => W::F64Eq,
        (StaticType::F64, BinOpKind::Ne) => W::F64Ne,
        (StaticType::F64, BinOpKind::Lt) => W::F64Lt,
        (StaticType::F64, BinOpKind::Le) => W::F64Le,
        (StaticType::F64, BinOpKind::Gt) => W::F64Gt,
        (StaticType::F64, BinOpKind::Ge) => W::F64Ge,
        (StaticType::F32, BinOpKind::Add) => W::F32Add,
        (StaticType::F32, BinOpKind::Sub) => W::F32Sub,
        (StaticType::F32, BinOpKind::Mul) => W::F32Mul,
        (StaticType::F32, BinOpKind::Div) => W::F32Div,
        (StaticType::F32, BinOpKind::Eq) => W::F32Eq,
        (StaticType::F32, BinOpKind::Ne) => W::F32Ne,
        (StaticType::F32, BinOpKind::Lt) => W::F32Lt,
        (StaticType::F32, BinOpKind::Le) => W::F32Le,
        (StaticType::F32, BinOpKind::Gt) => W::F32Gt,
        (StaticType::F32, BinOpKind::Ge) => W::F32Ge,
        (StaticType::Bool, BinOpKind::And) => W::I32And,
        (StaticType::Bool, BinOpKind::Or) => W::I32Or,
        (StaticType::I32 | StaticType::U8, BinOpKind::Add) => W::I32Add,
        (StaticType::I32 | StaticType::U8, BinOpKind::Sub) => W::I32Sub,
        (StaticType::I32 | StaticType::U8, BinOpKind::Mul) => W::I32Mul,
        (StaticType::I32, BinOpKind::Div) => W::I32DivS,
        (StaticType::U8, BinOpKind::Div) => W::I32DivU,
        (StaticType::I32, BinOpKind::Rem) => W::I32RemS,
        (StaticType::U8, BinOpKind::Rem) => W::I32RemU,
        (StaticType::I32 | StaticType::U8, BinOpKind::Shl) => W::I32Shl,
        (StaticType::I32, BinOpKind::Shr) => W::I32ShrS,
        (StaticType::U8, BinOpKind::Shr) => W::I32ShrU,
        (StaticType::I32 | StaticType::U8, BinOpKind::BitAnd) => W::I32And,
        (StaticType::I32 | StaticType::U8, BinOpKind::BitOr) => W::I32Or,
        (StaticType::I32 | StaticType::U8, BinOpKind::BitXor) => W::I32Xor,
        (StaticType::I32 | StaticType::U8, BinOpKind::Eq) => W::I32Eq,
        (StaticType::I32 | StaticType::U8, BinOpKind::Ne) => W::I32Ne,
        (StaticType::I32, BinOpKind::Lt) => W::I32LtS,
        (StaticType::I32, BinOpKind::Le) => W::I32LeS,
        (StaticType::I32, BinOpKind::Gt) => W::I32GtS,
        (StaticType::I32, BinOpKind::Ge) => W::I32GeS,
        (StaticType::U8, BinOpKind::Lt) => W::I32LtU,
        (StaticType::U8, BinOpKind::Le) => W::I32LeU,
        (StaticType::U8, BinOpKind::Gt) => W::I32GtU,
        (StaticType::U8, BinOpKind::Ge) => W::I32GeU,
        _ => {
            return Err(unsupported(format!(
                "Wasm AoT cannot emit `{op:?}` for `{}`",
                ty.julia_type_name()
            )))
        }
    };
    body.instruction(&instruction);
    Ok(())
}

pub(super) fn emit_unary(
    body: &mut Function,
    op: UnaryOpKind,
    operand: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    match (&operand.ty, op) {
        (StaticType::I64, UnaryOpKind::Neg) => {
            body.instruction(&W::I64Const(0));
            get(body, locals, operand)?;
            body.instruction(&W::I64Sub);
        }
        (StaticType::F64, UnaryOpKind::Neg) => {
            get(body, locals, operand)?;
            body.instruction(&W::F64Neg);
        }
        (StaticType::F32, UnaryOpKind::Neg) => {
            get(body, locals, operand)?;
            body.instruction(&W::F32Neg);
        }
        (StaticType::Bool, UnaryOpKind::Not) => {
            get(body, locals, operand)?;
            body.instruction(&W::I32Eqz);
        }
        (StaticType::I64, UnaryOpKind::BitNot) => {
            get(body, locals, operand)?;
            body.instruction(&W::I64Const(-1));
            body.instruction(&W::I64Xor);
        }
        (StaticType::I32 | StaticType::U8, UnaryOpKind::BitNot) => {
            get(body, locals, operand)?;
            body.instruction(&W::I32Const(-1));
            body.instruction(&W::I32Xor);
            if operand.ty == StaticType::U8 {
                normalize_u8(body);
            }
        }
        _ => {
            return Err(unsupported(format!(
                "Wasm AoT cannot emit unary `{op:?}` for `{}`",
                operand.ty.julia_type_name()
            )))
        }
    }
    Ok(())
}
