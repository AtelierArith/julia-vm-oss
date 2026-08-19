use std::collections::HashMap;

use crate::aot::ir::{BinOpKind, Instruction};
use crate::aot::AotResult;
use wasm_encoder::{Function, Instruction as W};

use super::super::types::{unsupported, DESCRIPTOR_LEN_OFFSET};
use super::memory::{emit_descriptor_check, emit_u8_address, memarg};
use super::ops::{emit_binop, emit_const, emit_conversion, emit_unary, get, normalize_u8, set};

pub(super) fn emit_instruction(
    body: &mut Function,
    instruction: &Instruction,
    locals: &HashMap<String, u32>,
    functions: &HashMap<String, u32>,
) -> AotResult<()> {
    match instruction {
        Instruction::LoadConst { dest, value } => {
            emit_const(body, value)?;
            set(body, locals, dest)?;
        }
        Instruction::Copy { dest, src } => {
            get(body, locals, src)?;
            emit_conversion(body, &src.ty, &dest.ty)?;
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
        Instruction::Call { dest, func, args } if func == "__sjulia_u8_len" => {
            emit_descriptor_check(body, &args[0], locals)?;
            get(body, locals, &args[0])?;
            body.instruction(&W::I32Load(memarg(DESCRIPTOR_LEN_OFFSET)));
            body.instruction(&W::I64ExtendI32U);
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
        Instruction::GetIndex { dest, array, index } => {
            emit_u8_address(body, array, index, locals)?;
            body.instruction(&W::I32Load8U(memarg(0)));
            set(body, locals, dest)?;
        }
        Instruction::SetIndex {
            array,
            index,
            value,
        } => {
            emit_u8_address(body, array, index, locals)?;
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
