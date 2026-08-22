use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::unsupported;
use super::array::emit_new;
use super::descriptor::emit_i64_load;
use super::locals::LocalLayout;
use super::memory::memarg;
use super::ops::get;
use super::rng::NEXT_NAME;

pub(super) fn emit_array_uniform(
    body: &mut Function,
    destination: &VarRef,
    dims: &[VarRef],
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    use crate::aot::ir::{ArrayInit, Instruction};
    
    // Create array with zero initialization
    let array_new = Instruction::ArrayNew {
        dest: destination.clone(),
        dims: dims.to_vec(),
        init: ArrayInit::Zero,
    };
    
    emit_new(body, &array_new, layout, functions)?;
    
    // Fill array with random values
    emit_fill_uniform(body, destination, layout, functions)?;
    
    Ok(())
}

fn emit_fill_uniform(
    body: &mut Function,
    array: &VarRef,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let _descriptor_layout = super::super::types::descriptor_layout(&array.ty)?;
    
    // Get data pointer
    get(body, &layout.locals, array)?;
    body.instruction(&W::I32Const(24));
    body.instruction(&W::I32Add);
    body.instruction(&W::I32Load(memarg(0)));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalSet(layout.memory.data_start));
    
    // Get element count
    emit_i64_load(body, array, &layout.locals, super::super::types::DESCRIPTOR_ELEMENT_COUNT_OFFSET)?;
    body.instruction(&W::LocalSet(layout.memory.product));
    
    // Loop: for i in 0..element_count
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.term));
    
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    
    // Check if i >= element_count
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));
    
    // Generate random value and store it
    match &array.ty {
        StaticType::Array { element, .. } => {
            match **element {
                StaticType::F32 => {
                    // Compute address: data_start + (i * 4)
                    body.instruction(&W::LocalGet(layout.memory.data_start));
                    body.instruction(&W::LocalGet(layout.memory.term));
                    body.instruction(&W::I64Const(4));
                    body.instruction(&W::I64Mul);
                    body.instruction(&W::I64Add);
                    body.instruction(&W::I32WrapI64);
                    
                    // Generate random value
                    body.instruction(&W::Call(functions[NEXT_NAME]));
                    body.instruction(&W::I64Const(11));
                    body.instruction(&W::I64ShrU);
                    body.instruction(&W::F64ConvertI64U);
                    body.instruction(&W::F64Const((1.0 / 9_007_199_254_740_992.0).into()));
                    body.instruction(&W::F64Mul);
                    
                    // Demote to F32 and store
                    body.instruction(&W::F32DemoteF64);
                    body.instruction(&W::F32Store(memarg(0)));
                }
                StaticType::F64 => {
                    // Compute address: data_start + (i * 8)
                    body.instruction(&W::LocalGet(layout.memory.data_start));
                    body.instruction(&W::LocalGet(layout.memory.term));
                    body.instruction(&W::I64Const(8));
                    body.instruction(&W::I64Mul);
                    body.instruction(&W::I64Add);
                    body.instruction(&W::I32WrapI64);
                    
                    // Generate random value
                    body.instruction(&W::Call(functions[NEXT_NAME]));
                    body.instruction(&W::I64Const(11));
                    body.instruction(&W::I64ShrU);
                    body.instruction(&W::F64ConvertI64U);
                    body.instruction(&W::F64Const((1.0 / 9_007_199_254_740_992.0).into()));
                    body.instruction(&W::F64Mul);
                    
                    // Store
                    body.instruction(&W::F64Store(memarg(0)));
                }
                _ => return Err(unsupported("Array rand requires Float32 or Float64 elements")),
            }
        }
        _ => return Err(unsupported("Array rand requires array type")),
    }
    
    // Increment counter
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(layout.memory.term));
    
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);
    
    Ok(())
}

pub(super) fn emit_array_normal(
    body: &mut Function,
    destination: &VarRef,
    dims: &[VarRef],
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    use crate::aot::ir::{ArrayInit, Instruction};
    
    // Create array with zero initialization
    let array_new = Instruction::ArrayNew {
        dest: destination.clone(),
        dims: dims.to_vec(),
        init: ArrayInit::Zero,
    };
    
    emit_new(body, &array_new, layout, functions)?;
    
    // Fill array with random normal values
    emit_fill_normal(body, destination, layout, functions)?;
    
    Ok(())
}

fn emit_fill_normal(
    body: &mut Function,
    array: &VarRef,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let _descriptor_layout = super::super::types::descriptor_layout(&array.ty)?;
    
    // Get data pointer
    get(body, &layout.locals, array)?;
    body.instruction(&W::I32Const(24));
    body.instruction(&W::I32Add);
    body.instruction(&W::I32Load(memarg(0)));
    body.instruction(&W::I64ExtendI32U);
    body.instruction(&W::LocalSet(layout.memory.data_start));
    
    // Get element count
    emit_i64_load(body, array, &layout.locals, super::super::types::DESCRIPTOR_ELEMENT_COUNT_OFFSET)?;
    body.instruction(&W::LocalSet(layout.memory.product));
    
    // Loop: for i in 0..element_count
    body.instruction(&W::I64Const(0));
    body.instruction(&W::LocalSet(layout.memory.term));
    
    body.instruction(&W::Block(BlockType::Empty));
    body.instruction(&W::Loop(BlockType::Empty));
    
    // Check if i >= element_count
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::LocalGet(layout.memory.product));
    body.instruction(&W::I64GeU);
    body.instruction(&W::BrIf(1));
    
    // Generate random normal value and store it
    match &array.ty {
        StaticType::Array { element, .. } => {
            match **element {
                StaticType::F32 => {
                    // Compute address: data_start + (i * 4)
                    body.instruction(&W::LocalGet(layout.memory.data_start));
                    body.instruction(&W::LocalGet(layout.memory.term));
                    body.instruction(&W::I64Const(4));
                    body.instruction(&W::I64Mul);
                    body.instruction(&W::I64Add);
                    body.instruction(&W::I32WrapI64);
                    
                    // Generate random normal value
                    body.instruction(&W::Call(functions["__sjulia_rng_randn"]));
                    
                    // Demote to F32 and store
                    body.instruction(&W::F32DemoteF64);
                    body.instruction(&W::F32Store(memarg(0)));
                }
                StaticType::F64 => {
                    // Compute address: data_start + (i * 8)
                    body.instruction(&W::LocalGet(layout.memory.data_start));
                    body.instruction(&W::LocalGet(layout.memory.term));
                    body.instruction(&W::I64Const(8));
                    body.instruction(&W::I64Mul);
                    body.instruction(&W::I64Add);
                    body.instruction(&W::I32WrapI64);
                    
                    // Generate random normal value
                    body.instruction(&W::Call(functions["__sjulia_rng_randn"]));
                    
                    // Store
                    body.instruction(&W::F64Store(memarg(0)));
                }
                _ => return Err(unsupported("Array randn requires Float32 or Float64 elements")),
            }
        }
        _ => return Err(unsupported("Array randn requires array type")),
    }
    
    // Increment counter
    body.instruction(&W::LocalGet(layout.memory.term));
    body.instruction(&W::I64Const(1));
    body.instruction(&W::I64Add);
    body.instruction(&W::LocalSet(layout.memory.term));
    
    body.instruction(&W::Br(0));
    body.instruction(&W::End);
    body.instruction(&W::End);
    
    Ok(())
}
