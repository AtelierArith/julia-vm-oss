use crate::aot::ir::{AotBuiltinOp, AotExpr, ConstValue, Instruction, IrFunction, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::ops::{ensure_type, map_binop, map_unary};
use super::Lowerer;

impl Lowerer<'_> {
    pub(super) fn expr(&mut self, expr: &AotExpr, function: &mut IrFunction) -> AotResult<VarRef> {
        match expr {
            AotExpr::LitI64(value) => {
                self.constant(ConstValue::Int64(*value), StaticType::I64, function)
            }
            AotExpr::LitI32(value) => {
                self.constant(ConstValue::Int32(*value), StaticType::I32, function)
            }
            AotExpr::LitF64(value) => {
                self.constant(ConstValue::Float64(*value), StaticType::F64, function)
            }
            AotExpr::LitF32(value) => {
                self.constant(ConstValue::Float32(*value), StaticType::F32, function)
            }
            AotExpr::LitBool(value) => {
                self.constant(ConstValue::Bool(*value), StaticType::Bool, function)
            }
            AotExpr::Var { name, .. } => self.vars.get(name).cloned().ok_or_else(|| {
                unsupported(format!("Wasm AoT could not resolve variable `{name}`"))
            }),
            AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty,
            } => {
                ensure_type(result_ty)?;
                let left = self.expr(left, function)?;
                let right = self.expr(right, function)?;
                let dest = self.temporary(result_ty.clone());
                self.current_block_mut(function)?.push(Instruction::BinOp {
                    dest: dest.clone(),
                    op: map_binop(*op)?,
                    left,
                    right,
                });
                Ok(dest)
            }
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => {
                let operand = self.expr(operand, function)?;
                if matches!(op, crate::aot::ir::AotUnaryOp::Pos) {
                    return Ok(operand);
                }
                let dest = self.temporary(result_ty.clone());
                self.current_block_mut(function)?
                    .push(Instruction::UnaryOp {
                        dest: dest.clone(),
                        op: map_unary(*op)?,
                        operand,
                    });
                Ok(dest)
            }
            AotExpr::CallStatic {
                function: callee,
                args,
                return_ty,
                ..
            } => {
                let args = args
                    .iter()
                    .map(|arg| self.expr(arg, function))
                    .collect::<AotResult<Vec<_>>>()?;
                let dest =
                    (*return_ty != StaticType::Nothing).then(|| self.temporary(return_ty.clone()));
                self.current_block_mut(function)?.push(Instruction::Call {
                    dest: dest.clone(),
                    func: callee.clone(),
                    args,
                });
                dest.ok_or_else(|| unsupported("Nothing-returning call cannot be used as a value"))
            }
            AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::Length,
                args,
                return_ty,
            } if args.len() == 1 => {
                let args = vec![self.expr(&args[0], function)?];
                let dest = self.temporary(return_ty.clone());
                self.current_block_mut(function)?.push(Instruction::Call {
                    dest: Some(dest.clone()),
                    func: "__sjulia_u8_len".to_string(),
                    args,
                });
                Ok(dest)
            }
            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } if matches!(
                builtin,
                AotBuiltinOp::Abs
                    | AotBuiltinOp::Floor
                    | AotBuiltinOp::Ceil
                    | AotBuiltinOp::Trunc
                    | AotBuiltinOp::Round
                    | AotBuiltinOp::Sqrt
                    | AotBuiltinOp::Exp
                    | AotBuiltinOp::Log
                    | AotBuiltinOp::Min
                    | AotBuiltinOp::Max
                    | AotBuiltinOp::Clamp
                    | AotBuiltinOp::Isnan
                    | AotBuiltinOp::Isinf
                    | AotBuiltinOp::Isfinite
            ) => {
                ensure_type(return_ty)?;
                let args = args
                    .iter()
                    .map(|arg| self.expr(arg, function))
                    .collect::<AotResult<Vec<_>>>()?;
                let dest = self.temporary(return_ty.clone());
                self.current_block_mut(function)?.push(Instruction::Builtin {
                    dest: dest.clone(),
                    op: *builtin,
                    args,
                });
                Ok(dest)
            }
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple: false,
            } => {
                let array = self.expr(array, function)?;
                let indices = indices
                    .iter()
                    .map(|index| self.expr(index, function))
                    .collect::<AotResult<Vec<_>>>()?;
                let dest = self.temporary(elem_ty.clone());
                self.current_block_mut(function)?
                    .push(Instruction::GetIndex {
                        dest: dest.clone(),
                        array,
                        indices,
                    });
                Ok(dest)
            }
            AotExpr::Convert { value, target_ty }
                if matches!(
                    target_ty,
                    StaticType::U8
                        | StaticType::I64
                        | StaticType::I32
                        | StaticType::F32
                        | StaticType::F64
                        | StaticType::Bool
                ) =>
            {
                let src = self.expr(value, function)?;
                let dest = self.temporary(target_ty.clone());
                self.current_block_mut(function)?.push(Instruction::Copy {
                    dest: dest.clone(),
                    src,
                });
                Ok(dest)
            }
            _ => Err(unsupported(format!(
                "Wasm AoT cannot lower expression `{expr:?}`"
            ))),
        }
    }

    fn constant(
        &mut self,
        value: ConstValue,
        ty: StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let dest = self.temporary(ty);
        self.current_block_mut(function)?
            .push(Instruction::LoadConst {
                dest: dest.clone(),
                value,
            });
        Ok(dest)
    }
}
