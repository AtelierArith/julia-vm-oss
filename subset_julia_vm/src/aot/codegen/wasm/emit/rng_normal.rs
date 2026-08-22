use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use subset_julia_vm_bytecode::rng::{ZIGGURAT_NOR_INV_R, ZIGGURAT_NOR_R};
use wasm_encoder::{BlockType, Function, Instruction as W, MemArg, ValType};

use super::super::types::unsupported;
use super::locals::MathLocals;
use super::rng_tables::RngTables;
use super::transcendental_approx::{emit_exp_classified, emit_log};

pub(super) const RANDN_NAME: &str = "__sjulia_rng_randn";

pub(super) fn emit_normal(
    body: &mut Function,
    destination: &VarRef,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    body.instruction(&W::Call(functions[RANDN_NAME]));
    match destination.ty {
        StaticType::F64 => {}
        StaticType::F32 => {
            body.instruction(&W::F32DemoteF64);
        }
        ref ty => {
            return Err(unsupported(format!(
                "Wasm randn requires Float32 or Float64, got `{}`",
                ty.julia_type_name()
            )))
        }
    }
    Ok(())
}

pub(super) fn emit_randn(next: u32, tables: &RngTables) -> Function {
    let mut body = Function::new([(2, ValType::I64), (1, ValType::I32), (10, ValType::F64)]);
    let math = MathLocals {
        x: 7,
        y: 8,
        term: 9,
        sum: 10,
        factor: 11,
        exponent: 12,
        log_adjust: 4,
    };
    body.instruction(&W::Loop(BlockType::Empty));
    emit_candidate(&mut body, next, tables, 0, 1, 2, 3);
    emit_fast_accept(&mut body, tables, 1, 2, 3);
    emit_unlikely(&mut body, next, tables, [1, 2, 3, 4, 5, 6], &math);
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::Unreachable);
    body.instruction(&W::F64Const(0.0.into()));
    body.instruction(&W::End);
    body
}

fn emit_candidate(
    body: &mut Function,
    next: u32,
    tables: &RngTables,
    r: u32,
    rabs: u32,
    idx: u32,
    x: u32,
) {
    body.instruction(&W::Call(next));
    body.instruction(&W::I64Const(0x000f_ffff_ffff_ffff));
    body.instruction(&W::I64And);
    body.instruction(&W::LocalTee(r));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64ShrU);
    body.instruction(&W::LocalTee(rabs));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Const(255));
    body.instruction(&W::I32And);
    body.instruction(&W::LocalSet(idx));
    body.instruction(&W::LocalGet(rabs));
    body.instruction(&W::F64ConvertI64U);
    load_f64(body, tables.wi(), idx);
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(x));
    body.instruction(&W::LocalGet(r));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(x));
    body.instruction(&W::F64Neg);
    body.instruction(&W::LocalSet(x));
    body.instruction(&W::End);
}

fn emit_fast_accept(body: &mut Function, tables: &RngTables, rabs: u32, idx: u32, x: u32) {
    body.instruction(&W::LocalGet(rabs));
    load_i64(body, tables.ki(), idx);
    body.instruction(&W::I64LtU);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(x));
    body.instruction(&W::Return);
    body.instruction(&W::End);
}

fn emit_unlikely(
    body: &mut Function,
    next: u32,
    tables: &RngTables,
    locals: [u32; 6],
    math: &MathLocals,
) {
    let [rabs, idx, x, uniform, xx, yy] = locals;
    body.instruction(&W::LocalGet(idx));
    body.instruction(&W::I32Eqz);
    body.instruction(&W::If(BlockType::Empty));
    emit_tail(body, next, [rabs, uniform, xx, yy], math);
    body.instruction(&W::Else);
    emit_triangle(body, next, tables, [idx, x, uniform], math);
    body.instruction(&W::End);
}

fn emit_tail(body: &mut Function, next: u32, locals: [u32; 4], math: &MathLocals) {
    let [rabs, uniform, xx, yy] = locals;
    body.instruction(&W::Loop(BlockType::Empty));
    emit_uniform(body, next);
    body.instruction(&W::LocalSet(uniform));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalGet(uniform));
    body.instruction(&W::F64Sub);
    body.instruction(&W::LocalSet(math.x));
    emit_log(body, math);
    body.instruction(&W::F64Const((-ZIGGURAT_NOR_INV_R).into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(xx));
    emit_uniform(body, next);
    body.instruction(&W::LocalSet(uniform));
    body.instruction(&W::F64Const(1.0.into()));
    body.instruction(&W::LocalGet(uniform));
    body.instruction(&W::F64Sub);
    body.instruction(&W::LocalSet(math.x));
    emit_log(body, math);
    body.instruction(&W::F64Neg);
    body.instruction(&W::LocalSet(yy));
    body.instruction(&W::LocalGet(yy));
    body.instruction(&W::LocalGet(yy));
    body.instruction(&W::F64Add);
    body.instruction(&W::LocalGet(xx));
    body.instruction(&W::LocalGet(xx));
    body.instruction(&W::F64Mul);
    body.instruction(&W::F64Le);
    body.instruction(&W::BrIf(0));
    body.instruction(&W::F64Const(ZIGGURAT_NOR_R.into()));
    body.instruction(&W::LocalGet(xx));
    body.instruction(&W::F64Add);
    body.instruction(&W::LocalSet(xx));
    body.instruction(&W::LocalGet(rabs));
    body.instruction(&W::I64Const(8));
    body.instruction(&W::I64ShrU);
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(xx));
    body.instruction(&W::F64Neg);
    body.instruction(&W::LocalSet(xx));
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(xx));
    body.instruction(&W::Return);
    body.instruction(&W::End);
}

fn emit_triangle(
    body: &mut Function,
    next: u32,
    tables: &RngTables,
    locals: [u32; 3],
    math: &MathLocals,
) {
    let [idx, x, uniform] = locals;
    body.instruction(&W::LocalGet(idx));
    body.instruction(&W::I32Const(1));
    body.instruction(&W::I32Sub);
    body.instruction(&W::LocalSet(idx));
    load_f64(body, tables.fi(), idx);
    body.instruction(&W::LocalGet(idx));
    body.instruction(&W::I32Const(1));
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalSet(idx));
    load_f64(body, tables.fi(), idx);
    body.instruction(&W::F64Sub);
    emit_uniform(body, next);
    body.instruction(&W::LocalSet(uniform));
    body.instruction(&W::LocalGet(uniform));
    body.instruction(&W::F64Mul);
    load_f64(body, tables.fi(), idx);
    body.instruction(&W::F64Add);
    body.instruction(&W::LocalGet(x));
    body.instruction(&W::LocalGet(x));
    body.instruction(&W::F64Mul);
    body.instruction(&W::F64Const((-0.5).into()));
    body.instruction(&W::F64Mul);
    body.instruction(&W::LocalSet(math.x));
    emit_exp_classified(body, math);
    body.instruction(&W::F64Lt);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(x));
    body.instruction(&W::Return);
    body.instruction(&W::End);
}

fn emit_uniform(body: &mut Function, next: u32) {
    body.instruction(&W::Call(next));
    body.instruction(&W::I64Const(11));
    body.instruction(&W::I64ShrU);
    body.instruction(&W::F64ConvertI64U);
    body.instruction(&W::F64Const((1.0 / 9_007_199_254_740_992.0).into()));
    body.instruction(&W::F64Mul);
}

fn load_i64(body: &mut Function, base: i32, idx: u32) {
    address(body, base, idx);
    body.instruction(&W::I64Load(memarg()));
}

fn load_f64(body: &mut Function, base: i32, idx: u32) {
    address(body, base, idx);
    body.instruction(&W::F64Load(memarg()));
}

fn address(body: &mut Function, base: i32, idx: u32) {
    body.instruction(&W::I32Const(base));
    body.instruction(&W::LocalGet(idx));
    body.instruction(&W::I32Const(8));
    body.instruction(&W::I32Mul);
    body.instruction(&W::I32Add);
}

const fn memarg() -> MemArg {
    MemArg {
        offset: 0,
        align: 3,
        memory_index: 0,
    }
}
