//! Statement compilation for CoreCompiler.
//!
//! This module contains statement-level compilation methods including
//! block, function body, and individual statement compilation.

use crate::ir::core::{Block, Expr, Literal, Stmt};
use crate::types::JuliaType;

mod stmt_try_catch;
use crate::vm::{Instr, ValueType};

use super::types::{err, CResult, CompileError};
use super::{analyze_free_variables, is_stdlib_module, CoreCompiler, LoopContext};
use std::collections::HashSet;

/// Check if a direct type conversion is possible between two value types.
///
/// Only I64↔F64 conversions are supported by dedicated VM instructions
/// (ToF64 and ToI64). All other type coercions go through Pure Julia `convert()`.
pub(super) fn can_convert_type(from: ValueType, to: ValueType) -> bool {
    matches!(
        (from, to),
        (ValueType::I64, ValueType::F64) | (ValueType::F64, ValueType::I64)
    )
}

/// Determine the iteration strategy for a type known at compile time.
///
/// Returns:
/// - `Some(true)`  — call Pure Julia `iterate()` (custom struct iterators, `Any` dispatch)
/// - `Some(false)` — emit a VM builtin instruction (faster path for known collections)
/// - `None`        — type is unknown; requires a runtime method-table lookup
///
/// The `None` case is handled by `should_use_pure_julia_iterate`, which falls back to
/// checking `self.method_tables` at compile time.
pub(super) fn static_iterate_strategy(ty: &JuliaType) -> Option<bool> {
    match ty {
        // CartesianIndices uses VM builtin iterate for better performance
        JuliaType::Struct(name) if name == "CartesianIndices" => Some(false),
        // All other struct types use Pure Julia iterate (custom iterators)
        JuliaType::Struct(_) => Some(true),
        // Any type: use Pure Julia dispatch (handles unknown runtime structs)
        JuliaType::Any => Some(true),
        // Builtin collection types: faster VM instructions
        JuliaType::Array | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => Some(false),
        JuliaType::Tuple | JuliaType::TupleOf(_) => Some(false),
        JuliaType::String => Some(false),
        JuliaType::Int64 => Some(false), // Range-like types
        // Unknown type; let the caller perform a dynamic method-table lookup
        _ => None,
    }
}

impl CoreCompiler<'_> {
    pub(super) fn compile_block(&mut self, block: &Block) -> CResult<()> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    /// Compile a function body with implicit return handling.
    /// In Julia, the last expression in a function is its return value.
    pub(super) fn compile_function_body(
        &mut self,
        block: &Block,
        return_type: ValueType,
    ) -> CResult<()> {
        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty function - return default value
            self.emit_default_return(return_type);
            return Ok(());
        }

        // Compile all statements except the last one normally
        for stmt in &stmts[..stmts.len() - 1] {
            self.compile_stmt(stmt)?;
        }

        // Handle the last statement specially
        let last_stmt = &stmts[stmts.len() - 1];
        match last_stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                // Explicit return with value - compile and return it
                let ty = self.compile_expr(expr)?;
                self.emit_return_for_type(ty);
            }
            Stmt::Return { value: None, .. } => {
                // Explicit return without value
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                // Implicit return - the last expression is the return value
                let actual_ty = self.compile_expr(expr)?;
                // Try to convert to the declared return type if needed
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else {
                    // Use the actual type when conversion isn't possible
                    // This handles DataType returns and other non-convertible types
                    self.emit_return_for_type(actual_ty);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // If statement as last statement in function - handle implicit return
                // Each branch should return its last expression's value
                self.compile_if_with_implicit_return(
                    condition,
                    then_branch,
                    else_branch.as_ref(),
                    return_type,
                )?;
            }
            _ => {
                // Other statements (while, for, etc.) - compile normally and add default return
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
        }

        Ok(())
    }

    fn emit_default_return(&mut self, return_type: ValueType) {
        match return_type {
            ValueType::I64 => {
                self.emit(Instr::PushI64(0));
                self.emit(Instr::ReturnI64);
            }
            ValueType::F64 => {
                self.emit(Instr::PushF64(0.0));
                self.emit(Instr::ReturnF64);
            }
            ValueType::Struct(_type_id) => {
                // For struct return types without explicit return, return Nothing
                self.emit(Instr::ReturnNothing);
            }
            _ => {
                self.emit(Instr::ReturnNothing);
            }
        }
    }

    pub(super) fn emit_return_for_type(&mut self, ty: ValueType) {
        match ty {
            ValueType::I64 => self.emit(Instr::ReturnI64),
            ValueType::F64 => self.emit(Instr::ReturnF64),
            ValueType::Array | ValueType::ArrayOf(_) => self.emit(Instr::ReturnArray),
            ValueType::Str => self.emit(Instr::ReturnAny), // String uses dynamic return
            // Nothing type: use ReturnAny to consume the Nothing value pushed by compile_expr.
            // ReturnNothing does NOT pop the stack, so using it here would leave an orphaned
            // Nothing on the stack, corrupting nested call chains (Issue #2072).
            ValueType::Nothing => self.emit(Instr::ReturnAny),
            ValueType::Missing => self.emit(Instr::ReturnAny),
            ValueType::Struct(_) => self.emit(Instr::ReturnStruct), // All structs including Complex
            ValueType::Rng => self.emit(Instr::ReturnRng),
            ValueType::Range => self.emit(Instr::ReturnRange),
            ValueType::Tuple => self.emit(Instr::ReturnTuple),
            ValueType::NamedTuple => self.emit(Instr::ReturnNamedTuple),
            ValueType::Dict | ValueType::Set => self.emit(Instr::ReturnDict),
            ValueType::Generator => self.emit(Instr::ReturnAny),
            ValueType::Char => self.emit(Instr::ReturnAny),
            ValueType::Any => self.emit(Instr::ReturnAny),
            ValueType::DataType => self.emit(Instr::ReturnAny),
            ValueType::Module => self.emit(Instr::ReturnAny),
            ValueType::BigInt => self.emit(Instr::ReturnAny),
            ValueType::BigFloat => self.emit(Instr::ReturnAny),
            ValueType::IO => self.emit(Instr::ReturnAny),
            ValueType::Function => self.emit(Instr::ReturnAny),
            // Narrow integer types: ReturnI64 handler already preserves the original Value type
            // (I8/I16/I32/I128/U8–U128/Bool) via `preserved_val`, so using ReturnI64 is safe
            // and informs the AoT compiler that the return type is integer-family. (Issue #3255)
            ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I128
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::Bool => self.emit(Instr::ReturnI64),
            ValueType::F32 => self.emit(Instr::ReturnF32),
            ValueType::F16 => self.emit(Instr::ReturnF16),
            // Macro system types
            ValueType::Symbol
            | ValueType::Expr
            | ValueType::QuoteNode
            | ValueType::LineNumberNode
            | ValueType::GlobalRef => self.emit(Instr::ReturnAny),
            // Pairs type (for kwargs...)
            ValueType::Pairs => self.emit(Instr::ReturnAny),
            // Regex types
            ValueType::Regex | ValueType::RegexMatch => self.emit(Instr::ReturnAny),
            // Enum type
            ValueType::Enum => self.emit(Instr::ReturnAny),
            // Union type
            ValueType::Union(_) => self.emit(Instr::ReturnAny),
            // Memory type
            ValueType::Memory | ValueType::MemoryOf(_) => self.emit(Instr::ReturnAny),
        }
    }

    /// Emit type conversion instructions from actual to target type.
    /// Note: Complex conversions are handled via Pure Julia convert() functions.
    fn emit_type_conversion(&mut self, from: ValueType, to: ValueType) {
        match (from, to) {
            (ValueType::I64, ValueType::F64) => self.emit(Instr::ToF64),
            (ValueType::F64, ValueType::I64) => self.emit(Instr::ToI64),
            // Other conversions are not needed or not possible
            _ => {}
        }
    }

    /// Compile an if statement as the last statement in a function with implicit return.
    /// Each branch returns its last expression's value instead of falling through.
    fn compile_if_with_implicit_return(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
        return_type: ValueType,
    ) -> CResult<()> {
        // Dead code elimination: skip provably dead branches (Issue #3364)
        if let Expr::Literal(Literal::Bool(b), _) = condition {
            if *b {
                // Condition is always true: only compile then-branch
                self.compile_block_with_implicit_return(then_branch, return_type)?;
            } else if let Some(else_block) = else_branch {
                // Condition is always false: only compile else-branch
                self.compile_block_with_implicit_return(else_block, return_type)?;
            } else {
                // Condition is always false, no else: return default
                self.emit_default_return(return_type);
            }
            return Ok(());
        }

        // Compile condition
        let _cond_ty = self.compile_expr(condition)?;
        let j_else = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));

        // Compile then-branch with implicit return
        self.compile_block_with_implicit_return(then_branch, return_type.clone())?;

        // If there's an else branch, we need to jump over it after then-branch
        // (But since then-branch ends with a return, this jump is actually unreachable)
        // However, we still need the else label for the JumpIfZero
        let else_start = self.here();
        self.patch_jump(j_else, else_start);

        // Compile else-branch with implicit return
        if let Some(else_block) = else_branch {
            self.compile_block_with_implicit_return(else_block, return_type)?;
        } else {
            // No else branch - return default value
            self.emit_default_return(return_type);
        }

        Ok(())
    }

    /// Compile a block with implicit return (the last statement returns its value).
    fn compile_block_with_implicit_return(
        &mut self,
        block: &Block,
        return_type: ValueType,
    ) -> CResult<()> {
        let stmts = &block.stmts;

        if stmts.is_empty() {
            // Empty block - return default value
            self.emit_default_return(return_type);
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
                self.emit_return_for_type(ty);
            }
            Stmt::Return { value: None, .. } => {
                self.emit(Instr::ReturnNothing);
            }
            Stmt::Expr { expr, .. } => {
                let actual_ty = self.compile_expr(expr)?;
                if actual_ty != return_type
                    && can_convert_type(actual_ty.clone(), return_type.clone())
                {
                    self.emit_type_conversion(actual_ty, return_type.clone());
                    self.emit_return_for_type(return_type);
                } else {
                    self.emit_return_for_type(actual_ty);
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Nested if - recursively handle
                self.compile_if_with_implicit_return(
                    condition,
                    then_branch,
                    else_branch.as_ref(),
                    return_type,
                )?;
            }
            _ => {
                // Other statements - compile normally and return default
                self.compile_stmt(last_stmt)?;
                self.emit_default_return(return_type);
            }
        }

        Ok(())
    }

    pub(super) fn compile_stmt(&mut self, stmt: &Stmt) -> CResult<()> {
        if self.compile_try_stmt(stmt)?.is_some() {
            return Ok(());
        }

        match stmt {
            Stmt::Block(block) => {
                // Inline block: compile all statements in the block
                self.compile_block(block)?;
                Ok(())
            }
            Stmt::Assign { var, value, .. } => {
                // Check for module assignment: S = Statistics, R = Random, etc.
                // Also handle transitive aliases: T = S where S is already a module alias
                if let Expr::Var(module_name, _) = value {
                    // Check if it's a known stdlib module
                    if is_stdlib_module(module_name) {
                        self.module_aliases.insert(var.clone(), module_name.clone());
                        self.locals.insert(var.clone(), ValueType::Module);
                        return Ok(());
                    }
                    // Check if it's an existing module alias (transitive alias)
                    if let Some(resolved) = self.module_aliases.get(module_name).cloned() {
                        self.module_aliases.insert(var.clone(), resolved);
                        self.locals.insert(var.clone(), ValueType::Module);
                        return Ok(());
                    }
                }

                // Track JuliaType for parametric types to enable proper dispatch.
                //
                // DESIGN PRINCIPLE: Track based on *inferred type*, not *expression form*.
                // This ensures all sources of parametric types are covered: literals,
                // variable reassignment (t2 = t1), function returns (t3 = make_pair()),
                // conditional expressions (t = if c; (1,2) else (3,4) end), etc.
                //
                // Non-parametric ValueTypes (Tuple, Array) cannot distinguish between
                // Tuple{Int64, Int64} and Tuple{String, Float64}, or Vector{Int64} and
                // Vector{Any}. We store the full JuliaType in `julia_type_locals` so
                // that `infer_julia_type()` can recover the parametric type for method
                // dispatch.
                //
                // See Issue #1748 (original), #2305 (reassignment), #2319 (conditional),
                // #2352 (VectorOf/MatrixOf dispatch).
                {
                    let julia_type = self.infer_julia_type(value);
                    // Track TupleOf, VectorOf, and MatrixOf for parametric dispatch
                    if matches!(
                        julia_type,
                        JuliaType::TupleOf(_) | JuliaType::VectorOf(_) | JuliaType::MatrixOf(_)
                    ) {
                        self.julia_type_locals.insert(var.clone(), julia_type);
                    }
                }

                // Check if there's a pre-populated "wider" type for this variable
                // This ensures consistent type usage when a variable starts as I64
                // but later receives F64 values (e.g., sum = 0; sum = sum + f64_val)
                let target_ty = self.locals.get(var).cloned();
                let ty = self.compile_expr(value)?;

                // Check if this is a compound assignment pattern (var = var op mixed_type_var)
                // where the operand is a variable in mixed_type_vars.
                // This only applies when we know the operand is from a mixed I64/F64 variable,
                // NOT when it's an untyped parameter (which could be any type at runtime).
                let is_mixed_type_compound_assignment = match value {
                    Expr::BinaryOp { left, right, .. } => {
                        let is_left_var =
                            matches!(left.as_ref(), Expr::Var(name, _) if name == var);
                        let right_is_mixed = matches!(right.as_ref(), Expr::Var(name, _) if self.mixed_type_vars.contains(name));
                        is_left_var && right_is_mixed
                    }
                    _ => false,
                };

                let final_ty = match (target_ty, ty.clone()) {
                    // If target is Any AND it's a function parameter with no type annotation,
                    // keep it as Any to use StoreAny/LoadAny for dynamic type handling.
                    (Some(ValueType::Any), _) if self.any_params.contains(var) => ValueType::Any,
                    // If target is Any AND it's a mixed-type variable (F64+I64 in different branches),
                    // use dynamic typing to allow runtime type changes (Julia semantics).
                    (Some(ValueType::Any), ValueType::I64)
                    | (Some(ValueType::Any), ValueType::F64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        ValueType::Any
                    }
                    // For mixed-type variables (F64+I64 in sequence), use dynamic typing.
                    // This allows `x = 1.0; x = 2` to have typeof(x) == Int64, not Float64.
                    (Some(ValueType::F64), ValueType::I64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        // Use the actual type (I64) for proper dynamic typing
                        ty
                    }
                    (Some(ValueType::I64), ValueType::F64)
                        if self.mixed_type_vars.contains(var) =>
                    {
                        // Use the actual type (F64) for proper dynamic typing
                        ty
                    }
                    // If pre-populated type is F64 but compiled type is I64, convert.
                    // This is needed for widening where the type inference determined
                    // that a variable can be both F64 and I64 (e.g., in control flow).
                    // Only applies to non-mixed-type variables (checked above).
                    (Some(ValueType::F64), ValueType::I64) => {
                        self.emit(Instr::ToF64);
                        ValueType::F64
                    }
                    // Compound assignments (x = x op y) where y is a mixed-type variable:
                    // Preserve x's numeric type because y will be numeric at runtime.
                    // This does NOT apply when y is an untyped parameter (could be any type).
                    (Some(ValueType::I64), ValueType::Any) if is_mixed_type_compound_assignment => {
                        self.emit(Instr::DynamicToI64);
                        ValueType::I64
                    }
                    (Some(ValueType::F64), ValueType::Any) if is_mixed_type_compound_assignment => {
                        self.emit(Instr::DynamicToF64);
                        ValueType::F64
                    }
                    // If pre-populated type is Struct but compiled type is Any,
                    // preserve the struct type (compile_binary_op may return Any
                    // for dynamic dispatch but type inference correctly identified the type)
                    (Some(ValueType::Struct(type_id)), ValueType::Any) => {
                        ValueType::Struct(type_id)
                    }
                    // Note: Complex type conversions are now handled via Pure Julia convert().
                    // Otherwise, use the compiled type.
                    _ => ty,
                };

                self.store_local(var, final_ty);
                Ok(())
            }
            Stmt::AddAssign { var, value, .. } => {
                let var_ty = self.locals.get(var).cloned().unwrap_or(ValueType::I64);
                self.load_local(var)?;
                self.compile_expr_as(value, var_ty.clone())?;
                self.emit(match var_ty {
                    ValueType::I64 => Instr::AddI64,
                    ValueType::F64 => Instr::AddF64,
                    _ => return err("AddAssign not supported for this type"),
                });
                self.store_local(var, var_ty);
                Ok(())
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                ..
            } => {
                // For loop: for var in start:end or start:step:end
                self.locals.insert(var.clone(), ValueType::I64);

                let stop_var = self.new_temp("stop");
                let step_var = self.new_temp("step");

                // Compile and store stop value
                self.compile_expr_as(end, ValueType::I64)?;
                self.emit(Instr::StoreI64(stop_var.clone()));

                // Compile and store step value (default 1 if not specified)
                if let Some(step_expr) = step {
                    self.compile_expr_as(step_expr, ValueType::I64)?;
                } else {
                    self.emit(Instr::PushI64(1));
                }
                self.emit(Instr::StoreI64(step_var.clone()));

                // Initialize loop variable
                self.compile_expr_as(start, ValueType::I64)?;
                self.emit(Instr::StoreI64(var.clone()));

                let loop_start = self.here();

                // Push loop context for break/continue
                let mut loop_ctx = LoopContext {
                    exit_patches: Vec::new(),
                    continue_patches: Vec::new(),
                };

                // Check loop condition based on step sign:
                // If step > 0: continue while var <= stop (exit when var > stop)
                // If step < 0: continue while var >= stop (exit when var < stop)
                // We check: (step > 0 && var > stop) || (step < 0 && var < stop)

                // Check if step > 0
                self.emit(Instr::LoadI64(step_var.clone()));
                self.emit(Instr::PushI64(0));
                self.emit(Instr::GtI64);
                let j_positive = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // jump to negative check if step <= 0

                // Step is positive: check var > stop
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(stop_var.clone()));
                self.emit(Instr::GtI64);
                let j_exit_pos = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // continue if var <= stop
                let j_to_exit1 = self.here();
                self.emit(Instr::Jump(usize::MAX)); // exit loop
                loop_ctx.exit_patches.push(j_to_exit1);

                // Step is negative: check var < stop
                let negative_check = self.here();
                self.patch_jump(j_positive, negative_check);
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(stop_var.clone()));
                self.emit(Instr::LtI64);
                let j_exit_neg = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // continue if var >= stop
                let j_to_exit2 = self.here();
                self.emit(Instr::Jump(usize::MAX)); // exit loop
                loop_ctx.exit_patches.push(j_to_exit2);

                let body_start = self.here();
                self.patch_jump(j_exit_pos, body_start);
                self.patch_jump(j_exit_neg, body_start);

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                self.compile_block(body)?;
                let loop_ctx = self.loop_stack.pop().unwrap();

                let continue_target = self.here();

                // Increment by step
                self.emit(Instr::LoadI64(var.clone()));
                self.emit(Instr::LoadI64(step_var.clone()));
                self.emit(Instr::AddI64);
                self.emit(Instr::StoreI64(var.clone()));

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Patch all exit jumps (from condition and any break statements)
                for patch_pos in loop_ctx.exit_patches {
                    self.patch_jump(patch_pos, exit);
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                Ok(())
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // ForEach loop: for var in iterable
                // Strategy:
                // 1. Compile and store iterable
                // 2. Call iterate(collection) to get (element, state) or Nothing
                // 3. If Nothing, exit loop
                // 4. Store element in loop variable, execute body
                // 5. Call iterate(collection, state) to get next (element, state) or Nothing
                // 6. If Nothing, exit; otherwise loop back to step 4
                //
                // For custom iterators (struct types), we use Pure Julia iterate methods.
                // For builtin types (Array, Range, Tuple, String), we use VM instructions.

                // Check if we should use Pure Julia iterate (for struct types)
                let iterable_ty = self.infer_julia_type(iterable);
                let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);

                // Store the iterable
                let iterable_var = self.new_temp("iterable");
                let state_var = self.new_temp("state");
                let iter_result_var = self.new_temp("iter_result");
                self.compile_expr(iterable)?;
                self.emit(Instr::StoreAny(iterable_var.clone()));

                // Get first iteration result: iterate(collection)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_1(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateFirst);
                }
                // Stack: (element, state) or Nothing
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_exit_first = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // Continue if NOT Nothing
                let j_to_exit_first = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Exit if Nothing

                let continue_after_check = self.here();
                self.patch_jump(j_exit_first, continue_after_check);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond); // Get state
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst); // Get element

                let loop_start = self.here();

                // Store element in loop variable
                self.emit(Instr::StoreAny(var.clone()));
                self.locals.insert(var.clone(), ValueType::Any);

                // Push loop context for break/continue
                let loop_ctx = LoopContext {
                    exit_patches: vec![j_to_exit_first],
                    continue_patches: Vec::new(),
                };

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                self.compile_block(body)?;
                let loop_ctx = self.loop_stack.pop().unwrap();

                let continue_target = self.here();

                // Get next iteration result: iterate(collection, state)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit(Instr::LoadAny(state_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_2(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateNext);
                }
                // Stack: (element, state) or Nothing
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_check_loop = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX)); // Continue if NOT Nothing
                let j_to_exit_loop = self.here();
                self.emit(Instr::Jump(usize::MAX)); // Exit if Nothing

                let continue_after_check2 = self.here();
                self.patch_jump(j_check_loop, continue_after_check2);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond); // Get state
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst); // Get element

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();

                // Patch all exit jumps
                self.patch_jump(j_to_exit_first, exit);
                self.patch_jump(j_to_exit_loop, exit);
                for patch_pos in loop_ctx.exit_patches {
                    if patch_pos != j_to_exit_first {
                        self.patch_jump(patch_pos, exit);
                    }
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                Ok(())
            }
            Stmt::ForEachTuple {
                vars,
                iterable,
                body,
                ..
            } => {
                // ForEachTuple loop: for (a, b) in iterable
                // Similar to ForEach but destructures each element into multiple vars
                //
                // For custom iterators (struct types), we use Pure Julia iterate methods.
                // For builtin types (Array, Range, Tuple, String), we use VM instructions.

                // Check if we should use Pure Julia iterate (for struct types)
                let iterable_ty = self.infer_julia_type(iterable);
                let use_pure_julia_iterate = self.should_use_pure_julia_iterate(&iterable_ty);

                let iterable_var = self.new_temp("iterable");
                let state_var = self.new_temp("state");
                let iter_result_var = self.new_temp("iter_result");
                let elem_var = self.new_temp("elem");
                self.compile_expr(iterable)?;
                self.emit(Instr::StoreAny(iterable_var.clone()));

                // Get first iteration result: iterate(collection)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_1(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateFirst);
                }
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_exit_first = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_to_exit_first = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let continue_after_check = self.here();
                self.patch_jump(j_exit_first, continue_after_check);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond);
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst);
                self.emit(Instr::StoreAny(elem_var.clone()));

                let loop_start = self.here();

                // Destructure element tuple into individual variables
                // Element is already a tuple like (1, 10), extract each component
                for (i, var) in vars.iter().enumerate() {
                    self.emit(Instr::LoadAny(elem_var.clone()));
                    self.emit(Instr::PushI64((i + 1) as i64)); // 1-indexed
                    self.emit(Instr::TupleGet);
                    self.emit(Instr::StoreAny(var.clone()));
                    self.locals.insert(var.clone(), ValueType::Any);
                }

                // Push loop context for break/continue
                let loop_ctx = LoopContext {
                    exit_patches: vec![j_to_exit_first],
                    continue_patches: Vec::new(),
                };

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                self.compile_block(body)?;
                let loop_ctx = self.loop_stack.pop().unwrap();

                let continue_target = self.here();

                // Get next iteration result: iterate(collection, state)
                self.emit(Instr::LoadAny(iterable_var.clone()));
                self.emit(Instr::LoadAny(state_var.clone()));
                if use_pure_julia_iterate {
                    self.emit_iterate_call_2(&iterable_ty)?;
                } else {
                    self.emit(Instr::IterateNext);
                }
                self.emit(Instr::StoreAny(iter_result_var.clone()));

                // Check if Nothing
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::IsNothing);
                let j_check_loop = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                let j_to_exit_loop = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let continue_after_check2 = self.here();
                self.patch_jump(j_check_loop, continue_after_check2);

                // Extract element and state from tuple
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleSecond);
                self.emit(Instr::StoreAny(state_var.clone()));
                self.emit(Instr::LoadAny(iter_result_var.clone()));
                self.emit(Instr::TupleFirst);
                self.emit(Instr::StoreAny(elem_var.clone()));

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();

                // Patch all exit jumps
                self.patch_jump(j_to_exit_first, exit);
                self.patch_jump(j_to_exit_loop, exit);
                for patch_pos in loop_ctx.exit_patches {
                    if patch_pos != j_to_exit_first {
                        self.patch_jump(patch_pos, exit);
                    }
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, continue_target);
                }

                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                let loop_start = self.here();

                // Push loop context for break/continue
                let mut loop_ctx = LoopContext {
                    exit_patches: Vec::new(),
                    continue_patches: Vec::new(),
                };

                // Compile condition without type coercion - JumpIfZero will check for Bool
                self.compile_expr(condition)?;
                let j_exit = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));
                loop_ctx.exit_patches.push(j_exit);

                // Compile body with loop context
                self.loop_stack.push(loop_ctx);
                self.compile_block(body)?;
                let loop_ctx = self.loop_stack.pop().unwrap();

                self.emit(Instr::Jump(loop_start));

                let exit = self.here();
                // Patch all exit jumps (from condition and any break statements)
                for patch_pos in loop_ctx.exit_patches {
                    self.patch_jump(patch_pos, exit);
                }
                for patch_pos in loop_ctx.continue_patches {
                    self.patch_jump(patch_pos, loop_start);
                }
                Ok(())
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Dead code elimination: skip provably dead branches (Issue #3364)
                if let Expr::Literal(Literal::Bool(b), _) = condition {
                    if *b {
                        // Condition is always true: only compile then-branch
                        self.compile_block(then_branch)?;
                    } else if let Some(else_block) = else_branch {
                        // Condition is always false: only compile else-branch
                        self.compile_block(else_block)?;
                    }
                    return Ok(());
                }

                // Compile condition without type coercion - JumpIfZero will check for Bool
                self.compile_expr(condition)?;
                let j_else = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));

                self.compile_block(then_branch)?;
                let j_end = self.here();
                self.emit(Instr::Jump(usize::MAX));

                let else_start = self.here();
                self.patch_jump(j_else, else_start);

                if let Some(else_block) = else_branch {
                    self.compile_block(else_block)?;
                }

                let end = self.here();
                self.patch_jump(j_end, end);
                Ok(())
            }
            Stmt::Return { value, .. } => {
                // Check if there are pending finally blocks
                if self.finally_stack.is_empty() {
                    // No finally blocks - original behavior
                    if let Some(expr) = value {
                        let ty = self.compile_expr(expr)?;
                        self.emit(match ty {
                            ValueType::I64 => Instr::ReturnI64,
                            ValueType::F64 => Instr::ReturnF64,
                            ValueType::Array | ValueType::ArrayOf(_) => Instr::ReturnArray,
                            ValueType::Str => Instr::ReturnAny,
                            // Use ReturnAny for Nothing to consume the pushed value (Issue #2072)
                            ValueType::Nothing => Instr::ReturnAny,
                            ValueType::Missing => Instr::ReturnAny,
                            ValueType::Struct(_) => Instr::ReturnStruct,
                            ValueType::Rng => Instr::ReturnRng,
                            ValueType::Range => Instr::ReturnRange,
                            ValueType::Tuple => Instr::ReturnTuple,
                            ValueType::NamedTuple => Instr::ReturnNamedTuple,
                            ValueType::Dict | ValueType::Set => Instr::ReturnDict,
                            ValueType::Generator => Instr::ReturnAny,
                            ValueType::Char => Instr::ReturnAny,
                            ValueType::Any => Instr::ReturnAny,
                            ValueType::DataType => Instr::ReturnAny,
                            ValueType::Module => Instr::ReturnAny,
                            ValueType::BigInt => Instr::ReturnAny,
                            ValueType::BigFloat => Instr::ReturnAny,
                            ValueType::IO => Instr::ReturnAny,
                            ValueType::Function => Instr::ReturnAny,
                            ValueType::I8 | ValueType::I16 | ValueType::I32 | ValueType::I128 => {
                                Instr::ReturnAny
                            }
                            ValueType::U8
                            | ValueType::U16
                            | ValueType::U32
                            | ValueType::U64
                            | ValueType::U128 => Instr::ReturnAny,
                            ValueType::F32 => Instr::ReturnF32,
                            ValueType::F16 => Instr::ReturnF16,
                            ValueType::Bool => Instr::ReturnAny,
                            ValueType::Symbol
                            | ValueType::Expr
                            | ValueType::QuoteNode
                            | ValueType::LineNumberNode
                            | ValueType::GlobalRef => Instr::ReturnAny,
                            ValueType::Pairs => Instr::ReturnAny,
                            ValueType::Regex | ValueType::RegexMatch => Instr::ReturnAny,
                            ValueType::Enum => Instr::ReturnAny,
                            ValueType::Union(_) => Instr::ReturnAny,
                            ValueType::Memory | ValueType::MemoryOf(_) => Instr::ReturnAny,
                        });
                    } else {
                        self.emit(Instr::ReturnNothing);
                    }
                } else {
                    // Has finally blocks - save return value, execute finally, then return
                    let (saved_temp, saved_ty) = if let Some(expr) = value {
                        let ty = self.compile_expr(expr)?;
                        let temp = self.new_temp("return_val");
                        match ty {
                            ValueType::I64 => self.emit(Instr::StoreI64(temp.clone())),
                            ValueType::F64 => self.emit(Instr::StoreF64(temp.clone())),
                            ValueType::Array | ValueType::ArrayOf(_) => {
                                self.emit(Instr::StoreArray(temp.clone()))
                            }
                            ValueType::Tuple => self.emit(Instr::StoreTuple(temp.clone())),
                            ValueType::NamedTuple => {
                                self.emit(Instr::StoreNamedTuple(temp.clone()))
                            }
                            ValueType::Dict | ValueType::Set => {
                                self.emit(Instr::StoreDict(temp.clone()))
                            }
                            ValueType::Range => self.emit(Instr::StoreRange(temp.clone())),
                            ValueType::Rng => self.emit(Instr::StoreRng(temp.clone())),
                            ValueType::Struct(_) => self.emit(Instr::StoreStruct(temp.clone())),
                            _ => self.emit(Instr::StoreAny(temp.clone())),
                        }
                        self.locals.insert(temp.clone(), ty.clone());
                        (Some(temp), ty)
                    } else {
                        (None, ValueType::Nothing)
                    };

                    // Execute all pending finally blocks in reverse order
                    let finally_blocks: Vec<_> = self
                        .finally_stack
                        .iter()
                        .map(|ctx| ctx.finally_block.clone())
                        .collect();
                    for block in finally_blocks.iter().rev() {
                        self.compile_block(block)?;
                    }

                    // Load return value and return
                    if let Some(ref temp) = saved_temp {
                        match saved_ty {
                            ValueType::I64 => self.emit(Instr::LoadI64(temp.clone())),
                            ValueType::F64 => self.emit(Instr::LoadF64(temp.clone())),
                            ValueType::Array | ValueType::ArrayOf(_) => {
                                self.emit(Instr::LoadArray(temp.clone()))
                            }
                            ValueType::Tuple => self.emit(Instr::LoadTuple(temp.clone())),
                            ValueType::NamedTuple => self.emit(Instr::LoadNamedTuple(temp.clone())),
                            ValueType::Dict | ValueType::Set => {
                                self.emit(Instr::LoadDict(temp.clone()))
                            }
                            ValueType::Range => self.emit(Instr::LoadRange(temp.clone())),
                            ValueType::Rng => self.emit(Instr::LoadRng(temp.clone())),
                            ValueType::Struct(_) => self.emit(Instr::LoadStruct(temp.clone())),
                            _ => self.emit(Instr::LoadAny(temp.clone())),
                        }
                    }
                    self.emit(match saved_ty {
                        ValueType::I64 => Instr::ReturnI64,
                        ValueType::F64 => Instr::ReturnF64,
                        ValueType::Array | ValueType::ArrayOf(_) => Instr::ReturnArray,
                        ValueType::Struct(_) => Instr::ReturnStruct,
                        ValueType::Rng => Instr::ReturnRng,
                        ValueType::Range => Instr::ReturnRange,
                        ValueType::Tuple => Instr::ReturnTuple,
                        ValueType::NamedTuple => Instr::ReturnNamedTuple,
                        ValueType::Dict | ValueType::Set => Instr::ReturnDict,
                        // When saved_temp is Some, a Load pushed a value — use ReturnAny
                        // to consume it. When None, no value on stack — use ReturnNothing.
                        // (Issue #2072)
                        ValueType::Nothing => {
                            if saved_temp.is_some() {
                                Instr::ReturnAny
                            } else {
                                Instr::ReturnNothing
                            }
                        }
                        _ => Instr::ReturnAny,
                    });
                }
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                let ty = self.compile_expr(expr)?;
                // Pop unused value by storing to dummy variable
                let dummy = self.new_temp("discard");
                match ty {
                    ValueType::I64 => self.emit(Instr::StoreI64(dummy)),
                    ValueType::F64 => self.emit(Instr::StoreF64(dummy)),
                    ValueType::Array | ValueType::ArrayOf(_) => self.emit(Instr::StoreArray(dummy)),
                    ValueType::Str => {}       // Skip for strings
                    ValueType::Nothing => {}   // Nothing doesn't need storing
                    ValueType::Missing => {}   // Missing doesn't need storing
                    ValueType::Struct(_) => {} // Skip for structs (including Complex)
                    ValueType::Rng => self.emit(Instr::StoreRng(dummy)),
                    ValueType::Range => self.emit(Instr::StoreRange(dummy)),
                    ValueType::Tuple => self.emit(Instr::StoreTuple(dummy)),
                    ValueType::NamedTuple => self.emit(Instr::StoreNamedTuple(dummy)),
                    ValueType::Dict | ValueType::Set => self.emit(Instr::StoreDict(dummy)),
                    ValueType::Generator => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Char => self.emit(Instr::StoreAny(dummy)),
                    ValueType::DataType => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Module => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Any => self.emit(Instr::StoreAny(dummy)),
                    ValueType::BigInt => self.emit(Instr::StoreAny(dummy)),
                    ValueType::BigFloat => self.emit(Instr::StoreAny(dummy)),
                    ValueType::IO => self.emit(Instr::StoreAny(dummy)),
                    ValueType::Function => self.emit(Instr::StoreAny(dummy)),
                    // Narrow integer types use StoreAny which dispatches to locals_narrow_int
                    // at runtime, preserving the exact Value type (e.g. I8(42), U32(99)).
                    ValueType::I8 | ValueType::I16 | ValueType::I32 | ValueType::I128 => {
                        self.emit(Instr::StoreAny(dummy))
                    }
                    ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128 => self.emit(Instr::StoreAny(dummy)),
                    ValueType::F32 => self.emit(Instr::StoreF32(dummy)),
                    ValueType::F16 => self.emit(Instr::StoreF16(dummy)),
                    ValueType::Bool => self.emit(Instr::StoreAny(dummy)),
                    // Macro system types
                    ValueType::Symbol
                    | ValueType::Expr
                    | ValueType::QuoteNode
                    | ValueType::LineNumberNode
                    | ValueType::GlobalRef => self.emit(Instr::StoreAny(dummy)),
                    // Pairs type (for kwargs...)
                    ValueType::Pairs => self.emit(Instr::StoreAny(dummy)),
                    // Regex types
                    ValueType::Regex | ValueType::RegexMatch => self.emit(Instr::StoreAny(dummy)),
                    // Enum type
                    ValueType::Enum => self.emit(Instr::StoreAny(dummy)),
                    // Union type
                    ValueType::Union(_) => self.emit(Instr::StoreAny(dummy)),
                    // Memory type
                    ValueType::Memory | ValueType::MemoryOf(_) => self.emit(Instr::StoreAny(dummy)),
                }
                Ok(())
            }
            Stmt::Break { .. } => {
                // Jump to the exit of the innermost loop
                if self.loop_stack.is_empty() {
                    return err("break outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();

                // Execute finally blocks inside the current loop
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|ctx| ctx.loop_depth >= current_loop_depth)
                    .map(|ctx| ctx.finally_block.clone())
                    .collect();
                for block in finally_blocks.iter().rev() {
                    self.compile_block(block)?;
                }

                let j_exit = self.here();
                self.emit(Instr::Jump(usize::MAX));
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.exit_patches.push(j_exit);
                }
                Ok(())
            }
            Stmt::Continue { .. } => {
                // Jump to the entry of the innermost loop
                if self.loop_stack.is_empty() {
                    return err("continue outside of loop");
                }
                let current_loop_depth = self.loop_stack.len();

                // Execute finally blocks inside the current loop
                let finally_blocks: Vec<_> = self
                    .finally_stack
                    .iter()
                    .filter(|ctx| ctx.loop_depth >= current_loop_depth)
                    .map(|ctx| ctx.finally_block.clone())
                    .collect();
                for block in finally_blocks.iter().rev() {
                    self.compile_block(block)?;
                }

                let j_continue = self.here();
                self.emit(Instr::Jump(usize::MAX));
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_patches.push(j_continue);
                }
                Ok(())
            }
            Stmt::Test {
                condition, message, ..
            } => {
                self.compile_expr_as(condition, ValueType::Bool)?;
                let msg = message.clone().unwrap_or_default();
                self.emit(Instr::Test(msg));
                Ok(())
            }
            Stmt::TestSet { name, body, .. } => {
                self.emit(Instr::TestSetBegin(name.clone()));
                self.compile_block(body)?;
                self.emit(Instr::TestSetEnd);
                Ok(())
            }
            Stmt::TestThrows {
                exception_type,
                expr,
                ..
            } => {
                // @test_throws ExceptionType expr
                // Uses try/catch pattern: if exception is thrown, it's a pass; if not, it's a fail
                let catch_start = self.here();
                self.emit(Instr::PushHandler(None, None)); // placeholder, will be patched

                // Set up test_throws state
                self.emit(Instr::TestThrowsBegin(exception_type.clone()));

                // Compile the expression that should throw
                self.compile_expr(expr)?;
                self.emit(Instr::Pop);

                // If we reach here, no exception was thrown - that's a failure
                self.emit(Instr::PopHandler);
                self.emit(Instr::TestThrowsEnd); // Will report failure (no exception)
                let jump_to_end = self.here();
                self.emit(Instr::Jump(usize::MAX)); // placeholder

                // Catch block - exception was thrown
                let catch_ip = self.here();
                self.emit(Instr::ClearError);
                self.emit(Instr::TestThrowsEnd); // Will report success

                // Patch the handler to jump to catch
                self.code[catch_start] = Instr::PushHandler(Some(catch_ip), None);

                // Patch the jump to skip catch block
                let end = self.here();
                self.code[jump_to_end] = Instr::Jump(end);

                Ok(())
            }
            Stmt::Timed { body, .. } => {
                self.emit(Instr::TimeNs);
                self.emit(Instr::StoreI64("__time_start".to_string()));

                self.compile_block(body)?;

                self.emit(Instr::TimeNs);
                self.emit(Instr::LoadI64("__time_start".to_string()));
                self.emit(Instr::SubI64);
                self.emit(Instr::ToF64);
                self.emit(Instr::PushF64(1_000_000_000.0));
                self.emit(Instr::DivF64);
                self.emit(Instr::PushStr("  ".to_string()));
                self.emit(Instr::PrintStrNoNewline);
                self.emit(Instr::PrintF64NoNewline);
                self.emit(Instr::PushStr(" seconds".to_string()));
                self.emit(Instr::PrintStr);
                Ok(())
            }
            Stmt::IndexAssign {
                array,
                indices,
                value,
                ..
            } => {
                // Julia-compliant: arr[i] = v is equivalent to setindex!(arr, v, i)
                // We implement this directly with VM instructions for efficiency,
                // and store the modified collection back to the variable.
                let target_ty = self.locals.get(array).cloned();
                // Check if this is a global variable (in global_types but not in locals)
                let is_global =
                    target_ty.is_none() && self.shared_ctx.global_types.contains_key(array);
                match target_ty {
                    Some(ValueType::Dict) => {
                        // Dict assignment: setindex!(d, value, key)
                        if indices.len() != 1 {
                            return err("Dict indexing requires exactly one key");
                        }
                        self.emit(Instr::LoadDict(array.clone()));
                        self.compile_expr(&indices[0])?;
                        self.compile_expr(value)?;
                        self.emit(Instr::DictSet);
                        self.emit(Instr::StoreDict(array.clone()));
                        Ok(())
                    }
                    _ => {
                        // Array/struct assignment: setindex!(collection, value, indices...)
                        // Use typed load so StructRef (e.g., SubArray) is supported by IndexStore.
                        self.load_local(array)?;
                        for idx in indices {
                            // When collection type is Any, the index may be a non-integer
                            // key for Dict indexing. Don't force I64 conversion to allow
                            // runtime Dict dispatch (Issue #1814).
                            let idx_type = self.infer_expr_type(idx);
                            if matches!(target_ty, Some(ValueType::Any) | None)
                                && matches!(
                                    idx_type,
                                    ValueType::Any | ValueType::Str | ValueType::Symbol
                                )
                            {
                                self.compile_expr(idx)?;
                            } else {
                                self.compile_expr_as(idx, ValueType::I64)?;
                            }
                        }
                        // Compile value without type coercion to support tuples and other types
                        let val_ty = self.compile_expr(value)?;
                        // Only coerce to F64 if it's a numeric type (not Tuple, Struct, etc.)
                        match val_ty {
                            ValueType::I64 | ValueType::I32 | ValueType::F32 => {
                                self.emit(Instr::ToF64);
                            }
                            _ => {}
                        }
                        self.emit(Instr::IndexStore(indices.len()));
                        // For global arrays, don't emit StoreArray because:
                        // 1. Arrays are passed by reference - IndexStore modifies in place
                        // 2. StoreArray would create a local slot, shadowing the global
                        // 3. The slotized LoadSlot would then fail to find the value
                        // Instead, just pop the modified array reference from the stack.
                        if is_global {
                            self.emit(Instr::Pop);
                        } else {
                            self.emit(Instr::StoreArray(array.clone()));
                        }
                        Ok(())
                    }
                }
            }
            Stmt::FieldAssign {
                object,
                field,
                value,
                ..
            } => {
                // Get the struct type from the local variable
                let obj_ty =
                    self.locals.get(object).cloned().ok_or_else(|| {
                        CompileError::Msg(format!("Unknown variable: {}", object))
                    })?;

                match obj_ty {
                    ValueType::Struct(type_id) => {
                        // Find the struct info and field index
                        let mut field_idx = None;
                        let mut field_ty = ValueType::F64;
                        let mut is_mutable = false;

                        for (_, struct_info) in self.shared_ctx.struct_table.iter() {
                            if struct_info.type_id == type_id {
                                is_mutable = struct_info.is_mutable;
                                for (idx, (field_name, fty)) in
                                    struct_info.fields.iter().enumerate()
                                {
                                    if field_name == field {
                                        field_idx = Some(idx);
                                        field_ty = fty.clone();
                                        break;
                                    }
                                }
                                break;
                            }
                        }

                        if !is_mutable {
                            return err("Cannot assign to field of immutable struct".to_string());
                        }

                        let idx = field_idx.ok_or_else(|| {
                            CompileError::Msg(format!("Unknown field: {}", field))
                        })?;

                        // Load the struct
                        self.emit(Instr::LoadStruct(object.clone()));

                        // Compile the new value
                        self.compile_expr_as(value, field_ty)?;

                        // Set the field
                        self.emit(Instr::SetField(idx));

                        // Store the modified struct back
                        self.emit(Instr::StoreStruct(object.clone()));

                        Ok(())
                    }
                    ValueType::Any => {
                        // For Any type, we need to do runtime type checking.
                        // Use SetFieldByName for runtime field lookup to avoid
                        // non-deterministic struct_table iteration order (Issue #2748).
                        let mut found_field = false;

                        for (_, struct_info) in self.shared_ctx.struct_table.iter() {
                            if struct_info.fields.iter().any(|(name, _)| name == field) {
                                found_field = true;
                                break;
                            }
                        }

                        // Also check parametric struct definitions
                        if !found_field {
                            for (_, param_def) in self.shared_ctx.parametric_structs.iter() {
                                if param_def.def.fields.iter().any(|f| f.name == *field) {
                                    found_field = true;
                                    break;
                                }
                            }
                        }

                        if !found_field {
                            return err(format!("Unknown field: {}", field));
                        }

                        // Load the struct (runtime will check if it's actually a struct)
                        self.emit(Instr::LoadAny(object.clone()));

                        // Compile the new value as Any (runtime will handle type)
                        self.compile_expr(value)?;

                        // Set the field by name at runtime (resolves correct index)
                        self.emit(Instr::SetFieldByName(field.to_string()));

                        // Store the modified struct back
                        self.emit(Instr::StoreAny(object.clone()));

                        Ok(())
                    }
                    _ => err("Field assignment requires a struct variable"),
                }
            }
            Stmt::Try { .. } => {
                err("internal: Try statement reached compile_stmt (should be handled by compile_try_stmt)")
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                // Compile the tuple value
                let ty = self.compile_expr(value)?;
                if ty != ValueType::Tuple {
                    return err("Destructuring assignment requires a tuple");
                }
                // Store the tuple temporarily
                let temp_tuple = self.new_temp("tuple");
                self.emit(Instr::StoreTuple(temp_tuple.clone()));

                // Extract each element and assign to targets
                for (i, target) in targets.iter().enumerate() {
                    self.emit(Instr::LoadTuple(temp_tuple.clone()));
                    self.emit(Instr::PushI64((i + 1) as i64));
                    self.emit(Instr::TupleGet);
                    // Tuple element type is unknown at compile time - use Any
                    self.emit(Instr::StoreAny(target.clone()));
                    self.locals.insert(target.clone(), ValueType::Any);
                }
                Ok(())
            }
            Stmt::DictAssign {
                dict, key, value, ..
            } => {
                // dict[key] = value
                self.emit(Instr::LoadDict(dict.clone()));
                self.compile_expr(key)?;
                self.compile_expr(value)?;
                self.emit(Instr::DictSet);
                self.emit(Instr::StoreDict(dict.clone()));
                Ok(())
            }
            Stmt::Using { .. } => {
                // Using statements are processed at the program level,
                // not during statement compilation. They're already
                // collected in program.usings for function resolution.
                Ok(())
            }
            Stmt::Export { .. } => {
                // Export statements are processed at the module level,
                // not during statement compilation. They're already
                // collected in module.exports.
                Ok(())
            }
            Stmt::FunctionDef { func, .. } => {
                // Function definitions inside blocks (e.g., inside @testset, or nested functions).
                // The function has already been compiled during the initial compilation pass.

                // Create a qualified function name for disambiguation when multiple parent
                // functions have nested functions with the same name (Issue #1743).
                // Format: "parent_function#nested_function"
                let qualified_name = if let Some(parent_name) = &self.current_function_name {
                    format!("{}#{}", parent_name, func.name)
                } else {
                    func.name.clone()
                };

                // Check if this is a nested function that needs to capture variables
                // from the enclosing scope (closure).
                // This runs at BOTH function level (strict_undefined_check=true) AND
                // module level (strict_undefined_check=false) to support closures defined
                // at top-level or in @testset blocks (Issue #2358).
                // Include both local variables AND captured variables from ancestor scopes
                // to support 3+ levels of closure nesting (Issue #1744)
                let mut outer_scope_vars: HashSet<String> = self.locals.keys().cloned().collect();
                outer_scope_vars.extend(self.captured_vars.iter().cloned());
                let free_vars = analyze_free_variables(func, &outer_scope_vars);

                if !free_vars.is_empty() {
                    // This is a closure - store capture info for when the function is compiled
                    // Use qualified name to avoid collision between nested functions with same name
                    self.shared_ctx
                        .closure_captures
                        .insert(qualified_name.clone(), free_vars.clone());

                    // Emit CreateClosure with the QUALIFIED function name
                    // FunctionInfo.name also uses the qualified name for nested functions,
                    // so the runtime lookup will find the correct function (Issue #1743)
                    let capture_names: Vec<String> = free_vars.into_iter().collect();
                    self.emit(Instr::CreateClosure {
                        func_name: qualified_name,
                        capture_names,
                    });
                    // Store the closure in the local scope using the ORIGINAL name
                    // (so the local variable `inner` can be accessed normally in user code)
                    self.emit(Instr::StoreAny(func.name.clone()));
                    self.locals.insert(func.name.clone(), ValueType::Any);
                    return Ok(());
                }

                // Regular function definition (not a closure)
                // For nested functions (inside other functions), we need to make them
                // accessible as local variables so they can be called by name.
                if self.strict_undefined_check {
                    // Inside a function body - store as a Function value in local scope
                    self.emit(Instr::PushFunction(qualified_name.clone()));
                    self.emit(Instr::StoreAny(func.name.clone()));
                    self.locals.insert(func.name.clone(), ValueType::Function);
                }

                // Look it up by qualified name and emit a DefineFunction instruction.
                // This instruction is a no-op at runtime but marks that the function
                // definition was executed.
                if let Some(idx) = self.shared_ctx.function_indices.get(&qualified_name) {
                    self.emit(Instr::DefineFunction(*idx));
                }
                // Even if not found, this is OK - the function might be defined
                // elsewhere or be a forward reference.
                Ok(())
            }
            Stmt::Label { name, .. } => {
                // Record the label position for @goto to jump to.
                // The label marks the current instruction position.
                let position = self.here();
                self.label_positions.insert(name.clone(), position);
                Ok(())
            }
            Stmt::Goto { name, span } => {
                // Emit a Jump instruction and record it for patching.
                // We use usize::MAX as a placeholder, which will be patched
                // after all labels are collected.
                let patch_position = self.here();
                self.emit(Instr::Jump(usize::MAX));
                self.goto_patches.push((patch_position, name.clone()));
                // Note: The patch will be applied after compilation by patch_goto_jumps()
                let _ = span; // Span is kept for potential future error reporting
                Ok(())
            }
            Stmt::EnumDef { enum_def, .. } => {
                // Register the enum members as global variables with Enum type
                // The actual values will be loaded by the VM at runtime
                for member in &enum_def.members {
                    // Mark the member as an Enum type in global_types
                    self.shared_ctx
                        .global_types
                        .insert(member.name.clone(), ValueType::Enum);
                }
                Ok(())
            }
        }
    }

    // ==========================================================================
    // Iteration Protocol Helpers
    // ==========================================================================

    /// Check if we should use Pure Julia iterate for this type.
    /// Returns true for struct types (custom iterators), false for builtin types.
    fn should_use_pure_julia_iterate(&self, ty: &JuliaType) -> bool {
        if let Some(result) = static_iterate_strategy(ty) {
            return result;
        }
        // Dynamic fallback: check if there's an iterate method registered
        if let Some(table) = self.method_tables.get("iterate") {
            !table.methods.is_empty()
        } else {
            false
        }
    }

    /// Emit a call to iterate(collection) - 1 argument version.
    /// Looks up the iterate method from method tables and emits a Call instruction.
    fn emit_iterate_call_1(&mut self, ty: &JuliaType) -> CResult<()> {
        if let Some(table) = self.method_tables.get("iterate") {
            let arg_types = vec![ty.clone()];
            if let Ok(method) = table.dispatch(&arg_types) {
                self.emit(Instr::Call(method.global_index, 1));
                return Ok(());
            }
            // Try Any dispatch
            let arg_types_any = vec![JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_any) {
                self.emit(Instr::Call(method.global_index, 1));
                return Ok(());
            }
            // For Any type, use IterateDynamic for runtime struct dispatch
            // This handles cases where the collection is a struct type unknown at compile time
            // (e.g., zip(a, b, c) returns Any, but at runtime it's Zip3)
            if matches!(ty, JuliaType::Any) {
                let candidates: Vec<(usize, String)> = table
                    .methods
                    .iter()
                    .filter(|m| m.params.len() == 1)
                    .filter_map(|m| {
                        if let (_, JuliaType::Struct(type_name)) = &m.params[0] {
                            Some((m.global_index, type_name.clone()))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !candidates.is_empty() {
                    self.emit(Instr::IterateDynamic(1, candidates));
                    return Ok(());
                }
            }
        }
        // Fall back to VM instruction - handles Array, Tuple, String, Range at runtime
        self.emit(Instr::IterateFirst);
        Ok(())
    }

    /// Emit a call to iterate(collection, state) - 2 argument version.
    /// Looks up the iterate method from method tables and emits a Call instruction.
    fn emit_iterate_call_2(&mut self, ty: &JuliaType) -> CResult<()> {
        if let Some(table) = self.method_tables.get("iterate") {
            // Try to find method with (collection_type, Int64) signature
            let arg_types = vec![ty.clone(), JuliaType::Int64];
            if let Ok(method) = table.dispatch(&arg_types) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // Try with Any as second argument
            let arg_types_any = vec![ty.clone(), JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_any) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // Try with both as Any
            let arg_types_both_any = vec![JuliaType::Any, JuliaType::Any];
            if let Ok(method) = table.dispatch(&arg_types_both_any) {
                self.emit(Instr::Call(method.global_index, 2));
                return Ok(());
            }
            // For Any type, use IterateDynamic for runtime struct dispatch
            if matches!(ty, JuliaType::Any) {
                let candidates: Vec<(usize, String)> = table
                    .methods
                    .iter()
                    .filter(|m| m.params.len() == 2)
                    .filter_map(|m| {
                        if let (_, JuliaType::Struct(type_name)) = &m.params[0] {
                            Some((m.global_index, type_name.clone()))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !candidates.is_empty() {
                    self.emit(Instr::IterateDynamic(2, candidates));
                    return Ok(());
                }
            }
        }
        // Fall back to VM instruction - handles Array, Tuple, String, Range at runtime
        self.emit(Instr::IterateNext);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JuliaType;
    use crate::vm::ValueType;

    // ── static_iterate_strategy ───────────────────────────────────────────────

    #[test]
    fn test_static_iterate_strategy_struct_uses_pure_julia() {
        let ty = JuliaType::Struct("Point".to_string());
        assert_eq!(static_iterate_strategy(&ty), Some(true));
    }

    #[test]
    fn test_static_iterate_strategy_cartesian_indices_uses_builtin() {
        let ty = JuliaType::Struct("CartesianIndices".to_string());
        assert_eq!(
            static_iterate_strategy(&ty),
            Some(false),
            "CartesianIndices is special-cased to use VM builtin iterate"
        );
    }

    #[test]
    fn test_static_iterate_strategy_any_uses_pure_julia() {
        assert_eq!(
            static_iterate_strategy(&JuliaType::Any),
            Some(true),
            "Any uses Pure Julia dispatch for runtime struct resolution"
        );
    }

    #[test]
    fn test_static_iterate_strategy_array_types_use_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::Array), Some(false));
        assert_eq!(
            static_iterate_strategy(&JuliaType::VectorOf(Box::new(JuliaType::Int64))),
            Some(false)
        );
        assert_eq!(
            static_iterate_strategy(&JuliaType::MatrixOf(Box::new(JuliaType::Float64))),
            Some(false)
        );
    }

    #[test]
    fn test_static_iterate_strategy_tuple_types_use_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::Tuple), Some(false));
        assert_eq!(
            static_iterate_strategy(&JuliaType::TupleOf(vec![JuliaType::Int64])),
            Some(false)
        );
    }

    #[test]
    fn test_static_iterate_strategy_string_uses_builtin() {
        assert_eq!(static_iterate_strategy(&JuliaType::String), Some(false));
    }

    #[test]
    fn test_static_iterate_strategy_int64_uses_builtin() {
        // Range-like types use VM builtin iterate
        assert_eq!(static_iterate_strategy(&JuliaType::Int64), Some(false));
    }

    #[test]
    fn test_static_iterate_strategy_unknown_types_return_none() {
        // These types require runtime method-table lookup
        assert_eq!(static_iterate_strategy(&JuliaType::Bool), None);
        assert_eq!(static_iterate_strategy(&JuliaType::Float64), None);
        assert_eq!(static_iterate_strategy(&JuliaType::Dict), None);
    }

    // ── can_convert_type ──────────────────────────────────────────────────────

    #[test]
    fn test_can_convert_i64_to_f64() {
        assert!(
            can_convert_type(ValueType::I64, ValueType::F64),
            "I64 → F64 conversion should be supported"
        );
    }

    #[test]
    fn test_can_convert_f64_to_i64() {
        assert!(
            can_convert_type(ValueType::F64, ValueType::I64),
            "F64 → I64 conversion should be supported"
        );
    }

    #[test]
    fn test_cannot_convert_same_type() {
        assert!(
            !can_convert_type(ValueType::I64, ValueType::I64),
            "I64 → I64 is not a conversion (same type)"
        );
        assert!(
            !can_convert_type(ValueType::F64, ValueType::F64),
            "F64 → F64 is not a conversion (same type)"
        );
    }

    #[test]
    fn test_cannot_convert_unrelated_types() {
        assert!(
            !can_convert_type(ValueType::Bool, ValueType::I64),
            "Bool → I64 is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::Str, ValueType::Any),
            "Str → Any is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::I64, ValueType::Bool),
            "I64 → Bool is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::F32, ValueType::F64),
            "F32 → F64 is not a direct VM conversion (no dedicated instruction)"
        );
    }

    #[test]
    fn test_cannot_convert_any_to_concrete() {
        assert!(
            !can_convert_type(ValueType::Any, ValueType::I64),
            "Any → I64 is not a direct VM conversion"
        );
        assert!(
            !can_convert_type(ValueType::Any, ValueType::F64),
            "Any → F64 is not a direct VM conversion"
        );
    }
}
