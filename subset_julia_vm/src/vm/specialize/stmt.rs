//! Statement specialization.

use std::collections::HashMap;

use crate::builtins::BuiltinId;
use crate::ir::core::{BinaryOp, Block, Expr, Stmt};
use crate::vm::{Instr, ValueType};

use super::helpers::stmt_variant_name;
use super::{FunctionSpecializer, SpecializationError};

impl FunctionSpecializer {
    pub(super) fn new(locals: HashMap<String, ValueType>) -> Self {
        Self {
            code: Vec::new(),
            locals,
            current_return_type: ValueType::Nothing,
            break_positions: Vec::new(),
            continue_positions: Vec::new(),
        }
    }

    pub(super) fn emit(&mut self, instr: Instr) {
        self.code.push(instr);
    }

    pub(super) fn compile_block(&mut self, block: &Block) -> Result<(), SpecializationError> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    /// Compile a function body with implicit return handling (Issue #1719, #1726).
    ///
    /// In Julia, the last expression in a function is its implicit return value.
    /// This method distinguishes between "statement position" (all statements except
    /// the last) and "return position" (the last statement). Statements in return
    /// position emit `Return*` instructions, while statements in statement position
    /// use regular control flow (`Jump`).
    ///
    /// When the last statement is an `if` block, we delegate to
    /// [`compile_if_with_implicit_return`] so that each branch emits a `Return`
    /// instruction instead of a `Jump` to end.
    pub(super) fn compile_function_body(&mut self, block: &Block) -> Result<(), SpecializationError> {
        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty function - return nothing
            self.emit(Instr::PushNothing);
            self.current_return_type = ValueType::Nothing;
            self.emit(Instr::ReturnNothing);
            return Ok(());
        }

        // Compile all statements except the last one normally
        for stmt in &stmts[..stmts.len() - 1] {
            self.compile_stmt(stmt)?;
        }

        // Handle the last statement - it determines the return value
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                // Explicit return with value
                let ty = self.compile_expr(expr)?;
                self.current_return_type = ty.clone();
                self.emit_return(ty);
            }
            Stmt::Return { value: None, .. } => {
                // Explicit return without value
                self.emit(Instr::PushNothing);
                self.current_return_type = ValueType::Nothing;
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                // Implicit return - the last expression is the return value
                let ty = self.compile_expr(expr)?;
                self.current_return_type = ty.clone();
                self.emit_return(ty);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // If statement as last statement - each branch should return
                self.compile_if_with_implicit_return(condition, then_branch, else_branch.as_ref())?;
            }
            _ => {
                // Other statements - compile normally and return nothing
                self.compile_stmt(last_stmt)?;
                self.emit(Instr::PushNothing);
                self.current_return_type = ValueType::Nothing;
                self.emit(Instr::ReturnNothing);
            }
        }

        Ok(())
    }

    /// Compile an if statement that is in "return position" (Issue #1719, #1726).
    ///
    /// Unlike [`compile_if`] (used for if statements in statement position that
    /// use `Jump` to skip branches), this method ensures each branch emits a
    /// `Return*` instruction for its last expression. This is necessary because
    /// Julia treats the last expression in a function as the implicit return
    /// value, and if statements can be that last expression.
    ///
    /// If there is no else branch, the false path returns `nothing`.
    pub(super) fn compile_if_with_implicit_return(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
    ) -> Result<(), SpecializationError> {
        // Compile condition
        self.compile_expr(condition)?;

        // Placeholder for jump-if-zero
        let jump_else_pos = self.code.len();
        self.emit(Instr::JumpIfZero(0)); // Will be patched

        // Compile then branch with implicit return
        self.compile_block_with_implicit_return(then_branch)?;

        // Patch jump to else (or end if no else)
        let else_pos = self.code.len();
        self.code[jump_else_pos] = Instr::JumpIfZero(else_pos);

        // Compile else branch with implicit return
        if let Some(else_block) = else_branch {
            self.compile_block_with_implicit_return(else_block)?;
        } else {
            // No else branch - return nothing
            self.emit(Instr::PushNothing);
            self.current_return_type = ValueType::Nothing;
            self.emit(Instr::ReturnNothing);
        }

        Ok(())
    }

    /// Compile a block where the last statement is in "return position" (Issue #1719, #1726).
    ///
    /// All statements except the last are compiled normally (statement position).
    /// The last statement emits a `Return*` instruction for its value. Supports
    /// nested if statements by recursively delegating to
    /// [`compile_if_with_implicit_return`].
    pub(super) fn compile_block_with_implicit_return(
        &mut self,
        block: &Block,
    ) -> Result<(), SpecializationError> {
        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty block - return nothing
            self.emit(Instr::PushNothing);
            self.current_return_type = ValueType::Nothing;
            self.emit(Instr::ReturnNothing);
            return Ok(());
        }

        // Compile all statements except the last one normally
        for stmt in &stmts[..stmts.len() - 1] {
            self.compile_stmt(stmt)?;
        }

        // Handle the last statement - it determines the return value
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                let ty = self.compile_expr(expr)?;
                self.current_return_type = ty.clone();
                self.emit_return(ty);
            }
            Stmt::Return { value: None, .. } => {
                self.emit(Instr::PushNothing);
                self.current_return_type = ValueType::Nothing;
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                let ty = self.compile_expr(expr)?;
                self.current_return_type = ty.clone();
                self.emit_return(ty);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Nested if - recursively handle
                self.compile_if_with_implicit_return(condition, then_branch, else_branch.as_ref())?;
            }
            _ => {
                // Other statements - compile normally and return nothing
                self.compile_stmt(last_stmt)?;
                self.emit(Instr::PushNothing);
                self.current_return_type = ValueType::Nothing;
                self.emit(Instr::ReturnNothing);
            }
        }

        Ok(())
    }

    pub(super) fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), SpecializationError> {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                // Check for self-referential assignments that might change type
                // e.g., result = result * arr[i] where arr[i] returns Any
                let is_self_referential_any = if let Expr::BinaryOp { left, right, .. } = value {
                    let left_is_self = matches!(left.as_ref(), Expr::Var(name, _) if name == var);
                    let right_might_be_any = self.expr_might_produce_any(right);
                    left_is_self && right_might_be_any
                } else {
                    false
                };

                if is_self_referential_any {
                    // Use dynamic path to avoid type change issues in loops
                    // The variable needs to be loaded as Any since result might change
                    if let Expr::BinaryOp {
                        op, left: _, right, ..
                    } = value
                    {
                        self.emit(Instr::LoadAny(var.to_string()));
                        self.compile_expr(right)?;
                        match op {
                            BinaryOp::Add => self.emit(Instr::DynamicAdd),
                            BinaryOp::Sub => self.emit(Instr::DynamicSub),
                            BinaryOp::Mul => self.emit(Instr::DynamicMul),
                            BinaryOp::Div => self.emit(Instr::DynamicDiv),
                            _ => {
                                // For other ops, fall back to regular compilation
                                // Pop the already-loaded Any and recompile normally
                                self.emit(Instr::Pop);
                                let ty = self.compile_expr(value)?;
                                self.locals.insert(var.clone(), ty.clone());
                                self.emit_store(var, ty);
                                return Ok(());
                            }
                        }
                        self.locals.insert(var.clone(), ValueType::Any);
                        self.emit(Instr::StoreAny(var.to_string()));
                    }
                } else {
                    let ty = self.compile_expr(value)?;
                    self.locals.insert(var.clone(), ty.clone());
                    self.emit_store(var, ty);
                }
            }
            Stmt::AddAssign { var, value, .. } => {
                // x += y  ->  x = x + y
                // Check if value might produce Any type (e.g., array indexing)
                // to avoid type change issues in loops
                if self.expr_might_produce_any(value) {
                    // Use dynamic path for safety
                    self.emit(Instr::LoadAny(var.to_string()));
                    self.compile_expr(value)?;
                    self.emit(Instr::DynamicAdd);
                    self.locals.insert(var.clone(), ValueType::Any);
                    self.emit(Instr::StoreAny(var.to_string()));
                } else {
                    let var_ty = self.locals.get(var).cloned().unwrap_or(ValueType::Any);
                    self.compile_var(var)?;
                    let val_ty = self.compile_expr(value)?;
                    let result_ty = self.emit_binary_op(BinaryOp::Add, var_ty, val_ty)?;
                    self.locals.insert(var.clone(), result_ty.clone());
                    self.emit_store(var, result_ty);
                }
            }
            Stmt::Return {
                value: Some(expr), ..
            } => {
                let ty = self.compile_expr(expr)?;
                self.current_return_type = ty.clone();
                self.emit_return(ty);
            }
            Stmt::Return { value: None, .. } => {
                self.emit(Instr::PushNothing);
                self.current_return_type = ValueType::Nothing;
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                let ty = self.compile_expr(expr)?;
                // Update return type for implicit return (last expression)
                self.current_return_type = ty;
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.compile_if(condition, then_branch, else_branch.as_ref())?;
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                ..
            } => {
                // For loops with step are complex to specialize (negative steps need >=)
                // Fall back to generic code for safety
                if step.is_some() {
                    return Err(SpecializationError::Unsupported(
                        "For loop with step not supported in specializer".to_string(),
                    ));
                }
                self.compile_for(var, start, end, step.as_ref(), body)?;
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // ForEach over Range with step != 1 is complex to specialize correctly
                // (e.g., for i in n:-1:1), so we fall back to generic code
                if matches!(iterable, Expr::Range { step: Some(_), .. }) {
                    return Err(SpecializationError::Unsupported(
                        "ForEach over range with step not supported in specializer".to_string(),
                    ));
                }
                self.compile_foreach(var, iterable, body)?;
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.compile_while(condition, body)?;
            }
            Stmt::Block(block) => {
                self.compile_block(block)?;
            }
            Stmt::Break { .. } => {
                // Break - jump to end of loop (will be patched)
                let break_pos = self.code.len();
                self.emit(Instr::Jump(0)); // Will be patched by loop
                self.break_positions.push(break_pos);
            }
            Stmt::Continue { .. } => {
                // Continue - jump to loop start (will be patched)
                let continue_pos = self.code.len();
                self.emit(Instr::Jump(0)); // Will be patched by loop
                self.continue_positions.push(continue_pos);
            }
            _ => {
                return Err(SpecializationError::Unsupported(format!(
                    "Statement type not yet supported for specialization: {}",
                    stmt_variant_name(stmt)
                )));
            }
        }
        Ok(())
    }

    /// Compile an if statement in "statement position" (not at the end of a function).
    ///
    /// Uses `Jump` instructions to skip over branches. This is different from
    /// [`compile_if_with_implicit_return`], which emits `Return*` instructions
    /// because the if statement IS the function's return value.
    pub(super) fn compile_if(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
    ) -> Result<(), SpecializationError> {
        // Compile condition
        self.compile_expr(condition)?;

        // Placeholder for jump-if-zero
        let jump_else_pos = self.code.len();
        self.emit(Instr::JumpIfZero(0)); // Will be patched

        // Compile then branch
        self.compile_block(then_branch)?;

        if let Some(else_block) = else_branch {
            // Jump over else branch
            let jump_end_pos = self.code.len();
            self.emit(Instr::Jump(0)); // Will be patched

            // Patch jump to else
            let else_pos = self.code.len();
            self.code[jump_else_pos] = Instr::JumpIfZero(else_pos);

            // Compile else branch
            self.compile_block(else_block)?;

            // Patch jump to end
            let end_pos = self.code.len();
            self.code[jump_end_pos] = Instr::Jump(end_pos);
        } else {
            // Patch jump to end
            let end_pos = self.code.len();
            self.code[jump_else_pos] = Instr::JumpIfZero(end_pos);
        }

        Ok(())
    }

    pub(super) fn compile_for(
        &mut self,
        var: &str,
        start: &Expr,
        end: &Expr,
        step: Option<&Expr>,
        body: &Block,
    ) -> Result<(), SpecializationError> {
        // Initialize loop variable
        self.compile_expr(start)?;
        self.emit_store(var, ValueType::I64);
        self.locals.insert(var.to_string(), ValueType::I64);

        // Loop start
        let loop_start = self.code.len();

        // Compile condition (var <= end)
        self.emit(Instr::LoadI64(var.to_string()));
        let end_ty = self.compile_expr(end)?;

        // Handle mixed type comparison (I64 vs F64)
        if end_ty == ValueType::F64 {
            // Convert var (I64) to F64 for comparison
            self.emit(Instr::Swap);
            self.emit(Instr::ToF64);
            self.emit(Instr::Swap);
            self.emit(Instr::LeF64);
        } else {
            self.emit(Instr::LeI64);
        }

        // Jump if false
        let jump_end_pos = self.code.len();
        self.emit(Instr::JumpIfZero(0)); // Will be patched

        // Compile body
        self.compile_block(body)?;

        // Increment
        self.emit(Instr::LoadI64(var.to_string()));
        if let Some(step_expr) = step {
            self.compile_expr(step_expr)?;
            self.emit(Instr::AddI64);
        } else {
            self.emit(Instr::PushI64(1));
            self.emit(Instr::AddI64);
        }
        self.emit(Instr::StoreI64(var.to_string()));

        // Jump back to loop start
        self.emit(Instr::Jump(loop_start));

        // Patch jump to end
        let end_pos = self.code.len();
        self.code[jump_end_pos] = Instr::JumpIfZero(end_pos);

        Ok(())
    }

    pub(super) fn compile_while(&mut self, condition: &Expr, body: &Block) -> Result<(), SpecializationError> {
        // Loop start
        let loop_start = self.code.len();

        // Compile condition
        self.compile_expr(condition)?;

        // Jump if false
        let jump_end_pos = self.code.len();
        self.emit(Instr::JumpIfZero(0)); // Will be patched

        // Compile body
        self.compile_block(body)?;

        // Jump back to loop start
        self.emit(Instr::Jump(loop_start));

        // Patch jump to end
        let end_pos = self.code.len();
        self.code[jump_end_pos] = Instr::JumpIfZero(end_pos);

        Ok(())
    }

    pub(super) fn emit_store(&mut self, var: &str, ty: ValueType) {
        match ty {
            ValueType::I64 => self.emit(Instr::StoreI64(var.to_string())),
            ValueType::F64 => self.emit(Instr::StoreF64(var.to_string())),
            ValueType::F32 => self.emit(Instr::StoreF32(var.to_string())),
            ValueType::F16 => self.emit(Instr::StoreF16(var.to_string())),
            ValueType::Str => self.emit(Instr::StoreStr(var.to_string())),
            _ => self.emit(Instr::StoreAny(var.to_string())),
        }
    }

    pub(super) fn emit_return(&mut self, ty: ValueType) {
        match ty {
            ValueType::I64 => self.emit(Instr::ReturnI64),
            ValueType::F64 => self.emit(Instr::ReturnF64),
            ValueType::F32 => self.emit(Instr::ReturnF32),
            ValueType::F16 => self.emit(Instr::ReturnF16),
            ValueType::Nothing => self.emit(Instr::ReturnNothing),
            _ => self.emit(Instr::ReturnAny),
        }
    }

    /// Compile for-each loop: for var in iterable ... end
    pub(super) fn compile_foreach(
        &mut self,
        var: &str,
        iterable: &Expr,
        body: &Block,
    ) -> Result<(), SpecializationError> {
        // Compile iterable
        self.compile_expr(iterable)?;

        // Store iterable in temp
        let iter_var = format!("__iter_{}", var);
        self.emit(Instr::StoreAny(iter_var.clone()));

        // Initialize index
        let idx_var = format!("__idx_{}", var);
        self.emit(Instr::PushI64(1)); // Julia is 1-indexed
        self.emit(Instr::StoreI64(idx_var.clone()));
        self.locals.insert(idx_var.clone(), ValueType::I64);

        // Get length using builtin
        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
        let len_var = format!("__len_{}", var);
        self.emit(Instr::StoreI64(len_var.clone()));
        self.locals.insert(len_var.clone(), ValueType::I64);

        // Loop start
        let loop_start = self.code.len();

        // Check condition: idx <= len
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::LoadI64(len_var.clone()));
        self.emit(Instr::LeI64);

        // Jump if false
        let jump_end_pos = self.code.len();
        self.emit(Instr::JumpIfZero(0));

        // Load element: var = iterable[idx]
        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::IndexLoad(1));
        self.emit(Instr::StoreAny(var.to_string()));
        self.locals.insert(var.to_string(), ValueType::Any);

        // Compile body
        self.compile_block(body)?;

        // Increment index
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::PushI64(1));
        self.emit(Instr::AddI64);
        self.emit(Instr::StoreI64(idx_var));

        // Jump back to loop start
        self.emit(Instr::Jump(loop_start));

        // Patch jump to end
        let end_pos = self.code.len();
        self.code[jump_end_pos] = Instr::JumpIfZero(end_pos);

        Ok(())
    }
}
