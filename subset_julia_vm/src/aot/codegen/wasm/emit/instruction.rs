use std::collections::HashMap;

use crate::aot::ir::{BinOpKind, Instruction};
use crate::aot::AotResult;
use wasm_encoder::{Function, Instruction as W};

use super::super::types::{descriptor_layout, unsupported, DESCRIPTOR_ELEMENT_COUNT_OFFSET};
use super::conversion::emit_checked_conversion;
use super::descriptor::{
    emit_descriptor_validation, emit_i64_load, DescriptorAccess, DescriptorContext,
};
use super::locals::LocalLayout;
use super::memory::{emit_u8_address, memarg};
use super::math::{emit_math_builtin, emit_pow};
use super::ops::{emit_binop, emit_const, emit_conversion, emit_unary, get, normalize_u8, set};

pub(super) fn emit_instruction(
    body: &mut Function,
    instruction: &Instruction,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    let locals = &layout.locals;
    match instruction {
        Instruction::LoadConst { dest, value } => {
            emit_const(body, value)?;
            set(body, locals, dest)?;
        }
        Instruction::Copy { dest, src } => {
            emit_checked_conversion(body, src, &dest.ty, locals)?;
            set(body, locals, dest)?;
        }
        Instruction::BinOp {
            dest,
            op,
            left,
            right,
        } => {
            let comparisons = matches!(
                op,
                BinOpKind::Eq
                    | BinOpKind::Ne
                    | BinOpKind::Lt
                    | BinOpKind::Le
                    | BinOpKind::Gt
                    | BinOpKind::Ge
            );
            let operand_type = if comparisons { &left.ty } else { &dest.ty };
            get(body, locals, left)?;
            emit_conversion(body, &left.ty, operand_type)?;
            get(body, locals, right)?;
            emit_conversion(body, &right.ty, operand_type)?;
            if *op == BinOpKind::Pow {
                emit_pow(body, left, right, locals, &layout.math)?;
                set(body, locals, dest)?;
                return Ok(());
            }
            emit_binop(body, *op, operand_type)?;
            if !comparisons && *operand_type == crate::aot::types::StaticType::U8 {
                normalize_u8(body);
            }
            set(body, locals, dest)?;
        }
        Instruction::UnaryOp { dest, op, operand } => {
            emit_unary(body, *op, operand, locals)?;
            set(body, locals, dest)?;
        }
        Instruction::Builtin { dest, op, args } => {
            emit_math_builtin(body, *op, args, locals, &layout.math)?;
            set(body, locals, dest)?;
        }
        Instruction::Call { dest, func, args } if func == "__sjulia_u8_len" => {
            let descriptor = &args[0];
            let descriptor_layout = descriptor_layout(&descriptor.ty)?;
            emit_descriptor_validation(
                body,
                descriptor,
                descriptor_layout,
                &DescriptorContext {
                    locals,
                    scratch: &layout.memory,
                },
                DescriptorAccess::Read,
            )?;
            emit_i64_load(body, descriptor, locals, DESCRIPTOR_ELEMENT_COUNT_OFFSET)?;
            if let Some(dest) = dest {
                set(body, locals, dest)?;
            }
        }
        Instruction::Call { dest, func, args } => {
            for arg in args {
                get(body, locals, arg)?;
            }
            let index = functions.get(func).ok_or_else(|| {
                unsupported(format!("Wasm AoT cannot resolve direct call `{func}`"))
            })?;
            body.instruction(&W::Call(*index));
            if let Some(dest) = dest {
                set(body, locals, dest)?;
            }
        }
        Instruction::GetIndex {
            dest,
            array,
            indices,
        } => {
            emit_u8_address(body, array, indices, layout, DescriptorAccess::Read)?;
            body.instruction(&W::I32Load8U(memarg(0)));
            set(body, locals, dest)?;
        }
        Instruction::SetIndex {
            array,
            indices,
            value,
        } => {
            emit_u8_address(body, array, indices, layout, DescriptorAccess::Write)?;
            get(body, locals, value)?;
            body.instruction(&W::I32Store8(memarg(0)));
        }
        Instruction::Phi { .. } => {}
        other => {
            return Err(unsupported(format!(
                "Wasm AoT cannot emit instruction `{other:?}`"
            )))
        }
    }
    Ok(())
}
