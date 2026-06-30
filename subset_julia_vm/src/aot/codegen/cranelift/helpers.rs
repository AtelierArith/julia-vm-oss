use crate::aot::ir::{Instruction, IrFunction};
use crate::aot::rooting::static_type_requires_rooting_model;
use crate::aot::types::StaticType;

use cranelift_codegen::ir::types as cl_types;
use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_codegen::isa::CallConv;

use super::{CompileCtx, CraneliftError};

/// Convert a `StaticType` carrier to a Cranelift type.
pub(super) fn static_type_to_cranelift(ty: &StaticType) -> Result<cl_types::Type, CraneliftError> {
    if static_type_requires_rooting_model(ty) {
        return Err(CraneliftError::Unsupported(format!(
            "Cranelift backend does not yet satisfy AoT runtime Value rooting/safepoint contract for {:?}",
            ty
        )));
    }

    match ty {
        StaticType::I8 => Ok(cl_types::I8),
        StaticType::I16 => Ok(cl_types::I16),
        StaticType::I32 => Ok(cl_types::I32),
        StaticType::I64 => Ok(cl_types::I64),
        StaticType::I128 => Ok(cl_types::I128),
        StaticType::U8 => Ok(cl_types::I8),
        StaticType::U16 => Ok(cl_types::I16),
        StaticType::U32 => Ok(cl_types::I32),
        StaticType::U64 => Ok(cl_types::I64),
        StaticType::U128 => Ok(cl_types::I128),
        StaticType::F16 => Ok(cl_types::F32),
        StaticType::F32 => Ok(cl_types::F32),
        StaticType::F64 => Ok(cl_types::F64),
        StaticType::Bool => Ok(cl_types::I8),
        StaticType::Char => Ok(cl_types::I32),
        StaticType::Nothing => Ok(cl_types::I8),
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
        let cl_type = static_type_to_cranelift(ty)?;
        sig.params.push(AbiParam::new(cl_type));
    }

    match &func.return_type {
        StaticType::Nothing => {}
        StaticType::Tuple(elements) => {
            for ty in elements {
                let cl_type = static_type_to_cranelift(ty)?;
                sig.returns.push(AbiParam::new(cl_type));
            }
        }
        ty => {
            let cl_type = static_type_to_cranelift(ty)?;
            sig.returns.push(AbiParam::new(cl_type));
        }
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
