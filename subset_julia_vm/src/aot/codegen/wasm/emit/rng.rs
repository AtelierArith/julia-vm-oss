use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{Function, GlobalType, Instruction as W, ValType};

use super::super::types::unsupported;

pub(super) const NEXT_NAME: &str = "__sjulia_rng_next";

pub(super) const fn state_global_type() -> GlobalType {
    GlobalType {
        val_type: ValType::I64,
        mutable: true,
        shared: false,
    }
}

pub(super) fn emit_next(state: [u32; 4]) -> Function {
    let mut body = Function::new([(2, ValType::I64)]);
    body.instruction(&W::GlobalGet(state[0]));
    body.instruction(&W::GlobalGet(state[3]));
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(23));
    body.instruction(&W::I64Rotl);
    body.instruction(&W::GlobalGet(state[0]));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(0));
    body.instruction(&W::GlobalGet(state[1]));
    body.instruction(&W::I64Const(17));
    body.instruction(&W::I64Shl);
    body.instruction(&W::LocalSet(1));
    xor_global(&mut body, state[2], state[0]);
    xor_global(&mut body, state[3], state[1]);
    xor_global(&mut body, state[1], state[2]);
    xor_global(&mut body, state[0], state[3]);
    body.instruction(&W::GlobalGet(state[2]));
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I64Xor);
    body.instruction(&W::GlobalSet(state[2]));
    body.instruction(&W::GlobalGet(state[3]));
    body.instruction(&W::I64Const(45));
    body.instruction(&W::I64Rotl);
    body.instruction(&W::GlobalSet(state[3]));
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::End);
    body
}

fn xor_global(body: &mut Function, destination: u32, source: u32) {
    body.instruction(&W::GlobalGet(destination));
    body.instruction(&W::GlobalGet(source));
    body.instruction(&W::I64Xor);
    body.instruction(&W::GlobalSet(destination));
}

pub(super) fn emit_uniform(
    body: &mut Function,
    destination: &VarRef,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    body.instruction(&W::Call(functions[NEXT_NAME]));
    body.instruction(&W::I64Const(11));
    body.instruction(&W::I64ShrU);
    body.instruction(&W::F64ConvertI64U);
    body.instruction(&W::F64Const((1.0 / 9_007_199_254_740_992.0).into()));
    body.instruction(&W::F64Mul);
    match destination.ty {
        StaticType::F64 => {}
        StaticType::F32 => {
            body.instruction(&W::F32DemoteF64);
        }
        ref ty => {
            return Err(unsupported(format!(
                "Wasm rand requires Float32 or Float64, got `{}`",
                ty.julia_type_name()
            )))
        }
    }
    Ok(())
}
