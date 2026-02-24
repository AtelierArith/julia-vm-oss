//! Loop optimization for AoT IR
//!
//! This module implements loop optimizations including:
//! - Loop Invariant Code Motion (LICM)
//! - Loop unrolling for constant bounds

use crate::aot::ir::{AotBuiltinOp, AotExpr, AotProgram, AotStmt};
use crate::aot::types::StaticType;
use std::collections::HashSet;

/// Configuration for loop optimizations
#[derive(Debug, Clone)]
pub struct LoopOptConfig {
    /// Enable loop invariant code motion
    pub enable_licm: bool,
    /// Enable loop unrolling
    pub enable_unrolling: bool,
    /// Maximum number of iterations for loop unrolling
    pub max_unroll_iterations: usize,
    /// Maximum statements in loop body for unrolling
    pub max_unroll_body_size: usize,
}

impl Default for LoopOptConfig {
    fn default() -> Self {
        Self {
            enable_licm: true,
            enable_unrolling: true,
            max_unroll_iterations: 8,
            max_unroll_body_size: 10,
        }
    }
}

/// Loop optimizer for AoT IR
#[derive(Debug)]
pub struct AotLoopOptimizer {
    /// Configuration
    pub config: LoopOptConfig,
    /// Variable counter for generating unique names
    var_counter: usize,
    /// Statistics
    licm_count: usize,
    unroll_count: usize,
}

impl AotLoopOptimizer {
    /// Create a new loop optimizer with default config
    pub fn new() -> Self {
        Self::with_config(LoopOptConfig::default())
    }

    /// Create a new loop optimizer with custom config
    pub fn with_config(config: LoopOptConfig) -> Self {
        Self {
            config,
            var_counter: 0,
            licm_count: 0,
            unroll_count: 0,
        }
    }

    /// Optimize an AoT program
    pub fn optimize_program(&mut self, program: &mut AotProgram) -> usize {
        let mut total_optimized = 0;

        // Optimize functions
        for func in &mut program.functions {
            total_optimized += self.optimize_stmts(&mut func.body);
        }

        // Optimize main block
        total_optimized += self.optimize_stmts(&mut program.main);

        total_optimized
    }

    /// Get LICM count
    pub fn licm_count(&self) -> usize {
        self.licm_count
    }

    /// Get unroll count
    pub fn unroll_count(&self) -> usize {
        self.unroll_count
    }

    /// Optimize a list of statements
    fn optimize_stmts(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        let mut total_optimized = 0;
        let mut i = 0;

        while i < stmts.len() {
            // Try to optimize this statement
            match &mut stmts[i] {
                AotStmt::ForRange {
                    var,
                    start,
                    stop,
                    step,
                    body,
                } => {
                    // First, recursively optimize nested loops in the body
                    total_optimized += self.optimize_stmts(body);

                    // Try loop unrolling for constant ranges
                    if self.config.enable_unrolling {
                        if let Some(unrolled) =
                            self.try_unroll_for_range(var, start, stop, step, body)
                        {
                            // Replace the loop with unrolled statements
                            stmts.splice(i..=i, unrolled.into_iter());
                            self.unroll_count += 1;
                            total_optimized += 1;
                            continue; // Don't increment i, we replaced the loop
                        }
                    }

                    // Try LICM
                    if self.config.enable_licm {
                        let hoisted = self.try_hoist_invariants(var, body);
                        if !hoisted.is_empty() {
                            let num_hoisted = hoisted.len();
                            // Insert hoisted statements before the loop
                            for (j, stmt) in hoisted.into_iter().enumerate() {
                                stmts.insert(i + j, stmt);
                            }
                            self.licm_count += 1;
                            total_optimized += 1;
                            // Skip over the inserted statements to avoid processing them
                            i += num_hoisted;
                        }
                    }
                }
                AotStmt::ForEach { var, iter: _, body } => {
                    // Recursively optimize body
                    total_optimized += self.optimize_stmts(body);

                    // Try LICM
                    if self.config.enable_licm {
                        let hoisted = self.try_hoist_invariants(var, body);
                        if !hoisted.is_empty() {
                            let num_hoisted = hoisted.len();
                            for (j, stmt) in hoisted.into_iter().enumerate() {
                                stmts.insert(i + j, stmt);
                            }
                            self.licm_count += 1;
                            total_optimized += 1;
                            // Skip over the inserted statements to avoid processing them
                            i += num_hoisted;
                        }
                    }
                }
                AotStmt::While { condition: _, body } => {
                    // Recursively optimize body
                    total_optimized += self.optimize_stmts(body);

                    // For while loops, we can't easily determine invariants
                    // because we don't have a clear loop variable
                }
                AotStmt::If {
                    condition: _,
                    then_branch,
                    else_branch,
                } => {
                    // Recursively optimize branches
                    total_optimized += self.optimize_stmts(then_branch);
                    if let Some(else_b) = else_branch {
                        total_optimized += self.optimize_stmts(else_b);
                    }
                }
                _ => {}
            }
            i += 1;
        }

        total_optimized
    }

    /// Try to unroll a for-range loop with constant bounds
    fn try_unroll_for_range(
        &mut self,
        var: &str,
        start: &AotExpr,
        stop: &AotExpr,
        step: &Option<AotExpr>,
        body: &[AotStmt],
    ) -> Option<Vec<AotStmt>> {
        // Get constant start value
        let start_val = Self::get_constant_i64(start)?;
        // Get constant stop value
        let stop_val = Self::get_constant_i64(stop)?;
        // Get constant step value (default 1)
        let step_val = match step {
            Some(s) => Self::get_constant_i64(s)?,
            None => 1,
        };

        // Sanity checks
        if step_val == 0 {
            return None;
        }

        // Calculate number of iterations
        let iterations = if step_val > 0 {
            if stop_val < start_val {
                0
            } else {
                ((stop_val - start_val) / step_val + 1) as usize
            }
        } else {
            if stop_val > start_val {
                0
            } else {
                ((start_val - stop_val) / (-step_val) + 1) as usize
            }
        };

        // Check if we should unroll
        let body_size = Self::count_body_statements(body);
        if iterations > self.config.max_unroll_iterations {
            return None;
        }
        if body_size > self.config.max_unroll_body_size {
            return None;
        }
        if iterations == 0 {
            return Some(vec![]); // Empty loop, just remove it
        }

        // Generate unrolled statements
        let mut unrolled = Vec::new();
        let mut current_val = start_val;

        for _ in 0..iterations {
            // For each iteration, substitute the loop variable with the current value
            for stmt in body {
                let substituted = self.substitute_var_in_stmt(stmt, var, current_val);
                unrolled.push(substituted);
            }
            current_val += step_val;
        }

        Some(unrolled)
    }

    /// Try to hoist loop invariants out of a loop
    fn try_hoist_invariants(&mut self, loop_var: &str, body: &mut Vec<AotStmt>) -> Vec<AotStmt> {
        let mut hoisted = Vec::new();
        let mut i = 0;

        while i < body.len() {
            if let AotStmt::Let {
                name,
                ty,
                value,
                is_mutable,
            } = &body[i]
            {
                // Skip simple variable references - they're already efficient and hoisting
                // them would just add indirection. This also prevents infinite loops where
                // a hoisted variable reference itself gets hoisted repeatedly.
                let is_simple_var = matches!(value, AotExpr::Var { .. });

                // Check if the value is loop invariant and worth hoisting
                if !is_simple_var && self.is_loop_invariant(value, loop_var, body) && !is_mutable {
                    // Create a new unique name for the hoisted variable
                    let new_name = format!("_licm{}_{}", self.var_counter, name);
                    self.var_counter += 1;

                    // Add the hoisted statement
                    hoisted.push(AotStmt::Let {
                        name: new_name.clone(),
                        ty: ty.clone(),
                        value: value.clone(),
                        is_mutable: false,
                    });

                    // Replace the original statement with a reference to the hoisted variable
                    body[i] = AotStmt::Let {
                        name: name.clone(),
                        ty: ty.clone(),
                        value: AotExpr::Var {
                            name: new_name,
                            ty: ty.clone(),
                        },
                        is_mutable: false,
                    };
                }
            }
            i += 1;
        }

        hoisted
    }

    /// Check if an expression is loop invariant
    fn is_loop_invariant(&self, expr: &AotExpr, loop_var: &str, body: &[AotStmt]) -> bool {
        // Collect all variables modified in the loop
        let modified_vars = Self::collect_modified_vars(body);

        // Check if the expression only depends on invariant values
        Self::expr_is_invariant(expr, loop_var, &modified_vars)
    }

    /// Collect all variables that are modified in a list of statements
    fn collect_modified_vars(stmts: &[AotStmt]) -> HashSet<String> {
        let mut modified = HashSet::new();

        for stmt in stmts {
            match stmt {
                AotStmt::Let {
                    name, is_mutable, ..
                } => {
                    if *is_mutable {
                        modified.insert(name.clone());
                    }
                }
                AotStmt::Assign { target, .. } => {
                    if let AotExpr::Var { name, .. } = target {
                        modified.insert(name.clone());
                    }
                }
                AotStmt::CompoundAssign { target, .. } => {
                    if let AotExpr::Var { name, .. } = target {
                        modified.insert(name.clone());
                    }
                }
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    modified.extend(Self::collect_modified_vars(then_branch));
                    if let Some(else_b) = else_branch {
                        modified.extend(Self::collect_modified_vars(else_b));
                    }
                }
                AotStmt::While { body, .. }
                | AotStmt::ForRange { body, .. }
                | AotStmt::ForEach { body, .. } => {
                    modified.extend(Self::collect_modified_vars(body));
                }
                _ => {}
            }
        }

        modified
    }

    /// Check if an expression is invariant with respect to loop variables
    fn expr_is_invariant(expr: &AotExpr, loop_var: &str, modified_vars: &HashSet<String>) -> bool {
        match expr {
            // Literals are always invariant
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing => true,

            // Variable is invariant if it's not the loop variable and not modified in the loop
            AotExpr::Var { name, .. } => name != loop_var && !modified_vars.contains(name),

            // Binary operations are invariant if both operands are invariant
            AotExpr::BinOpStatic { left, right, .. }
            | AotExpr::BinOpDynamic { left, right, .. } => {
                Self::expr_is_invariant(left, loop_var, modified_vars)
                    && Self::expr_is_invariant(right, loop_var, modified_vars)
            }

            // Unary operations are invariant if operand is invariant
            AotExpr::UnaryOp { operand, .. } => {
                Self::expr_is_invariant(operand, loop_var, modified_vars)
            }

            // Pure builtin calls are invariant if all args are invariant
            AotExpr::CallBuiltin { builtin, args, .. } => {
                let is_pure = matches!(
                    builtin,
                    AotBuiltinOp::Sqrt
                        | AotBuiltinOp::Sin
                        | AotBuiltinOp::Cos
                        | AotBuiltinOp::Tan
                        | AotBuiltinOp::Abs
                        | AotBuiltinOp::Floor
                        | AotBuiltinOp::Ceil
                        | AotBuiltinOp::Round
                        | AotBuiltinOp::Min
                        | AotBuiltinOp::Max
                        | AotBuiltinOp::Length
                );
                is_pure
                    && args
                        .iter()
                        .all(|a| Self::expr_is_invariant(a, loop_var, modified_vars))
            }

            // Function calls are generally not invariant (may have side effects)
            AotExpr::CallStatic { .. } | AotExpr::CallDynamic { .. } => false,

            // Array literals are invariant if all elements are invariant
            AotExpr::ArrayLit { elements, .. } | AotExpr::TupleLit { elements } => elements
                .iter()
                .all(|e| Self::expr_is_invariant(e, loop_var, modified_vars)),

            // Array access is not invariant if the array or index depends on loop var
            AotExpr::Index { array, indices, .. } => {
                Self::expr_is_invariant(array, loop_var, modified_vars)
                    && indices
                        .iter()
                        .all(|i| Self::expr_is_invariant(i, loop_var, modified_vars))
            }

            // Range is invariant if bounds are invariant
            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::expr_is_invariant(start, loop_var, modified_vars)
                    && Self::expr_is_invariant(stop, loop_var, modified_vars)
                    && step.as_ref().map_or(true, |s| {
                        Self::expr_is_invariant(s, loop_var, modified_vars)
                    })
            }

            // Field access is invariant if object is invariant
            AotExpr::FieldAccess { object, .. } => {
                Self::expr_is_invariant(object, loop_var, modified_vars)
            }

            // Ternary is invariant if all parts are invariant
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_is_invariant(condition, loop_var, modified_vars)
                    && Self::expr_is_invariant(then_expr, loop_var, modified_vars)
                    && Self::expr_is_invariant(else_expr, loop_var, modified_vars)
            }

            // Struct creation is invariant if all fields are invariant
            AotExpr::StructNew { fields, .. } => fields
                .iter()
                .all(|f| Self::expr_is_invariant(f, loop_var, modified_vars)),

            // Box/Unbox/Convert are invariant if inner is invariant
            AotExpr::Box(inner)
            | AotExpr::Unbox { value: inner, .. }
            | AotExpr::Convert { value: inner, .. } => {
                Self::expr_is_invariant(inner, loop_var, modified_vars)
            }

            // Lambdas are invariant (the definition itself doesn't change)
            AotExpr::Lambda { .. } => true,
        }
    }

    /// Get a constant i64 value from an expression
    fn get_constant_i64(expr: &AotExpr) -> Option<i64> {
        match expr {
            AotExpr::LitI64(v) => Some(*v),
            AotExpr::LitI32(v) => Some(*v as i64),
            _ => None,
        }
    }

    /// Count statements in a body (non-recursive, just top level)
    fn count_body_statements(body: &[AotStmt]) -> usize {
        body.len()
    }

    /// Substitute a variable with a constant value in a statement
    fn substitute_var_in_stmt(&self, stmt: &AotStmt, var: &str, value: i64) -> AotStmt {
        match stmt {
            AotStmt::Let {
                name,
                ty,
                value: expr,
                is_mutable,
            } => AotStmt::Let {
                name: name.clone(),
                ty: ty.clone(),
                value: self.substitute_var_in_expr(expr, var, value),
                is_mutable: *is_mutable,
            },
            AotStmt::Assign {
                target,
                value: expr,
            } => AotStmt::Assign {
                target: self.substitute_var_in_expr(target, var, value),
                value: self.substitute_var_in_expr(expr, var, value),
            },
            AotStmt::CompoundAssign {
                target,
                op,
                value: expr,
            } => AotStmt::CompoundAssign {
                target: self.substitute_var_in_expr(target, var, value),
                op: *op,
                value: self.substitute_var_in_expr(expr, var, value),
            },
            AotStmt::Expr(expr) => AotStmt::Expr(self.substitute_var_in_expr(expr, var, value)),
            AotStmt::Return(Some(expr)) => {
                AotStmt::Return(Some(self.substitute_var_in_expr(expr, var, value)))
            }
            AotStmt::Return(None) => AotStmt::Return(None),
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => AotStmt::If {
                condition: self.substitute_var_in_expr(condition, var, value),
                then_branch: then_branch
                    .iter()
                    .map(|s| self.substitute_var_in_stmt(s, var, value))
                    .collect(),
                else_branch: else_branch.as_ref().map(|stmts| {
                    stmts
                        .iter()
                        .map(|s| self.substitute_var_in_stmt(s, var, value))
                        .collect()
                }),
            },
            AotStmt::While { condition, body } => AotStmt::While {
                condition: self.substitute_var_in_expr(condition, var, value),
                body: body
                    .iter()
                    .map(|s| self.substitute_var_in_stmt(s, var, value))
                    .collect(),
            },
            AotStmt::ForRange {
                var: loop_var,
                start,
                stop,
                step,
                body,
            } => {
                // Don't substitute in nested loops that shadow the variable
                if loop_var == var {
                    stmt.clone()
                } else {
                    AotStmt::ForRange {
                        var: loop_var.clone(),
                        start: self.substitute_var_in_expr(start, var, value),
                        stop: self.substitute_var_in_expr(stop, var, value),
                        step: step
                            .as_ref()
                            .map(|s| self.substitute_var_in_expr(s, var, value)),
                        body: body
                            .iter()
                            .map(|s| self.substitute_var_in_stmt(s, var, value))
                            .collect(),
                    }
                }
            }
            AotStmt::ForEach {
                var: loop_var,
                iter,
                body,
            } => {
                if loop_var == var {
                    stmt.clone()
                } else {
                    AotStmt::ForEach {
                        var: loop_var.clone(),
                        iter: self.substitute_var_in_expr(iter, var, value),
                        body: body
                            .iter()
                            .map(|s| self.substitute_var_in_stmt(s, var, value))
                            .collect(),
                    }
                }
            }
            AotStmt::Break => AotStmt::Break,
            AotStmt::Continue => AotStmt::Continue,
        }
    }

    /// Substitute a variable with a constant value in an expression
    fn substitute_var_in_expr(&self, expr: &AotExpr, var: &str, value: i64) -> AotExpr {
        match expr {
            AotExpr::Var { name, ty } => {
                if name == var {
                    // Replace with constant
                    match ty {
                        StaticType::I64 => AotExpr::LitI64(value),
                        StaticType::I32 => AotExpr::LitI32(value as i32),
                        _ => AotExpr::LitI64(value), // Default to i64
                    }
                } else {
                    expr.clone()
                }
            }
            AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty,
            } => AotExpr::BinOpStatic {
                op: *op,
                left: Box::new(self.substitute_var_in_expr(left, var, value)),
                right: Box::new(self.substitute_var_in_expr(right, var, value)),
                result_ty: result_ty.clone(),
            },
            AotExpr::BinOpDynamic { op, left, right } => AotExpr::BinOpDynamic {
                op: *op,
                left: Box::new(self.substitute_var_in_expr(left, var, value)),
                right: Box::new(self.substitute_var_in_expr(right, var, value)),
            },
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => AotExpr::UnaryOp {
                op: *op,
                operand: Box::new(self.substitute_var_in_expr(operand, var, value)),
                result_ty: result_ty.clone(),
            },
            AotExpr::CallStatic {
                function,
                args,
                return_ty,
            } => AotExpr::CallStatic {
                function: function.clone(),
                args: args
                    .iter()
                    .map(|a| self.substitute_var_in_expr(a, var, value))
                    .collect(),
                return_ty: return_ty.clone(),
            },
            AotExpr::CallDynamic { function, args } => AotExpr::CallDynamic {
                function: function.clone(),
                args: args
                    .iter()
                    .map(|a| self.substitute_var_in_expr(a, var, value))
                    .collect(),
            },
            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } => AotExpr::CallBuiltin {
                builtin: *builtin,
                args: args
                    .iter()
                    .map(|a| self.substitute_var_in_expr(a, var, value))
                    .collect(),
                return_ty: return_ty.clone(),
            },
            AotExpr::ArrayLit {
                elements,
                elem_ty,
                shape,
            } => AotExpr::ArrayLit {
                elements: elements
                    .iter()
                    .map(|e| self.substitute_var_in_expr(e, var, value))
                    .collect(),
                elem_ty: elem_ty.clone(),
                shape: shape.clone(),
            },
            AotExpr::TupleLit { elements } => AotExpr::TupleLit {
                elements: elements
                    .iter()
                    .map(|e| self.substitute_var_in_expr(e, var, value))
                    .collect(),
            },
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple,
            } => AotExpr::Index {
                array: Box::new(self.substitute_var_in_expr(array, var, value)),
                indices: indices
                    .iter()
                    .map(|i| self.substitute_var_in_expr(i, var, value))
                    .collect(),
                elem_ty: elem_ty.clone(),
                is_tuple: *is_tuple,
            },
            AotExpr::Range {
                start,
                stop,
                step,
                elem_ty,
            } => AotExpr::Range {
                start: Box::new(self.substitute_var_in_expr(start, var, value)),
                stop: Box::new(self.substitute_var_in_expr(stop, var, value)),
                step: step
                    .as_ref()
                    .map(|s| Box::new(self.substitute_var_in_expr(s, var, value))),
                elem_ty: elem_ty.clone(),
            },
            AotExpr::StructNew { name, fields } => AotExpr::StructNew {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|f| self.substitute_var_in_expr(f, var, value))
                    .collect(),
            },
            AotExpr::FieldAccess {
                object,
                field,
                field_ty,
            } => AotExpr::FieldAccess {
                object: Box::new(self.substitute_var_in_expr(object, var, value)),
                field: field.clone(),
                field_ty: field_ty.clone(),
            },
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => AotExpr::Ternary {
                condition: Box::new(self.substitute_var_in_expr(condition, var, value)),
                then_expr: Box::new(self.substitute_var_in_expr(then_expr, var, value)),
                else_expr: Box::new(self.substitute_var_in_expr(else_expr, var, value)),
                result_ty: result_ty.clone(),
            },
            AotExpr::Box(inner) => {
                AotExpr::Box(Box::new(self.substitute_var_in_expr(inner, var, value)))
            }
            AotExpr::Unbox {
                value: inner,
                target_ty,
            } => AotExpr::Unbox {
                value: Box::new(self.substitute_var_in_expr(inner, var, value)),
                target_ty: target_ty.clone(),
            },
            AotExpr::Convert {
                value: inner,
                target_ty,
            } => AotExpr::Convert {
                value: Box::new(self.substitute_var_in_expr(inner, var, value)),
                target_ty: target_ty.clone(),
            },
            AotExpr::Lambda {
                params,
                body,
                captures,
                return_ty,
            } => {
                // Don't substitute if lambda shadows the variable
                if params.iter().any(|(name, _)| name == var) {
                    expr.clone()
                } else {
                    AotExpr::Lambda {
                        params: params.clone(),
                        body: Box::new(self.substitute_var_in_expr(body, var, value)),
                        captures: captures.clone(),
                        return_ty: return_ty.clone(),
                    }
                }
            }
            // Literals don't need substitution
            _ => expr.clone(),
        }
    }
}

impl Default for AotLoopOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Optimize an AoT program with loop optimizations
pub fn optimize_aot_program_with_loops(program: &mut AotProgram) -> usize {
    let mut optimizer = AotLoopOptimizer::new();
    optimizer.optimize_program(program)
}

/// Optimize an AoT program with loop optimizations using custom config
pub fn optimize_aot_program_with_loops_config(
    program: &mut AotProgram,
    config: LoopOptConfig,
) -> usize {
    let mut optimizer = AotLoopOptimizer::with_config(config);
    optimizer.optimize_program(program)
}
