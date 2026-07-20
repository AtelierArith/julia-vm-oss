use super::escape_rust_ident;
use super::AotCodeGenerator;
use crate::aot::abi::AotAbiValue;
use crate::aot::ir::{AotExpr, AotStmt};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};

impl AotCodeGenerator {
    // ========== Statement Generation ==========

    /// Emit a statement
    pub(super) fn emit_stmt(&mut self, stmt: &AotStmt) -> AotResult<()> {
        match stmt {
            AotStmt::Let {
                name,
                ty,
                value,
                is_mutable,
            } => {
                let value_str = self.emit_value_for_binding_type(value, ty, "let binding")?;
                if self.current_function_hoisted_locals.contains(name) {
                    // Declaration was hoisted to a deferred `let mut` at function
                    // top (Issue #8181); emit this in-block initialization as a
                    // plain assignment so it does not re-shadow inside the block.
                    self.write_line(&format!("{} = {};", escape_rust_ident(name), value_str));
                } else {
                    let rust_ty = self.type_to_rust(ty);
                    let mut_kw = if *is_mutable { "mut " } else { "" };
                    self.write_line(&format!(
                        "let {}{}: {} = {};",
                        mut_kw,
                        escape_rust_ident(name),
                        rust_ty,
                        value_str
                    ));
                }
            }

            AotStmt::Assign { target, value } => {
                if let AotExpr::Index {
                    array,
                    indices,
                    elem_ty,
                    ..
                } = target
                {
                    if matches!(array.get_type(), StaticType::Dict { .. }) && indices.len() == 1 {
                        let dict_str = self.emit_expr_to_string(array)?;
                        let key_str = self.emit_expr_to_string(&indices[0])?;
                        let value_str =
                            self.emit_value_for_binding_type(value, elem_ty, "Dict assignment")?;
                        self.write_line(&format!(
                            "let _ = {}.insert({}, {});",
                            dict_str, key_str, value_str
                        ));
                        return Ok(());
                    }
                }
                let target_str = self.emit_expr_to_string(target)?;
                let target_ty = target.get_type();
                let value_str =
                    self.emit_value_for_binding_type(value, &target_ty, "assignment")?;
                self.write_line(&format!("{} = {};", target_str, value_str));
            }

            AotStmt::CompoundAssign { target, op, value } => {
                self.emit_compound_assign(target, *op, value)?;
            }

            AotStmt::ValueCarrier(_) => {}

            AotStmt::Expr(expr) => {
                let expr_str = self.emit_expr_to_string(expr)?;
                self.write_line(&format!("{};", expr_str));
            }

            AotStmt::Return(Some(expr)) => {
                let expr_str = if self
                    .current_function_return_type
                    .as_ref()
                    .is_some_and(|ty| AotAbiValue::from_static_type(ty).needs_runtime_value())
                {
                    self.emit_expr_as_value(expr)?
                } else {
                    self.emit_expr_to_string(expr)?
                };
                self.write_line(&format!("return {};", expr_str));
            }

            AotStmt::Return(None) => {
                self.write_line("return;");
            }

            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.emit_if_stmt(condition, then_branch, else_branch)?;
            }

            AotStmt::While { condition, body } => {
                if matches!(condition, AotExpr::LitBool(true)) {
                    self.write_line("loop {");
                } else {
                    let cond_str = self.emit_condition_expr(condition)?;
                    self.write_line(&format!("while {} {{", cond_str));
                }
                self.indent();
                for stmt in body {
                    self.emit_stmt(stmt)?;
                }
                self.dedent();
                self.write_line("}");
            }

            AotStmt::ForRange {
                var,
                start,
                stop,
                step,
                body,
            } => {
                self.emit_for_range(var, start, stop, step.as_ref(), body)?;
            }

            AotStmt::ForEach { var, iter, body } => {
                self.emit_for_each(var, iter, body)?;
            }

            AotStmt::Break => {
                self.write_line("break;");
            }

            AotStmt::Continue => {
                self.write_line("continue;");
            }
        }

        Ok(())
    }

    /// Emit statement with special handling for the last statement (implicit return)
    /// If the last statement is an expression and the function has a non-Nothing return type,
    /// emit it without a trailing semicolon so it becomes the return value.
    pub(super) fn emit_stmt_maybe_return(
        &mut self,
        stmt: &AotStmt,
        return_type: &StaticType,
    ) -> AotResult<()> {
        // Skip implicit return handling for Nothing/Unit return types
        if *return_type == StaticType::Nothing {
            return self.emit_stmt(stmt);
        }

        match stmt {
            // Expression statement - emit without semicolon
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => {
                let expr_str = if AotAbiValue::from_static_type(return_type).needs_runtime_value() {
                    self.emit_expr_as_value(expr)?
                } else {
                    self.emit_expr_to_string(expr)?
                };
                self.write_line(&expr_str);
                Ok(())
            }

            // If statement - recursively handle branches for implicit return
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => self.emit_if_stmt_as_expr(condition, then_branch, else_branch, return_type),

            // Other statements - emit normally
            _ => self.emit_stmt(stmt),
        }
    }

    /// Emit if statement as an expression (for implicit return in function body)
    fn emit_if_stmt_as_expr(
        &mut self,
        condition: &AotExpr,
        then_branch: &[AotStmt],
        else_branch: &Option<Vec<AotStmt>>,
        return_type: &StaticType,
    ) -> AotResult<()> {
        let cond_str = self.emit_condition_expr(condition)?;
        self.write_line(&format!("if {} {{", cond_str));
        self.indent();

        // Emit all but the last statement normally
        let then_len = then_branch.len();
        for (i, stmt) in then_branch.iter().enumerate() {
            if i == then_len - 1 {
                self.emit_stmt_maybe_return(stmt, return_type)?;
            } else {
                self.emit_stmt(stmt)?;
            }
        }

        self.dedent();

        if let Some(else_stmts) = else_branch {
            // Check for elseif chain
            if else_stmts.len() == 1 {
                if let AotStmt::If {
                    condition: else_cond,
                    then_branch: else_then,
                    else_branch: else_else,
                } = &else_stmts[0]
                {
                    let else_cond_str = self.emit_condition_expr(else_cond)?;
                    self.write_line(&format!("}} else if {} {{", else_cond_str));
                    self.indent();

                    let else_then_len = else_then.len();
                    for (i, stmt) in else_then.iter().enumerate() {
                        if i == else_then_len - 1 {
                            self.emit_stmt_maybe_return(stmt, return_type)?;
                        } else {
                            self.emit_stmt(stmt)?;
                        }
                    }

                    self.dedent();

                    if let Some(inner_else) = else_else {
                        self.emit_else_chain_as_expr(inner_else, return_type)?;
                    } else {
                        self.write_line("}");
                    }
                    return Ok(());
                }
            }

            // Regular else block
            self.write_line("} else {");
            self.indent();

            let else_len = else_stmts.len();
            for (i, stmt) in else_stmts.iter().enumerate() {
                if i == else_len - 1 {
                    self.emit_stmt_maybe_return(stmt, return_type)?;
                } else {
                    self.emit_stmt(stmt)?;
                }
            }

            self.dedent();
        }

        self.write_line("}");
        Ok(())
    }

    /// Helper to emit else chain as expression
    fn emit_else_chain_as_expr(
        &mut self,
        else_stmts: &[AotStmt],
        return_type: &StaticType,
    ) -> AotResult<()> {
        if else_stmts.len() == 1 {
            if let AotStmt::If {
                condition: else_cond,
                then_branch: else_then,
                else_branch: else_else,
            } = &else_stmts[0]
            {
                let else_cond_str = self.emit_condition_expr(else_cond)?;
                self.write_line(&format!("}} else if {} {{", else_cond_str));
                self.indent();

                let else_then_len = else_then.len();
                for (i, stmt) in else_then.iter().enumerate() {
                    if i == else_then_len - 1 {
                        self.emit_stmt_maybe_return(stmt, return_type)?;
                    } else {
                        self.emit_stmt(stmt)?;
                    }
                }

                self.dedent();

                if let Some(inner_else) = else_else {
                    self.emit_else_chain_as_expr(inner_else, return_type)?;
                } else {
                    self.write_line("}");
                }
                return Ok(());
            }
        }

        // Regular else block
        self.write_line("} else {");
        self.indent();

        let else_len = else_stmts.len();
        for (i, stmt) in else_stmts.iter().enumerate() {
            if i == else_len - 1 {
                self.emit_stmt_maybe_return(stmt, return_type)?;
            } else {
                self.emit_stmt(stmt)?;
            }
        }

        self.dedent();
        self.write_line("}");
        Ok(())
    }

    pub(super) fn emit_value_for_binding_type(
        &self,
        value: &AotExpr,
        target_ty: &StaticType,
        context: &str,
    ) -> AotResult<String> {
        if AotAbiValue::from_static_type(target_ty).needs_runtime_value() {
            return self.emit_expr_as_value(value);
        }

        let value_ty = value.get_type();
        if &value_ty == target_ty {
            return self.emit_expr_to_string(value);
        }

        self.emit_expr_as_static_type(value, target_ty)
            .map_err(|err| {
                AotError::CodegenError(format!(
                    "AoT {} cannot store value of type {} in slot type {}; type-unstable \
                 variables must use an Any/Union runtime Value boundary (Issue #6978): {}",
                    context, value_ty, target_ty, err
                ))
            })
    }
}
