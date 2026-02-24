use super::*;

impl<'a> IrConverter<'a> {
    /// Convert statements that may need flattening into zero or more AoT statements.
    ///
    /// This is primarily used for macro-lowered forms like `@time`, which lower into
    /// `Expr::LetBlock` with temporary `#...` variables and `AssignExpr`.
    ///
    /// Also handles `Stmt::Block` (i.e. Julia `begin...end`): in Julia, `begin...end`
    /// blocks do **not** introduce a new scope — all variables declared inside are
    /// visible in the enclosing scope.  We therefore flatten them into the surrounding
    /// statement list.
    fn convert_stmt_expanded(&mut self, stmt: &Stmt) -> AotResult<Vec<AotStmt>> {
        match stmt {
            Stmt::Timed { body, .. } => self.convert_block(body),
            Stmt::Expr {
                expr: Expr::LetBlock { bindings, body, .. },
                ..
            } => self.convert_let_block_stmt(bindings, body),
            // In Julia, `begin...end` shares the enclosing scope (Issue #2868).
            // Flatten all statements into the parent block.
            Stmt::Block(block) => self.convert_block(block),
            _ => Ok(vec![self.convert_stmt(stmt)?]),
        }
    }

    /// Convert a lowered `LetBlock` statement form (e.g. from `@time`) into executable AoT stmts.
    fn convert_let_block_stmt(
        &mut self,
        bindings: &[(String, Expr)],
        body: &Block,
    ) -> AotResult<Vec<AotStmt>> {
        let mut out = Vec::new();

        // Keep explicit non-temporary bindings.
        for (name, value) in bindings {
            if name.starts_with('#') {
                continue;
            }
            let synthetic = Stmt::Assign {
                var: name.clone(),
                value: value.clone(),
                span: value.span(),
            };
            out.push(self.convert_stmt(&synthetic)?);
        }

        for stmt in &body.stmts {
            match stmt {
                // `tmp = (grid = expr)` => materialize as `grid = expr` and drop tmp if temporary.
                Stmt::Assign { var, value, span } => {
                    if let Expr::AssignExpr {
                        var: inner_var,
                        value: inner_value,
                        ..
                    } = value
                    {
                        let synthetic = Stmt::Assign {
                            var: inner_var.clone(),
                            value: (*inner_value.clone()),
                            span: *span,
                        };
                        out.push(self.convert_stmt(&synthetic)?);

                        if !var.starts_with('#') {
                            let alias = Stmt::Assign {
                                var: var.clone(),
                                value: Expr::Var(inner_var.clone(), *span),
                                span: *span,
                            };
                            out.push(self.convert_stmt(&alias)?);
                        }
                        continue;
                    }

                    if var.starts_with('#') {
                        continue;
                    }

                    out.push(self.convert_stmt(stmt)?);
                }
                // Drop temporary-value passthrough (`#result`) but keep any other effects.
                Stmt::Expr {
                    expr: Expr::Var(name, _),
                    ..
                } if name.starts_with('#') => {}
                Stmt::Expr { expr, .. } if Self::expr_uses_temporary(expr) => {}
                _ => out.extend(self.convert_stmt_expanded(stmt)?),
            }
        }

        Ok(out)
    }

    /// Detect whether an expression references compiler-generated temporary names (`#...`).
    fn expr_uses_temporary(expr: &Expr) -> bool {
        match expr {
            Expr::Var(name, _) => name.starts_with('#'),
            Expr::BinaryOp { left, right, .. } => {
                Self::expr_uses_temporary(left) || Self::expr_uses_temporary(right)
            }
            Expr::UnaryOp { operand, .. } => Self::expr_uses_temporary(operand),
            Expr::Call { args, kwargs, .. } => {
                args.iter().any(Self::expr_uses_temporary)
                    || kwargs.iter().any(|(_, v)| Self::expr_uses_temporary(v))
            }
            Expr::Builtin { args, .. } => args.iter().any(Self::expr_uses_temporary),
            Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
                elements.iter().any(Self::expr_uses_temporary)
            }
            Expr::Index { array, indices, .. } => {
                Self::expr_uses_temporary(array) || indices.iter().any(Self::expr_uses_temporary)
            }
            Expr::Range {
                start, step, stop, ..
            } => {
                Self::expr_uses_temporary(start)
                    || step.as_ref().is_some_and(|s| Self::expr_uses_temporary(s))
                    || Self::expr_uses_temporary(stop)
            }
            Expr::FieldAccess { object, .. } => Self::expr_uses_temporary(object),
            Expr::NamedTupleLiteral { fields, .. } => {
                fields.iter().any(|(_, v)| Self::expr_uses_temporary(v))
            }
            Expr::DictLiteral { pairs, .. } => pairs
                .iter()
                .any(|(k, v)| Self::expr_uses_temporary(k) || Self::expr_uses_temporary(v)),
            Expr::Comprehension {
                body, iter, filter, ..
            } => {
                Self::expr_uses_temporary(body)
                    || Self::expr_uses_temporary(iter)
                    || filter
                        .as_ref()
                        .is_some_and(|f| Self::expr_uses_temporary(f))
            }
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                Self::expr_uses_temporary(body)
                    || iterations
                        .iter()
                        .any(|(_, it)| Self::expr_uses_temporary(it))
                    || filter
                        .as_ref()
                        .is_some_and(|f| Self::expr_uses_temporary(f))
            }
            Expr::Generator {
                body, iter, filter, ..
            } => {
                Self::expr_uses_temporary(body)
                    || Self::expr_uses_temporary(iter)
                    || filter
                        .as_ref()
                        .is_some_and(|f| Self::expr_uses_temporary(f))
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_uses_temporary(condition)
                    || Self::expr_uses_temporary(then_expr)
                    || Self::expr_uses_temporary(else_expr)
            }
            Expr::StringConcat { parts, .. } => parts.iter().any(Self::expr_uses_temporary),
            Expr::ModuleCall { args, kwargs, .. } => {
                args.iter().any(Self::expr_uses_temporary)
                    || kwargs.iter().any(|(_, v)| Self::expr_uses_temporary(v))
            }
            Expr::New { args, .. } => args.iter().any(Self::expr_uses_temporary),
            Expr::DynamicTypeConstruct { type_args, .. } => {
                type_args.iter().any(Self::expr_uses_temporary)
            }
            Expr::QuoteLiteral { constructor, .. } => Self::expr_uses_temporary(constructor),
            Expr::LetBlock { bindings, body, .. } => {
                bindings.iter().any(|(_, v)| Self::expr_uses_temporary(v))
                    || body.stmts.iter().any(|stmt| match stmt {
                        Stmt::Expr { expr, .. } => Self::expr_uses_temporary(expr),
                        Stmt::Assign { value, .. } => Self::expr_uses_temporary(value),
                        Stmt::AddAssign { value, .. } => Self::expr_uses_temporary(value),
                        _ => false,
                    })
            }
            Expr::AssignExpr { value, .. } => Self::expr_uses_temporary(value),
            Expr::ReturnExpr { value, .. } => {
                value.as_ref().is_some_and(|v| Self::expr_uses_temporary(v))
            }
            Expr::Pair { key, value, .. } => {
                Self::expr_uses_temporary(key) || Self::expr_uses_temporary(value)
            }
            Expr::Literal(_, _)
            | Expr::TypedEmptyArray { .. }
            | Expr::SliceAll { .. }
            | Expr::FunctionRef { .. }
            | Expr::BreakExpr { .. }
            | Expr::ContinueExpr { .. } => false,
        }
    }

    /// Convert a single lowered statement to exactly **one** AoT statement.
    ///
    /// # When to use this function
    ///
    /// Use `convert_stmt` only for statement forms that always produce exactly
    /// one output statement (`Assign`, `Return`, `If`, `WhileLoop`, etc.).
    ///
    /// # When NOT to use this function
    ///
    /// For statements that may expand into **multiple** AoT statements — in
    /// particular `Stmt::Block` (`begin...end`) and `Expr::LetBlock` forms —
    /// you MUST call [`convert_stmt_expanded`] instead.  Calling `convert_stmt`
    /// directly on those forms executes all sub-statements for their side
    /// effects but returns only the **last** one; all preceding statements are
    /// silently dropped from the parent statement list (Issue #2868).
    ///
    /// [`convert_stmt_expanded`]: Self::convert_stmt_expanded
    pub(crate) fn convert_stmt(&mut self, stmt: &Stmt) -> AotResult<AotStmt> {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                // First convert the expression, then get its type from the AotExpr
                // This ensures user-defined function return types are correctly inferred
                let aot_value = self.convert_expr(value)?;
                let ty = aot_value.get_type();

                if self.declared_locals.contains(var) {
                    // Reassignment
                    Ok(AotStmt::Assign {
                        target: AotExpr::Var {
                            name: var.clone(),
                            ty: ty.clone(),
                        },
                        value: aot_value,
                    })
                } else {
                    // New variable declaration
                    self.declared_locals.insert(var.clone());
                    self.engine.env.insert(var.clone(), ty.clone());
                    Ok(AotStmt::Let {
                        name: var.clone(),
                        ty,
                        value: aot_value,
                        is_mutable: true, // All Julia variables are mutable by default
                    })
                }
            }

            Stmt::Expr { expr, .. } => {
                let aot_expr = self.convert_expr(expr)?;
                Ok(AotStmt::Expr(aot_expr))
            }

            Stmt::Return { value, .. } => {
                let aot_value = value
                    .as_ref()
                    .map(|e| {
                        let expr = self.convert_expr(e)?;
                        // Apply type coercion if function has an explicit return type
                        if let Some(ref return_ty) = self.current_return_type {
                            let expr_ty = expr.get_type();
                            // Only coerce if types differ and both are known
                            if expr_ty != *return_ty
                                && expr_ty != StaticType::Any
                                && *return_ty != StaticType::Any
                            {
                                Ok(AotExpr::Convert {
                                    value: Box::new(expr),
                                    target_ty: return_ty.clone(),
                                })
                            } else {
                                Ok(expr)
                            }
                        } else {
                            Ok(expr)
                        }
                    })
                    .transpose()?;
                Ok(AotStmt::Return(aot_value))
            }

            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let aot_cond = self.convert_expr(condition)?;
                let aot_then = self.convert_block(then_branch)?;
                let aot_else = else_branch
                    .as_ref()
                    .map(|b| self.convert_block(b))
                    .transpose()?;

                Ok(AotStmt::If {
                    condition: aot_cond,
                    then_branch: aot_then,
                    else_branch: aot_else,
                })
            }

            Stmt::While {
                condition, body, ..
            } => {
                let aot_cond = self.convert_expr(condition)?;
                let aot_body = self.convert_block(body)?;

                Ok(AotStmt::While {
                    condition: aot_cond,
                    body: aot_body,
                })
            }

            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                ..
            } => {
                let aot_start = self.convert_expr(start)?;
                let aot_stop = self.convert_expr(end)?;
                let aot_step = step.as_ref().map(|s| self.convert_expr(s)).transpose()?;

                // Add loop variable to scope - infer type from start/end
                let start_ty = self.engine.infer_expr_type(start);
                let end_ty = self.engine.infer_expr_type(end);
                let elem_ty = self.engine.unify_types(&start_ty, &end_ty);
                self.declared_locals.insert(var.clone());
                self.engine.env.insert(var.clone(), elem_ty);

                let aot_body = self.convert_block(body)?;

                Ok(AotStmt::ForRange {
                    var: var.clone(),
                    start: aot_start,
                    stop: aot_stop,
                    step: aot_step,
                    body: aot_body,
                })
            }

            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                let aot_iter = self.convert_expr(iterable)?;

                // Add loop variable to scope
                let elem_ty = self.engine.infer_iterator_element_type(iterable);
                self.declared_locals.insert(var.clone());
                self.engine.env.insert(var.clone(), elem_ty);

                let aot_body = self.convert_block(body)?;

                Ok(AotStmt::ForEach {
                    var: var.clone(),
                    iter: aot_iter,
                    body: aot_body,
                })
            }

            Stmt::Break { .. } => Ok(AotStmt::Break),

            Stmt::Continue { .. } => Ok(AotStmt::Continue),

            Stmt::Block(block) => {
                // `begin...end` blocks are normally intercepted by `convert_stmt_expanded`
                // and flattened into the surrounding statement list.  This arm is a fallback
                // for callers that invoke `convert_stmt` directly (e.g. synthetic Assign
                // statements inside `convert_let_block_stmt`).
                //
                // In expression-position, a Julia block evaluates to its last expression.
                // When called as a single-statement context we convert every statement for
                // side effects and return the last as the "value".
                let mut last = AotStmt::Expr(AotExpr::LitNothing);
                for stmt in &block.stmts {
                    last = self.convert_stmt(stmt)?;
                }
                Ok(last)
            }

            // Field assignment: obj.field = value
            Stmt::FieldAssign {
                object,
                field,
                value,
                ..
            } => {
                // Get the object's type from the environment
                let obj_ty = self
                    .engine
                    .env
                    .get(object)
                    .cloned()
                    .unwrap_or(StaticType::Any);
                let field_ty = self.engine.field_type(&obj_ty, field);

                let aot_object = AotExpr::Var {
                    name: object.clone(),
                    ty: obj_ty,
                };
                let aot_value = self.convert_expr(value)?;

                // Generate assignment to field access
                Ok(AotStmt::Assign {
                    target: AotExpr::FieldAccess {
                        object: Box::new(aot_object),
                        field: field.clone(),
                        field_ty,
                    },
                    value: aot_value,
                })
            }

            // Index assignment: arr[i] = value, arr[i, j] = value
            Stmt::IndexAssign {
                array,
                indices,
                value,
                ..
            } => {
                // Get the array's type from the environment
                let arr_ty = self
                    .engine
                    .env
                    .get(array)
                    .cloned()
                    .unwrap_or(StaticType::Any);

                // Get element type
                let elem_ty = if let StaticType::Array { element, .. } = &arr_ty {
                    (**element).clone()
                } else {
                    StaticType::Any
                };

                let aot_array = AotExpr::Var {
                    name: array.clone(),
                    ty: arr_ty,
                };

                // Convert indices
                let aot_indices: Vec<AotExpr> = indices
                    .iter()
                    .map(|idx| self.convert_expr(idx))
                    .collect::<AotResult<_>>()?;

                let aot_value = self.convert_expr(value)?;

                // Generate assignment to index expression
                Ok(AotStmt::Assign {
                    target: AotExpr::Index {
                        array: Box::new(aot_array),
                        indices: aot_indices,
                        elem_ty,
                        is_tuple: false,
                    },
                    value: aot_value,
                })
            }

            // Enum definitions are handled at the program level in convert_program,
            // so inline occurrences are no-ops
            Stmt::EnumDef { .. } => Ok(AotStmt::Expr(AotExpr::LitNothing)),

            // Handle other statement types
            _ => {
                // For unhandled statements, convert to expression statement if possible
                Ok(AotStmt::Expr(AotExpr::LitNothing))
            }
        }
    }

    /// Convert a block to a list of statements
    fn convert_block(&mut self, block: &Block) -> AotResult<Vec<AotStmt>> {
        let mut stmts = Vec::new();
        for stmt in &block.stmts {
            let expanded = self.convert_stmt_expanded(stmt)?;
            stmts.extend(expanded);
        }
        Ok(stmts)
    }
}
