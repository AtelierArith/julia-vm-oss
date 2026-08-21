use crate::aot::ir::ArrayInit;
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::descriptor_layout;
use super::locals::LocalLayout;
use super::memory::memarg;

pub(super) fn initialize(
    body: &mut Function,
    init: ArrayInit,
    ty: &StaticType,
    locals: &LocalLayout,
) -> AotResult<()> {
    let descriptor = descriptor_layout(ty)?;
    body.instruction(&W::LocalGet(locals.memory.product));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(locals.memory.data_start));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Const(0));
    body.instruction(&W::LocalGet(locals.memory.product));
    body.instruction(&W::I64Const(i64::from(descriptor.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::MemoryFill(0));
    if init == ArrayInit::One {
        emit_one(body, ty, locals)?;
    }
    body.instruction(&W::End);
    Ok(())
}

fn emit_one(body: &mut Function, ty: &StaticType, locals: &LocalLayout) -> AotResult<()> {
    let StaticType::Array { element, .. } = ty else {
        return Err(AotError::InvalidIR(
            "array initializer type mismatch".into(),
        ));
    };
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(locals.memory.max_offset));
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    body.instruction(&W::LocalGet(locals.memory.max_offset));
    body.instruction(&W::LocalGet(locals.memory.product));
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));
    body.instruction(&W::LocalGet(locals.memory.data_start));
    body.instruction(&W::LocalGet(locals.memory.max_offset));
    body.instruction(&W::I64Const(i64::from(descriptor_layout(ty)?.element_size)));
    body.instruction(&W::I64Mul);
    body.instruction(&W::I64Add);
    body.instruction(&W::I32WrapI64);
    match element.as_ref() {
        StaticType::U8 | StaticType::Bool => store_i32(body, W::I32Store8(memarg(0))),
        StaticType::I32 => store_i32(body, W::I32Store(memarg(0))),
        StaticType::I64 => {
            body.instruction(&W::I64Const(1));
            body.instruction(&W::I64Store(memarg(0)));
        }
        StaticType::F32 => {
            body.instruction(&W::F32Const(1.0.into()));
            body.instruction(&W::F32Store(memarg(0)));
        }
        StaticType::F64 => {
            body.instruction(&W::F64Const(1.0.into()));
            body.instruction(&W::F64Store(memarg(0)));
        }
        other => {
            return Err(AotError::InvalidIR(format!(
                "unsupported one initializer `{other}`"
            )))
        }
    }
    body.instruction(&W::LocalGet(locals.memory.max_offset));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(locals.memory.max_offset));
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);
    Ok(())
}

fn store_i32(body: &mut Function, store: W<'_>) {
    body.instruction(&W::I32Const(1));
    body.instruction(&store);
}
