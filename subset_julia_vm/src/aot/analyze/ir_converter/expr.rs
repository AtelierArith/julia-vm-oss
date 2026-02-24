use super::*;

impl<'a> IrConverter<'a> {
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
    fn unwrap_ref_expr<'b>(expr: &'b Expr) -> &'b Expr {
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
            Expr::FunctionRef { name, .. } => name.clone(),
            Expr::Var(name, _) => name.clone(),
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
            let mul_impl = format!(
                "{}_{}_{}",
                AotFunction::sanitize_function_name("*"),
                lhs_ty.mangle_suffix(),
                rhs_elem_ty.mangle_suffix()
            );
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
            }));
        }

        // row_matrix .+ vector (column expansion)
        if fn_name == "+" && shape(&lhs_ty) == 2 && shape(&rhs_ty) == 1 {
            let lhs_elem_ty = elem_ty(&lhs_ty);
            let rhs_elem_ty = elem_ty(&rhs_ty);
            let result_elem =
                self.engine
                    .binop_result_type_static(&AotBinOp::Add, &lhs_elem_ty, &rhs_elem_ty);
            let add_impl = format!(
                "{}_{}_{}",
                AotFunction::sanitize_function_name("+"),
                lhs_elem_ty.mangle_suffix(),
                rhs_elem_ty.mangle_suffix()
            );
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

            return Ok(Some(AotExpr::CallStatic {
                function: "__aot_broadcast_call_matrix_scalar_2".to_string(),
                args: vec![
                    AotExpr::Var {
                        name: AotFunction::sanitize_function_name(&fn_name),
                        ty: StaticType::Any,
                    },
                    lhs_aot,
                    rhs_aot,
                ],
                return_ty: StaticType::Array {
                    element: Box::new(return_elem_ty),
                    ndims: Some(2),
                },
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
            Expr::Var(name, _) => {
                if !bound.contains(name) {
                    free.insert(name.clone());
                }
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
            name: "Complex".to_string(),
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
                Stmt::Return { value, .. } => {
                    if let Some(v) = value {
                        let expr_free = self.collect_free_variables(v, &local_bound);
                        free.extend(expr_free);
                    }
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
            .filter_map(|name| {
                // Look up type in current environment (outer scope)
                let ty = self
                    .engine
                    .env
                    .get(&name)
                    .cloned()
                    .unwrap_or(StaticType::Any);
                Some((name, ty))
            })
            .collect();

        // Convert the body - typically a single return statement in a lambda
        // Extract the return expression
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
                // For other statement types, return placeholder
                AotExpr::LitNothing
            }
        } else {
            // Multi-statement body - not yet supported
            AotExpr::LitNothing
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
                let ty = self
                    .engine
                    .env
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| self.engine.lookup_global_or_const(name));
                Ok(AotExpr::Var {
                    name: name.clone(),
                    ty,
                })
            }

            // Assignment expression returns the assigned value.
            // Side effects are handled at statement-flattening sites.
            Expr::AssignExpr { value, .. } => self.convert_expr(value),

            // Let blocks used as expressions are lowered at statement level when possible.
            // Fallback here returns the type/value of the last expression in the block.
            Expr::LetBlock { body, .. } => {
                if let Some(Stmt::Expr { expr, .. }) = body.stmts.last() {
                    self.convert_expr(expr)
                } else {
                    Ok(AotExpr::LitNothing)
                }
            }

            Expr::BinaryOp {
                op, left, right, ..
            } => {
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
                ..
            } => {
                // AoT broadcast lowering: materialize(Broadcasted(...)) -> static helper calls
                if let Some(broadcast_expr) = self.try_convert_broadcast_call(function, args)? {
                    return Ok(broadcast_expr);
                }

                let call_args: Vec<&Expr> =
                    args.iter().chain(kwargs.iter().map(|(_, v)| v)).collect();

                // Ref(x) in broadcast contexts should remain scalar.
                if function == "Ref" && call_args.len() == 1 {
                    return self.convert_expr(call_args[0]);
                }

                let arg_types: Vec<_> = call_args
                    .iter()
                    .map(|a| self.engine.infer_expr_type(a))
                    .collect();
                let aot_args: Vec<_> = call_args
                    .iter()
                    .map(|a| self.convert_expr(a))
                    .collect::<AotResult<_>>()?;

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

                // Special handling for multi-argument operator calls: *(a, b, c) => ((a * b) * c)
                // Julia flattens chained operators like `a * b * c` into `*(a, b, c)` for method dispatch
                // We need to unfold these back to nested binary operations for Rust codegen
                if call_args.len() > 2 {
                    if let Some(aot_op) = self.map_operator_to_binop(function) {
                        // Unfold: *(a, b, c, d) => (((a * b) * c) * d)
                        let mut aot_args_iter = aot_args.into_iter();
                        let mut result = aot_args_iter.next().unwrap();

                        for arg in aot_args_iter {
                            let left_ty = result.get_type();
                            let right_ty = arg.get_type();
                            let result_ty = self
                                .engine
                                .binop_result_type_static(&aot_op, &left_ty, &right_ty);
                            result = AotExpr::BinOpStatic {
                                op: aot_op.clone(),
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
                        name: function.clone(),
                        fields: aot_args,
                    });
                }

                // Check if all argument types are fully static
                let all_static = arg_types.iter().all(|t| t.is_fully_static());

                if all_static {
                    // First check if this is a user-defined function with known return type
                    // This is essential for recursive function calls
                    let return_ty = self
                        .get_function_return_type(function, &arg_types)
                        .unwrap_or_else(|| self.engine.call_result_type(function, &arg_types));
                    Ok(AotExpr::CallStatic {
                        function: function.clone(),
                        args: aot_args,
                        return_ty,
                    })
                } else {
                    Ok(AotExpr::CallDynamic {
                        function: function.clone(),
                        args: aot_args,
                    })
                }
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
                let elem_ty = if matches!(arr_ty, StaticType::Tuple(_)) && indices.len() == 1 {
                    // For tuple indexing with a constant index, get the specific element type
                    if let Expr::Literal(Literal::Int(idx), _) = &indices[0] {
                        self.engine.tuple_element_type_at(&arr_ty, *idx as usize)
                    } else {
                        self.engine.element_type(&arr_ty)
                    }
                } else {
                    // For arrays or dynamic indexing, use generic element type
                    self.engine.element_type(&arr_ty)
                };

                // Check if we're indexing a tuple (uses `.0`, `.1` syntax in Rust)
                let is_tuple = matches!(arr_ty, StaticType::Tuple(_));

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
                let elem_ty = self.engine.infer_expr_type(start);

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

                Ok(AotExpr::FieldAccess {
                    object: Box::new(aot_object),
                    field: field.clone(),
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
            Expr::FunctionRef { name, .. } => {
                // Check if this is a lambda function
                if let Some(lambda_func) = self.get_lambda_function(name) {
                    // Convert lambda function to AotExpr::Lambda
                    self.convert_lambda_function(lambda_func)
                } else {
                    // Preserve non-lambda function refs as value expressions.
                    let ty = self.engine.lookup_global_or_const(name);
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
