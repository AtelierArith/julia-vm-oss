use std::collections::HashMap;

use crate::aot::ir::VarRef;
use crate::aot::AotResult;
use wasm_encoder::Function;

use super::array::emit_new;
use super::locals::LocalLayout;
use super::rng_array_fill::{emit_fill_normal, emit_fill_uniform};

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
