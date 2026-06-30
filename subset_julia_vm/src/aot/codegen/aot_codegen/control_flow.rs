use super::escape_rust_ident;
use super::AotCodeGenerator;
use crate::aot::ir::{AotExpr, AotStmt, AotUnaryOp, CompoundAssignOp};
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult};

impl AotCodeGenerator {
    // ========== Control Flow Generation ==========

    /// Generate if statement with proper elseif chain support
    ///
    /// This method handles the Julia pattern of if-elseif-else chains,
    /// generating proper `if ... else if ...` syntax in Rust instead of
    /// nested `if` statements inside `else` blocks.
    pub(super) fn emit_if_stmt(
        &mut self,
        condition: &AotExpr,
        then_branch: &[AotStmt],
        else_branch: &Option<Vec<AotStmt>>,
    ) -> AotResult<()> {
        let cond_str = self.emit_condition_expr(condition)?;
        self.write_line(&format!("if {} {{", cond_str));
        self.indent();
        for stmt in then_branch {
            self.emit_stmt(stmt)?;
        }
        self.dedent();

        if let Some(else_stmts) = else_branch {
            // Check if the else branch is a single if statement (elseif chain)
            if else_stmts.len() == 1 {
                if let AotStmt::If {
                    condition: else_cond,
                    then_branch: else_then,
                    else_branch: else_else,
                } = &else_stmts[0]
                {
                    // Generate "} else if" instead of "} else { if"
                    let else_cond_str = self.emit_condition_expr(else_cond)?;
                    self.write_line(&format!("}} else if {} {{", else_cond_str));
                    self.indent();
                    for stmt in else_then {
                        self.emit_stmt(stmt)?;
                    }
                    self.dedent();

                    // Recursively handle further elseif/else chains
                    if let Some(inner_else) = else_else {
                        self.emit_else_chain(inner_else)?;
                    } else {
                        self.write_line("}");
                    }
                    return Ok(());
                }
            }

            // Regular else block
            self.write_line("} else {");
            self.indent();
            for stmt in else_stmts {
                self.emit_stmt(stmt)?;
            }
            self.dedent();
        }
        self.write_line("}");
        Ok(())
    }

    /// Helper to emit the remaining else/elseif chain
    fn emit_else_chain(&mut self, else_stmts: &[AotStmt]) -> AotResult<()> {
        // Check if this is another elseif
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
                for stmt in else_then {
                    self.emit_stmt(stmt)?;
                }
                self.dedent();

                if let Some(inner_else) = else_else {
                    self.emit_else_chain(inner_else)?;
                } else {
                    self.write_line("}");
                }
                return Ok(());
            }
        }

        // Final else block
        self.write_line("} else {");
        self.indent();
        for stmt in else_stmts {
            self.emit_stmt(stmt)?;
        }
        self.dedent();
        self.write_line("}");
        Ok(())
    }

    // ========== Loop Generation ==========

    /// Generate for loop with range
    ///
    /// Handles Julia's for i in start:stop and for i in start:step:stop patterns.
    /// Properly handles:
    /// - Simple forward ranges (1:10)
    /// - Ranges with positive step (1:2:10)
    /// - Reverse ranges (10:-1:1) using .rev()
    pub(super) fn emit_for_range(
        &mut self,
        var: &str,
        start: &AotExpr,
        stop: &AotExpr,
        step: Option<&AotExpr>,
        body: &[AotStmt],
    ) -> AotResult<()> {
        let evar = escape_rust_ident(var);
        if matches!(start.get_type(), StaticType::Char)
            || matches!(stop.get_type(), StaticType::Char)
        {
            if step.is_some() {
                return Err(AotError::CodegenError(
                    "AoT for-range codegen supports unit-step Char ranges only (Issue #7039)"
                        .to_string(),
                ));
            }
            let start_str = self.emit_expr_to_string(start)?;
            let stop_str = self.emit_expr_to_string(stop)?;
            self.write_line(&format!(
                "for {} in SjuliaCharRange::new({}, {}) {{",
                evar, start_str, stop_str
            ));
            self.indent();
            for stmt in body {
                self.emit_stmt(stmt)?;
            }
            self.dedent();
            self.write_line("}");
            return Ok(());
        }

        let start_str = self.emit_expr_to_string(start)?;
        let stop_str = self.emit_expr_to_string(stop)?;

        match step {
            Some(step_expr) => {
                // Check if step is a negative literal for reverse iteration
                if let AotExpr::LitI64(step_val) = step_expr {
                    if *step_val < 0 {
                        // Reverse range: for i in (stop..=start).rev().step_by(abs(step))
                        let abs_step = step_val.abs();
                        if abs_step == 1 {
                            self.write_line(&format!(
                                "for {} in ({}..={}).rev() {{",
                                evar, stop_str, start_str
                            ));
                        } else {
                            self.write_line(&format!(
                                "for {} in ({}..={}).rev().step_by({} as usize) {{",
                                evar, stop_str, start_str, abs_step
                            ));
                        }
                    } else {
                        // Positive step
                        if *step_val == 1 {
                            self.write_line(&format!(
                                "for {} in {}..={} {{",
                                evar, start_str, stop_str
                            ));
                        } else {
                            self.write_line(&format!(
                                "for {} in ({}..={}).step_by({} as usize) {{",
                                evar, start_str, stop_str, step_val
                            ));
                        }
                    }
                } else if let AotExpr::UnaryOp {
                    op: AotUnaryOp::Neg,
                    operand,
                    ..
                } = step_expr
                {
                    // Negative step as unary negation: -step
                    let step_str = self.emit_expr_to_string(operand)?;
                    self.write_line(&format!(
                        "for {} in ({}..={}).rev().step_by({} as usize) {{",
                        evar, stop_str, start_str, step_str
                    ));
                } else {
                    // Dynamic step - generate runtime check
                    let step_str = self.emit_expr_to_string(step_expr)?;
                    self.write_line(&format!(
                        "for {} in ({}..={}).step_by({} as usize) {{",
                        evar, start_str, stop_str, step_str
                    ));
                }
            }
            None => {
                // Julia ranges are inclusive
                self.write_line(&format!("for {} in {}..={} {{", evar, start_str, stop_str));
            }
        }

        self.indent();
        for stmt in body {
            self.emit_stmt(stmt)?;
        }
        self.dedent();
        self.write_line("}");
        Ok(())
    }

    /// Generate for-each loop over iterator
    ///
    /// For arrays and collections, generates proper reference iteration.
    pub(super) fn emit_for_each(
        &mut self,
        var: &str,
        iter: &AotExpr,
        body: &[AotStmt],
    ) -> AotResult<()> {
        let evar = escape_rust_ident(var);
        let iter_str = self.emit_expr_to_string(iter)?;

        // Check if we need to iterate by reference
        let iter_pattern = match iter {
            // For arrays/vectors, clone elements so the Rust loop binding matches
            // the AoT iterator element type used in loop-body expressions.
            AotExpr::Var { ty, .. } if ty.is_array() => {
                format!("{}.iter().cloned()", iter_str)
            }
            AotExpr::Var { ty, .. } if ty.is_set() => {
                format!("{}.iter().cloned()", iter_str)
            }
            AotExpr::Var { ty, .. } if ty.is_dict() => {
                format!(
                    "{}.iter().map(|(_sjulia_k, _sjulia_v)| (_sjulia_k.clone(), _sjulia_v.clone()))",
                    iter_str
                )
            }
            // For array literals, iterate directly
            AotExpr::ArrayLit { .. } => iter_str,
            AotExpr::SetFromIter { .. } => format!("({}).into_iter()", iter_str),
            AotExpr::Var { ty, .. } if ty.is_range() => format!("{}.clone().into_iter()", iter_str),
            AotExpr::Range { .. } => format!("({}).into_iter()", iter_str),
            // Default: use the expression as-is
            _ => iter_str,
        };

        self.write_line(&format!("for {} in {} {{", evar, iter_pattern));
        self.indent();
        for stmt in body {
            self.emit_stmt(stmt)?;
        }
        self.dedent();
        self.write_line("}");
        Ok(())
    }

    /// Generate while loop
    #[allow(dead_code)] // retained codegen helper: while-loop emission API
    pub(super) fn emit_while_loop(
        &mut self,
        condition: &AotExpr,
        body: &[AotStmt],
    ) -> AotResult<()> {
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
        Ok(())
    }

    pub(super) fn emit_condition_expr(&self, condition: &AotExpr) -> AotResult<String> {
        let condition_ty = condition.get_type();
        if condition_ty != StaticType::Bool {
            return Err(AotError::CodegenError(format!(
                "AoT control-flow condition requires Bool, got {}; Julia does not treat \
                 non-Bool values as truthy/falsy (Issue #6980)",
                condition_ty
            )));
        }
        self.emit_expr_to_string(condition)
    }

    // ========== Variable and Assignment Generation ==========

    /// Generate compound assignment statement (+=, -=, *=, etc.)
    ///
    /// Handles Julia's compound assignment operators, generating proper Rust code.
    /// Power assignment (^=) needs special handling since Rust doesn't have ^= for power.
    pub(super) fn emit_compound_assign(
        &mut self,
        target: &AotExpr,
        op: CompoundAssignOp,
        value: &AotExpr,
    ) -> AotResult<()> {
        let target_str = self.emit_expr_to_string(target)?;
        let value_str = self.emit_expr_to_string(value)?;

        if let AotExpr::Var { ty, .. } = target {
            if Self::uses_wrapping_integer_arithmetic(ty) {
                if let Some(method) = Self::wrapping_integer_method(op.to_binop()) {
                    self.write_line(&format!(
                        "{} = {}.{}({});",
                        target_str, target_str, method, value_str
                    ));
                    return Ok(());
                }
            }
        }

        if op.needs_special_handling() {
            // Power assignment: x ^= 2 → x = x.pow(2)
            match op {
                CompoundAssignOp::PowAssign => {
                    // Determine if we need powi (integer exponent) or powf (float exponent)
                    let pow_call = match value {
                        AotExpr::LitI64(_) | AotExpr::LitI32(_) => {
                            format!("{}.powi({} as i32)", target_str, value_str)
                        }
                        _ => format!("{}.powf({})", target_str, value_str),
                    };
                    self.write_line(&format!("{} = {};", target_str, pow_call));
                }
                _ => {
                    // Fallback for other special cases
                    let op_str = op.to_rust_op();
                    self.write_line(&format!("{} {} {};", target_str, op_str, value_str));
                }
            }
        } else {
            let op_str = op.to_rust_op();
            self.write_line(&format!("{} {} {};", target_str, op_str, value_str));
        }
        Ok(())
    }
}
