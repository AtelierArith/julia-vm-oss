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
use super::math::emit_math_builtin;
use super::memory::{emit_array_address, memarg};
use super::ops::{emit_binop, emit_const, emit_conversion, emit_unary, get, normalize_u8, set};
use super::strings::StaticStrings;
use super::transcendental::emit_pow;

pub(super) fn emit_instruction(
    body: &mut Function,
    instruction: &Instruction,
    layout: &LocalLayout,
    functions: &HashMap<String, u32>,
    strings: &StaticStrings,
) -> AotResult<()> {
    let locals = &layout.locals;
    match instruction {
        Instruction::LoadConst { dest, value } => {
            emit_const(body, value, strings)?;
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
        Instruction::Call { dest, func, args } if func == "__sjulia_array_len" => {
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
        Instruction::Call { dest, func, args } if func == "__sjulia_array_size_axis" => {
            let descriptor = &args[0];
            let axis = &args[1];
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
            get(body, locals, axis)?;
            body.instruction(&W::I64Const(1));
            body.instruction(&W::I64LtS);
            super::descriptor::trap_on_stack(body);
            get(body, locals, axis)?;
            body.instruction(&W::I64Const(
                i64::try_from(descriptor_layout.rank).map_err(|_| unsupported("rank overflow"))?,
            ));
            body.instruction(&W::I64GtU);
            body.instruction(&W::If(wasm_encoder::BlockType::Result(
                wasm_encoder::ValType::I64,
            )));
            body.instruction(&W::I64Const(1));
            body.instruction(&W::Else);
            get(body, locals, descriptor)?;
            get(body, locals, axis)?;
            body.instruction(&W::I64Const(1));
            body.instruction(&W::I64Sub);
            body.instruction(&W::I64Const(super::super::types::DESCRIPTOR_AXIS_SIZE));
            body.instruction(&W::I64Mul);
            body.instruction(&W::I32WrapI64);
            body.instruction(&W::I32Add);
            body.instruction(&W::I64Load(memarg(
                super::super::types::DESCRIPTOR_DIM_OFFSET,
            )));
            body.instruction(&W::End);
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
            emit_array_address(body, array, indices, layout, DescriptorAccess::Read)?;
            emit_array_load(body, &dest.ty)?;
            if dest.ty == crate::aot::types::StaticType::Bool {
                normalize_bool(body);
            }
            set(body, locals, dest)?;
        }
        Instruction::SetIndex {
            array,
            indices,
            value,
        } => {
            emit_array_address(body, array, indices, layout, DescriptorAccess::Write)?;
            get(body, locals, value)?;
            emit_array_store(body, &value.ty)?;
        }
        Instruction::StructNew { .. } => {
            super::aggregate::emit_new(body, instruction, locals, functions)?;
        }
        Instruction::ArrayNew { .. } => {
            super::array::emit_new(body, instruction, layout, functions)?;
        }
        Instruction::ArraySlice { .. } => {
            super::array_slice::emit(body, instruction, layout, functions)?;
        }
        Instruction::ArraySliceAssign { .. } => {
            super::array_slice_assign::emit(body, instruction, layout, functions)?;
        }
        Instruction::UnitRangeLength { dest, start, stop } => {
            get(body, locals, stop)?;
            get(body, locals, start)?;
            body.instruction(&W::I64LtS);
            body.instruction(&W::If(wasm_encoder::BlockType::Result(
                wasm_encoder::ValType::I64,
            )));
            body.instruction(&W::I64Const(0));
            body.instruction(&W::Else);
            get(body, locals, stop)?;
            get(body, locals, start)?;
            body.instruction(&W::I64Sub);
            body.instruction(&W::I64Const(1));
            body.instruction(&W::I64Add);
            body.instruction(&W::End);
            set(body, locals, dest)?;
        }
        Instruction::GetFieldOffset {
            dest,
            object,
            layout_id,
            offset,
        } => {
            super::aggregate::emit_get(body, dest, object, *layout_id, *offset, locals)?;
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

pub(super) fn emit_array_load(
    body: &mut Function,
    ty: &crate::aot::types::StaticType,
) -> AotResult<()> {
    match ty {
        crate::aot::types::StaticType::U8 | crate::aot::types::StaticType::Bool => {
            body.instruction(&W::I32Load8U(memarg(0)));
        }
        crate::aot::types::StaticType::I32 => {
            body.instruction(&W::I32Load(memarg(0)));
        }
        crate::aot::types::StaticType::I64 => {
            body.instruction(&W::I64Load(memarg(0)));
        }
        crate::aot::types::StaticType::F32 => {
            body.instruction(&W::F32Load(memarg(0)));
        }
        crate::aot::types::StaticType::F64 => {
            body.instruction(&W::F64Load(memarg(0)));
        }
        other => {
            return Err(unsupported(format!(
                "unsupported Wasm array load `{other}`"
            )))
        }
    };
    Ok(())
}

pub(super) fn emit_array_store(
    body: &mut Function,
    ty: &crate::aot::types::StaticType,
) -> AotResult<()> {
    match ty {
        crate::aot::types::StaticType::U8 | crate::aot::types::StaticType::Bool => {
            body.instruction(&W::I32Store8(memarg(0)));
        }
        crate::aot::types::StaticType::I32 => {
            body.instruction(&W::I32Store(memarg(0)));
        }
        crate::aot::types::StaticType::I64 => {
            body.instruction(&W::I64Store(memarg(0)));
        }
        crate::aot::types::StaticType::F32 => {
            body.instruction(&W::F32Store(memarg(0)));
        }
        crate::aot::types::StaticType::F64 => {
            body.instruction(&W::F64Store(memarg(0)));
        }
        other => {
            return Err(unsupported(format!(
                "unsupported Wasm array store `{other}`"
            )))
        }
    };
    Ok(())
}

fn normalize_bool(body: &mut Function) {
    body.instruction(&W::I32Eqz);
    body.instruction(&W::I32Eqz);
}
