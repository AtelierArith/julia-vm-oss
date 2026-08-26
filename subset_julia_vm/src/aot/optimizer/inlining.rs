//! Function Inlining optimization for AoT IR
//!
//! This module implements function inlining that replaces function calls
//! with the body of the called function.

use crate::aot::abi::AotAbiValue;
use crate::aot::ir::{AotBuiltinOp, AotExpr, AotFunction, AotInlinePolicy, AotProgram, AotStmt};
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
    /// Metadata-derived inline policy.
    pub inline_policy: AotInlinePolicy,
    /// Whether the function returns through the runtime `Value` boundary.
    pub return_needs_value: bool,
}

impl InlineCandidate {
    /// Check if this candidate should be inlined
    pub fn should_inline(&self, max_size: usize) -> bool {
        // Runtime-boxed returns need caller-context return rewriting before they
        // can be inlined safely. (Issue #7012)
        if self.return_needs_value {
            return false;
        }

        match self.inline_policy {
            AotInlinePolicy::Never => false,
            AotInlinePolicy::Always => !self.is_recursive,
            AotInlinePolicy::Auto => !self.is_recursive && self.size <= max_size && self.score > 0,
        }
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
    specialized_boxed_returns: HashSet<String>,
}

impl AotInliner {
    /// Create a new inliner
    pub fn new(max_inline_size: usize) -> Self {
        Self {
            max_inline_size,
            var_counter: 0,
            inline_candidates: HashMap::new(),
            specialized_boxed_returns: HashSet::new(),
        }
    }

    /// Get the maximum inline size
    #[cfg(test)]
    pub fn max_inline_size(&self) -> usize {
        self.max_inline_size
    }

    /// Analyze a program to find inline candidates
    pub fn analyze_program(&mut self, program: &AotProgram) {
        let function_info: Vec<_> = program
            .functions
            .iter()
            .map(|func| {
                (
                    func,
                    Self::count_statements(&func.body),
                    Self::is_recursive(func, program),
                )
            })
            .collect();

        let mut pure_functions = HashSet::new();
        loop {
            let mut changed = false;
            for (func, _, is_recursive) in &function_info {
                if *is_recursive || pure_functions.contains(&func.name) {
                    continue;
                }
                if Self::is_pure_function(func, &pure_functions) {
                    changed |= pure_functions.insert(func.name.clone());
                }
            }
            if !changed {
                break;
            }
        }

        for (func, size, is_recursive) in function_info {
            let is_pure = pure_functions.contains(&func.name);

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
                    inline_policy: func.inline_policy,
                    return_needs_value: AotAbiValue::from_static_type(&func.return_type)
                        .needs_runtime_value(),
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
            total_inlined += self.inline_local_lambdas(&mut func.body);
            let mut caller_functions = function_bodies.clone();
            caller_functions.remove(&func.name);
            let inlined = self.inline_calls_in_stmts(&mut func.body, &caller_functions, 0);
            total_inlined += inlined;
            total_inlined += self.inline_local_lambdas(&mut func.body);
        }

        // Inline in main block
        total_inlined += self.inline_local_lambdas(&mut program.main);
        let inlined = self.inline_calls_in_stmts(&mut program.main, &function_bodies, 0);
        total_inlined += inlined;
        total_inlined += self.inline_local_lambdas(&mut program.main);

        let specialized = self.specialized_boxed_returns.clone();
        let optimized_functions = program.functions.clone();
        let remaining_specialized_refs = specialized
            .iter()
            .filter(|target| {
                program.main.iter().any(|stmt| {
                    Self::stmt_calls_function(target, stmt, &AotProgram::new(), &mut HashSet::new())
                }) || optimized_functions.iter().any(|caller| {
                    &caller.name != *target
                        && caller.body.iter().any(|stmt| {
                            Self::stmt_calls_function(
                                target,
                                stmt,
                                &AotProgram::new(),
                                &mut HashSet::new(),
                            )
                        })
                })
            })
            .cloned()
            .collect::<HashSet<_>>();
        program.functions.retain(|func| {
            !specialized.contains(&func.name) || remaining_specialized_refs.contains(&func.name)
        });

        total_inlined
    }

    fn inline_local_lambdas(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        let mut total = 0;
        for stmt in stmts.iter_mut() {
            total += match stmt {
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    let mut count = self.inline_local_lambdas(then_branch);
                    if let Some(else_branch) = else_branch {
                        count += self.inline_local_lambdas(else_branch);
                    }
                    count
                }
                AotStmt::While { body, .. }
                | AotStmt::ForRange { body, .. }
                | AotStmt::ForEach { body, .. } => self.inline_local_lambdas(body),
                _ => 0,
            };
        }
        let mut index = 0;
        while index < stmts.len() {
            let Some((name, params, body, return_ty)) = (match &stmts[index] {
                AotStmt::Let {
                    name,
                    value:
                        AotExpr::Lambda {
                            params,
                            body,
                            return_ty,
                            ..
                        },
                    ..
                } => Some((
                    name.clone(),
                    params.clone(),
                    body.clone(),
                    return_ty.clone(),
                )),
                _ => None,
            }) else {
                index += 1;
                continue;
            };
            let mut consumed = 0;
            let lambda = AotFunction::new(name.clone(), params, return_ty);
            let lambda = AotFunction { body, ..lambda };
            let mut cursor = index + 1;
            while cursor < stmts.len() {
                if let Some((prefix, result, count)) =
                    self.try_inline_local_lambda_stmt(&stmts[cursor], &lambda)
                {
                    let mut replacement = prefix;
                    replacement.push(match &stmts[cursor] {
                        AotStmt::Let {
                            name, is_mutable, ..
                        } => AotStmt::Let {
                            name: name.clone(),
                            ty: result.get_type(),
                            value: result,
                            is_mutable: *is_mutable,
                        },
                        AotStmt::Return(Some(_)) => AotStmt::Return(Some(result)),
                        _ => AotStmt::Expr(result),
                    });
                    let replacement_len = replacement.len();
                    stmts.splice(cursor..=cursor, replacement);
                    cursor += replacement_len;
                    consumed += count;
                } else {
                    cursor += 1;
                }
            }
            if consumed > 0
                && !stmts[index + 1..].iter().any(|stmt| {
                    Self::stmt_calls_function(&name, stmt, &AotProgram::new(), &mut HashSet::new())
                })
            {
                stmts.remove(index);
                total += consumed;
            } else {
                index += 1;
            }
        }
        total
    }

    fn try_inline_local_lambda_stmt(
        &mut self,
        stmt: &AotStmt,
        lambda: &AotFunction,
    ) -> Option<(Vec<AotStmt>, AotExpr, usize)> {
        let expr = match stmt {
            AotStmt::Let { value, .. }
            | AotStmt::Expr(value)
            | AotStmt::ValueCarrier(value)
            | AotStmt::Return(Some(value)) => value,
            _ => return None,
        };
        let (call, wrapper) = match expr {
            call @ AotExpr::CallStatic { .. } => (call, None),
            AotExpr::Convert { value, target_ty } => (value.as_ref(), Some(target_ty)),
            _ => return None,
        };
        let AotExpr::CallStatic { function, args, .. } = call else {
            return None;
        };
        if function != &lambda.name || args.len() != lambda.params.len() {
            return None;
        }
        self.inline_function_call(lambda, args, &lambda.return_type, 0)
            .map(|(stmts, result, count)| {
                let result = wrapper.map_or(result.clone(), |target_ty| AotExpr::Convert {
                    value: Box::new(result),
                    target_ty: target_ty.clone(),
                });
                (stmts, result, count)
            })
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
            AotStmt::Let { value, .. }
            | AotStmt::Assign { value, .. }
            | AotStmt::Expr(value)
            | AotStmt::ValueCarrier(value) => {
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
                        .is_some_and(|e| Self::calls_function(target, e, program, visited))
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
                    || step
                        .as_ref()
                        .is_some_and(|s| Self::expr_calls_function(target, s, program, visited))
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
            AotExpr::SetFromIter { iter, .. } => {
                Self::expr_calls_function(target, iter, program, visited)
            }
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
                    || step
                        .as_ref()
                        .is_some_and(|s| Self::expr_calls_function(target, s, program, visited))
            }
            AotExpr::Lambda { body, .. } => Self::calls_function(target, body, program, visited),
            AotExpr::Generator {
                body, iter, filter, ..
            } => {
                Self::expr_calls_function(target, iter, program, visited)
                    || filter.as_ref().is_some_and(|filter| {
                        Self::expr_calls_function(target, filter, program, visited)
                    })
                    || Self::expr_calls_function(target, body, program, visited)
            }
            _ => false,
        }
    }

    /// Check if a function is pure (no side effects)
    fn is_pure_function(func: &AotFunction, pure_functions: &HashSet<String>) -> bool {
        Self::stmts_are_pure_with_known(&func.body, pure_functions)
    }

    fn stmts_are_pure_with_known(stmts: &[AotStmt], pure_functions: &HashSet<String>) -> bool {
        stmts
            .iter()
            .all(|stmt| Self::stmt_is_pure_with_known(stmt, pure_functions))
    }

    fn stmt_is_pure_with_known(stmt: &AotStmt, pure_functions: &HashSet<String>) -> bool {
        match stmt {
            AotStmt::Let { value, .. } => Self::expr_is_pure_with_known(value, pure_functions),
            AotStmt::Assign { value, target, .. } => {
                // Assignment to array index is impure
                if matches!(target, AotExpr::Index { .. }) {
                    return false;
                }
                Self::expr_is_pure_with_known(value, pure_functions)
            }
            AotStmt::CompoundAssign { .. } => true, // Local mutation is ok
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => {
                Self::expr_is_pure_with_known(expr, pure_functions)
            }
            AotStmt::Return(Some(expr)) => Self::expr_is_pure_with_known(expr, pure_functions),
            AotStmt::Return(None) => true,
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::expr_is_pure_with_known(condition, pure_functions)
                    && Self::stmts_are_pure_with_known(then_branch, pure_functions)
                    && else_branch
                        .as_ref()
                        .is_none_or(|e| Self::stmts_are_pure_with_known(e, pure_functions))
            }
            AotStmt::While {
                condition, body, ..
            } => {
                Self::expr_is_pure_with_known(condition, pure_functions)
                    && Self::stmts_are_pure_with_known(body, pure_functions)
            }
            AotStmt::ForRange { body, .. } | AotStmt::ForEach { body, .. } => {
                Self::stmts_are_pure_with_known(body, pure_functions)
            }
            AotStmt::Break | AotStmt::Continue => true,
        }
    }

    /// Check if an expression is pure
    pub fn expr_is_pure(expr: &AotExpr) -> bool {
        Self::expr_is_pure_with_known(expr, &HashSet::new())
    }

    fn expr_is_pure_with_known(expr: &AotExpr, pure_functions: &HashSet<String>) -> bool {
        match expr {
            // Literals are pure
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing
            | AotExpr::LitMissing => true,

            // Variables are pure
            AotExpr::Var { .. } => true,

            // Operators are pure if operands are
            AotExpr::BinOpStatic { left, right, .. }
            | AotExpr::BinOpDynamic { left, right, .. } => {
                Self::expr_is_pure_with_known(left, pure_functions)
                    && Self::expr_is_pure_with_known(right, pure_functions)
            }
            AotExpr::UnaryOp { operand, .. } => {
                Self::expr_is_pure_with_known(operand, pure_functions)
            }

            AotExpr::CallStatic { function, args, .. } => {
                pure_functions.contains(function)
                    && args
                        .iter()
                        .all(|arg| Self::expr_is_pure_with_known(arg, pure_functions))
            }
            AotExpr::CallDynamic { .. } => false,

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
                        | AotBuiltinOp::In
                        | AotBuiltinOp::Sum
                );
                builtin_is_pure
                    && args
                        .iter()
                        .all(|arg| Self::expr_is_pure_with_known(arg, pure_functions))
            }

            // Collections
            AotExpr::ArrayLit { elements, .. }
            | AotExpr::TupleLit { elements }
            | AotExpr::StructNew {
                fields: elements, ..
            } => elements
                .iter()
                .all(|element| Self::expr_is_pure_with_known(element, pure_functions)),
            AotExpr::SetFromIter { iter, .. } => {
                Self::expr_is_pure_with_known(iter, pure_functions)
            }
            AotExpr::NamedTupleLit { fields } => fields
                .iter()
                .all(|(_, field)| Self::expr_is_pure_with_known(field, pure_functions)),
            AotExpr::Comprehension {
                body, iter, filter, ..
            }
            | AotExpr::Generator {
                body, iter, filter, ..
            } => {
                Self::expr_is_pure_with_known(iter, pure_functions)
                    && filter
                        .as_ref()
                        .is_none_or(|filter| Self::expr_is_pure_with_known(filter, pure_functions))
                    && Self::expr_is_pure_with_known(body, pure_functions)
            }
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                iterations
                    .iter()
                    .all(|(_, iter)| Self::expr_is_pure_with_known(iter, pure_functions))
                    && filter
                        .as_ref()
                        .is_none_or(|filter| Self::expr_is_pure_with_known(filter, pure_functions))
                    && Self::expr_is_pure_with_known(body, pure_functions)
            }

            AotExpr::Index { array, indices, .. } => {
                Self::expr_is_pure_with_known(array, pure_functions)
                    && indices
                        .iter()
                        .all(|index| Self::expr_is_pure_with_known(index, pure_functions))
            }

            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::expr_is_pure_with_known(start, pure_functions)
                    && Self::expr_is_pure_with_known(stop, pure_functions)
                    && step
                        .as_ref()
                        .is_none_or(|s| Self::expr_is_pure_with_known(s, pure_functions))
            }

            AotExpr::FieldAccess { object, .. } => {
                Self::expr_is_pure_with_known(object, pure_functions)
            }

            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_is_pure_with_known(condition, pure_functions)
                    && Self::expr_is_pure_with_known(then_expr, pure_functions)
                    && Self::expr_is_pure_with_known(else_expr, pure_functions)
            }

            AotExpr::Box(inner)
            | AotExpr::Unbox { value: inner, .. }
            | AotExpr::Convert { value: inner, .. } => {
                Self::expr_is_pure_with_known(inner, pure_functions)
            }

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
        if depth > 64 {
            return 0; // Prevent infinite inlining
        }

        let mut total_inlined = 0;
        let mut i = 0;

        while i < stmts.len() {
            // Try to inline calls in this statement
            let (new_stmts, inlined) = self.try_inline_stmt(&stmts[i], functions, depth);

            if inlined > 0 {
                // Replace the statement with inlined version
                let refined_local = new_stmts.last().and_then(|stmt| match stmt {
                    AotStmt::Let { name, ty, .. } if !matches!(ty, StaticType::Any) => {
                        Some((name.clone(), ty.clone()))
                    }
                    _ => None,
                });
                let replacement_len = new_stmts.len();
                stmts.splice(i..=i, new_stmts);
                if let Some((name, ty)) = refined_local {
                    for stmt in &mut stmts[i + replacement_len..] {
                        Self::refine_stmt_var_type(stmt, &name, &ty);
                    }
                }
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

    fn refine_stmt_var_type(stmt: &mut AotStmt, name: &str, ty: &StaticType) {
        match stmt {
            AotStmt::Let { value, .. } => Self::refine_expr_var_type(value, name, ty),
            AotStmt::Assign { target, value } | AotStmt::CompoundAssign { target, value, .. } => {
                Self::refine_expr_var_type(target, name, ty);
                Self::refine_expr_var_type(value, name, ty);
            }
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) | AotStmt::Return(Some(expr)) => {
                Self::refine_expr_var_type(expr, name, ty)
            }
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::refine_expr_var_type(condition, name, ty);
                for stmt in then_branch {
                    Self::refine_stmt_var_type(stmt, name, ty);
                }
                if let Some(else_branch) = else_branch {
                    for stmt in else_branch {
                        Self::refine_stmt_var_type(stmt, name, ty);
                    }
                }
            }
            AotStmt::While { condition, body } => {
                Self::refine_expr_var_type(condition, name, ty);
                for stmt in body {
                    Self::refine_stmt_var_type(stmt, name, ty);
                }
            }
            AotStmt::ForRange {
                start,
                stop,
                step,
                body,
                ..
            } => {
                Self::refine_expr_var_type(start, name, ty);
                Self::refine_expr_var_type(stop, name, ty);
                if let Some(step) = step {
                    Self::refine_expr_var_type(step, name, ty);
                }
                for stmt in body {
                    Self::refine_stmt_var_type(stmt, name, ty);
                }
            }
            AotStmt::ForEach { iter, body, .. } => {
                Self::refine_expr_var_type(iter, name, ty);
                for stmt in body {
                    Self::refine_stmt_var_type(stmt, name, ty);
                }
            }
            AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => {}
        }
    }

    fn refine_expr_var_type(expr: &mut AotExpr, name: &str, ty: &StaticType) {
        match expr {
            AotExpr::Var {
                name: var,
                ty: var_ty,
            } if var == name => *var_ty = ty.clone(),
            AotExpr::CallStatic { args, .. }
            | AotExpr::CallDynamic { args, .. }
            | AotExpr::CallBuiltin { args, .. }
            | AotExpr::ArrayLit { elements: args, .. }
            | AotExpr::TupleLit { elements: args }
            | AotExpr::StructNew { fields: args, .. } => {
                for arg in args {
                    Self::refine_expr_var_type(arg, name, ty);
                }
            }
            AotExpr::BinOpStatic { left, right, .. }
            | AotExpr::BinOpDynamic { left, right, .. } => {
                Self::refine_expr_var_type(left, name, ty);
                Self::refine_expr_var_type(right, name, ty);
            }
            AotExpr::UnaryOp { operand, .. }
            | AotExpr::Box(operand)
            | AotExpr::Unbox { value: operand, .. }
            | AotExpr::Convert { value: operand, .. }
            | AotExpr::SetFromIter { iter: operand, .. }
            | AotExpr::FieldAccess {
                object: operand, ..
            } => Self::refine_expr_var_type(operand, name, ty),
            AotExpr::Index { array, indices, .. } => {
                Self::refine_expr_var_type(array, name, ty);
                for index in indices {
                    Self::refine_expr_var_type(index, name, ty);
                }
            }
            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::refine_expr_var_type(start, name, ty);
                Self::refine_expr_var_type(stop, name, ty);
                if let Some(step) = step {
                    Self::refine_expr_var_type(step, name, ty);
                }
            }
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::refine_expr_var_type(condition, name, ty);
                Self::refine_expr_var_type(then_expr, name, ty);
                Self::refine_expr_var_type(else_expr, name, ty);
            }
            AotExpr::Lambda { body, .. } => {
                for stmt in body {
                    Self::refine_stmt_var_type(stmt, name, ty);
                }
            }
            AotExpr::Comprehension {
                body, iter, filter, ..
            }
            | AotExpr::Generator {
                body, iter, filter, ..
            } => {
                Self::refine_expr_var_type(body, name, ty);
                Self::refine_expr_var_type(iter, name, ty);
                if let Some(filter) = filter {
                    Self::refine_expr_var_type(filter, name, ty);
                }
            }
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                Self::refine_expr_var_type(body, name, ty);
                for (_, iter) in iterations {
                    Self::refine_expr_var_type(iter, name, ty);
                }
                if let Some(filter) = filter {
                    Self::refine_expr_var_type(filter, name, ty);
                }
            }
            AotExpr::NamedTupleLit { fields } => {
                for (_, field) in fields {
                    Self::refine_expr_var_type(field, name, ty);
                }
            }
            AotExpr::Var { .. }
            | AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing
            | AotExpr::LitMissing => {}
        }
    }

    /// True when an inlined call's result expression, discarded in statement
    /// position, has no effects: a variable read, a literal, or a numeric
    /// `Convert` wrapper (from a return-type annotation) over one of those.
    /// Such a result would codegen as a bare Rust path statement (Issue #10796).
    fn inline_result_is_effect_free(expr: &AotExpr) -> bool {
        match expr {
            AotExpr::Var { .. }
            | AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing
            | AotExpr::LitMissing => true,
            AotExpr::Convert { value, .. } => Self::inline_result_is_effect_free(value),
            _ => false,
        }
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
                    let result_ty = result_expr.get_type();
                    stmts.push(AotStmt::Let {
                        name: name.clone(),
                        ty: if matches!(ty, StaticType::Any) {
                            result_ty
                        } else {
                            ty.clone()
                        },
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
                    // Statement position discards the value. An effect-free
                    // result (the inlined body's accumulator variable or a
                    // literal, possibly under a return-annotation Convert
                    // wrapper) would codegen as a bare Rust path statement —
                    // `_inline0_0_total;` — tripping `path_statements` under
                    // `-D warnings` (Issue #10796). Drop it instead.
                    if !Self::inline_result_is_effect_free(&result_expr) {
                        stmts.push(AotStmt::Expr(result_expr));
                    }
                    return (stmts, count);
                }
            }
            AotStmt::ValueCarrier(expr) => {
                if let Some((inlined_stmts, result_expr, count)) =
                    self.try_inline_expr(expr, functions, depth)
                {
                    let mut stmts = inlined_stmts;
                    stmts.push(AotStmt::ValueCarrier(result_expr));
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
        if let AotExpr::CallDynamic { function, args } = expr {
            for (index, arg) in args.iter().enumerate() {
                if let Some((stmts, replacement, count)) =
                    self.try_inline_expr(arg, functions, depth + 1)
                {
                    let mut rewritten_args = args.clone();
                    rewritten_args[index] = replacement;
                    return Some((
                        stmts,
                        AotExpr::CallDynamic {
                            function: function.clone(),
                            args: rewritten_args,
                        },
                        count,
                    ));
                }
            }
            if let Some(func) = functions.get(function) {
                if Self::boxed_return_is_callable_specializable(func, args) {
                    let inlined = self.inline_function_call(func, args, &StaticType::Any, depth);
                    if inlined.is_some() {
                        self.specialized_boxed_returns.insert(function.clone());
                    }
                    return inlined;
                }
            }
        }
        if let AotExpr::CallStatic {
            function,
            args,
            return_ty,
            inline_policy,
        } = expr
        {
            for (index, arg) in args.iter().enumerate() {
                if let Some((stmts, replacement, count)) =
                    self.try_inline_expr(arg, functions, depth + 1)
                {
                    let mut rewritten_args = args.clone();
                    rewritten_args[index] = replacement;
                    return Some((
                        stmts,
                        AotExpr::CallStatic {
                            function: function.clone(),
                            args: rewritten_args,
                            return_ty: return_ty.clone(),
                            inline_policy: *inline_policy,
                        },
                        count,
                    ));
                }
            }
        }
        if let AotExpr::CallStatic {
            function,
            args,
            return_ty,
            inline_policy,
        } = expr
        {
            // Check if this function should be inlined
            if let Some(candidate) = self.inline_candidates.get(function).cloned() {
                let should_inline = match inline_policy {
                    AotInlinePolicy::Never => false,
                    AotInlinePolicy::Always => {
                        !candidate.is_recursive
                            && (!candidate.return_needs_value
                                || functions.get(function).is_some_and(|func| {
                                    Self::boxed_return_is_callable_specializable(func, args)
                                }))
                    }
                    AotInlinePolicy::Auto => {
                        candidate.should_inline(self.max_inline_size)
                            || (candidate.return_needs_value
                                && !candidate.is_recursive
                                && candidate.size <= self.max_inline_size
                                && candidate.score > 0
                                && functions.get(function).is_some_and(|func| {
                                    Self::boxed_return_is_callable_specializable(func, args)
                                }))
                    }
                };
                if should_inline {
                    if let Some(func) = functions.get(function) {
                        let inlined = self.inline_function_call(func, args, return_ty, depth);
                        if inlined.is_some() {
                            self.specialized_boxed_returns.insert(function.clone());
                        }
                        return inlined;
                    }
                }
            }
        }
        None
    }

    fn boxed_return_is_callable_specializable(func: &AotFunction, args: &[AotExpr]) -> bool {
        func.params.iter().zip(args).any(|((_, param_ty), arg)| {
            matches!(param_ty, StaticType::Any)
                && matches!(arg.get_type(), StaticType::Function { .. })
        }) || func
            .body
            .iter()
            .any(|stmt| matches!(stmt, AotStmt::Return(Some(AotExpr::Lambda { .. }))))
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
        let specialized_types = func
            .params
            .iter()
            .zip(args)
            .map(|((name, _), arg)| (name.clone(), arg.get_type()))
            .collect::<HashMap<_, _>>();

        // Create bindings for parameters
        for ((param_name, param_ty), arg) in func.params.iter().zip(args.iter()) {
            if matches!(arg.get_type(), StaticType::Function { .. }) {
                continue;
            }
            let new_name = format!("{}{}", prefix, param_name);
            // Check if we need to convert the argument type to match the parameter type
            let arg_ty = arg.get_type();
            let binding_ty = if matches!(param_ty, StaticType::Any) {
                arg_ty.clone()
            } else {
                param_ty.clone()
            };
            let converted_arg =
                if arg_ty != binding_ty && Self::needs_type_conversion(&arg_ty, &binding_ty) {
                    // Wrap in Convert expression to handle type promotion
                    AotExpr::Convert {
                        value: Box::new(arg.clone()),
                        target_ty: binding_ty.clone(),
                    }
                } else {
                    arg.clone()
                };
            stmts.push(AotStmt::Let {
                name: new_name,
                ty: binding_ty,
                value: converted_arg,
                is_mutable: false,
            });
        }

        // Build variable rename map
        let mut rename_map: HashMap<String, String> = func
            .params
            .iter()
            .zip(args)
            .map(|((name, _), arg)| {
                let replacement = match arg {
                    AotExpr::Var {
                        name,
                        ty: StaticType::Function { .. },
                    } => name.clone(),
                    _ => format!("{}{}", prefix, name),
                };
                (name.clone(), replacement)
            })
            .collect();

        // Process function body
        let mut result_expr = AotExpr::LitNothing;

        for (i, stmt) in func.body.iter().enumerate() {
            let is_last = i == func.body.len() - 1;
            let renamed_stmt =
                self.rename_variables_in_stmt(stmt, &prefix, &mut rename_map, &specialized_types);

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
                        match &renamed_stmt {
                            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => {
                                result_expr = expr.clone();
                            }
                            _ => stmts.push(renamed_stmt),
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
        specialized_types: &HashMap<String, StaticType>,
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
                    value: self.rename_variables_in_expr(value, rename_map, specialized_types),
                    is_mutable: *is_mutable,
                }
            }
            AotStmt::Assign { target, value } => AotStmt::Assign {
                target: self.rename_variables_in_expr(target, rename_map, specialized_types),
                value: self.rename_variables_in_expr(value, rename_map, specialized_types),
            },
            AotStmt::CompoundAssign { target, op, value } => AotStmt::CompoundAssign {
                target: self.rename_variables_in_expr(target, rename_map, specialized_types),
                op: *op,
                value: self.rename_variables_in_expr(value, rename_map, specialized_types),
            },
            AotStmt::Expr(expr) => {
                AotStmt::Expr(self.rename_variables_in_expr(expr, rename_map, specialized_types))
            }
            AotStmt::ValueCarrier(expr) => AotStmt::ValueCarrier(self.rename_variables_in_expr(
                expr,
                rename_map,
                specialized_types,
            )),
            AotStmt::Return(opt_expr) => AotStmt::Return(
                opt_expr
                    .as_ref()
                    .map(|e| self.rename_variables_in_expr(e, rename_map, specialized_types)),
            ),
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => AotStmt::If {
                condition: self.rename_variables_in_expr(condition, rename_map, specialized_types),
                then_branch: then_branch
                    .iter()
                    .map(|s| {
                        self.rename_variables_in_stmt(s, prefix, rename_map, specialized_types)
                    })
                    .collect(),
                else_branch: else_branch.as_ref().map(|stmts| {
                    stmts
                        .iter()
                        .map(|s| {
                            self.rename_variables_in_stmt(s, prefix, rename_map, specialized_types)
                        })
                        .collect()
                }),
            },
            AotStmt::While { condition, body } => AotStmt::While {
                condition: self.rename_variables_in_expr(condition, rename_map, specialized_types),
                body: body
                    .iter()
                    .map(|s| {
                        self.rename_variables_in_stmt(s, prefix, rename_map, specialized_types)
                    })
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
                    start: self.rename_variables_in_expr(start, rename_map, specialized_types),
                    stop: self.rename_variables_in_expr(stop, rename_map, specialized_types),
                    step: step
                        .as_ref()
                        .map(|s| self.rename_variables_in_expr(s, rename_map, specialized_types)),
                    body: body
                        .iter()
                        .map(|s| {
                            self.rename_variables_in_stmt(s, prefix, rename_map, specialized_types)
                        })
                        .collect(),
                }
            }
            AotStmt::ForEach { var, iter, body } => {
                let new_var = format!("{}{}", prefix, var);
                rename_map.insert(var.clone(), new_var.clone());
                AotStmt::ForEach {
                    var: new_var,
                    iter: self.rename_variables_in_expr(iter, rename_map, specialized_types),
                    body: body
                        .iter()
                        .map(|s| {
                            self.rename_variables_in_stmt(s, prefix, rename_map, specialized_types)
                        })
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
        specialized_types: &HashMap<String, StaticType>,
    ) -> AotExpr {
        match expr {
            AotExpr::Var { name, ty } => {
                if let Some(new_name) = rename_map.get(name) {
                    AotExpr::Var {
                        name: new_name.clone(),
                        ty: specialized_types
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| ty.clone()),
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
                left: Box::new(self.rename_variables_in_expr(left, rename_map, specialized_types)),
                right: Box::new(self.rename_variables_in_expr(
                    right,
                    rename_map,
                    specialized_types,
                )),
                result_ty: result_ty.clone(),
            },
            AotExpr::BinOpDynamic { op, left, right } => AotExpr::BinOpDynamic {
                op: *op,
                left: Box::new(self.rename_variables_in_expr(left, rename_map, specialized_types)),
                right: Box::new(self.rename_variables_in_expr(
                    right,
                    rename_map,
                    specialized_types,
                )),
            },
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => AotExpr::UnaryOp {
                op: *op,
                operand: Box::new(self.rename_variables_in_expr(
                    operand,
                    rename_map,
                    specialized_types,
                )),
                result_ty: result_ty.clone(),
            },
            AotExpr::CallStatic {
                function,
                args,
                return_ty,
                inline_policy,
            } => AotExpr::CallStatic {
                function: rename_map
                    .get(function)
                    .cloned()
                    .unwrap_or_else(|| function.clone()),
                args: args
                    .iter()
                    .map(|a| self.rename_variables_in_expr(a, rename_map, specialized_types))
                    .collect(),
                return_ty: match specialized_types.get(function) {
                    Some(StaticType::Function { ret, .. }) => ret.as_ref().clone(),
                    _ => return_ty.clone(),
                },
                inline_policy: *inline_policy,
            },
            AotExpr::CallDynamic { function, args } => {
                let function_name = rename_map
                    .get(function)
                    .cloned()
                    .unwrap_or_else(|| function.clone());
                let args = args
                    .iter()
                    .map(|a| self.rename_variables_in_expr(a, rename_map, specialized_types))
                    .collect();
                match specialized_types.get(function) {
                    Some(StaticType::Function { ret, .. }) => AotExpr::CallStatic {
                        function: function_name,
                        args,
                        return_ty: ret.as_ref().clone(),
                        inline_policy: AotInlinePolicy::Auto,
                    },
                    _ => AotExpr::CallDynamic {
                        function: function_name,
                        args,
                    },
                }
            }
            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } => AotExpr::CallBuiltin {
                builtin: *builtin,
                args: args
                    .iter()
                    .map(|a| self.rename_variables_in_expr(a, rename_map, specialized_types))
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
                    .map(|e| self.rename_variables_in_expr(e, rename_map, specialized_types))
                    .collect(),
                elem_ty: elem_ty.clone(),
                shape: shape.clone(),
            },
            AotExpr::SetFromIter { iter, elem_ty } => AotExpr::SetFromIter {
                iter: Box::new(self.rename_variables_in_expr(iter, rename_map, specialized_types)),
                elem_ty: elem_ty.clone(),
            },
            AotExpr::TupleLit { elements } => AotExpr::TupleLit {
                elements: elements
                    .iter()
                    .map(|e| self.rename_variables_in_expr(e, rename_map, specialized_types))
                    .collect(),
            },
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple,
            } => AotExpr::Index {
                array: Box::new(self.rename_variables_in_expr(
                    array,
                    rename_map,
                    specialized_types,
                )),
                indices: indices
                    .iter()
                    .map(|i| self.rename_variables_in_expr(i, rename_map, specialized_types))
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
                start: Box::new(self.rename_variables_in_expr(
                    start,
                    rename_map,
                    specialized_types,
                )),
                stop: Box::new(self.rename_variables_in_expr(stop, rename_map, specialized_types)),
                step: step.as_ref().map(|s| {
                    Box::new(self.rename_variables_in_expr(s, rename_map, specialized_types))
                }),
                elem_ty: elem_ty.clone(),
            },
            AotExpr::Generator {
                body,
                var,
                iter,
                filter,
                elem_ty,
            } => {
                let mut inner_map = rename_map.clone();
                inner_map.remove(var);
                AotExpr::Generator {
                    body: Box::new(self.rename_variables_in_expr(
                        body,
                        &inner_map,
                        specialized_types,
                    )),
                    var: var.clone(),
                    iter: Box::new(self.rename_variables_in_expr(
                        iter,
                        rename_map,
                        specialized_types,
                    )),
                    filter: filter.as_ref().map(|filter| {
                        Box::new(self.rename_variables_in_expr(
                            filter,
                            &inner_map,
                            specialized_types,
                        ))
                    }),
                    elem_ty: elem_ty.clone(),
                }
            }
            AotExpr::StructNew { name, fields } => AotExpr::StructNew {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|f| self.rename_variables_in_expr(f, rename_map, specialized_types))
                    .collect(),
            },
            AotExpr::FieldAccess {
                object,
                field,
                field_ty,
            } => AotExpr::FieldAccess {
                object: Box::new(self.rename_variables_in_expr(
                    object,
                    rename_map,
                    specialized_types,
                )),
                field: field.clone(),
                field_ty: field_ty.clone(),
            },
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => AotExpr::Ternary {
                condition: Box::new(self.rename_variables_in_expr(
                    condition,
                    rename_map,
                    specialized_types,
                )),
                then_expr: Box::new(self.rename_variables_in_expr(
                    then_expr,
                    rename_map,
                    specialized_types,
                )),
                else_expr: Box::new(self.rename_variables_in_expr(
                    else_expr,
                    rename_map,
                    specialized_types,
                )),
                result_ty: result_ty.clone(),
            },
            AotExpr::Box(inner) => AotExpr::Box(Box::new(self.rename_variables_in_expr(
                inner,
                rename_map,
                specialized_types,
            ))),
            AotExpr::Unbox { value, target_ty } => AotExpr::Unbox {
                value: Box::new(self.rename_variables_in_expr(
                    value,
                    rename_map,
                    specialized_types,
                )),
                target_ty: target_ty.clone(),
            },
            AotExpr::Convert { value, target_ty } => {
                let value = self.rename_variables_in_expr(value, rename_map, specialized_types);
                if matches!(target_ty, StaticType::Any) {
                    value
                } else {
                    AotExpr::Convert {
                        value: Box::new(value),
                        target_ty: target_ty.clone(),
                    }
                }
            }
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
                    body: body
                        .iter()
                        .map(|stmt| {
                            self.rename_variables_in_stmt(
                                stmt,
                                "",
                                &mut inner_map,
                                specialized_types,
                            )
                        })
                        .collect(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::ir::AotBinOp;

    fn lit_add_function(name: &str) -> AotFunction {
        let mut func = AotFunction::new(name.to_string(), vec![], StaticType::I64);
        func.body.push(AotStmt::Return(Some(AotExpr::BinOpStatic {
            op: AotBinOp::Add,
            left: Box::new(AotExpr::LitI64(1)),
            right: Box::new(AotExpr::LitI64(2)),
            result_ty: StaticType::I64,
        })));
        func
    }

    #[test]
    fn statement_position_inline_drops_effect_free_result_issue_10796() {
        // count_to-style callee: mutable accumulator, Convert-wrapped tail
        // return (from the ::Int64 annotation). Inlined in statement position
        // (value unused), the effect-free result must NOT be emitted as a
        // bare path statement.
        let mut func = AotFunction::new("acc".to_string(), vec![], StaticType::I64);
        func.body.push(AotStmt::Let {
            name: "total".to_string(),
            ty: StaticType::I64,
            value: AotExpr::LitI64(0),
            is_mutable: true,
        });
        func.body.push(AotStmt::Return(Some(AotExpr::Convert {
            value: Box::new(AotExpr::Var {
                name: "total".to_string(),
                ty: StaticType::I64,
            }),
            target_ty: StaticType::I64,
        })));
        let mut program = AotProgram::new();
        program.add_function(func);
        program.main.push(AotStmt::Expr(AotExpr::CallStatic {
            function: "acc".to_string(),
            args: vec![],
            return_ty: StaticType::I64,
            inline_policy: AotInlinePolicy::Always,
        }));

        let mut inliner = AotInliner::new(10);
        let inlined = inliner.optimize_program(&mut program);
        assert!(inlined > 0, "call was not inlined");
        // No statement in main may be a bare effect-free Expr.
        for stmt in &program.main {
            if let AotStmt::Expr(expr) = stmt {
                assert!(
                    !AotInliner::inline_result_is_effect_free(expr),
                    "bare effect-free path statement survived: {expr:?}"
                );
            }
        }
    }

    #[test]
    fn static_calls_to_known_pure_functions_are_pure_issue_6981() {
        let mut program = AotProgram::new();
        program.add_function(lit_add_function("leaf"));

        let mut wrapper = AotFunction::new("wrapper".to_string(), vec![], StaticType::I64);
        wrapper.body.push(AotStmt::Return(Some(AotExpr::CallStatic {
            function: "leaf".to_string(),
            args: vec![],
            return_ty: StaticType::I64,
            inline_policy: AotInlinePolicy::Auto,
        })));
        program.add_function(wrapper);

        let mut inliner = AotInliner::new(10);
        inliner.analyze_program(&program);
        assert!(inliner.get_candidates()["leaf"].is_pure);
        assert!(inliner.get_candidates()["wrapper"].is_pure);
        assert!(inliner.get_candidates()["wrapper"].score > 10);
    }

    #[test]
    fn dynamic_calls_remain_impure_issue_6981() {
        let mut program = AotProgram::new();
        let mut wrapper = AotFunction::new("wrapper".to_string(), vec![], StaticType::Any);
        wrapper
            .body
            .push(AotStmt::Return(Some(AotExpr::CallDynamic {
                function: "f".to_string(),
                args: vec![AotExpr::LitI64(1)],
            })));
        program.add_function(wrapper);

        let mut inliner = AotInliner::new(10);
        inliner.analyze_program(&program);
        assert!(!inliner.get_candidates()["wrapper"].is_pure);
    }

    #[test]
    fn callable_argument_specializes_boxed_wrapper_issue_3() {
        let function_ty = StaticType::Function {
            params: vec![StaticType::I64],
            ret: Box::new(StaticType::I64),
        };
        let mut wrapper = AotFunction::new(
            "apply".to_string(),
            vec![
                ("x".to_string(), StaticType::Any),
                ("f".to_string(), StaticType::Any),
            ],
            StaticType::Any,
        );
        wrapper
            .body
            .push(AotStmt::Return(Some(AotExpr::CallDynamic {
                function: "f".to_string(),
                args: vec![AotExpr::Var {
                    name: "x".to_string(),
                    ty: StaticType::Any,
                }],
            })));
        let mut program = AotProgram::new();
        program.add_function(wrapper);
        program.main.push(AotStmt::Let {
            name: "result".to_string(),
            ty: StaticType::Any,
            value: AotExpr::CallStatic {
                function: "apply".to_string(),
                args: vec![
                    AotExpr::LitI64(3),
                    AotExpr::Var {
                        name: "increment".to_string(),
                        ty: function_ty,
                    },
                ],
                return_ty: StaticType::Any,
                inline_policy: AotInlinePolicy::Auto,
            },
            is_mutable: false,
        });

        let mut inliner = AotInliner::new(10);
        assert_eq!(inliner.optimize_program(&mut program), 1);
        assert!(program.functions.is_empty());
        assert!(matches!(
            program.main.last(),
            Some(AotStmt::Let {
                ty: StaticType::I64,
                value: AotExpr::CallStatic {
                    function,
                    return_ty: StaticType::I64,
                    ..
                },
                ..
            }) if function == "increment"
        ));
    }

    #[test]
    fn nested_calls_inline_beyond_legacy_depth_limit_issue_3() {
        let mut leaf = AotFunction::new(
            "identity".to_string(),
            vec![("value".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        leaf.body.push(AotStmt::Return(Some(AotExpr::Var {
            name: "value".to_string(),
            ty: StaticType::I64,
        })));
        let mut nested = AotExpr::LitI64(7);
        for _ in 0..12 {
            nested = AotExpr::CallStatic {
                function: "identity".to_string(),
                args: vec![nested],
                return_ty: StaticType::I64,
                inline_policy: AotInlinePolicy::Auto,
            };
        }
        let mut program = AotProgram::new();
        program.add_function(leaf);
        program.main.push(AotStmt::Let {
            name: "result".to_string(),
            ty: StaticType::I64,
            value: nested,
            is_mutable: false,
        });

        let mut inliner = AotInliner::new(10);
        assert!(inliner.optimize_program(&mut program) >= 12);
        assert!(matches!(
            program.main.last(),
            Some(AotStmt::Let {
                value: AotExpr::Var {
                    ty: StaticType::I64,
                    ..
                },
                ..
            }) | Some(AotStmt::Let {
                value: AotExpr::LitI64(7),
                ..
            })
        ));
    }

    #[test]
    fn runtime_boxed_return_calls_do_not_inline_into_main_issue_7012() {
        let mut program = AotProgram::new();
        let mut union_like = AotFunction::new("union_like".to_string(), vec![], StaticType::Any);
        union_like.body.push(AotStmt::Return(Some(AotExpr::Ternary {
            condition: Box::new(AotExpr::LitBool(true)),
            then_expr: Box::new(AotExpr::LitI64(1)),
            else_expr: Box::new(AotExpr::LitStr("fallback".to_string())),
            result_ty: StaticType::Any,
        })));
        program.add_function(union_like);
        program.main.push(AotStmt::Expr(AotExpr::CallStatic {
            function: "union_like".to_string(),
            args: vec![],
            return_ty: StaticType::Any,
            inline_policy: AotInlinePolicy::Always,
        }));

        let inlined = optimize_aot_program_with_inlining(&mut program, 10);

        assert_eq!(inlined, 0);
        assert!(
            matches!(
                program.main.as_slice(),
                [AotStmt::Expr(AotExpr::CallStatic { function, .. })]
                    if function == "union_like"
            ),
            "runtime-boxed return call should stay as a call, got {:?}",
            program.main
        );
    }
}
