use wasm_encoder::{BlockType, Function, Instruction as W, ValType};

use super::allocator::{load_header_end, validate_header_magic};
use super::descriptor::{trap_if, trap_on_stack};
use super::memory::memarg;

pub(super) fn emit_free(heap_global: u32, heap_base: i32) -> Function {
    let mut body = Function::new([(2, ValType::I64)]);
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I32Eqz);
    trap_on_stack(&mut body);
    body.instruction(&W::I64Const(i64::from(heap_base)));
    body.instruction(&W::LocalSet(1));
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::GlobalGet(heap_global));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));
    validate_header_magic(&mut body, 1);
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Load(memarg(8)));
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I32Eq);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Load(memarg(4)));
    body.instruction(&W::I32Const(1));
    trap_if(&mut body, W::I32Ne);
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Const(2));
    body.instruction(&W::I32Store(memarg(4)));
    body.instruction(&W::Return);
    body.instruction(&W::End);
    load_header_end(&mut body, 1);
    body.instruction(&W::LocalSet(1));
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);
    body.instruction(&W::Unreachable);
    body.instruction(&W::End);
    body
}
