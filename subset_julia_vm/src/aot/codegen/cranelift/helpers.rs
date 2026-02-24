use crate::aot::ir::{Instruction, IrFunction};
use crate::aot::types::JuliaType;

use cranelift_codegen::ir::types as cl_types;
use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_codegen::isa::CallConv;

use super::{CompileCtx, CraneliftError};

/// Convert Julia type to Cranelift type.
pub(super) fn julia_type_to_cranelift(ty: &JuliaType) -> Result<cl_types::Type, CraneliftError> {
    match ty {
        JuliaType::Int8 => Ok(cl_types::I8),
        JuliaType::Int16 => Ok(cl_types::I16),
        JuliaType::Int32 => Ok(cl_types::I32),
        JuliaType::Int64 => Ok(cl_types::I64),
        JuliaType::UInt8 => Ok(cl_types::I8),
        JuliaType::UInt16 => Ok(cl_types::I16),
        JuliaType::UInt32 => Ok(cl_types::I32),
        JuliaType::UInt64 => Ok(cl_types::I64),
        JuliaType::Float32 => Ok(cl_types::F32),
        JuliaType::Float64 => Ok(cl_types::F64),
        JuliaType::Bool => Ok(cl_types::I8),
        JuliaType::Char => Ok(cl_types::I32),
        JuliaType::Nothing => Ok(cl_types::I8),
        JuliaType::String
        | JuliaType::Array { .. }
        | JuliaType::Tuple(_)
        | JuliaType::Struct { .. } => Ok(cl_types::I64),
        JuliaType::Any | JuliaType::Unknown => Ok(cl_types::I64),
        _ => Err(CraneliftError::TypeConversion(format!(
            "Unsupported type: {:?}",
            ty
        ))),
    }
}

/// Create a function signature from IR function.
pub(super) fn create_signature(func: &IrFunction) -> Result<Signature, CraneliftError> {
    let mut sig = Signature::new(CallConv::SystemV);

    for (_, ty) in &func.params {
        let cl_type = julia_type_to_cranelift(ty)?;
        sig.params.push(AbiParam::new(cl_type));
    }

    if func.return_type != JuliaType::Nothing {
        let cl_type = julia_type_to_cranelift(&func.return_type)?;
        sig.returns.push(AbiParam::new(cl_type));
    }

    Ok(sig)
}

/// Collect phi node information from all blocks in a function.
pub(super) fn collect_phi_info(func: &IrFunction, ctx: &mut CompileCtx) {
    for block in &func.blocks {
        let mut phi_dests = Vec::new();
        for inst in &block.instructions {
            if let Instruction::Phi { dest, incoming } = inst {
                phi_dests.push(dest.clone());
                for (src_label, src_var) in incoming {
                    ctx.phi_incoming
                        .entry((src_label.clone(), block.label.clone()))
                        .or_default()
                        .push(src_var.clone());
                }
            }
        }
        if !phi_dests.is_empty() {
            ctx.phi_params.insert(block.label.clone(), phi_dests);
        }
    }
}

/// Compute field offset from field name.
pub(super) fn field_name_to_offset(field: &str) -> i32 {
    if let Ok(idx) = field.parse::<i32>() {
        return idx * 8;
    }

    let hash = field
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    ((hash % 32) * 8) as i32
}
