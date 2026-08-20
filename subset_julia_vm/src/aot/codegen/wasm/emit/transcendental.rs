use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::unsupported;
use super::locals::MathLocals;
use super::transcendental_approx::{emit_exp_classified, emit_log, get_as_f64};

pub(super) fn emit_pow(
    body: &mut Function,
    base: &VarRef,
    exponent: &VarRef,
    locals: &HashMap<String, u32>,
    scratch: &MathLocals,
) -> AotResult<()> {
    if base.ty != exponent.ty || !matches!(base.ty, StaticType::F32 | StaticType::F64) {
        return Err(unsupported(
            "Wasm power requires homogeneous Float32 or Float64 arguments",
        ));
    }
    get_as_f64(body, base, locals)?;
    body.instruction(&W::LocalSet(scratch.x));
    get_as_f64(body, exponent, locals)?;
    body.instruction(&W::LocalSet(scratch.y));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::LocalSet(scratch.exponent));
    emit_pow_value(body, scratch);
    if base.ty == StaticType::F32 {
        body.instruction(&W::F32DemoteF64);
    }
    Ok(())
}

fn emit_pow_value(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalSet(scratch.factor));
    emit_negative_odd_factor(body, scratch);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Ne);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::Else);
    emit_non_nan_pow(body, scratch);
    body.instruction(&W::End);
    body.instruction(&W::End);
    body.instruction(&W::End);
}

fn emit_non_nan_pow(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Abs);
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    emit_zero_or_infinity(body, scratch, false);
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Abs);
    body.instruction(&W::F64Const(f64::INFINITY.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    emit_zero_or_infinity(body, scratch, true);
    body.instruction(&W::Else);
    emit_finite_pow(body, scratch);
    body.instruction(&W::End);
    body.instruction(&W::End);
}

fn emit_zero_or_infinity(body: &mut Function, scratch: &MathLocals, base_is_infinite: bool) {
    if base_is_infinite {
        body.instruction(&W::LocalGet(scratch.x));
        body.instruction(&W::I64ReinterpretF64);
        body.instruction(&W::I64Const(0));
        body.instruction(&W::I64LtS);
        body.instruction(&W::LocalGet(scratch.exponent));
        body.instruction(&W::LocalGet(scratch.exponent));
        body.instruction(&W::F64Trunc);
        body.instruction(&W::F64Ne);
        body.instruction(&W::I32And);
        body.instruction(&W::If(BlockType::Empty));
        body.instruction(&W::Unreachable);
        body.instruction(&W::End);
    }
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Lt);
    if base_is_infinite {
        body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
        body.instruction(&W::F64Const(0.0.into()));
        body.instruction(&W::Else);
        body.instruction(&W::F64Const(f64::INFINITY.into()));
    } else {
        body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
        body.instruction(&W::F64Const(f64::INFINITY.into()));
        body.instruction(&W::Else);
        body.instruction(&W::F64Const(0.0.into()));
    }
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(scratch.factor));
    if base_is_infinite {
        body.instruction(&W::F64Const(1.0.into()));
        body.instruction(&W::LocalGet(scratch.exponent));
        body.instruction(&W::F64Const(0.0.into()));
        body.instruction(&W::F64Gt);
        body.instruction(&W::Select);
    }
    body.instruction(&W::F64Mul);
}

fn emit_negative_odd_factor(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::I64ReinterpretF64);
    body.instruction(&W::I64Const(0));
    body.instruction(&W::I64LtS);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Trunc);
    body.instruction(&W::F64Eq);
    body.instruction(&W::I32And);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Const(2.0.into()));
    body.instruction(&W::F64Div);
    body.instruction(&W::F64Trunc);
    body.instruction(&W::F64Const(2.0.into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Ne);
    body.instruction(&W::I32And);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::F64Const((-1.0).into()));
    body.instruction(&W::LocalSet(scratch.factor));
    body.instruction(&W::End);
}

fn emit_finite_pow(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Lt);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::LocalGet(scratch.y));
    body.instruction(&W::F64Trunc);
    body.instruction(&W::F64Ne);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Abs);
    body.instruction(&W::LocalSet(scratch.x));
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Abs);
    body.instruction(&W::F64Const(f64::INFINITY.into()));
    body.instruction(&W::F64Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    emit_infinite_exponent_pow(body, scratch);
    body.instruction(&W::Else);
    emit_finite_exponent_pow(body, scratch);
    body.instruction(&W::End);
}

fn emit_infinite_exponent_pow(body: &mut Function, scratch: &MathLocals) {
    body.instruction(&W::LocalGet(scratch.x));
    body.instruction(&W::F64Abs);
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::F64Gt);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::F64Gt);
    body.instruction(&W::I32Eq);
    body.instruction(&W::If(BlockType::Result(wasm_encoder::ValType::F64)));
    body.instruction(&W::F64Const(f64::INFINITY.into()));
    body.instruction(&W::Else);
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::End);
}

fn emit_finite_exponent_pow(body: &mut Function, scratch: &MathLocals) {
    emit_log(body, scratch);
    body.instruction(&W::LocalGet(scratch.exponent));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(scratch.x));
    emit_exp_classified(body, scratch);
    body.instruction(&W::LocalGet(scratch.factor));
    body.instruction(&W::F64Mul);
}
