use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::locals::MathLocals;
use super::ops::get;

const LN_2: f64 = std::f64::consts::LN_2;

pub(super) fn emit_exp_builtin(
    body: &mut Function,
    arg: &VarRef,
    locals: &HashMap<String, u32>,
    scratch: &MathLocals,
) -> AotResult<()> {
    get_as_f64(body, arg, locals)?;
    body.instruction(&W::LocalSet(scratch.x));
    emit_exp_classified(body, scratch);
    if arg.ty == StaticType::F32 {
        body.instruction(&W::F32DemoteF64);
    }
    Ok(())
}

pub(super) fn emit_log_builtin(
    body: &mut Function,
    arg: &VarRef,
    locals: &HashMap<String, u32>,
    scratch: &MathLocals,
) -> AotResult<()> {
    get_as_f64(body, arg, locals)?;
    body.instruction(&W::LocalSet(scratch.x));
    emit_log_classified(body, scratch);
    if arg.ty == StaticType::F32 {
        body.instruction(&W::F32DemoteF64);
    }
    Ok(())
}

pub(super) fn emit_exp_classified(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Ne);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(709.782712893384.into()));
    body.instruction(&W::F64Gt);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::F64Const(f64::INFINITY.into()));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const((-745.1332191019411).into()));
    body.instruction(&W::F64Lt);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::Else);
    emit_finite_exp(body, scratch);
    body.instruction(&W::End);
    body.instruction(&W::End);
    body.instruction(&W::End);
}

fn emit_finite_exp(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(f64::MIN_POSITIVE.ln().into()));
    body.instruction(&W::F64Lt);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const((600.0 * LN_2).into()));
    body.instruction(&W::F64Add);
    body.instruction(&W::LocalSet(scratch.x));
    emit_exp(body, scratch);
    body.instruction(&W::F64Const(f64::from_bits((1023 - 600) << 52).into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::Else);
    emit_exp(body, scratch);
    body.instruction(&W::End);
}

fn emit_log_classified(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Ne);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::F64Const(f64::NEG_INFINITY.into()));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(f64::INFINITY.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::F64Const(f64::INFINITY.into()));
    body.instruction(&W::Else);
    emit_log(body, scratch);
    body.instruction(&W::End);
    body.instruction(&W::End);
    body.instruction(&W::End);
}

pub(super) fn get_as_f64(
    body: &mut Function,
    value: &VarRef,
    locals: &HashMap<String, u32>,
) -> AotResult<()> {
    get(body, locals, value)?;
    if value.ty == StaticType::F32 {
        body.instruction(&W::F64PromoteF32);
    }
    Ok(())
}

pub(super) fn emit_log(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Le);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::LocalSet(scratch.log_adjust));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(f64::MIN_POSITIVE.into()));
    body.instruction(&W::F64Lt);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(4_503_599_627_370_496.0.into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::F64Const((-52.0).into()));
    body.instruction(&W::LocalSet(scratch.log_adjust));
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::I64ReinterpretF64);
    body.instruction(&W::I64Const(52));
    body.instruction(&W::I64ShrU);
    body.instruction(&W::I64Const(0x7ff));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Const(1023));
    body.instruction(&W::I64Sub);
    body.instruction(&W::F64ConvertI64S);
    body.instruction(&W::LocalGet(scratch.log_adjust));
    body.instruction(&W::F64Add);
    body.instruction(&W::F64Const(LN_2.into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(scratch.sum));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::I64ReinterpretF64);
    body.instruction(&W::I64Const(0x000f_ffff_ffff_ffff));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Const(0x3ff0_0000_0000_0000));
    body.instruction(&W::I64Or);
    body.instruction(&W::F64ReinterpretI64);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::F64Sub);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::F64Add);
    body.instruction(&W::F64Div);
    body.instruction(&W::LocalSet(scratch.y));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::LocalSet(scratch.term));
    for denominator in (1..=39).step_by(2) {
        body.instruction(&W::LocalGet(scratch.sum));
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::F64Const((2.0 / f64::from(denominator)).into()));
        body.instruction(&W::F64Mul);
        body.instruction(&W::F64Add);
        body.instruction(&W::LocalSet(scratch.sum));
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::LocalGet(scratch.y));
        body.instruction(&W::F64Mul);
        body.instruction(&W::LocalGet(scratch.y));
        body.instruction(&W::F64Mul);
        body.instruction(&W::LocalSet(scratch.term));
    }
    body.instruction(&W::LocalGet(scratch.sum));
}

pub(super) fn emit_exp(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(LN_2.into()));
    body.instruction(&W::F64Div);
    body.instruction(&W::F64Floor);
    body.instruction(&W::LocalSet(scratch.y));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::F64Const(LN_2.into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::F64Sub);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalSet(scratch.sum));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalSet(scratch.term));
    for divisor in 1..=20 {
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::LocalGet(scratch.x));
        body.instruction(&W::F64Mul);
        body.instruction(&W::F64Const(f64::from(divisor).into()));
        body.instruction(&W::F64Div);
        body.instruction(&W::LocalSet(scratch.term));
        body.instruction(&W::LocalGet(scratch.sum));
        body.instruction(&W::LocalGet(scratch.term));
        body.instruction(&W::F64Add);
        body.instruction(&W::LocalSet(scratch.sum));
    }
    body.instruction(&W::LocalGet(scratch.sum));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::I64TruncSatF64S);
    body.instruction(&W::I64Const(1023));
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(52));
    body.instruction(&W::I64Shl);
    body.instruction(&W::F64ReinterpretI64);
    body.instruction(&W::F64Mul);
}
