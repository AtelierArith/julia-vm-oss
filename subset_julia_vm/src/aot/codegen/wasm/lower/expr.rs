use crate::aot::ir::{
    AotBuiltinOp, AotExpr, ArrayInit, ConstValue, Instruction, IrFunction, StructFieldInit, VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::ops::{ensure_type, map_binop, map_unary};
use super::Lowerer;

impl Lowerer<'_, '_> {
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
            AotExpr::LitStr(value) => {
                self.constant(ConstValue::String(value.clone()), StaticType::Str, function)
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
            AotExpr::CallDynamic {
                function: callee,
                args,
            } if callee == "convert" && args.len() == 2 =>
            {
                self.expr(&args[1], function)
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
                    func: "__sjulia_array_len".to_string(),
                    args,
                });
                Ok(dest)
            }
            AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::Ndims,
                args,
                return_ty,
            } if args.len() == 1 => {
                let array = self.expr(&args[0], function)?;
                let rank = match &array.ty {
                    StaticType::Array { ndims: Some(rank), .. } => *rank,
                    _ => return Err(unsupported("ndims requires a statically ranked array")),
                };
                let rank = i64::try_from(rank)
                    .map_err(|_| unsupported("array rank exceeds Julia Int64"))?;
                self.constant(ConstValue::Int64(rank), return_ty.clone(), function)
            }
            AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::Size,
                args,
                return_ty,
            } if args.len() == 2 => {
                let args = args
                    .iter()
                    .map(|arg| self.expr(arg, function))
                    .collect::<AotResult<Vec<_>>>()?;
                let dest = self.temporary(return_ty.clone());
                self.current_block_mut(function)?.push(Instruction::Call {
                    dest: Some(dest.clone()),
                    func: "__sjulia_array_size_axis".to_string(),
                    args,
                });
                Ok(dest)
            }
            AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::Size,
                args,
                return_ty,
            } if args.len() == 1 => {
                let array = self.expr(&args[0], function)?;
                let rank = match &array.ty {
                    StaticType::Array { ndims: Some(rank), .. } => *rank,
                    _ => return Err(unsupported("size requires a statically ranked array")),
                };
                let layout = self.layouts.layout(return_ty)?.clone();
                if layout.fields.len() != rank {
                    return Err(unsupported("size tuple arity does not match array rank"));
                }
                let mut fields = Vec::with_capacity(rank);
                for (axis, field) in layout.fields.iter().enumerate() {
                    let axis = i64::try_from(axis + 1)
                        .map_err(|_| unsupported("array axis exceeds Julia Int64"))?;
                    let axis = self.constant(ConstValue::Int64(axis), StaticType::I64, function)?;
                    let value = self.temporary(StaticType::I64);
                    self.current_block_mut(function)?.push(Instruction::Call {
                        dest: Some(value.clone()),
                        func: "__sjulia_array_size_axis".to_string(),
                        args: vec![array.clone(), axis],
                    });
                    fields.push(StructFieldInit {
                        offset: i32::try_from(field.offset)
                            .map_err(|_| unsupported("size tuple field offset overflow"))?,
                        value,
                    });
                }
                let dest = self.temporary(return_ty.clone());
                self.current_block_mut(function)?.push(Instruction::StructNew {
                    dest: dest.clone(),
                    layout_id: layout.id,
                    size: layout.size,
                    align: layout.align,
                    fields,
                });
                Ok(dest)
            }
            AotExpr::CallBuiltin {
                builtin: builtin @ (AotBuiltinOp::Zeros | AotBuiltinOp::Ones),
                args,
                return_ty,
            } => {
                let dims = args
                    .iter()
                    .map(|arg| self.expr(arg, function))
                    .collect::<AotResult<Vec<_>>>()?;
                let dest = self.temporary(return_ty.clone());
                self.current_block_mut(function)?.push(Instruction::ArrayNew {
                    dest: dest.clone(),
                    dims,
                    init: match builtin {
                        AotBuiltinOp::Zeros => ArrayInit::Zero,
                        AotBuiltinOp::Ones => ArrayInit::One,
                        _ => unreachable!(),
                    },
                });
                Ok(dest)
            }
            AotExpr::CallBuiltin {
                builtin: AotBuiltinOp::StringConcat,
                ..
            } => Err(unsupported(
                "Wasm AoT does not support dynamic string concatenation or interpolation; only static UTF-8 literals are supported",
            )),
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
            ) =>
            {
                ensure_type(return_ty)?;
                let args = args
                    .iter()
                    .map(|arg| self.expr(arg, function))
                    .collect::<AotResult<Vec<_>>>()?;
                let dest = self.temporary(return_ty.clone());
                self.current_block_mut(function)?
                    .push(Instruction::Builtin {
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
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple: true,
            } => self.tuple_index(array, indices, elem_ty, function),
            AotExpr::TupleLit { elements } => {
                let ty = expr.get_type();
                self.aggregate(ty, elements, function)
            }
            AotExpr::StructNew { name, fields } => self.aggregate(
                StaticType::Struct {
                    type_id: 0,
                    name: name.clone(),
                },
                fields,
                function,
            ),
            AotExpr::FieldAccess {
                object,
                field,
                field_ty,
            } => self.field_access(object, field, field_ty, function),
            AotExpr::Convert { value, target_ty }
                if value.get_type() == *target_ty && *target_ty == StaticType::Str =>
            {
                self.expr(value, function)
            }
            AotExpr::Convert { value, target_ty }
                if matches!(value.as_ref(), AotExpr::CallBuiltin { builtin: AotBuiltinOp::Size, args, .. } if args.len() == 1)
                    && matches!(target_ty, StaticType::Tuple(_)) =>
            {
                let AotExpr::CallBuiltin { builtin, args, .. } = value.as_ref() else {
                    unreachable!()
                };
                self.expr(
                    &AotExpr::CallBuiltin {
                        builtin: *builtin,
                        args: args.clone(),
                        return_ty: target_ty.clone(),
                    },
                    function,
                )
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
