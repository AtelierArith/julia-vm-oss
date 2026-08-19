use std::collections::HashMap;

use crate::aot::ir::{Terminator, VarRef};
use crate::aot::{AotError, AotResult};
use wasm_encoder::{BlockType, Function, Instruction as W};

use super::super::types::unsupported;
use super::locals::{LocalLayout, PhiEdges};
use super::ops::{emit_conversion, get, set};

pub(super) fn emit_terminator(
    body: &mut Function,
    source: &str,
    terminator: &Terminator,
    blocks: &HashMap<String, i32>,
    phi_edges: &PhiEdges,
    layout: &LocalLayout,
) -> AotResult<()> {
    match terminator {
        Terminator::Return(value) => {
            if let Some(value) = value {
                get(body, &layout.locals, value)?;
            }
            body.instruction(&W::Return);
        }
        Terminator::Jump(target) => {
            emit_phi_copies(body, source, target, phi_edges, layout)?;
            set_pc(body, blocks, target, layout.pc)?;
            body.instruction(&W::Br(1));
        }
        Terminator::Branch {
            cond,
            then_block,
            else_block,
        } => {
            get(body, &layout.locals, cond)?;
            body.instruction(&W::If(BlockType::Empty));
            emit_phi_copies(body, source, then_block, phi_edges, layout)?;
            set_pc(body, blocks, then_block, layout.pc)?;
            body.instruction(&W::Else);
            emit_phi_copies(body, source, else_block, phi_edges, layout)?;
            set_pc(body, blocks, else_block, layout.pc)?;
            body.instruction(&W::End);
            body.instruction(&W::Br(1));
        }
        other => {
            return Err(unsupported(format!(
                "Wasm AoT cannot emit terminator `{other:?}`"
            )))
        }
    }
    Ok(())
}

fn emit_phi_copies(
    body: &mut Function,
    source: &str,
    target: &str,
    phi_edges: &PhiEdges,
    layout: &LocalLayout,
) -> AotResult<()> {
    let key = (source.to_string(), target.to_string());
    if let Some(copies) = phi_edges.get(&key) {
        for (dest, src) in copies {
            get(body, &layout.locals, src)?;
            emit_conversion(body, &src.ty, &dest.ty)?;
            body.instruction(&W::LocalSet(scratch(layout, dest)?));
        }
        for (dest, _) in copies {
            body.instruction(&W::LocalGet(scratch(layout, dest)?));
            set(body, &layout.locals, dest)?;
        }
    }
    Ok(())
}

fn scratch(layout: &LocalLayout, dest: &VarRef) -> AotResult<u32> {
    layout
        .phi_scratch
        .get(&dest.name)
        .copied()
        .ok_or_else(|| AotError::InvalidIR(format!("missing phi scratch local `{}`", dest.name)))
}

fn set_pc(
    body: &mut Function,
    blocks: &HashMap<String, i32>,
    target: &str,
    pc: u32,
) -> AotResult<()> {
    let index = blocks
        .get(target)
        .ok_or_else(|| AotError::InvalidIR(format!("unknown Wasm IR block `{target}`")))?;
    body.instruction(&W::I32Const(*index));
    body.instruction(&W::LocalSet(pc));
    Ok(())
}
