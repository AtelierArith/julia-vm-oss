use wasm_encoder::{BlockType, Function, GlobalType, Instruction as W, ValType};

use super::descriptor::{trap_if, trap_on_stack};
use super::memory::memarg;

pub(super) const ALLOC_NAME: &str = "__sjulia_alloc";
pub(super) const FREE_NAME: &str = "__sjulia_free";
pub(super) const DROP_NAME: &str = "__sjulia_drop";
pub(super) const HEAP_BASE: i32 = 4_096;
pub(super) const MAX_MEMORY_PAGES: u64 = 256;

const HEADER_SIZE: i32 = 32;
const HEADER_MAGIC: i32 = 0x534a_414c;
const STATE_LIVE: i32 = 1;
const STATE_FREE: i32 = 2;
const MAX_ALIGNMENT: i32 = 65_536;
const PAGE_SIZE: i64 = 65_536;

const MAGIC_OFFSET: u64 = 0;
const STATE_OFFSET: u64 = 4;
const PAYLOAD_OFFSET: u64 = 8;
const END_OFFSET: u64 = 12;
const SIZE_OFFSET: u64 = 16;
const ALIGN_OFFSET: u64 = 24;
const RESERVED_OFFSET: u64 = 28;

pub(super) fn heap_global_type() -> GlobalType {
    GlobalType {
        val_type: ValType::I32,
        mutable: true,
        shared: false,
    }
}

pub(super) fn emit_alloc(heap_global: u32) -> Function {
    let mut body = Function::new([(6, ValType::I64), (3, ValType::I32)]);
    validate_request(&mut body);
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I64Eqz);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::I32Const(0));
    body.instruction(&W::Return);
    body.instruction(&W::End);
    body.instruction(&W::I64Const(i64::from(HEAP_BASE)));
    body.instruction(&W::LocalSet(2));
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    body.instruction(&W::LocalGet(2));
    body.instruction(&W::GlobalGet(heap_global));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));
    validate_header(&mut body, heap_global, 2);
    body.instruction(&W::LocalGet(2));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Load(memarg(STATE_OFFSET)));
    body.instruction(&W::I32Const(STATE_FREE));
    body.instruction(&W::I32Eq);
    body.instruction(&W::If(BlockType::Empty));
    aligned_payload(&mut body, 2, 1, 3);
    body.instruction(&W::LocalGet(3));
    requested_extent(&mut body, 0);
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalGet(2));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Load(memarg(END_OFFSET)));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64LeU);
    body.instruction(&W::If(BlockType::Empty));
    write_live_header(&mut body, 2, 3);
    body.instruction(&W::LocalGet(3));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::Return);
    body.instruction(&W::End);
    body.instruction(&W::End);
    load_header_end(&mut body, 2);
    body.instruction(&W::LocalSet(2));
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);

    body.instruction(&W::GlobalGet(heap_global));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalSet(2));
    aligned_payload(&mut body, 2, 1, 3);
    body.instruction(&W::LocalGet(3));
    requested_extent(&mut body, 0);
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(7));
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(-8));
    body.instruction(&W::I64And);
    body.instruction(&W::LocalTee(4));
    body.instruction(&W::I64Const(u32::MAX.into()));
    trap_if(&mut body, W::I64GtU);
    body.instruction(&W::MemorySize(0));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Const(PAGE_SIZE));
    body.instruction(&W::I64Mul);
    body.instruction(&W::LocalSet(5));
    body.instruction(&W::LocalGet(4));
    body.instruction(&W::LocalGet(5));
    body.instruction(&W::I64GtU);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::LocalGet(4));
    body.instruction(&W::LocalGet(5));
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64Const(PAGE_SIZE - 1));
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(PAGE_SIZE));
    body.instruction(&W::I64DivU);
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::MemoryGrow(0));
    body.instruction(&W::I32Const(-1));
    body.instruction(&W::I32Eq);
    body.instruction(&W::If(BlockType::Empty));
    body.instruction(&W::I32Const(0));
    body.instruction(&W::Return);
    body.instruction(&W::End);
    body.instruction(&W::End);
    body.instruction(&W::LocalGet(2));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::LocalGet(4));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Store(memarg(END_OFFSET)));
    write_live_header(&mut body, 2, 3);
    body.instruction(&W::LocalGet(4));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::GlobalSet(heap_global));
    body.instruction(&W::LocalGet(3));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::End);
    body
}

fn validate_request(body: &mut Function) {
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I64Const(0));
    trap_if(body, W::I64LtS);
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I64Const(i64::from(u32::MAX - HEAP_BASE as u32)));
    trap_if(body, W::I64GtU);
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I32Const(1));
    trap_if(body, W::I32LtS);
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I32Const(MAX_ALIGNMENT));
    trap_if(body, W::I32GtU);
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I32Const(1));
    body.instruction(&W::I32Sub);
    body.instruction(&W::I32And);
    body.instruction(&W::I32Const(0));
    trap_if(body, W::I32Ne);
}

fn aligned_payload(body: &mut Function, header: u32, alignment: u32, destination: u32) {
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I64Const(i64::from(HEADER_SIZE)));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalGet(alignment));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64Add);
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalGet(alignment));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::I64Sub);
    body.instruction(&W::I64And);
    body.instruction(&W::LocalSet(destination));
}

fn requested_extent(body: &mut Function, size: u32) {
    body.instruction(&W::LocalGet(size));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64LtU);
    body.instruction(&W::If(BlockType::Result(ValType::I64)));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::Else);
    body.instruction(&W::LocalGet(size));
    body.instruction(&W::End);
}

fn write_live_header(body: &mut Function, header: u32, payload: u32) {
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Const(HEADER_MAGIC));
    body.instruction(&W::I32Store(memarg(MAGIC_OFFSET)));
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::LocalGet(payload));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Store(memarg(PAYLOAD_OFFSET)));
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I64Store(memarg(SIZE_OFFSET)));
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::LocalGet(1));
    body.instruction(&W::I32Store(memarg(ALIGN_OFFSET)));
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Const(0));
    body.instruction(&W::I32Store(memarg(RESERVED_OFFSET)));
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Const(STATE_LIVE));
    body.instruction(&W::I32Store(memarg(STATE_OFFSET)));
}

fn validate_header(body: &mut Function, heap_global: u32, header: u32) {
    validate_header_magic(body, header);
    load_header_end(body, header);
    body.instruction(&W::LocalGet(header));
    trap_if(body, W::I64LeU);
    load_header_end(body, header);
    body.instruction(&W::GlobalGet(heap_global));
    body.instruction(&W::I64ExtendI32U);
    trap_if(body, W::I64GtU);
    load_header_end(body, header);
    body.instruction(&W::I64Const(7));
    body.instruction(&W::I64And);
    body.instruction(&W::I64Eqz);
    body.instruction(&W::I32Eqz);
    trap_on_stack(body);
}

pub(super) fn validate_header_magic(body: &mut Function, header: u32) {
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Load(memarg(MAGIC_OFFSET)));
    body.instruction(&W::I32Const(HEADER_MAGIC));
    trap_if(body, W::I32Ne);
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Load(memarg(RESERVED_OFFSET)));
    body.instruction(&W::I32Eqz);
    body.instruction(&W::I32Eqz);
    trap_on_stack(body);
}

pub(super) fn load_header_end(body: &mut Function, header: u32) {
    body.instruction(&W::LocalGet(header));
    body.instruction(&W::I32WrapI64);
    body.instruction(&W::I32Load(memarg(END_OFFSET)));
    body.instruction(&W::I64ExtendI32U);
}
