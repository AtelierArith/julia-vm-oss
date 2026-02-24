//! Function Inlining optimization for AoT IR
//!
//! This module implements function inlining that replaces function calls
//! with the body of the called function.

use crate::aot::ir::{AotBuiltinOp, AotExpr, AotFunction, AotProgram, AotStmt};
use crate::aot::types::StaticType;
use std::collections::{HashMap, HashSet};

/// Inline candidate information
#[derive(Debug, Clone)]
pub struct InlineCandidate {
    /// Function name
    pub name: String,
    /// Function size (statement count)
    pub size: usize,
    /// Whether the function is recursive
    pub is_recursive: bool,
    /// Whether the function is a pure function (no side effects)
    pub is_pure: bool,
    /// Score for inlining priority (higher = more likely to inline)
    pub score: i32,
}

impl InlineCandidate {
    /// Check if this candidate should be inlined
    pub fn should_inline(&self, max_size: usize) -> bool {
        !self.is_recursive && self.size <= max_size && self.score > 0
    }
}

/// AoT program inliner
#[derive(Debug)]
pub struct AotInliner {
    /// Maximum function size to inline
    max_inline_size: usize,
    /// Variable counter for generating unique names
    var_counter: usize,
    /// Functions that have been analyzed
    inline_candidates: HashMap<String, InlineCandidate>,
}

impl AotInliner {
    /// Create a new inliner
    pub fn new(max_inline_size: usize) -> Self {
        Self {
            max_inline_size,
            var_counter: 0,
            inline_candidates: HashMap::new(),
        }
    }

    /// Get the maximum inline size
    #[cfg(test)]
    pub fn max_inline_size(&self) -> usize {
        self.max_inline_size
    }

    /// Analyze a program to find inline candidates
    pub fn analyze_program(&mut self, program: &AotProgram) {
        // First pass: collect function info
        for func in &program.functions {
            let size = Self::count_statements(&func.body);
            let is_recursive = Self::is_recursive(func, program);
            let is_pure = Self::is_pure_function(func);

            // Calculate inlining score
            let mut score: i32 = 10;
            if size <= 3 {
                score += 10; // Small functions get bonus
            } else if size <= 5 {
                score += 5;
            }
            if is_pure {
                score += 5; // Pure functions are easier to inline
            }
            if is_recursive {
                score = i32::MIN; // Never inline recursive functions
            }

            self.inline_candidates.insert(
                func.name.clone(),
                InlineCandidate {
                    name: func.name.clone(),
                    size,
                    is_recursive,
                    is_pure,
                    score,
                },
            );
        }
    }

    /// Run inlining optimization on a program
    pub fn optimize_program(&mut self, program: &mut AotProgram) -> usize {
        // Analyze first
        self.analyze_program(program);

        let mut total_inlined = 0;

        // Build a map of function bodies for quick lookup
        let function_bodies: HashMap<String, AotFunction> = program
            .functions
            .iter()
            .map(|f| (f.name.clone(), f.clone()))
            .collect();

        // Inline in functions
        for func in &mut program.functions {
            let inlined = self.inline_calls_in_stmts(&mut func.body, &function_bodies, 0);
            total_inlined += inlined;
        }

        // Inline in main block
        let inlined = self.inline_calls_in_stmts(&mut program.main, &function_bodies, 0);
        total_inlined += inlined;

        total_inlined
    }

    /// Count statements in a function body
    pub fn count_statements(stmts: &[AotStmt]) -> usize {
        let mut count = 0;
        for stmt in stmts {
            count += 1;
            count += match stmt {
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    Self::count_statements(then_branch)
                        + else_branch
                            .as_ref()
                            .map_or(0, |e| Self::count_statements(e))
                }
                AotStmt::While { body, .. } => Self::count_statements(body),
                AotStmt::ForRange { body, .. } => Self::count_statements(body),
                AotStmt::ForEach { body, .. } => Self::count_statements(body),
                _ => 0,
            };
        }
        count
    }

    /// Check if a function is recursive
    fn is_recursive(func: &AotFunction, program: &AotProgram) -> bool {
        let mut visited = HashSet::new();
        Self::calls_function(&func.name, &func.body, program, &mut visited)
    }

    /// Check if statements call a specific function (possibly indirectly)
    fn calls_function(
        target: &str,
        stmts: &[AotStmt],
        program: &AotProgram,
        visited: &mut HashSet<String>,
    ) -> bool {
        for stmt in stmts {
            if Self::stmt_calls_function(target, stmt, program, visited) {
                return true;
            }
        }
        false
    }

    /// Check if a statement calls a specific function
    fn stmt_calls_function(
        target: &str,
        stmt: &AotStmt,
        program: &AotProgram,
        visited: &mut HashSet<String>,
    ) -> bool {
        match stmt {
            AotStmt::Let { value, .. } | AotStmt::Assign { value, .. } | AotStmt::Expr(value) => {
                Self::expr_calls_function(target, value, program, visited)
            }
            AotStmt::CompoundAssign { value, .. } => {
                Self::expr_calls_function(target, value, program, visited)
            }
            AotStmt::Return(Some(expr)) => {
                Self::expr_calls_function(target, expr, program, visited)
            }
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::expr_calls_function(target, condition, program, visited)
                    || Self::calls_function(target, then_branch, program, visited)
                    || else_branch
                        .as_ref()
                        .map_or(false, |e| Self::calls_function(target, e, program, visited))
            }
            AotStmt::While {
                condition, body, ..
            } => {
                Self::expr_calls_function(target, condition, program, visited)
                    || Self::calls_function(target, body, program, visited)
            }
            AotStmt::ForRange {
                start,
                stop,
                step,
                body,
                ..
            } => {
                Self::expr_calls_function(target, start, program, visited)
                    || Self::expr_calls_function(target, stop, program, visited)
                    || step.as_ref().map_or(false, |s| {
                        Self::expr_calls_function(target, s, program, visited)
                    })
                    || Self::calls_function(target, body, program, visited)
            }
            AotStmt::ForEach { iter, body, .. } => {
                Self::expr_calls_function(target, iter, program, visited)
                    || Self::calls_function(target, body, program, visited)
            }
            _ => false,
        }
    }

    /// Check if an expression calls a specific function
    fn expr_calls_function(
        target: &str,
        expr: &AotExpr,
        program: &AotProgram,
        visited: &mut HashSet<String>,
    ) -> bool {
        match expr {
            AotExpr::CallStatic { function, args, .. }
            | AotExpr::CallDynamic { function, args, .. } => {
                if function == target {
                    return true;
                }
                // Check for indirect recursion
                if !visited.contains(function) {
                    visited.insert(function.clone());
                    if let Some(callee) = program.functions.iter().find(|f| &f.name == function) {
                        if Self::calls_function(target, &callee.body, program, visited) {
                            return true;
                        }
                    }
                }
                args.iter()
                    .any(|a| Self::expr_calls_function(target, a, program, visited))
            }
            AotExpr::CallBuiltin { args, .. } => args
                .iter()
                .any(|a| Self::expr_calls_function(target, a, program, visited)),
            AotExpr::BinOpStatic { left, right, .. }
            | AotExpr::BinOpDynamic { left, right, .. } => {
                Self::expr_calls_function(target, left, program, visited)
                    || Self::expr_calls_function(target, right, program, visited)
            }
            AotExpr::UnaryOp { operand, .. } => {
                Self::expr_calls_function(target, operand, program, visited)
            }
            AotExpr::Index { array, indices, .. } => {
                Self::expr_calls_function(target, array, program, visited)
                    || indices
                        .iter()
                        .any(|i| Self::expr_calls_function(target, i, program, visited))
            }
            AotExpr::FieldAccess { object, .. } => {
                Self::expr_calls_function(target, object, program, visited)
            }
            AotExpr::ArrayLit { elements, .. }
            | AotExpr::TupleLit { elements }
            | AotExpr::StructNew {
                fields: elements, ..
            } => elements
                .iter()
                .any(|e| Self::expr_calls_function(target, e, program, visited)),
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_calls_function(target, condition, program, visited)
                    || Self::expr_calls_function(target, then_expr, program, visited)
                    || Self::expr_calls_function(target, else_expr, program, visited)
            }
            AotExpr::Box(inner)
            | AotExpr::Unbox { value: inner, .. }
            | AotExpr::Convert { value: inner, .. } => {
                Self::expr_calls_function(target, inner, program, visited)
            }
            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::expr_calls_function(target, start, program, visited)
                    || Self::expr_calls_function(target, stop, program, visited)
                    || step.as_ref().map_or(false, |s| {
                        Self::expr_calls_function(target, s, program, visited)
                    })
            }
            AotExpr::Lambda { body, .. } => {
                Self::expr_calls_function(target, body, program, visited)
            }
            _ => false,
        }
    }

    /// Check if a function is pure (no side effects)
    fn is_pure_function(func: &AotFunction) -> bool {
        Self::stmts_are_pure(&func.body)
    }

    /// Check if statements are pure
    fn stmts_are_pure(stmts: &[AotStmt]) -> bool {
        stmts.iter().all(Self::stmt_is_pure)
    }

    /// Check if a statement is pure
    fn stmt_is_pure(stmt: &AotStmt) -> bool {
        match stmt {
            AotStmt::Let { value, .. } => Self::expr_is_pure(value),
            AotStmt::Assign { value, target, .. } => {
                // Assignment to array index is impure
                if matches!(target, AotExpr::Index { .. }) {
                    return false;
                }
                Self::expr_is_pure(value)
            }
            AotStmt::CompoundAssign { .. } => true, // Local mutation is ok
            AotStmt::Expr(expr) => Self::expr_is_pure(expr),
            AotStmt::Return(Some(expr)) => Self::expr_is_pure(expr),
            AotStmt::Return(None) => true,
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::expr_is_pure(condition)
                    && Self::stmts_are_pure(then_branch)
                    && else_branch
                        .as_ref()
                        .map_or(true, |e| Self::stmts_are_pure(e))
            }
            AotStmt::While {
                condition, body, ..
            } => Self::expr_is_pure(condition) && Self::stmts_are_pure(body),
            AotStmt::ForRange { body, .. } | AotStmt::ForEach { body, .. } => {
                Self::stmts_are_pure(body)
            }
            AotStmt::Break | AotStmt::Continue => true,
        }
    }

    /// Check if an expression is pure
    pub fn expr_is_pure(expr: &AotExpr) -> bool {
        match expr {
            // Literals are pure
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing => true,

            // Variables are pure
            AotExpr::Var { .. } => true,

            // Operators are pure if operands are
            AotExpr::BinOpStatic { left, right, .. }
            | AotExpr::BinOpDynamic { left, right, .. } => {
                Self::expr_is_pure(left) && Self::expr_is_pure(right)
            }
            AotExpr::UnaryOp { operand, .. } => Self::expr_is_pure(operand),

            // Function calls - assume impure for safety (could be refined)
            AotExpr::CallStatic { .. } | AotExpr::CallDynamic { .. } => false,

            // Builtins - some are pure
            AotExpr::CallBuiltin { builtin, args, .. } => {
                let builtin_is_pure = matches!(
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
                        | AotBuiltinOp::Sum
                );
                builtin_is_pure && args.iter().all(Self::expr_is_pure)
            }

            // Collections
            AotExpr::ArrayLit { elements, .. }
            | AotExpr::TupleLit { elements }
            | AotExpr::StructNew {
                fields: elements, ..
            } => elements.iter().all(Self::expr_is_pure),

            AotExpr::Index { array, indices, .. } => {
                Self::expr_is_pure(array) && indices.iter().all(Self::expr_is_pure)
            }

            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::expr_is_pure(start)
                    && Self::expr_is_pure(stop)
                    && step.as_ref().map_or(true, |s| Self::expr_is_pure(s))
            }

            AotExpr::FieldAccess { object, .. } => Self::expr_is_pure(object),

            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_is_pure(condition)
                    && Self::expr_is_pure(then_expr)
                    && Self::expr_is_pure(else_expr)
            }

            AotExpr::Box(inner)
            | AotExpr::Unbox { value: inner, .. }
            | AotExpr::Convert { value: inner, .. } => Self::expr_is_pure(inner),

            AotExpr::Lambda { .. } => true, // Lambda definition is pure
        }
    }

    /// Check if a type conversion is needed between two types
    /// Returns true for numeric promotions (e.g., i64 -> f64)
    fn needs_type_conversion(from: &StaticType, to: &StaticType) -> bool {
        use StaticType::*;
        match (from, to) {
            // Integer to float conversions
            (I64 | I32 | I16 | I8 | U64 | U32 | U16 | U8, F64 | F32) => true,
            // Smaller to larger integer
            (I8, I16 | I32 | I64) => true,
            (I16, I32 | I64) => true,
            (I32, I64) => true,
            (U8, U16 | U32 | U64 | I16 | I32 | I64) => true,
            (U16, U32 | U64 | I32 | I64) => true,
            (U32, U64 | I64) => true,
            // Float conversions
            (F32, F64) => true,
            // Bool to numeric
            (Bool, I64 | I32 | I16 | I8 | F64 | F32) => true,
            // No conversion needed or not supported
            _ => false,
        }
    }

    /// Inline function calls in statements
    fn inline_calls_in_stmts(
        &mut self,
        stmts: &mut Vec<AotStmt>,
        functions: &HashMap<String, AotFunction>,
        depth: usize,
    ) -> usize {
        if depth > 3 {
            return 0; // Prevent infinite inlining
        }

        let mut total_inlined = 0;
        let mut i = 0;

        while i < stmts.len() {
            // Try to inline calls in this statement
            let (new_stmts, inlined) = self.try_inline_stmt(&stmts[i], functions, depth);

            if inlined > 0 {
                // Replace the statement with inlined version
                stmts.splice(i..=i, new_stmts);
                total_inlined += inlined;
            } else {
                // Process nested blocks
                match &mut stmts[i] {
                    AotStmt::If {
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        total_inlined += self.inline_calls_in_stmts(then_branch, functions, depth);
                        if let Some(else_b) = else_branch {
                            total_inlined += self.inline_calls_in_stmts(else_b, functions, depth);
                        }
                    }
                    AotStmt::While { body, .. }
                    | AotStmt::ForRange { body, .. }
                    | AotStmt::ForEach { body, .. } => {
                        total_inlined += self.inline_calls_in_stmts(body, functions, depth);
                    }
                    _ => {}
                }
                i += 1;
            }
        }

        total_inlined
    }

    /// Try to inline a call in a statement
    fn try_inline_stmt(
        &mut self,
        stmt: &AotStmt,
        functions: &HashMap<String, AotFunction>,
        depth: usize,
    ) -> (Vec<AotStmt>, usize) {
        match stmt {
            AotStmt::Let {
                name,
                ty,
                value,
                is_mutable,
            } => {
                if let Some((inlined_stmts, result_expr, count)) =
                    self.try_inline_expr(value, functions, depth)
                {
                    let mut stmts = inlined_stmts;
                    stmts.push(AotStmt::Let {
                        name: name.clone(),
                        ty: ty.clone(),
                        value: result_expr,
                        is_mutable: *is_mutable,
                    });
                    return (stmts, count);
                }
            }
            AotStmt::Assign { target, value } => {
                if let Some((inlined_stmts, result_expr, count)) =
                    self.try_inline_expr(value, functions, depth)
                {
                    let mut stmts = inlined_stmts;
                    stmts.push(AotStmt::Assign {
                        target: target.clone(),
                        value: result_expr,
                    });
                    return (stmts, count);
                }
            }
            AotStmt::Expr(expr) => {
                if let Some((inlined_stmts, result_expr, count)) =
                    self.try_inline_expr(expr, functions, depth)
                {
                    let mut stmts = inlined_stmts;
                    stmts.push(AotStmt::Expr(result_expr));
                    return (stmts, count);
                }
            }
            AotStmt::Return(Some(expr)) => {
                if let Some((inlined_stmts, result_expr, count)) =
                    self.try_inline_expr(expr, functions, depth)
                {
                    let mut stmts = inlined_stmts;
                    stmts.push(AotStmt::Return(Some(result_expr)));
                    return (stmts, count);
                }
            }
            _ => {}
        }
        (vec![stmt.clone()], 0)
    }

    /// Try to inline a function call in an expression
    fn try_inline_expr(
        &mut self,
        expr: &AotExpr,
        functions: &HashMap<String, AotFunction>,
        depth: usize,
    ) -> Option<(Vec<AotStmt>, AotExpr, usize)> {
        match expr {
            AotExpr::CallStatic {
                function,
                args,
                return_ty,
            } => {
                // Check if this function should be inlined
                if let Some(candidate) = self.inline_candidates.get(function) {
                    if candidate.should_inline(self.max_inline_size) {
                        if let Some(func) = functions.get(function) {
                            return self.inline_function_call(func, args, return_ty, depth);
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Inline a function call
    fn inline_function_call(
        &mut self,
        func: &AotFunction,
        args: &[AotExpr],
        _return_ty: &StaticType,
        depth: usize,
    ) -> Option<(Vec<AotStmt>, AotExpr, usize)> {
        // Generate unique prefix for this inline
        let prefix = format!("_inline{}_{}_", depth, self.var_counter);
        self.var_counter += 1;

        let mut stmts = Vec::new();

        // Create bindings for parameters
        for ((param_name, param_ty), arg) in func.params.iter().zip(args.iter()) {
            let new_name = format!("{}{}", prefix, param_name);
            // Check if we need to convert the argument type to match the parameter type
            let arg_ty = arg.get_type();
            let converted_arg =
                if arg_ty != *param_ty && Self::needs_type_conversion(&arg_ty, param_ty) {
                    // Wrap in Convert expression to handle type promotion
                    AotExpr::Convert {
                        value: Box::new(arg.clone()),
                        target_ty: param_ty.clone(),
                    }
                } else {
                    arg.clone()
                };
            stmts.push(AotStmt::Let {
                name: new_name,
                ty: param_ty.clone(),
                value: converted_arg,
                is_mutable: false,
            });
        }

        // Build variable rename map
        let mut rename_map: HashMap<String, String> = func
            .params
            .iter()
            .map(|(name, _)| (name.clone(), format!("{}{}", prefix, name)))
            .collect();

        // Process function body
        let mut result_expr = AotExpr::LitNothing;

        for (i, stmt) in func.body.iter().enumerate() {
            let is_last = i == func.body.len() - 1;
            let renamed_stmt = self.rename_variables_in_stmt(stmt, &prefix, &mut rename_map);

            match renamed_stmt {
                AotStmt::Return(Some(expr)) => {
                    // The return value becomes the result
                    result_expr = expr;
                    break;
                }
                AotStmt::Return(None) => {
                    result_expr = AotExpr::LitNothing;
                    break;
                }
                _ => {
                    // For the last statement, if it's an expression, use it as result
                    if is_last {
                        if let AotStmt::Expr(expr) = &renamed_stmt {
                            result_expr = expr.clone();
                        } else {
                            stmts.push(renamed_stmt);
                        }
                    } else {
                        stmts.push(renamed_stmt);
                    }
                }
            }
        }

        Some((stmts, result_expr, 1))
    }

    /// Rename variables in a statement
    fn rename_variables_in_stmt(
        &self,
        stmt: &AotStmt,
        prefix: &str,
        rename_map: &mut HashMap<String, String>,
    ) -> AotStmt {
        match stmt {
            AotStmt::Let {
                name,
                ty,
                value,
                is_mutable,
            } => {
                let new_name = format!("{}{}", prefix, name);
                rename_map.insert(name.clone(), new_name.clone());
                AotStmt::Let {
                    name: new_name,
                    ty: ty.clone(),
                    value: self.rename_variables_in_expr(value, rename_map),
                    is_mutable: *is_mutable,
                }
            }
            AotStmt::Assign { target, value } => AotStmt::Assign {
                target: self.rename_variables_in_expr(target, rename_map),
                value: self.rename_variables_in_expr(value, rename_map),
            },
            AotStmt::CompoundAssign { target, op, value } => AotStmt::CompoundAssign {
                target: self.rename_variables_in_expr(target, rename_map),
                op: *op,
                value: self.rename_variables_in_expr(value, rename_map),
            },
            AotStmt::Expr(expr) => AotStmt::Expr(self.rename_variables_in_expr(expr, rename_map)),
            AotStmt::Return(opt_expr) => AotStmt::Return(
                opt_expr
                    .as_ref()
                    .map(|e| self.rename_variables_in_expr(e, rename_map)),
            ),
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => AotStmt::If {
                condition: self.rename_variables_in_expr(condition, rename_map),
                then_branch: then_branch
                    .iter()
                    .map(|s| self.rename_variables_in_stmt(s, prefix, rename_map))
                    .collect(),
                else_branch: else_branch.as_ref().map(|stmts| {
                    stmts
                        .iter()
                        .map(|s| self.rename_variables_in_stmt(s, prefix, rename_map))
                        .collect()
                }),
            },
            AotStmt::While { condition, body } => AotStmt::While {
                condition: self.rename_variables_in_expr(condition, rename_map),
                body: body
                    .iter()
                    .map(|s| self.rename_variables_in_stmt(s, prefix, rename_map))
                    .collect(),
            },
            AotStmt::ForRange {
                var,
                start,
                stop,
                step,
                body,
            } => {
                let new_var = format!("{}{}", prefix, var);
                rename_map.insert(var.clone(), new_var.clone());
                AotStmt::ForRange {
                    var: new_var,
                    start: self.rename_variables_in_expr(start, rename_map),
                    stop: self.rename_variables_in_expr(stop, rename_map),
                    step: step
                        .as_ref()
                        .map(|s| self.rename_variables_in_expr(s, rename_map)),
                    body: body
                        .iter()
                        .map(|s| self.rename_variables_in_stmt(s, prefix, rename_map))
                        .collect(),
                }
            }
            AotStmt::ForEach { var, iter, body } => {
                let new_var = format!("{}{}", prefix, var);
                rename_map.insert(var.clone(), new_var.clone());
                AotStmt::ForEach {
                    var: new_var,
                    iter: self.rename_variables_in_expr(iter, rename_map),
                    body: body
                        .iter()
                        .map(|s| self.rename_variables_in_stmt(s, prefix, rename_map))
                        .collect(),
                }
            }
            AotStmt::Break => AotStmt::Break,
            AotStmt::Continue => AotStmt::Continue,
        }
    }

    /// Rename variables in an expression
    fn rename_variables_in_expr(
        &self,
        expr: &AotExpr,
        rename_map: &HashMap<String, String>,
    ) -> AotExpr {
        match expr {
            AotExpr::Var { name, ty } => {
                if let Some(new_name) = rename_map.get(name) {
                    AotExpr::Var {
                        name: new_name.clone(),
                        ty: ty.clone(),
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
                left: Box::new(self.rename_variables_in_expr(left, rename_map)),
                right: Box::new(self.rename_variables_in_expr(right, rename_map)),
                result_ty: result_ty.clone(),
            },
            AotExpr::BinOpDynamic { op, left, right } => AotExpr::BinOpDynamic {
                op: *op,
                left: Box::new(self.rename_variables_in_expr(left, rename_map)),
                right: Box::new(self.rename_variables_in_expr(right, rename_map)),
            },
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => AotExpr::UnaryOp {
                op: *op,
                operand: Box::new(self.rename_variables_in_expr(operand, rename_map)),
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
                    .map(|a| self.rename_variables_in_expr(a, rename_map))
                    .collect(),
                return_ty: return_ty.clone(),
            },
            AotExpr::CallDynamic { function, args } => AotExpr::CallDynamic {
                function: function.clone(),
                args: args
                    .iter()
                    .map(|a| self.rename_variables_in_expr(a, rename_map))
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
                    .map(|a| self.rename_variables_in_expr(a, rename_map))
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
                    .map(|e| self.rename_variables_in_expr(e, rename_map))
                    .collect(),
                elem_ty: elem_ty.clone(),
                shape: shape.clone(),
            },
            AotExpr::TupleLit { elements } => AotExpr::TupleLit {
                elements: elements
                    .iter()
                    .map(|e| self.rename_variables_in_expr(e, rename_map))
                    .collect(),
            },
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple,
            } => AotExpr::Index {
                array: Box::new(self.rename_variables_in_expr(array, rename_map)),
                indices: indices
                    .iter()
                    .map(|i| self.rename_variables_in_expr(i, rename_map))
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
                start: Box::new(self.rename_variables_in_expr(start, rename_map)),
                stop: Box::new(self.rename_variables_in_expr(stop, rename_map)),
                step: step
                    .as_ref()
                    .map(|s| Box::new(self.rename_variables_in_expr(s, rename_map))),
                elem_ty: elem_ty.clone(),
            },
            AotExpr::StructNew { name, fields } => AotExpr::StructNew {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|f| self.rename_variables_in_expr(f, rename_map))
                    .collect(),
            },
            AotExpr::FieldAccess {
                object,
                field,
                field_ty,
            } => AotExpr::FieldAccess {
                object: Box::new(self.rename_variables_in_expr(object, rename_map)),
                field: field.clone(),
                field_ty: field_ty.clone(),
            },
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => AotExpr::Ternary {
                condition: Box::new(self.rename_variables_in_expr(condition, rename_map)),
                then_expr: Box::new(self.rename_variables_in_expr(then_expr, rename_map)),
                else_expr: Box::new(self.rename_variables_in_expr(else_expr, rename_map)),
                result_ty: result_ty.clone(),
            },
            AotExpr::Box(inner) => {
                AotExpr::Box(Box::new(self.rename_variables_in_expr(inner, rename_map)))
            }
            AotExpr::Unbox { value, target_ty } => AotExpr::Unbox {
                value: Box::new(self.rename_variables_in_expr(value, rename_map)),
                target_ty: target_ty.clone(),
            },
            AotExpr::Convert { value, target_ty } => AotExpr::Convert {
                value: Box::new(self.rename_variables_in_expr(value, rename_map)),
                target_ty: target_ty.clone(),
            },
            AotExpr::Lambda {
                params,
                body,
                captures,
                return_ty,
            } => {
                // Lambda parameters shadow outer variables, so create a new scope
                let mut inner_map = rename_map.clone();
                for (param_name, _) in params {
                    inner_map.remove(param_name);
                }
                AotExpr::Lambda {
                    params: params.clone(),
                    body: Box::new(self.rename_variables_in_expr(body, &inner_map)),
                    captures: captures.clone(),
                    return_ty: return_ty.clone(),
                }
            }
            // Literals don't need renaming
            _ => expr.clone(),
        }
    }

    /// Get inline statistics
    pub fn get_candidates(&self) -> &HashMap<String, InlineCandidate> {
        &self.inline_candidates
    }
}

/// Optimize an AoT program with inlining
pub fn optimize_aot_program_with_inlining(
    program: &mut AotProgram,
    max_inline_size: usize,
) -> usize {
    let mut inliner = AotInliner::new(max_inline_size);
    inliner.optimize_program(program)
}
