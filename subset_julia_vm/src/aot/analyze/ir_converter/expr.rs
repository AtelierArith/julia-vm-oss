#![allow(clippy::cast_sign_loss)] // known-safe index/counter casts (i64->usize)

use super::*;
use crate::aot::native_calls::{
    classify_direct_native_call, classify_module_native_call, reject_unsupported_native_call,
};
use crate::span::Span;

impl<'a> IrConverter<'a> {
    fn global_float_constant_expr(name: &str) -> Option<AotExpr> {
        match name {
            "Inf" | "Inf64" => Some(AotExpr::LitF64(f64::INFINITY)),
            "Inf32" => Some(AotExpr::LitF32(f32::INFINITY)),
            "NaN" | "NaN64" => Some(AotExpr::LitF64(f64::NAN)),
            "NaN32" => Some(AotExpr::LitF32(f32::NAN)),
            _ => None,
        }
    }

    fn abstract_convert_target(name: &str) -> Option<crate::inference_core::CoreType> {
        let core = crate::inference_core::CoreType::from_julia_name(name);
        matches!(core, crate::inference_core::CoreType::Abstract(_)).then_some(core)
    }

    fn static_type_satisfies_abstract(
        value_ty: &StaticType,
        target: &crate::inference_core::CoreType,
    ) -> bool {
        let value_core =
            crate::inference_core::CoreType::from_julia_name(&value_ty.julia_type_name());
        crate::inference_core::CoreSubtypeEngine::new().is_subtype(&value_core, target)
    }

    fn convert_complex_constructor_arg(value: AotExpr, target_ty: &StaticType) -> AotExpr {
        if value.get_type() == *target_ty {
            value
        } else {
            AotExpr::Convert {
                value: Box::new(value),
                target_ty: target_ty.clone(),
            }
        }
    }

    fn zero_for_static_type(ty: &StaticType) -> Option<AotExpr> {
        match ty {
            StaticType::F64 => Some(AotExpr::LitF64(0.0)),
            StaticType::F32 => Some(AotExpr::LitF32(0.0)),
            StaticType::I64 => Some(AotExpr::LitI64(0)),
            StaticType::I32 => Some(AotExpr::LitI32(0)),
            StaticType::I16
            | StaticType::I8
            | StaticType::U64
            | StaticType::U32
            | StaticType::U16
            | StaticType::U8 => Some(AotExpr::Convert {
                value: Box::new(AotExpr::LitI64(0)),
                target_ty: ty.clone(),
            }),
            _ => None,
        }
    }

    fn parametric_struct_constructor_expr(
        &self,
        function: &str,
        aot_args: &[AotExpr],
        arg_types: &[StaticType],
    ) -> Option<AotExpr> {
        let (name, field_types) = self
            .engine
            .parametric_constructor_info(function, arg_types)?;
        if field_types.len() != aot_args.len() {
            return None;
        }
        let fields = aot_args
            .iter()
            .cloned()
            .zip(field_types)
            .map(|(arg, target_ty)| {
                if matches!(target_ty, StaticType::Any) || arg.get_type() == target_ty {
                    arg
                } else {
                    AotExpr::Convert {
                        value: Box::new(arg),
                        target_ty,
                    }
                }
            })
            .collect();
        Some(AotExpr::StructNew { name, fields })
    }

    fn convert_tuple_tail_call(
        &self,
        tuple_expr: &Expr,
        start_expr: &Expr,
        span: Span,
    ) -> AotResult<AotExpr> {
        let tuple_ty = self.engine.infer_expr_type(tuple_expr);
        let StaticType::Tuple(elements) = tuple_ty else {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "AoT tuple destructuring rest/splat target requires a static tuple RHS, got {} (Issue #7391)",
                    tuple_ty
                ))
                .with_span(span)
                .with_workaround(
                    "return or bind a concrete tuple before using `rest...`, or run this code on the VM",
                ),
            ));
        };

        let Expr::Literal(Literal::Int(start_index), _) = start_expr else {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(
                    "AoT tuple destructuring rest/splat target requires a constant tuple-tail start index (Issue #7391)",
                )
                .with_span(span),
            ));
        };
        if *start_index < 1 || (*start_index as usize) > elements.len() + 1 {
            return Err(AotError::CodegenError(format!(
                "AoT tuple tail start index {} is out of bounds for tuple length {} (Issue #7391)",
                start_index,
                elements.len()
            )));
        }

        let aot_tuple = self.convert_expr(tuple_expr)?;
        let mut tail_elements = Vec::new();
        for one_based_index in (*start_index as usize)..=elements.len() {
            tail_elements.push(AotExpr::Index {
                array: Box::new(aot_tuple.clone()),
                indices: vec![AotExpr::LitI64(one_based_index as i64)],
                elem_ty: elements[one_based_index - 1].clone(),
                is_tuple: true,
            });
        }

        Ok(AotExpr::TupleLit {
            elements: tail_elements,
        })
    }

    fn apply_callsite_inline_policy(expr: AotExpr, policy: AotInlinePolicy) -> AotExpr {
        match expr {
            AotExpr::CallStatic {
                function,
                args,
                return_ty,
                inline_policy,
            } => AotExpr::CallStatic {
                function,
                args: args
                    .into_iter()
                    .map(|arg| Self::apply_callsite_inline_policy(arg, policy))
                    .collect(),
                return_ty,
                inline_policy: if inline_policy == AotInlinePolicy::Auto {
                    policy
                } else {
                    inline_policy
                },
            },
            AotExpr::CallDynamic { function, args } => AotExpr::CallDynamic {
                function,
                args: args
                    .into_iter()
                    .map(|arg| Self::apply_callsite_inline_policy(arg, policy))
                    .collect(),
            },
            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } => AotExpr::CallBuiltin {
                builtin,
                args: args
                    .into_iter()
                    .map(|arg| Self::apply_callsite_inline_policy(arg, policy))
                    .collect(),
                return_ty,
            },
            AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty,
            } => AotExpr::BinOpStatic {
                op,
                left: Box::new(Self::apply_callsite_inline_policy(*left, policy)),
                right: Box::new(Self::apply_callsite_inline_policy(*right, policy)),
                result_ty,
            },
            AotExpr::BinOpDynamic { op, left, right } => AotExpr::BinOpDynamic {
                op,
                left: Box::new(Self::apply_callsite_inline_policy(*left, policy)),
                right: Box::new(Self::apply_callsite_inline_policy(*right, policy)),
            },
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => AotExpr::UnaryOp {
                op,
                operand: Box::new(Self::apply_callsite_inline_policy(*operand, policy)),
                result_ty,
            },
            AotExpr::ArrayLit {
                elements,
                elem_ty,
                shape,
            } => AotExpr::ArrayLit {
                elements: elements
                    .into_iter()
                    .map(|elem| Self::apply_callsite_inline_policy(elem, policy))
                    .collect(),
                elem_ty,
                shape,
            },
            AotExpr::SetFromIter { iter, elem_ty } => AotExpr::SetFromIter {
                iter: Box::new(Self::apply_callsite_inline_policy(*iter, policy)),
                elem_ty,
            },
            AotExpr::TupleLit { elements } => AotExpr::TupleLit {
                elements: elements
                    .into_iter()
                    .map(|elem| Self::apply_callsite_inline_policy(elem, policy))
                    .collect(),
            },
            AotExpr::NamedTupleLit { fields } => AotExpr::NamedTupleLit {
                fields: fields
                    .into_iter()
                    .map(|(name, field)| (name, Self::apply_callsite_inline_policy(field, policy)))
                    .collect(),
            },
            AotExpr::Comprehension {
                body,
                var,
                iter,
                filter,
                elem_ty,
            } => AotExpr::Comprehension {
                body: Box::new(Self::apply_callsite_inline_policy(*body, policy)),
                var,
                iter: Box::new(Self::apply_callsite_inline_policy(*iter, policy)),
                filter: filter
                    .map(|filter| Box::new(Self::apply_callsite_inline_policy(*filter, policy))),
                elem_ty,
            },
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                elem_ty,
            } => AotExpr::MultiComprehension {
                body: Box::new(Self::apply_callsite_inline_policy(*body, policy)),
                iterations: iterations
                    .into_iter()
                    .map(|(var, iter)| (var, Self::apply_callsite_inline_policy(iter, policy)))
                    .collect(),
                filter: filter
                    .map(|filter| Box::new(Self::apply_callsite_inline_policy(*filter, policy))),
                elem_ty,
            },
            AotExpr::Generator {
                body,
                var,
                iter,
                filter,
                elem_ty,
            } => AotExpr::Generator {
                body: Box::new(Self::apply_callsite_inline_policy(*body, policy)),
                var,
                iter: Box::new(Self::apply_callsite_inline_policy(*iter, policy)),
                filter: filter
                    .map(|filter| Box::new(Self::apply_callsite_inline_policy(*filter, policy))),
                elem_ty,
            },
            AotExpr::StructNew { name, fields } => AotExpr::StructNew {
                name,
                fields: fields
                    .into_iter()
                    .map(|field| Self::apply_callsite_inline_policy(field, policy))
                    .collect(),
            },
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple,
            } => AotExpr::Index {
                array: Box::new(Self::apply_callsite_inline_policy(*array, policy)),
                indices: indices
                    .into_iter()
                    .map(|index| Self::apply_callsite_inline_policy(index, policy))
                    .collect(),
                elem_ty,
                is_tuple,
            },
            AotExpr::Range {
                start,
                stop,
                step,
                elem_ty,
            } => AotExpr::Range {
                start: Box::new(Self::apply_callsite_inline_policy(*start, policy)),
                stop: Box::new(Self::apply_callsite_inline_policy(*stop, policy)),
                step: step.map(|step| Box::new(Self::apply_callsite_inline_policy(*step, policy))),
                elem_ty,
            },
            AotExpr::FieldAccess {
                object,
                field,
                field_ty,
            } => AotExpr::FieldAccess {
                object: Box::new(Self::apply_callsite_inline_policy(*object, policy)),
                field,
                field_ty,
            },
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => AotExpr::Ternary {
                condition: Box::new(Self::apply_callsite_inline_policy(*condition, policy)),
                then_expr: Box::new(Self::apply_callsite_inline_policy(*then_expr, policy)),
                else_expr: Box::new(Self::apply_callsite_inline_policy(*else_expr, policy)),
                result_ty,
            },
            AotExpr::Box(inner) => {
                AotExpr::Box(Box::new(Self::apply_callsite_inline_policy(*inner, policy)))
            }
            AotExpr::Unbox { value, target_ty } => AotExpr::Unbox {
                value: Box::new(Self::apply_callsite_inline_policy(*value, policy)),
                target_ty,
            },
            AotExpr::Convert { value, target_ty } => AotExpr::Convert {
                value: Box::new(Self::apply_callsite_inline_policy(*value, policy)),
                target_ty,
            },
            AotExpr::Lambda {
                params,
                body,
                captures,
                return_ty,
            } => AotExpr::Lambda {
                params,
                body,
                captures,
                return_ty,
            },
            other => other,
        }
    }

    fn infer_expr_type_with_locals(&self, expr: &Expr, locals: &TypeEnv) -> StaticType {
        match expr {
            Expr::Literal(lit, _) => self.engine.literal_type(lit),
            Expr::Var(name, _) => locals
                .get(name.as_str())
                .cloned()
                .or_else(|| self.engine.env.get(name.as_str()).cloned())
                .unwrap_or_else(|| self.engine.lookup_global_or_const(name)),
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let left_ty = self.infer_expr_type_with_locals(left, locals);
                let right_ty = self.infer_expr_type_with_locals(right, locals);
                self.engine.binop_result_type(op, &left_ty, &right_ty)
            }
            Expr::UnaryOp { op, operand, .. } => {
                let operand_ty = self.infer_expr_type_with_locals(operand, locals);
                self.engine.unaryop_result_type(op, &operand_ty)
            }
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                let arg_types: Vec<_> = args
                    .iter()
                    .chain(kwargs.iter().map(|(_, value)| value))
                    .map(|arg| self.infer_expr_type_with_locals(arg, locals))
                    .collect();
                self.engine.call_result_type(function, &arg_types)
            }
            Expr::Index { array, indices, .. } => {
                let arr_ty = self.infer_expr_type_with_locals(array, locals);
                if arr_ty.is_tuple() && indices.len() == 1 {
                    if let Expr::Literal(Literal::Int(idx), _) = &indices[0] {
                        return self.engine.tuple_element_type_at(&arr_ty, *idx as usize);
                    }
                }
                if let StaticType::Dict { value, .. } = &arr_ty {
                    return value.as_ref().clone();
                }
                self.engine.element_type(&arr_ty)
            }
            Expr::ArrayLiteral {
                elements, shape, ..
            } => {
                let elem_ty = elements
                    .iter()
                    .map(|elem| self.infer_expr_type_with_locals(elem, locals))
                    .reduce(|left, right| self.engine.join_types(&left, &right))
                    .unwrap_or(StaticType::Any);
                StaticType::Array {
                    element: Box::new(elem_ty),
                    ndims: Some(shape.len()),
                }
            }
            Expr::TupleLiteral { elements, .. } => StaticType::Tuple(
                elements
                    .iter()
                    .map(|elem| self.infer_expr_type_with_locals(elem, locals))
                    .collect(),
            ),
            Expr::NamedTupleLiteral { fields, .. } => StaticType::NamedTuple(
                fields
                    .iter()
                    .map(|(name, expr)| {
                        (
                            name.to_string(),
                            self.infer_expr_type_with_locals(expr, locals),
                        )
                    })
                    .collect(),
            ),
            Expr::Pair { key, value, .. } => StaticType::Tuple(vec![
                self.infer_expr_type_with_locals(key, locals),
                self.infer_expr_type_with_locals(value, locals),
            ]),
            Expr::DictLiteral { pairs, .. } => {
                let pair_types: Vec<_> = pairs
                    .iter()
                    .map(|(key, value)| {
                        StaticType::Tuple(vec![
                            self.infer_expr_type_with_locals(key, locals),
                            self.infer_expr_type_with_locals(value, locals),
                        ])
                    })
                    .collect();
                self.dict_constructor_type("Dict", &pair_types)
                    .unwrap_or(StaticType::Dict {
                        key: Box::new(StaticType::Any),
                        value: Box::new(StaticType::Any),
                    })
            }
            Expr::Range {
                start, stop, step, ..
            } => {
                let start_ty = self.infer_expr_type_with_locals(start, locals);
                let stop_ty = self.infer_expr_type_with_locals(stop, locals);
                let mut elem_ty = self.engine.unify_types(&start_ty, &stop_ty);
                if let Some(step_expr) = step {
                    let step_ty = self.infer_expr_type_with_locals(step_expr, locals);
                    elem_ty = self.engine.unify_types(&elem_ty, &step_ty);
                }
                StaticType::Range {
                    element: Box::new(elem_ty),
                }
            }
            Expr::FieldAccess { object, field, .. } => {
                let obj_ty = self.infer_expr_type_with_locals(object, locals);
                self.engine.field_type(&obj_ty, field)
            }
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                let then_ty = self.infer_expr_type_with_locals(then_expr, locals);
                let else_ty = self.infer_expr_type_with_locals(else_expr, locals);
                self.engine.join_types(&then_ty, &else_ty)
            }
            _ => self.engine.infer_expr_type(expr),
        }
    }

    fn set_constructor_element_type(
        &self,
        function: &str,
        arg_types: &[StaticType],
    ) -> Option<StaticType> {
        crate::aot::inference::TypeInferenceEngine::set_constructor_element_type(
            function, arg_types,
        )
    }

    fn set_codegen_hashable_type(ty: &StaticType) -> bool {
        matches!(
            ty,
            StaticType::I64
                | StaticType::I128
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U128
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
                | StaticType::Bool
                | StaticType::Char
                | StaticType::Str
        )
    }

    fn dict_constructor_type(
        &self,
        function: &str,
        arg_types: &[StaticType],
    ) -> Option<StaticType> {
        crate::aot::inference::TypeInferenceEngine::dict_constructor_type(function, arg_types)
    }

    fn dict_constructor_expr(
        &self,
        function: &str,
        aot_args: &[AotExpr],
        arg_types: &[StaticType],
        span: Span,
    ) -> AotResult<AotExpr> {
        let dict_ty = self
            .dict_constructor_type(function, arg_types)
            .ok_or_else(|| {
                AotError::UnsupportedInstruction(
                    UnsupportedInstructionDiagnostic::new(format!(
                        "AoT codegen supports `{function}` construction only from static Pair arguments or typed empty Dict{{K,V}}() (Issue #7034)"
                    ))
                    .with_span(span)
                    .with_workaround(
                        "construct Dict from statically typed `key => value` pairs, use Dict{K,V}() for an empty Dict, or run this code on the VM",
                    ),
                )
            })?;

        let StaticType::Dict { key, value } = &dict_ty else {
            unreachable!("dict_constructor_type always returns Dict")
        };
        if !key.is_fully_static()
            || !value.is_fully_static()
            || !Self::set_codegen_hashable_type(key)
        {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "AoT Dict codegen requires a statically hashable key type and static value type; inferred `{}` (Issue #7034)",
                    dict_ty.julia_type_name()
                ))
                .with_span(span)
                .with_workaround(
                    "use a Dict with concrete integer, bool, char, or string keys and concrete values for AoT, or run this code on the VM",
                ),
            ));
        }

        Ok(AotExpr::CallBuiltin {
            builtin: AotBuiltinOp::Dict,
            args: aot_args.to_vec(),
            return_ty: dict_ty,
        })
    }

    fn set_constructor_expr(
        &self,
        function: &str,
        aot_args: &[AotExpr],
        arg_types: &[StaticType],
        span: Span,
    ) -> AotResult<AotExpr> {
        let element_ty = self
            .set_constructor_element_type(function, arg_types)
            .ok_or_else(|| {
                AotError::UnsupportedInstruction(
                    UnsupportedInstructionDiagnostic::new(format!(
                        "AoT codegen supports `{function}` construction only from zero or one static iterable argument (Issue #7035)"
                    ))
                    .with_span(span)
                    .with_workaround(
                        "construct Set from a statically typed iterable, use Set{T}() for an empty Set, or run this code on the VM",
                    ),
                )
            })?;

        if !element_ty.is_fully_static() || !Self::set_codegen_hashable_type(&element_ty) {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "AoT Set codegen requires a statically hashable element type; inferred `{}` (Issue #7035)",
                    element_ty.julia_type_name()
                ))
                .with_span(span)
                .with_workaround(
                    "use a Set with concrete integer, bool, char, or string elements for AoT, or run this code on the VM",
                ),
            ));
        }

        let iter = match aot_args {
            [] => AotExpr::ArrayLit {
                elements: vec![],
                elem_ty: element_ty.clone(),
                shape: vec![0],
            },
            [iter] => iter.clone(),
            _ => unreachable!("set_constructor_element_type accepted only zero or one argument"),
        };

        Ok(AotExpr::SetFromIter {
            iter: Box::new(iter),
            elem_ty: element_ty,
        })
    }

    fn convert_expr_with_locals(&self, expr: &Expr, locals: &TypeEnv) -> AotResult<AotExpr> {
        match expr {
            Expr::Var(name, _) => {
                if let Some(value) = self.engine.enum_member_values.get(name.as_str()) {
                    return Ok(AotExpr::LitI32(*value));
                }
                if let Some(ty) = locals.get(name.as_str()) {
                    return Ok(AotExpr::Var {
                        name: name.to_string(),
                        ty: ty.clone(),
                    });
                }
                self.convert_expr(expr)
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                if matches!(op, crate::ir::core::BinaryOp::Subtype) {
                    if let Some(folded) = self.try_fold_static_subtype(left, right) {
                        return Ok(AotExpr::LitBool(folded));
                    }
                }
                if let Some(folded) = self.try_fold_complex_literal(op, left, right) {
                    return Ok(folded);
                }

                let aot_left = self.convert_expr_with_locals(left, locals)?;
                let aot_right = self.convert_expr_with_locals(right, locals)?;
                let aot_op = AotBinOp::from(op);
                let left_ty = aot_left.get_type();
                let right_ty = aot_right.get_type();
                if left_ty.is_fully_static() && right_ty.is_fully_static() {
                    Ok(AotExpr::BinOpStatic {
                        op: aot_op,
                        left: Box::new(aot_left),
                        right: Box::new(aot_right),
                        result_ty: self.engine.binop_result_type(op, &left_ty, &right_ty),
                    })
                } else {
                    Ok(AotExpr::BinOpDynamic {
                        op: aot_op,
                        left: Box::new(aot_left),
                        right: Box::new(aot_right),
                    })
                }
            }
            Expr::UnaryOp { op, operand, .. } => {
                let aot_operand = self.convert_expr_with_locals(operand, locals)?;
                let operand_ty = aot_operand.get_type();
                Ok(AotExpr::UnaryOp {
                    op: AotUnaryOp::from(op),
                    operand: Box::new(aot_operand),
                    result_ty: self.engine.unaryop_result_type(op, &operand_ty),
                })
            }
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                let aot_args: Vec<_> = args
                    .iter()
                    .chain(kwargs.iter().map(|(_, value)| value))
                    .map(|arg| self.convert_expr_with_locals(arg, locals))
                    .collect::<AotResult<_>>()?;
                let arg_types: Vec<_> = aot_args.iter().map(AotExpr::get_type).collect();

                if aot_args.len() >= 2 {
                    if let Some(aot_op) = self.map_operator_to_binop(function) {
                        let mut iter = aot_args.into_iter();
                        let mut result = match iter.next() {
                            Some(first) => first,
                            None => {
                                // INTERNAL: `aot_args.len() >= 2` was just
                                // checked above, so `iter.next()` cannot be
                                // `None` here (Issue #10907).
                                return Err(AotError::InternalError(
                                    "operator call: aot_args.len() >= 2 was checked but iterator was empty"
                                        .to_string(),
                                ));
                            }
                        };
                        for arg in iter {
                            let left_ty = result.get_type();
                            let right_ty = arg.get_type();
                            result = AotExpr::BinOpStatic {
                                op: aot_op,
                                left: Box::new(result),
                                right: Box::new(arg),
                                result_ty: self
                                    .engine
                                    .binop_result_type_static(&aot_op, &left_ty, &right_ty),
                            };
                        }
                        return Ok(result);
                    }
                }

                if let Some(builtin) = AotBuiltinOp::from_name(function) {
                    return Ok(AotExpr::CallBuiltin {
                        builtin,
                        args: aot_args,
                        return_ty: builtin.return_type(&arg_types),
                    });
                }
                if let Some(expr) =
                    self.parametric_struct_constructor_expr(function, &aot_args, &arg_types)
                {
                    return Ok(expr);
                }
                if let Some(_struct_info) = self.typed.get_struct(function) {
                    return Ok(AotExpr::StructNew {
                        name: function.to_string(),
                        fields: aot_args,
                    });
                }

                if arg_types.iter().all(StaticType::is_fully_static) {
                    let known_return_ty = self.get_function_return_type(function, &arg_types);
                    let inferred_return_ty = self.engine.call_result_type(function, &arg_types);
                    Ok(AotExpr::CallStatic {
                        function: function.to_string(),
                        args: aot_args,
                        return_ty: known_return_ty.unwrap_or(inferred_return_ty),
                        inline_policy: AotInlinePolicy::Auto,
                    })
                } else {
                    Ok(AotExpr::CallDynamic {
                        function: function.to_string(),
                        args: aot_args,
                    })
                }
            }
            Expr::Index { array, indices, .. } => {
                let arr_ty = self.infer_expr_type_with_locals(array, locals);
                let aot_array = self.convert_expr_with_locals(array, locals)?;
                let aot_indices: Vec<_> = indices
                    .iter()
                    .map(|idx| self.convert_expr_with_locals(idx, locals))
                    .collect::<AotResult<_>>()?;
                let elem_ty = if arr_ty.is_tuple() && indices.len() == 1 {
                    if let Expr::Literal(Literal::Int(idx), _) = &indices[0] {
                        self.engine.tuple_element_type_at(&arr_ty, *idx as usize)
                    } else {
                        self.engine.element_type(&arr_ty)
                    }
                } else {
                    self.engine.element_type(&arr_ty)
                };
                Ok(AotExpr::Index {
                    array: Box::new(aot_array),
                    indices: aot_indices,
                    elem_ty,
                    is_tuple: arr_ty.is_tuple(),
                })
            }
            Expr::ArrayLiteral {
                elements, shape, ..
            } => {
                let aot_elements: Vec<_> = elements
                    .iter()
                    .map(|elem| self.convert_expr_with_locals(elem, locals))
                    .collect::<AotResult<_>>()?;
                let elem_ty = aot_elements
                    .iter()
                    .map(AotExpr::get_type)
                    .reduce(|left, right| self.engine.join_types(&left, &right))
                    .unwrap_or(StaticType::Any);
                let aot_shape = if shape.is_empty() {
                    vec![elements.len()]
                } else {
                    shape.clone()
                };
                Ok(AotExpr::ArrayLit {
                    elements: aot_elements,
                    elem_ty,
                    shape: aot_shape,
                })
            }
            Expr::TupleLiteral { elements, .. } => Ok(AotExpr::TupleLit {
                elements: elements
                    .iter()
                    .map(|elem| self.convert_expr_with_locals(elem, locals))
                    .collect::<AotResult<_>>()?,
            }),
            Expr::NamedTupleLiteral { fields, .. } => Ok(AotExpr::NamedTupleLit {
                fields: fields
                    .iter()
                    .map(|(name, expr)| {
                        Ok((
                            name.to_string(),
                            self.convert_expr_with_locals(expr, locals)?,
                        ))
                    })
                    .collect::<AotResult<_>>()?,
            }),
            Expr::Range {
                start, stop, step, ..
            } => {
                let aot_start = self.convert_expr_with_locals(start, locals)?;
                let aot_stop = self.convert_expr_with_locals(stop, locals)?;
                let aot_step = step
                    .as_ref()
                    .map(|s| self.convert_expr_with_locals(s, locals))
                    .transpose()?;
                let mut elem_ty = self
                    .engine
                    .unify_types(&aot_start.get_type(), &aot_stop.get_type());
                if let Some(step_expr) = &aot_step {
                    elem_ty = self.engine.unify_types(&elem_ty, &step_expr.get_type());
                }
                Ok(AotExpr::Range {
                    start: Box::new(aot_start),
                    stop: Box::new(aot_stop),
                    step: aot_step.map(Box::new),
                    elem_ty,
                })
            }
            Expr::Generator {
                body,
                var,
                iter,
                filter,
                ..
            } => self.convert_generator(body, var, iter, filter.as_deref()),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let aot_cond = self.convert_expr_with_locals(condition, locals)?;
                let aot_then = self.convert_expr_with_locals(then_expr, locals)?;
                let aot_else = self.convert_expr_with_locals(else_expr, locals)?;
                let result_ty = self
                    .engine
                    .unify_types(&aot_then.get_type(), &aot_else.get_type());
                Ok(AotExpr::Ternary {
                    condition: Box::new(aot_cond),
                    then_expr: Box::new(aot_then),
                    else_expr: Box::new(aot_else),
                    result_ty,
                })
            }
            _ => self.convert_expr(expr),
        }
    }

    fn convert_comprehension(
        &self,
        body: &Expr,
        iterations: &[(crate::ir::core::InternedStr, Expr)],
        filter: Option<&Expr>,
    ) -> AotResult<AotExpr> {
        let mut locals = TypeEnv::new();
        let mut aot_iterations = Vec::with_capacity(iterations.len());
        for (var, iter) in iterations {
            let aot_iter = self.convert_expr_with_locals(iter, &locals)?;
            let elem_ty = self.engine.element_type(&aot_iter.get_type());
            locals.insert(var.to_string(), elem_ty);
            aot_iterations.push((var.to_string(), aot_iter));
        }

        let aot_filter = filter
            .map(|expr| self.convert_expr_with_locals(expr, &locals))
            .transpose()?;
        let aot_body = self.convert_expr_with_locals(body, &locals)?;
        let elem_ty = aot_body.get_type();

        if aot_iterations.len() == 1 {
            let (var, iter) = match aot_iterations.into_iter().next() {
                Some(single) => single,
                None => {
                    // INTERNAL: `aot_iterations.len() == 1` was just checked
                    // above, so `.next()` cannot be `None` here (Issue #10907).
                    return Err(AotError::InternalError(
                        "comprehension: aot_iterations.len() == 1 was checked but iterator was empty"
                            .to_string(),
                    ));
                }
            };
            Ok(AotExpr::Comprehension {
                body: Box::new(aot_body),
                var,
                iter: Box::new(iter),
                filter: aot_filter.map(Box::new),
                elem_ty,
            })
        } else {
            Ok(AotExpr::MultiComprehension {
                body: Box::new(aot_body),
                iterations: aot_iterations,
                filter: aot_filter.map(Box::new),
                elem_ty,
            })
        }
    }

    fn convert_generator(
        &self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
    ) -> AotResult<AotExpr> {
        let mut locals = TypeEnv::new();
        let aot_iter = self.convert_expr_with_locals(iter, &locals)?;
        let elem_ty = self.engine.element_type(&aot_iter.get_type());
        locals.insert(var.to_string(), elem_ty);

        let aot_filter = filter
            .map(|expr| self.convert_expr_with_locals(expr, &locals))
            .transpose()?;
        let aot_body = self.convert_expr_with_locals(body, &locals)?;
        let elem_ty = aot_body.get_type();

        Ok(AotExpr::Generator {
            body: Box::new(aot_body),
            var: var.to_string(),
            iter: Box::new(aot_iter),
            filter: aot_filter.map(Box::new),
            elem_ty,
        })
    }

    fn get_function_return_type(&self, name: &str, arg_types: &[StaticType]) -> Option<StaticType> {
        if let Some(typed_funcs) = self.typed.get_functions(name) {
            // Try to find a matching signature
            for typed_func in typed_funcs {
                let sig = &typed_func.signature;
                // Check if parameter count matches
                if sig.param_types.len() == arg_types.len() {
                    // Check if all argument types match (with Any being a wildcard)
                    let all_match = sig.param_types.iter().zip(arg_types.iter()).all(|(p, a)| {
                        p == a || matches!(p, StaticType::Any) || matches!(a, StaticType::Any)
                    });
                    if all_match {
                        return Some(sig.return_type.clone());
                    }
                }
            }
            // Fall back to the first function with matching arity
            for typed_func in typed_funcs {
                if typed_func.signature.param_types.len() == arg_types.len() {
                    return Some(typed_func.signature.return_type.clone());
                }
            }
        }
        None
    }

    /// If expression is `Ref(x)`, return `x`; otherwise return the original expression.
    fn unwrap_ref_expr(expr: &Expr) -> &Expr {
        if let Expr::Call { function, args, .. } = expr {
            if function == "Ref" && args.len() == 1 {
                return &args[0];
            }
        }
        if let Expr::Builtin {
            name: crate::ir::core::BuiltinOp::Ref,
            args,
            ..
        } = expr
        {
            if args.len() == 1 {
                return &args[0];
            }
        }
        expr
    }

    fn broadcast_operator_impl_name(op: &str, lhs_ty: &StaticType, rhs_ty: &StaticType) -> String {
        fn suffix(ty: &StaticType) -> String {
            match ty {
                StaticType::Struct { name, .. }
                    if name == "Complex"
                        || matches!(
                            StaticType::complex_param_type_from_name(name),
                            Some(StaticType::F64)
                        ) =>
                {
                    "complex".to_string()
                }
                _ => ty.mangle_suffix(),
            }
        }

        format!(
            "{}_{}_{}",
            AotFunction::sanitize_function_name(op),
            suffix(lhs_ty),
            suffix(rhs_ty)
        )
    }

    /// Try converting `materialize(Broadcasted(...))` / `Broadcasted(...)` to static helper calls.
    fn try_convert_broadcast_call(
        &self,
        function: &str,
        args: &[Expr],
    ) -> AotResult<Option<AotExpr>> {
        match function {
            "materialize" if args.len() == 1 => {
                if let Expr::Call {
                    function: inner_fn,
                    args: inner_args,
                    ..
                } = &args[0]
                {
                    if inner_fn == "Broadcasted" {
                        return self.convert_broadcasted_call(inner_args);
                    }
                }
                Ok(None)
            }
            "Broadcasted" => self.convert_broadcasted_call(args),
            _ => Ok(None),
        }
    }

    /// Convert `Broadcasted(fn_ref, (args...))` into AoT broadcast helper calls.
    fn convert_broadcasted_call(&self, args: &[Expr]) -> AotResult<Option<AotExpr>> {
        if args.len() != 2 {
            return Ok(None);
        }

        let fn_name = match &args[0] {
            Expr::FunctionRef { name, .. } => name.to_string(),
            Expr::Var(name, _) => name.to_string(),
            Expr::Literal(Literal::Str(s), _) => s.clone(),
            _ => return Ok(None),
        };

        let tuple_args: Vec<&Expr> = match &args[1] {
            Expr::TupleLiteral { elements, .. } => elements.iter().collect(),
            other => vec![other],
        };

        if tuple_args.len() != 2 {
            return Ok(None);
        }

        let lhs_expr = Self::unwrap_ref_expr(tuple_args[0]);
        let rhs_expr = Self::unwrap_ref_expr(tuple_args[1]);

        let lhs_aot = if let Expr::Call {
            function: inner_fn,
            args: inner_args,
            ..
        } = lhs_expr
        {
            if let Some(inner) = self.try_convert_broadcast_call(inner_fn, inner_args)? {
                inner
            } else {
                self.convert_expr(lhs_expr)?
            }
        } else {
            self.convert_expr(lhs_expr)?
        };
        let rhs_aot = if let Expr::Call {
            function: inner_fn,
            args: inner_args,
            ..
        } = rhs_expr
        {
            if let Some(inner) = self.try_convert_broadcast_call(inner_fn, inner_args)? {
                inner
            } else {
                self.convert_expr(rhs_expr)?
            }
        } else {
            self.convert_expr(rhs_expr)?
        };

        let lhs_ty = lhs_aot.get_type();
        let rhs_ty = rhs_aot.get_type();

        let shape = |ty: &StaticType| -> usize {
            match ty {
                StaticType::Array { ndims: Some(n), .. } => *n,
                StaticType::Array { ndims: None, .. } => 1,
                _ => 0,
            }
        };
        let elem_ty = |ty: &StaticType| -> StaticType {
            if let StaticType::Array { element, .. } = ty {
                (**element).clone()
            } else {
                ty.clone()
            }
        };

        // scalar .* vector
        if fn_name == "*" && shape(&lhs_ty) == 0 && shape(&rhs_ty) == 1 {
            let rhs_elem_ty = elem_ty(&rhs_ty);
            let result_elem =
                self.engine
                    .binop_result_type_static(&AotBinOp::Mul, &lhs_ty, &rhs_elem_ty);
            let mul_impl = Self::broadcast_operator_impl_name("*", &lhs_ty, &rhs_elem_ty);
            return Ok(Some(AotExpr::CallStatic {
                function: "__aot_broadcast_mul_scalar_vec".to_string(),
                args: vec![
                    AotExpr::Var {
                        name: mul_impl,
                        ty: StaticType::Any,
                    },
                    lhs_aot,
                    rhs_aot,
                ],
                return_ty: StaticType::Array {
                    element: Box::new(result_elem),
                    ndims: Some(1),
                },
                inline_policy: AotInlinePolicy::Auto,
            }));
        }

        // row_matrix .+ vector (column expansion)
        if fn_name == "+" && shape(&lhs_ty) == 2 && shape(&rhs_ty) == 1 {
            let lhs_elem_ty = elem_ty(&lhs_ty);
            let rhs_elem_ty = elem_ty(&rhs_ty);
            let result_elem =
                self.engine
                    .binop_result_type_static(&AotBinOp::Add, &lhs_elem_ty, &rhs_elem_ty);
            let add_impl = Self::broadcast_operator_impl_name("+", &lhs_elem_ty, &rhs_elem_ty);
            return Ok(Some(AotExpr::CallStatic {
                function: "__aot_broadcast_add_row_vec".to_string(),
                args: vec![
                    AotExpr::Var {
                        name: add_impl,
                        ty: StaticType::Any,
                    },
                    lhs_aot,
                    rhs_aot,
                ],
                return_ty: StaticType::Array {
                    element: Box::new(result_elem),
                    ndims: Some(2),
                },
                inline_policy: AotInlinePolicy::Auto,
            }));
        }

        // 1D .op 1D outer product: row ⊕ column → 2D matrix (Issue #3410).
        // This handles patterns like `xs' .+ im .* ys` where both sides are 1D vectors.
        if shape(&lhs_ty) == 1 && shape(&rhs_ty) == 1 {
            let lhs_elem_ty = elem_ty(&lhs_ty);
            let rhs_elem_ty = elem_ty(&rhs_ty);
            let binop = match fn_name.as_str() {
                "+" => AotBinOp::Add,
                "-" => AotBinOp::Sub,
                "*" => AotBinOp::Mul,
                "/" => AotBinOp::Div,
                _ => AotBinOp::Add, // default
            };
            let result_elem =
                self.engine
                    .binop_result_type_static(&binop, &lhs_elem_ty, &rhs_elem_ty);
            let fn_impl = Self::broadcast_operator_impl_name(&fn_name, &lhs_elem_ty, &rhs_elem_ty);
            return Ok(Some(AotExpr::CallStatic {
                function: "__aot_broadcast_outer_product".to_string(),
                args: vec![
                    AotExpr::Var {
                        name: fn_impl,
                        ty: StaticType::Any,
                    },
                    lhs_aot,
                    rhs_aot,
                ],
                return_ty: StaticType::Array {
                    element: Box::new(result_elem),
                    ndims: Some(2),
                },
                inline_policy: AotInlinePolicy::Auto,
            }));
        }

        // matrix .(f, Ref(scalar))
        if shape(&lhs_ty) == 2 && shape(&rhs_ty) == 0 {
            let matrix_elem_ty = elem_ty(&lhs_ty);
            let return_elem_ty = self
                .get_function_return_type(&fn_name, &[matrix_elem_ty.clone(), rhs_ty.clone()])
                .unwrap_or_else(|| {
                    self.engine
                        .call_result_type(&fn_name, &[matrix_elem_ty.clone(), rhs_ty.clone()])
                });

            // Only use mangled name if the function has multiple dispatch methods.
            // For single-method functions, the codegen emits the sanitized name directly.
            let has_multiple_methods = self
                .typed
                .get_functions(&fn_name)
                .is_some_and(|methods| methods.len() > 1);
            let func_ref_name = if has_multiple_methods {
                // Use mangled name that matches the actual element types
                format!(
                    "{}_{}_{}",
                    AotFunction::sanitize_function_name(&fn_name),
                    matrix_elem_ty.mangle_suffix(),
                    rhs_ty.mangle_suffix()
                )
            } else {
                AotFunction::sanitize_function_name(&fn_name)
            };

            return Ok(Some(AotExpr::CallStatic {
                function: "__aot_broadcast_call_matrix_scalar_2".to_string(),
                args: vec![
                    AotExpr::Var {
                        name: func_ref_name,
                        ty: StaticType::Any,
                    },
                    lhs_aot,
                    rhs_aot,
                ],
                return_ty: StaticType::Array {
                    element: Box::new(return_elem_ty),
                    ndims: Some(2),
                },
                inline_policy: AotInlinePolicy::Auto,
            }));
        }

        Ok(None)
    }

    /// Collect free variables in an expression (variables used but not defined in scope)
    fn collect_free_variables(&self, expr: &Expr, bound: &HashSet<String>) -> HashSet<String> {
        let mut free = HashSet::new();
        self.collect_free_variables_impl(expr, bound, &mut free);
        free
    }

    /// Implementation of free variable collection
    fn collect_free_variables_impl(
        &self,
        expr: &Expr,
        bound: &HashSet<String>,
        free: &mut HashSet<String>,
    ) {
        match expr {
            Expr::Var(name, _) if !bound.contains(name.as_str()) => {
                free.insert(name.to_string());
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_free_variables_impl(left, bound, free);
                self.collect_free_variables_impl(right, bound, free);
            }
            Expr::UnaryOp { operand, .. } => {
                self.collect_free_variables_impl(operand, bound, free);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.collect_free_variables_impl(arg, bound, free);
                }
            }
            Expr::ArrayLiteral { elements, .. } => {
                for elem in elements {
                    self.collect_free_variables_impl(elem, bound, free);
                }
            }
            Expr::Index { array, indices, .. } => {
                self.collect_free_variables_impl(array, bound, free);
                for idx in indices {
                    self.collect_free_variables_impl(idx, bound, free);
                }
            }
            Expr::Range {
                start, stop, step, ..
            } => {
                self.collect_free_variables_impl(start, bound, free);
                self.collect_free_variables_impl(stop, bound, free);
                if let Some(s) = step {
                    self.collect_free_variables_impl(s, bound, free);
                }
            }
            Expr::FieldAccess { object, .. } => {
                self.collect_free_variables_impl(object, bound, free);
            }
            Expr::TupleLiteral { elements, .. } => {
                for elem in elements {
                    self.collect_free_variables_impl(elem, bound, free);
                }
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                self.collect_free_variables_impl(condition, bound, free);
                self.collect_free_variables_impl(then_expr, bound, free);
                self.collect_free_variables_impl(else_expr, bound, free);
            }
            Expr::Builtin { args, .. } => {
                for arg in args {
                    self.collect_free_variables_impl(arg, bound, free);
                }
            }
            // Other expressions don't contain free variables
            _ => {}
        }
    }

    /// Extract a human-readable error message from error()/throw() call arguments.
    ///
    /// When error() bodies are inlined, the arguments may reference variables from
    /// the original function scope that don't exist in the inlined context
    /// (e.g., `string(a, b, c, d)` from `error(a, b, c, d)` in base/error.jl).
    /// This method extracts string literal content directly from the Core IR
    /// expressions, avoiding undefined variable references (Issues #3405, #3406).
    fn extract_error_message(args: &[&Expr]) -> String {
        let mut parts = Vec::new();
        for arg in args {
            Self::collect_string_literals(arg, &mut parts);
        }
        if parts.is_empty() {
            "error".to_string()
        } else {
            parts.join("")
        }
    }

    /// Recursively collect string literal content from an expression tree.
    fn collect_string_literals(expr: &Expr, parts: &mut Vec<String>) {
        match expr {
            Expr::Literal(Literal::Str(s), _) => parts.push(s.clone()),
            Expr::Call { function, args, .. }
                // Recurse into nested calls like ErrorException(string(...))
                if (function == "ErrorException" || function == "string") => {
                    for arg in args {
                        Self::collect_string_literals(arg, parts);
                    }
                }
            _ => {}
        }
    }

    fn try_fold_complex_literal(
        &self,
        op: &crate::ir::core::BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Option<AotExpr> {
        use crate::ir::core::BinaryOp;
        if !matches!(op, BinaryOp::Add) {
            return None;
        }

        let re = match left {
            Expr::Literal(lit, _) => Self::literal_numeric_to_f64(lit)?,
            _ => return None,
        };

        let (imag_coeff_expr, imag_unit_expr) = match right {
            Expr::BinaryOp {
                op: BinaryOp::Mul,
                left,
                right,
                ..
            } => (&**left, &**right),
            _ => return None,
        };

        let im = match imag_coeff_expr {
            Expr::Literal(lit, _) if Self::is_im_unit_literal(imag_unit_expr) => {
                Self::literal_numeric_to_f64(lit)?
            }
            _ if Self::is_im_unit_literal(imag_coeff_expr) => match imag_unit_expr {
                Expr::Literal(lit, _) => Self::literal_numeric_to_f64(lit)?,
                _ => return None,
            },
            _ => return None,
        };

        Some(AotExpr::StructNew {
            name: "Complex{Float64}".to_string(),
            fields: vec![AotExpr::LitF64(re), AotExpr::LitF64(im)],
        })
    }

    /// Collect free variables from a block's statements
    fn collect_free_variables_block(
        &self,
        block: &Block,
        bound: &HashSet<String>,
    ) -> HashSet<String> {
        let mut free = HashSet::new();
        let mut local_bound = bound.clone();

        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { var, value, .. } => {
                    // First collect from value (before var is in scope)
                    let expr_free = self.collect_free_variables(value, &local_bound);
                    free.extend(expr_free);
                    // Then add var to bound set
                    local_bound.insert(var.clone());
                }
                Stmt::Expr { expr, .. } => {
                    let expr_free = self.collect_free_variables(expr, &local_bound);
                    free.extend(expr_free);
                }
                Stmt::Return { value: Some(v), .. } => {
                    let expr_free = self.collect_free_variables(v, &local_bound);
                    free.extend(expr_free);
                }
                _ => {}
            }
        }
        free
    }

    /// Convert a lambda function to AotExpr::Lambda
    ///
    /// This converts a lifted lambda function back to an inline closure expression,
    /// detecting captured variables from the outer scope.
    fn convert_lambda_function(&self, func: &Function) -> AotResult<AotExpr> {
        // Get parameter names and types
        let param_names: HashSet<String> = func.params.iter().map(|p| p.name.clone()).collect();
        let params: Vec<(String, StaticType)> = func
            .params
            .iter()
            .map(|p| {
                let ty = self.julia_type_to_static(&p.effective_type());
                (p.name.clone(), ty)
            })
            .collect();

        // Find free variables in the lambda body (captured from outer scope)
        let free_vars = self.collect_free_variables_block(&func.body, &param_names);

        // Convert free variables to captures with their types from outer scope
        let captures: Vec<(String, StaticType)> = free_vars
            .into_iter()
            .map(|name| {
                // Look up type in current environment (outer scope)
                let ty = self
                    .engine
                    .env
                    .get(&name)
                    .cloned()
                    .unwrap_or(StaticType::Any);
                (name, ty)
            })
            .collect();

        // Convert the body - currently AoT lambdas carry a single expression.
        let body_expr = if let Some(Stmt::Return {
            value: Some(expr), ..
        }) = func.body.stmts.first()
        {
            self.convert_expr(expr)?
        } else if func.body.stmts.len() == 1 {
            // Handle single expression statement
            if let Stmt::Expr { expr, .. } = &func.body.stmts[0] {
                self.convert_expr(expr)?
            } else {
                return Err(AotError::UnsupportedInstruction(
                    UnsupportedInstructionDiagnostic::new(format!(
                        "AoT lambda `{}` body is not a single expression or return expression",
                        func.name
                    ))
                    .with_span(func.body.span)
                    .with_workaround(
                        "rewrite the closure as a single expression, lift it to a named function supported by AoT, or run it on the VM",
                    ),
                ));
            }
        } else {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "AoT lambda `{}` has a multi-statement body, which is not supported yet",
                    func.name
                ))
                .with_span(func.body.span)
                .with_workaround(
                    "rewrite the closure as a single expression, lift it to a named function supported by AoT, or run it on the VM",
                ),
            ));
        };

        // Infer return type from body
        let return_ty = body_expr.get_type();

        Ok(AotExpr::Lambda {
            params,
            body: Box::new(body_expr),
            captures,
            return_ty,
        })
    }

    /// Convert a complete program
    /// Convert an expression
    pub(crate) fn convert_expr(&self, expr: &Expr) -> AotResult<AotExpr> {
        match expr {
            Expr::Literal(lit, _) => self.convert_literal(lit),

            Expr::Var(name, _) => {
                if let Some(value) = self.engine.enum_member_values.get(name.as_str()) {
                    return Ok(AotExpr::LitI32(*value));
                }
                if !self.engine.env.contains_key(name.as_str()) {
                    if let Some(constant) = Self::global_float_constant_expr(name) {
                        return Ok(constant);
                    }
                }
                let ty = self
                    .engine
                    .env
                    .get(name.as_str())
                    .cloned()
                    .unwrap_or_else(|| self.engine.lookup_global_or_const(name));
                Ok(AotExpr::Var {
                    name: name.to_string(),
                    ty,
                })
            }

            // Symbol literal `:foo` lowers to a Core IR `QuoteLiteral` wrapping
            // `Builtin { SymbolNew, ["foo"] }`. Carry the symbol as its interned
            // name string (the issue's "interned string carrier" design) so
            // display (`foo`, no colon) and symbol-vs-symbol equality are
            // correct. Quoted *expressions* (`:(a + b)`) build runtime Expr
            // objects and stay unsupported (Issue #7051).
            Expr::QuoteLiteral { constructor, span } => {
                if let Some(name) = Self::quote_symbol_name(constructor) {
                    Ok(AotExpr::LitStr(name))
                } else {
                    Err(AotError::UnsupportedInstruction(
                        UnsupportedInstructionDiagnostic::new(
                            "AoT codegen supports only bare Symbol literals (`:foo`); quoted expressions building runtime Expr objects are unsupported (Issue #7051)",
                        )
                        .with_span(*span)
                        .with_workaround("run quoted-expression code on the VM"),
                    ))
                }
            }

            // String interpolation: `"x = $x"` lowers to a Core IR
            // `StringConcat(["x = ", x])`. Lower it to the same `string(...)`
            // concat builtin (`AotBuiltinOp::StringConcat`), which renders each
            // part via Julia-faithful Display formatting. Without this arm the
            // expression fell through to the `LitNothing` catch-all and the
            // interpolation silently compiled to `()` (Issue #7052).
            Expr::StringConcat { parts, .. } => {
                let args = parts
                    .iter()
                    .map(|p| self.convert_expr(p))
                    .collect::<AotResult<Vec<_>>>()?;
                Ok(AotExpr::CallBuiltin {
                    builtin: AotBuiltinOp::StringConcat,
                    args,
                    return_ty: StaticType::Str,
                })
            }

            // Assignment expression returns the assigned value.
            // Side effects are handled at statement-flattening sites.
            Expr::AssignExpr { value, .. } => self.convert_expr(value),

            // Let blocks used as expressions require a sequence expression to preserve
            // side effects. Statement-position LetBlocks are flattened in stmt.rs.
            Expr::LetBlock {
                bindings,
                body,
                span,
            } => {
                if bindings.is_empty() && body.stmts.is_empty() {
                    return Ok(AotExpr::LitNothing);
                }
                if bindings.is_empty() && body.stmts.len() == 1 {
                    if let Some(Stmt::Expr { expr, .. }) = body.stmts.first() {
                        return self.convert_expr(expr);
                    }
                }
                // #7014 relaxation for the "nested function definitions + single
                // trailing expression" shape (Issue #9179). The #9103 generator-body
                // lift wraps a non-trivial generator body in a bindings-free let
                // block of the form
                //
                //     let
                //         function __gen_body_N(var); return <body>; end
                //         (__gen_body_N(var) for var in iter)
                //     end
                //
                // (see `lower_generator` / `lift_generator_body_as_nested`). When this
                // sits in expression position (a `collect(...)` / `sum(...)` argument),
                // reverse the lift by inlining the trivial call to the nested function
                // into the trailing expression, then convert the result. Inlining keeps
                // the body in the generator's own scope — matching the VM path and the
                // pre-#9103 codegen — instead of hoisting it to a standalone function
                // that AoT inference cannot specialize by the loop-variable type.
                //
                // The AoT pipeline normally reverses these lifts as a whole-program
                // pre-pass before inference (see `lift_reversal`), so this arm is a
                // backstop for any expression-position lift a caller did not normalize.
                if let Some(rewritten) =
                    crate::aot::analyze::lift_reversal::reverse_lifted_letblock(bindings, body)
                {
                    return self.convert_expr(&rewritten);
                }
                Err(AotError::UnsupportedInstruction(
                    UnsupportedInstructionDiagnostic::new(
                        "AoT codegen does not support expression-position begin/let blocks with side-effecting or multiple statements yet (Issue #7014)",
                    )
                    .with_span(*span)
                    .with_workaround(
                        "move the begin/let block to statement position before the call, or run this code on the VM",
                    ),
                ))
            }

            Expr::BinaryOp {
                op, left, right, ..
            } => {
                // The subtype operator `<:` is a type relation, not a value
                // comparison. When both operands are statically known type
                // names, const-fold the relation to a boolean literal; runtime
                // type values fall through to the dynamic gate (Issue #7037).
                if matches!(op, crate::ir::core::BinaryOp::Subtype) {
                    if let Some(folded) = self.try_fold_static_subtype(left, right) {
                        return Ok(AotExpr::LitBool(folded));
                    }
                }

                if let Some(folded) = self.try_fold_complex_literal(op, left, right) {
                    return Ok(folded);
                }

                // Convert operands first to get accurate types from AoT expressions
                // This is important for function calls where the engine doesn't know the return type
                let aot_left = self.convert_expr(left)?;
                let aot_right = self.convert_expr(right)?;
                let aot_op = AotBinOp::from(op);

                // Get types from the converted AoT expressions (more accurate than engine inference)
                let left_ty = aot_left.get_type();
                let right_ty = aot_right.get_type();

                // Determine if this is a static or dynamic operation
                if left_ty.is_fully_static() && right_ty.is_fully_static() {
                    let result_ty = self.engine.binop_result_type(op, &left_ty, &right_ty);
                    Ok(AotExpr::BinOpStatic {
                        op: aot_op,
                        left: Box::new(aot_left),
                        right: Box::new(aot_right),
                        result_ty,
                    })
                } else {
                    Ok(AotExpr::BinOpDynamic {
                        op: aot_op,
                        left: Box::new(aot_left),
                        right: Box::new(aot_right),
                    })
                }
            }

            Expr::UnaryOp { op, operand, .. } => {
                let operand_ty = self.engine.infer_expr_type(operand);
                let aot_operand = self.convert_expr(operand)?;
                let aot_op = AotUnaryOp::from(op);
                let result_ty = self.engine.unaryop_result_type(op, &operand_ty);

                Ok(AotExpr::UnaryOp {
                    op: aot_op,
                    operand: Box::new(aot_operand),
                    result_ty,
                })
            }

            Expr::Call {
                function,
                args,
                kwargs,
                span,
                ..
            } => {
                if function == "#__sjulia_declare_const__" {
                    return Err(AotError::UnsupportedInstruction(
                        UnsupportedInstructionDiagnostic::new(
                            "AoT codegen does not support top-level `const` declarations or const redefinition policy yet (Issue #7061)",
                        )
                        .with_span(*span)
                        .with_workaround(
                            "use a plain local binding in a `let` block for AoT, or run const-sensitive code on the VM",
                        ),
                    ));
                }

                if function == "#__sjulia_tuple_tail__" && args.len() == 2 && kwargs.is_empty() {
                    return self.convert_tuple_tail_call(&args[0], &args[1], *span);
                }

                if matches!(
                    function.as_str(),
                    "#__sjulia_inline__" | "#__sjulia_noinline__"
                ) && args.len() == 1
                    && kwargs.is_empty()
                {
                    let policy = if function == "#__sjulia_inline__" {
                        AotInlinePolicy::Always
                    } else {
                        AotInlinePolicy::Never
                    };

                    // If annotations are nested, preserve upstream's innermost-precedence rule.
                    if let Expr::Call {
                        function: inner_function,
                        ..
                    } = &args[0]
                    {
                        if matches!(
                            inner_function.as_str(),
                            "#__sjulia_inline__" | "#__sjulia_noinline__"
                        ) {
                            return self.convert_expr(&args[0]);
                        }
                    }

                    let inner = self.convert_expr(&args[0])?;
                    return Ok(Self::apply_callsite_inline_policy(inner, policy));
                }

                if let Some(boundary) = classify_direct_native_call(function, *span) {
                    reject_unsupported_native_call(&boundary)?;
                }

                // AoT broadcast lowering: materialize(Broadcasted(...)) -> static helper calls
                if let Some(broadcast_expr) = self.try_convert_broadcast_call(function, args)? {
                    return Ok(broadcast_expr);
                }

                // Build the positional argument list. When the callee is a user
                // function with keyword parameters, append each keyword in the
                // callee's declaration order — the provided value or the default
                // — so it lines up with the trailing positional parameters
                // emitted by `convert_function` (Issue #7042). Otherwise keep the
                // legacy "kwargs appended in call order" behavior.
                let call_args: Vec<&Expr> = match self.functions.get(function.as_str()).copied() {
                    Some(callee) if !callee.kwparams.is_empty() => {
                        let mut full: Vec<&Expr> = args.iter().collect();
                        for kwp in &callee.kwparams {
                            if kwp.is_varargs {
                                continue;
                            }
                            let provided =
                                kwargs.iter().find(|(k, _)| k == &kwp.name).map(|(_, v)| v);
                            full.push(provided.unwrap_or(&kwp.default));
                        }
                        full
                    }
                    _ => args.iter().chain(kwargs.iter().map(|(_, v)| v)).collect(),
                };

                // Ref(x) in broadcast contexts should remain scalar.
                if function == "Ref" && call_args.len() == 1 {
                    return self.convert_expr(call_args[0]);
                }

                // Special handling for error() and throw() — intercept BEFORE converting
                // arguments to avoid emitting undefined variable references from improperly
                // inlined function bodies (Issues #3405, #3406).
                if function == "error" || function == "throw" {
                    let message = Self::extract_error_message(&call_args);
                    return Ok(AotExpr::CallBuiltin {
                        builtin: AotBuiltinOp::Throw,
                        args: vec![AotExpr::LitStr(message)],
                        return_ty: StaticType::Nothing,
                    });
                }

                // Intercept range(start, stop; length=n) and emit linspace (Issue #3413).
                // The lowering phase inlines range() which produces broken nothing-dispatch code.
                // Instead, detect the keyword pattern and emit a call to the prelude linspace().
                if function == "range" && !kwargs.is_empty() {
                    // Check for `length` keyword argument
                    if let Some((_, length_expr)) = kwargs.iter().find(|(k, _)| k == "length") {
                        let aot_start = self.convert_expr(&args[0])?;
                        let aot_stop = if args.len() > 1 {
                            self.convert_expr(&args[1])?
                        } else {
                            AotExpr::LitF64(0.0)
                        };
                        let aot_length = self.convert_expr(length_expr)?;
                        return Ok(AotExpr::CallBuiltin {
                            builtin: AotBuiltinOp::Linspace,
                            args: vec![aot_start, aot_stop, aot_length],
                            return_ty: StaticType::Array {
                                element: Box::new(StaticType::F64),
                                ndims: Some(1),
                            },
                        });
                    }
                }

                let arg_types: Vec<_> = call_args
                    .iter()
                    .map(|a| self.engine.infer_expr_type(a))
                    .collect();
                let aot_args: Vec<_> = call_args
                    .iter()
                    .map(|a| self.convert_expr(a))
                    .collect::<AotResult<_>>()?;

                if function == "Dict"
                    || StaticType::parametric_type_parts(function)
                        .is_some_and(|(base, _)| base == "Dict")
                {
                    return self.dict_constructor_expr(function, &aot_args, &arg_types, *span);
                }

                if function == "Set"
                    || StaticType::parametric_type_parts(function)
                        .is_some_and(|(base, _)| base == "Set")
                {
                    return self.set_constructor_expr(function, &aot_args, &arg_types, *span);
                }

                if let Some(element_ty) = StaticType::complex_param_type_from_name(function) {
                    if !(1..=2).contains(&aot_args.len()) {
                        return Err(AotError::UnsupportedInstruction(
                            UnsupportedInstructionDiagnostic::new(format!(
                                "AoT codegen supports `{function}` construction with one or two positional arguments (Issue #7041)"
                            ))
                            .with_span(*span)
                            .with_workaround(
                                "call the Complex constructor with real and optional imaginary parts, or run this code on the VM",
                            ),
                        ));
                    }

                    let mut fields = Vec::with_capacity(2);
                    fields.push(Self::convert_complex_constructor_arg(
                        aot_args[0].clone(),
                        &element_ty,
                    ));
                    fields.push(if let Some(imag) = aot_args.get(1) {
                        Self::convert_complex_constructor_arg(imag.clone(), &element_ty)
                    } else {
                        Self::zero_for_static_type(&element_ty).ok_or_else(|| {
                            AotError::UnsupportedInstruction(
                                UnsupportedInstructionDiagnostic::new(format!(
                                    "AoT codegen cannot synthesize a zero imaginary part for `{function}` (Issue #7041)"
                                ))
                                .with_span(*span)
                                .with_workaround(
                                    "pass both real and imaginary constructor arguments explicitly",
                                ),
                            )
                        })?
                    });

                    return Ok(AotExpr::StructNew {
                        name: function.to_string(),
                        fields,
                    });
                }

                if let Some(expr) =
                    self.parametric_struct_constructor_expr(function, &aot_args, &arg_types)
                {
                    return Ok(expr);
                }

                if let Some(builtin @ (AotBuiltinOp::Zeros | AotBuiltinOp::Ones)) =
                    AotBuiltinOp::from_name(function)
                {
                    if let Some((element_ty, dim_args)) =
                        self.array_constructor_element_and_dims(function, &call_args)
                    {
                        let aot_dims: Vec<_> = dim_args
                            .iter()
                            .map(|arg| self.convert_expr(arg))
                            .collect::<AotResult<_>>()?;
                        let ndims = aot_dims.len();
                        return Ok(AotExpr::CallBuiltin {
                            builtin,
                            args: aot_dims,
                            return_ty: StaticType::Array {
                                element: Box::new(element_ty),
                                ndims: Some(ndims),
                            },
                        });
                    }
                }

                // Special handling for convert(Type, value) calls
                // These are generated by the lowering phase for return type coercion
                // Convert them to AotExpr::Convert for proper static type casting
                if function == "convert" && call_args.len() == 2 {
                    // First argument should be a type name (variable)
                    if let Expr::Var(type_name, _) = call_args[0] {
                        // Try to resolve the type name to a StaticType
                        if let Some(target_ty) = self.type_name_to_static(type_name) {
                            // Second argument is the value to convert
                            let value = aot_args[1].clone();
                            return Ok(AotExpr::Convert {
                                value: Box::new(value),
                                target_ty,
                            });
                        }
                        if let Some(target_core) = Self::abstract_convert_target(type_name) {
                            let value = aot_args[1].clone();
                            let value_ty = value.get_type();
                            if Self::static_type_satisfies_abstract(&value_ty, &target_core) {
                                return Ok(AotExpr::Convert {
                                    value: Box::new(value),
                                    target_ty: StaticType::Any,
                                });
                            }
                            return Err(AotError::UnsupportedInstruction(
                                UnsupportedInstructionDiagnostic::new(format!(
                                    "AoT codegen cannot lower convert({}, value::{}); abstract type conversion requires a statically known subtype",
                                    type_name, value_ty
                                ))
                                .with_span(*span)
                                .with_workaround(
                                    "return a value whose concrete type is known to satisfy the abstract annotation, or run this code on the VM",
                                ),
                            ));
                        }
                    }
                }

                // Special handling for type constructor calls: Float64(x), Int64(x), etc.
                // These are Julia-style type conversions that should be emitted as Rust casts
                if call_args.len() == 1 {
                    if let Some(target_ty) = self.type_name_to_static(function) {
                        let value = aot_args[0].clone();
                        return Ok(AotExpr::Convert {
                            value: Box::new(value),
                            target_ty,
                        });
                    }
                }

                // Lowered Julia aliases like `÷` arrive as `div(x, y)`. Keep
                // two-argument operator calls on the binary-op path so they use
                // the same result typing and codegen as infix expressions.
                if call_args.len() == 2 {
                    if let Some(aot_op) = self.map_operator_to_binop(function) {
                        let mut aot_args_iter = aot_args.into_iter();
                        let Some(left) = aot_args_iter.next() else {
                            return Err(AotError::InternalError(format!(
                                "operator `{function}` had no AoT arguments after conversion"
                            )));
                        };
                        let Some(right) = aot_args_iter.next() else {
                            return Err(AotError::InternalError(format!(
                                "operator `{function}` had one AoT argument after conversion"
                            )));
                        };

                        let left_ty = left.get_type();
                        let right_ty = right.get_type();
                        if left_ty.is_fully_static() && right_ty.is_fully_static() {
                            let result_ty = self
                                .engine
                                .binop_result_type_static(&aot_op, &left_ty, &right_ty);
                            return Ok(AotExpr::BinOpStatic {
                                op: aot_op,
                                left: Box::new(left),
                                right: Box::new(right),
                                result_ty,
                            });
                        }

                        return Ok(AotExpr::BinOpDynamic {
                            op: aot_op,
                            left: Box::new(left),
                            right: Box::new(right),
                        });
                    }
                }

                // Special handling for multi-argument operator calls: *(a, b, c) => ((a * b) * c)
                // Julia flattens chained operators like `a * b * c` into `*(a, b, c)` for method dispatch
                // We need to unfold these back to nested binary operations for Rust codegen
                if call_args.len() > 2 {
                    if let Some(aot_op) = self.map_operator_to_binop(function) {
                        // Unfold: *(a, b, c, d) => (((a * b) * c) * d)
                        let mut aot_args_iter = aot_args.into_iter();
                        let Some(mut result) = aot_args_iter.next() else {
                            return Err(AotError::InternalError(format!(
                                "operator `{function}` had no AoT arguments after conversion"
                            )));
                        };

                        for arg in aot_args_iter {
                            let left_ty = result.get_type();
                            let right_ty = arg.get_type();
                            let result_ty = self
                                .engine
                                .binop_result_type_static(&aot_op, &left_ty, &right_ty);
                            result = AotExpr::BinOpStatic {
                                op: aot_op,
                                left: Box::new(result),
                                right: Box::new(arg),
                                result_ty,
                            };
                        }
                        return Ok(result);
                    }
                }

                // Check if it's a builtin function
                if let Some(builtin) = AotBuiltinOp::from_name(function) {
                    let return_ty = builtin.return_type(&arg_types);
                    return Ok(AotExpr::CallBuiltin {
                        builtin,
                        args: aot_args,
                        return_ty,
                    });
                }

                // Check if it's a struct constructor
                if let Some(_struct_info) = self.typed.get_struct(function) {
                    return Ok(AotExpr::StructNew {
                        name: function.to_string(),
                        fields: aot_args,
                    });
                }

                // Check if all argument types are fully static
                let all_static = arg_types.iter().all(|t| t.is_fully_static());

                if all_static {
                    // First check if this is a user-defined function with known return type
                    // This is essential for recursive function calls
                    let known_return_ty = self.get_function_return_type(function, &arg_types);
                    let inferred_return_ty = self.engine.call_result_type(function, &arg_types);
                    if known_return_ty.is_none()
                        && matches!(inferred_return_ty, StaticType::Any)
                        && function.chars().next().is_some_and(char::is_uppercase)
                    {
                        return Err(AotError::UnsupportedInstruction(
                            UnsupportedInstructionDiagnostic::new(format!(
                                "AoT codegen cannot resolve constructor-like call `{function}`; parametric struct constructors are not supported yet (Issue #6975)",
                            ))
                            .with_span(*span)
                            .with_workaround(
                                "use a non-parametric struct with concrete field types for AoT, or run this code on the VM",
                            ),
                        ));
                    }
                    let return_ty = known_return_ty.unwrap_or(inferred_return_ty);
                    Ok(AotExpr::CallStatic {
                        function: function.to_string(),
                        args: aot_args,
                        return_ty,
                        inline_policy: AotInlinePolicy::Auto,
                    })
                } else {
                    Ok(AotExpr::CallDynamic {
                        function: function.to_string(),
                        args: aot_args,
                    })
                }
            }

            Expr::ModuleCall {
                module,
                function,
                span,
                ..
            } => {
                if let Some(boundary) = classify_module_native_call(module, function, *span) {
                    reject_unsupported_native_call(&boundary)?;
                }
                Ok(AotExpr::LitNothing)
            }

            Expr::ArrayLiteral {
                elements, shape, ..
            } => {
                let aot_elements: Vec<_> = elements
                    .iter()
                    .map(|e| self.convert_expr(e))
                    .collect::<AotResult<_>>()?;

                let elem_ty = if elements.is_empty() {
                    StaticType::Any
                } else {
                    self.engine.infer_expr_type(&elements[0])
                };

                // Use the shape from the Core IR, or default to 1D if empty
                let aot_shape = if shape.is_empty() {
                    vec![elements.len()]
                } else {
                    shape.clone()
                };

                Ok(AotExpr::ArrayLit {
                    elements: aot_elements,
                    elem_ty,
                    shape: aot_shape,
                })
            }

            Expr::TupleLiteral { elements, .. } => {
                let aot_elements: Vec<_> = elements
                    .iter()
                    .map(|e| self.convert_expr(e))
                    .collect::<AotResult<_>>()?;

                Ok(AotExpr::TupleLit {
                    elements: aot_elements,
                })
            }

            Expr::NamedTupleLiteral { fields, .. } => {
                let aot_fields: Vec<_> = fields
                    .iter()
                    .map(|(name, expr)| Ok((name.to_string(), self.convert_expr(expr)?)))
                    .collect::<AotResult<_>>()?;

                Ok(AotExpr::NamedTupleLit { fields: aot_fields })
            }

            Expr::Pair { key, value, .. } => Ok(AotExpr::TupleLit {
                elements: vec![self.convert_expr(key)?, self.convert_expr(value)?],
            }),

            Expr::DictLiteral { pairs, span } => {
                let mut aot_pairs = Vec::with_capacity(pairs.len());
                let mut pair_types = Vec::with_capacity(pairs.len());
                for (key, value) in pairs {
                    let key_ty = self.engine.infer_expr_type(key);
                    let value_ty = self.engine.infer_expr_type(value);
                    pair_types.push(StaticType::Tuple(vec![key_ty, value_ty]));
                    aot_pairs.push(AotExpr::TupleLit {
                        elements: vec![self.convert_expr(key)?, self.convert_expr(value)?],
                    });
                }
                self.dict_constructor_expr("Dict", &aot_pairs, &pair_types, *span)
            }

            Expr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            } => self.convert_comprehension(
                body,
                &[(*var, iter.as_ref().clone())],
                filter.as_deref(),
            ),

            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => self.convert_comprehension(body, iterations, filter.as_deref()),

            Expr::Generator {
                body,
                var,
                iter,
                filter,
                ..
            } => self.convert_generator(body, var, iter, filter.as_deref()),

            // Typed empty array literal: Int64[], Float64[], etc.
            Expr::TypedEmptyArray { element_type, .. } => {
                let elem_ty = self
                    .type_name_to_static(element_type)
                    .unwrap_or(StaticType::Any);
                Ok(AotExpr::ArrayLit {
                    elements: vec![],
                    elem_ty,
                    shape: vec![0],
                })
            }

            Expr::Index { array, indices, .. } => {
                let arr_ty = self.engine.infer_expr_type(array);
                let aot_array = self.convert_expr(array)?;

                // Convert all indices for multidimensional array support
                let aot_indices: Vec<AotExpr> = indices
                    .iter()
                    .map(|idx| self.convert_expr(idx))
                    .collect::<AotResult<_>>()?;

                // Determine element type based on container and index
                let elem_ty = if arr_ty.is_tuple() && indices.len() == 1 {
                    // For tuple indexing with a constant index, get the specific element type
                    if let Expr::Literal(Literal::Int(idx), _) = &indices[0] {
                        self.engine.tuple_element_type_at(&arr_ty, *idx as usize)
                    } else {
                        self.engine.element_type(&arr_ty)
                    }
                } else if let StaticType::Dict { value, .. } = &arr_ty {
                    value.as_ref().clone()
                } else {
                    // For arrays or dynamic indexing, use generic element type
                    self.engine.element_type(&arr_ty)
                };

                // Check if we're indexing a tuple (uses `.0`, `.1` syntax in Rust)
                let is_tuple = arr_ty.is_tuple();

                Ok(AotExpr::Index {
                    array: Box::new(aot_array),
                    indices: aot_indices,
                    elem_ty,
                    is_tuple,
                })
            }

            Expr::Range {
                start, stop, step, ..
            } => {
                let aot_start = self.convert_expr(start)?;
                let aot_stop = self.convert_expr(stop)?;
                let aot_step = step.as_ref().map(|s| self.convert_expr(s)).transpose()?;
                let start_ty = self.engine.infer_expr_type(start);
                let stop_ty = self.engine.infer_expr_type(stop);
                let mut elem_ty = self.engine.unify_types(&start_ty, &stop_ty);
                if let Some(step_expr) = step {
                    let step_ty = self.engine.infer_expr_type(step_expr);
                    elem_ty = self.engine.unify_types(&elem_ty, &step_ty);
                }

                Ok(AotExpr::Range {
                    start: Box::new(aot_start),
                    stop: Box::new(aot_stop),
                    step: aot_step.map(Box::new),
                    elem_ty,
                })
            }

            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let aot_cond = self.convert_expr(condition)?;
                let aot_then = self.convert_expr(then_expr)?;
                let aot_else = self.convert_expr(else_expr)?;

                let then_ty = self.engine.infer_expr_type(then_expr);
                let else_ty = self.engine.infer_expr_type(else_expr);
                let result_ty = self.engine.unify_types(&then_ty, &else_ty);

                Ok(AotExpr::Ternary {
                    condition: Box::new(aot_cond),
                    then_expr: Box::new(aot_then),
                    else_expr: Box::new(aot_else),
                    result_ty,
                })
            }

            Expr::FieldAccess { object, field, .. } => {
                let obj_ty = self.engine.infer_expr_type(object);
                let aot_object = self.convert_expr(object)?;
                let field_ty = self.engine.field_type(&obj_ty, field);

                if let StaticType::NamedTuple(fields) = &obj_ty {
                    let Some(field_index) = fields.iter().position(|(name, _)| name == field)
                    else {
                        return Err(AotError::CodegenError(format!(
                            "NamedTuple field `{}` not found in {} (Issue #7049)",
                            field, obj_ty
                        )));
                    };
                    return Ok(AotExpr::Index {
                        array: Box::new(aot_object),
                        indices: vec![AotExpr::LitI64((field_index + 1) as i64)],
                        elem_ty: field_ty,
                        is_tuple: true,
                    });
                }

                Ok(AotExpr::FieldAccess {
                    object: Box::new(aot_object),
                    field: field.to_string(),
                    field_ty,
                })
            }

            // Builtin function calls (zeros, ones, push!, pop!, etc.)
            Expr::Builtin { name, args, .. } => {
                let aot_args: Vec<AotExpr> = args
                    .iter()
                    .map(|a| self.convert_expr(a))
                    .collect::<AotResult<_>>()?;
                let arg_types: Vec<StaticType> = aot_args.iter().map(|a| a.get_type()).collect();

                // Convert BuiltinOp to AotBuiltinOp
                if let Some(builtin) = Self::builtin_op_to_aot(name) {
                    let return_ty = builtin.return_type(&arg_types);
                    Ok(AotExpr::CallBuiltin {
                        builtin,
                        args: aot_args,
                        return_ty,
                    })
                } else {
                    // Unknown builtin, return placeholder
                    Ok(AotExpr::LitNothing)
                }
            }

            // Function reference (for lambdas/closures passed as arguments)
            Expr::FunctionRef { name, span } => {
                // Check if this is a lambda function
                if let Some(lambda_func) = self.get_lambda_function(name) {
                    // Convert lambda function to AotExpr::Lambda
                    self.convert_lambda_function(lambda_func)
                } else {
                    // Preserve non-lambda function refs as value expressions.
                    let ty = self.engine.infer_expr_type(&Expr::FunctionRef {
                        name: *name,
                        span: *span,
                    });
                    Ok(AotExpr::Var {
                        name: AotFunction::sanitize_function_name(name),
                        ty,
                    })
                }
            }

            // For other expression types, return a placeholder
            _ => Ok(AotExpr::LitNothing),
        }
    }
}
