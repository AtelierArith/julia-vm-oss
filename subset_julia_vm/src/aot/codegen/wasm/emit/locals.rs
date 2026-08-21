use std::collections::{HashMap, HashSet};

use crate::aot::ir::{Instruction, IrFunction, VarRef};
use crate::aot::{AotError, AotResult};
use wasm_encoder::ValType;

use super::ops::required_type;

pub(super) type PhiEdges = HashMap<(String, String), Vec<(VarRef, VarRef)>>;

pub(super) struct LocalLayout {
    pub(super) locals: HashMap<String, u32>,
    pub(super) phi_scratch: HashMap<String, u32>,
    pub(super) declarations: Vec<(u32, ValType)>,
    pub(super) pc: u32,
    pub(super) memory: MemoryLocals,
    pub(super) math: MathLocals,
    pub(super) slice: SliceLocals,
}

pub(super) struct SliceLocals {
    pub(super) count: u32,
    pub(super) temporary: u32,
}

pub(super) struct MathLocals {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) term: u32,
    pub(super) sum: u32,
    pub(super) factor: u32,
    pub(super) exponent: u32,
    pub(super) log_adjust: u32,
}

pub(super) struct MemoryLocals {
    pub(super) descriptor_start: u32,
    pub(super) memory_bytes: u32,
    pub(super) metadata_end: u32,
    pub(super) element_count: u32,
    pub(super) product: u32,
    pub(super) max_offset: u32,
    pub(super) term: u32,
    pub(super) data_start: u32,
    pub(super) data_end: u32,
}

pub(super) fn build_local_layout(function: &IrFunction) -> AotResult<LocalLayout> {
    let locals = collect_locals(function)?;
    let param_count = u32::try_from(function.params.len())
        .map_err(|_| AotError::CodegenError("too many Wasm parameters".to_string()))?;
    let mut indices = HashMap::new();
    for (index, (name, _)) in function.params.iter().enumerate() {
        indices.insert(name.clone(), checked_index(index, "parameters")?);
    }
    for (offset, (name, _)) in locals.iter().enumerate() {
        indices.insert(name.clone(), param_count + checked_index(offset, "locals")?);
    }
    let pc = param_count + checked_index(locals.len(), "locals")?;
    let mut declarations: Vec<_> = locals.iter().map(|(_, ty)| (1, *ty)).collect();
    declarations.push((1, ValType::I32));
    let memory_start = pc + 1;
    declarations.push((9, ValType::I64));
    let memory = MemoryLocals {
        descriptor_start: memory_start,
        memory_bytes: memory_start + 1,
        metadata_end: memory_start + 2,
        element_count: memory_start + 3,
        product: memory_start + 4,
        max_offset: memory_start + 5,
        term: memory_start + 6,
        data_start: memory_start + 7,
        data_end: memory_start + 8,
    };
    let mut phi_scratch = HashMap::new();
    let math_start = memory_start + 9;
    declarations.push((7, ValType::F64));
    let math = MathLocals {
        x: math_start,
        y: math_start + 1,
        term: math_start + 2,
        sum: math_start + 3,
        factor: math_start + 4,
        exponent: math_start + 5,
        log_adjust: math_start + 6,
    };
    let slice_start = math_start + 7;
    declarations.push((2, ValType::I64));
    let slice = SliceLocals {
        count: slice_start,
        temporary: slice_start + 1,
    };
    for (offset, (name, ty)) in collect_phi_scratch(function)?.iter().enumerate() {
        phi_scratch.insert(
            name.clone(),
            slice_start + 2 + checked_index(offset, "phi locals")?,
        );
        declarations.push((1, *ty));
    }
    Ok(LocalLayout {
        locals: indices,
        phi_scratch,
        declarations,
        pc,
        memory,
        math,
        slice,
    })
}

fn checked_index(index: usize, kind: &str) -> AotResult<u32> {
    u32::try_from(index).map_err(|_| AotError::CodegenError(format!("too many Wasm {kind}")))
}

fn collect_locals(function: &IrFunction) -> AotResult<Vec<(String, ValType)>> {
    let params: HashSet<_> = function
        .params
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let mut locals = Vec::new();
    let mut seen = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            for dest in destinations(instruction) {
                if !params.contains(dest.name.as_str()) && seen.insert(dest.name.clone()) {
                    locals.push((dest.name.clone(), required_type(&dest.ty)?));
                }
            }
        }
    }
    Ok(locals)
}

fn collect_phi_scratch(function: &IrFunction) -> AotResult<Vec<(String, ValType)>> {
    let mut scratch = Vec::new();
    let mut seen = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Instruction::Phi { dest, .. } = instruction {
                if seen.insert(dest.name.clone()) {
                    scratch.push((dest.name.clone(), required_type(&dest.ty)?));
                }
            }
        }
    }
    Ok(scratch)
}

fn destinations(instruction: &Instruction) -> Vec<&VarRef> {
    match instruction {
        Instruction::LoadConst { dest, .. }
        | Instruction::Copy { dest, .. }
        | Instruction::BinOp { dest, .. }
        | Instruction::UnaryOp { dest, .. }
        | Instruction::Builtin { dest, .. }
        | Instruction::Rand { dest }
        | Instruction::UnitRangeLength { dest, .. }
        | Instruction::GetIndex { dest, .. }
        | Instruction::GetField { dest, .. }
        | Instruction::GetFieldOffset { dest, .. }
        | Instruction::TypeAssert { dest, .. }
        | Instruction::Phi { dest, .. }
        | Instruction::StructNew { dest, .. } => vec![dest],
        Instruction::ArrayNew { dest, .. } | Instruction::ArraySlice { dest, .. } => vec![dest],
        Instruction::Call { dest, .. } => dest.iter().collect(),
        Instruction::CallMulti { dests, .. } => dests.iter().collect(),
        Instruction::SetIndex { .. }
        | Instruction::ArraySliceAssign { .. }
        | Instruction::SetField { .. }
        | Instruction::SetFieldOffset { .. } => Vec::new(),
    }
}

pub(super) fn collect_phi_edges(function: &IrFunction) -> PhiEdges {
    let mut edges = HashMap::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Instruction::Phi { dest, incoming } = instruction {
                for (source, value) in incoming {
                    edges
                        .entry((source.clone(), block.label.clone()))
                        .or_insert_with(Vec::new)
                        .push((dest.clone(), value.clone()));
                }
            }
        }
    }
    edges
}
