//! IR-level pure expression consumers for Effects-driven optimizations.
//!
//! This pass covers the conservative slice needed by Issue #5185:
//! - straight-line CSE reuses an earlier local binding for the same pure value;
//! - loop-invariant pure expressions are hoisted into a generated temp before
//!   `for`/`foreach`/`while` loops when they do not depend on loop-mutated vars.

use crate::compile::effects::inference as effect_inference;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Module, Program, Stmt, UnaryOp};
use crate::span::Span;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const HOIST_TEMP_PREFIX: &str = "__sjulia_licm_";
const MAX_HOISTS_PER_LOOP: usize = 8;

#[derive(Debug, Clone)]
struct ExprKey {
    repr: String,
    vars: BTreeSet<String>,
    cost: usize,
    expr: Expr,
}

/// Result of the user-only optimization pass.
///
/// Only the segments the pass actually rewrites are materialized: user
/// functions, modules, and `main`. The Base function prefix and all other
/// `Program` fields are untouched by this pass, so callers keep reading them
/// from the input program instead of paying a full-IR deep clone per run
/// (4577 Base functions, ~5 ms for `println(1+1)`; Issue #6348).
#[derive(Debug)]
pub(super) struct UserSegmentOptimized {
    /// Optimized user functions (`program.functions[base_function_count..]`).
    pub user_functions: Vec<Function>,
    /// Optimized modules (stdlib/user modules are all eligible).
    pub modules: Vec<Module>,
    /// Optimized merged main block.
    pub main: Block,
}

pub(super) fn optimize_pure_expressions_user_only(
    program: &Program,
    base_function_count: usize,
) -> UserSegmentOptimized {
    let mut optimizer = IrOptimizer { next_temp: 0 };
    let base_function_count = base_function_count.min(program.functions.len());

    // Keep the original segment order (user functions → modules → main) so
    // generated LICM temp names stay deterministic across releases.
    let user_functions = program.functions[base_function_count..]
        .iter()
        .map(|func| optimizer.optimize_function(func))
        .collect();
    let modules = program
        .modules
        .iter()
        .map(|module| optimizer.optimize_module(module))
        .collect();
    let main = optimizer.optimize_block(&program.main);

    UserSegmentOptimized {
        user_functions,
        modules,
        main,
    }
}

struct IrOptimizer {
    next_temp: usize,
}

impl IrOptimizer {
    fn optimize_module(&mut self, module: &Module) -> Module {
        let mut result = module.clone();
        result.functions = module
            .functions
            .iter()
            .map(|func| self.optimize_function(func))
            .collect();
        result.submodules = module
            .submodules
            .iter()
            .map(|submodule| self.optimize_module(submodule))
            .collect();
        result.body = self.optimize_block(&module.body);
        result
    }

    fn optimize_function(&mut self, func: &Function) -> Function {
        let mut result = func.clone();
        result.body = self.optimize_block(&func.body);
        result
    }

    fn optimize_block(&mut self, block: &Block) -> Block {
        let mut stmts = Vec::with_capacity(block.stmts.len());
        let mut cse = HashMap::<String, CseEntry>::new();

        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { var, value, span } => {
                    let value = self.optimize_expr_with_cse(value, &cse);
                    invalidate_var(&mut cse, var);
                    if let Some(key) = pure_expr_key(&value) {
                        if key.cost > 1 && !key.vars.contains(var) {
                            cse.insert(
                                key.repr,
                                CseEntry {
                                    local: var.clone(),
                                    vars: key.vars,
                                },
                            );
                        }
                    } else {
                        cse.clear();
                    }
                    stmts.push(Stmt::Assign {
                        var: var.clone(),
                        value,
                        span: *span,
                    });
                }
                Stmt::Expr { expr, span } => {
                    let expr = self.optimize_expr_with_cse(expr, &cse);
                    if pure_expr_key(&expr).is_none() {
                        cse.clear();
                    }
                    stmts.push(Stmt::Expr { expr, span: *span });
                }
                Stmt::Return { value, span } => {
                    stmts.push(Stmt::Return {
                        value: value
                            .as_ref()
                            .map(|expr| self.optimize_expr_with_cse(expr, &cse)),
                        span: *span,
                    });
                    cse.clear();
                }
                Stmt::For { .. }
                | Stmt::ForEach { .. }
                | Stmt::ForEachTuple { .. }
                | Stmt::While { .. } => {
                    let (hoists, loop_stmt) = self.optimize_loop_stmt(stmt);
                    stmts.extend(hoists);
                    stmts.push(loop_stmt);
                    cse.clear();
                }
                _ => {
                    stmts.push(self.optimize_barrier_stmt(stmt));
                    cse.clear();
                }
            }
        }

        Block {
            stmts,
            span: block.span,
        }
    }

    fn optimize_barrier_stmt(&mut self, stmt: &Stmt) -> Stmt {
        match stmt {
            Stmt::Block(block) => Stmt::Block(self.optimize_block(block)),
            Stmt::AddAssign { var, value, span } => Stmt::AddAssign {
                var: var.clone(),
                value: self.optimize_expr_without_cse(value),
                span: *span,
            },
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Stmt::If {
                condition: self.optimize_expr_without_cse(condition),
                then_branch: self.optimize_block(then_branch),
                else_branch: else_branch.as_ref().map(|block| self.optimize_block(block)),
                span: *span,
            },
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block,
                finally_block,
                span,
            } => Stmt::Try {
                try_block: self.optimize_block(try_block),
                catch_var: catch_var.clone(),
                catch_block: catch_block.as_ref().map(|block| self.optimize_block(block)),
                else_block: else_block.as_ref().map(|block| self.optimize_block(block)),
                finally_block: finally_block
                    .as_ref()
                    .map(|block| self.optimize_block(block)),
                span: *span,
            },
            Stmt::Timed { body, span } => Stmt::Timed {
                body: self.optimize_block(body),
                span: *span,
            },
            Stmt::Test {
                condition,
                message,
                span,
            } => Stmt::Test {
                condition: self.optimize_expr_without_cse(condition),
                message: message.clone(),
                span: *span,
            },
            Stmt::TestSet { name, body, span } => Stmt::TestSet {
                name: name.clone(),
                body: self.optimize_block(body),
                span: *span,
            },
            Stmt::TestThrows {
                exception_type,
                expr,
                span,
            } => Stmt::TestThrows {
                exception_type: exception_type.clone(),
                expr: Box::new(self.optimize_expr_without_cse(expr)),
                span: *span,
            },
            Stmt::IndexAssign {
                array,
                indices,
                value,
                span,
            } => Stmt::IndexAssign {
                array: array.clone(),
                indices: indices
                    .iter()
                    .map(|expr| self.optimize_expr_without_cse(expr))
                    .collect(),
                value: self.optimize_expr_without_cse(value),
                span: *span,
            },
            Stmt::FieldAssign {
                object,
                field,
                value,
                span,
            } => Stmt::FieldAssign {
                object: object.clone(),
                field: field.clone(),
                value: self.optimize_expr_without_cse(value),
                span: *span,
            },
            Stmt::DestructuringAssign {
                targets,
                value,
                span,
            } => Stmt::DestructuringAssign {
                targets: targets.clone(),
                value: self.optimize_expr_without_cse(value),
                span: *span,
            },
            Stmt::DictAssign {
                dict,
                key,
                value,
                span,
            } => Stmt::DictAssign {
                dict: dict.clone(),
                key: self.optimize_expr_without_cse(key),
                value: self.optimize_expr_without_cse(value),
                span: *span,
            },
            Stmt::FunctionDef { func, span } => Stmt::FunctionDef {
                func: Box::new(self.optimize_function(func)),
                span: *span,
            },
            Stmt::EvalFunctionDef { func, span } => Stmt::EvalFunctionDef {
                func: Box::new(self.optimize_function(func)),
                span: *span,
            },
            _ => stmt.clone(),
        }
    }

    fn optimize_loop_stmt(&mut self, stmt: &Stmt) -> (Vec<Stmt>, Stmt) {
        let (mutated, loop_vars) = loop_mutation_set(stmt);
        match stmt {
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                span,
            } => {
                let body = self.optimize_block(body);
                let mutated = mutation_set_after_nested_optimization(mutated.clone(), &body);
                let (hoists, body) = self.hoist_loop_invariants(&body, &mutated, *span);
                (
                    hoists,
                    Stmt::For {
                        var: var.clone(),
                        start: self.optimize_expr_without_cse(start),
                        end: self.optimize_expr_without_cse(end),
                        step: step
                            .as_ref()
                            .map(|expr| self.optimize_expr_without_cse(expr)),
                        body,
                        span: *span,
                    },
                )
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                span,
            } => {
                let body = self.optimize_block(body);
                let mutated = mutation_set_after_nested_optimization(mutated.clone(), &body);
                let (hoists, body) = self.hoist_loop_invariants(&body, &mutated, *span);
                (
                    hoists,
                    Stmt::ForEach {
                        var: var.clone(),
                        iterable: self.optimize_expr_without_cse(iterable),
                        body,
                        span: *span,
                    },
                )
            }
            Stmt::ForEachTuple {
                vars,
                iterable,
                body,
                span,
            } => {
                let body = self.optimize_block(body);
                let mutated = mutation_set_after_nested_optimization(mutated.clone(), &body);
                let (hoists, body) = self.hoist_loop_invariants(&body, &mutated, *span);
                (
                    hoists,
                    Stmt::ForEachTuple {
                        vars: vars.clone(),
                        iterable: self.optimize_expr_without_cse(iterable),
                        body,
                        span: *span,
                    },
                )
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                let body = self.optimize_block(body);
                let condition = if loop_vars.is_empty() {
                    self.optimize_expr_without_cse(condition)
                } else {
                    condition.clone()
                };
                // A while body may execute zero times. Hoisting dispatchable
                // expressions such as `x + 1` before the condition can turn a
                // skipped body into a MethodError (Issue #5618).
                (
                    Vec::new(),
                    Stmt::While {
                        condition,
                        body,
                        span: *span,
                    },
                )
            }
            _ => (Vec::new(), stmt.clone()),
        }
    }

    fn hoist_loop_invariants(
        &mut self,
        body: &Block,
        mutated: &HashSet<String>,
        span: Span,
    ) -> (Vec<Stmt>, Block) {
        let mut candidates = BTreeMap::<String, ExprKey>::new();
        collect_loop_candidates_block(body, mutated, &mut candidates);

        let selected = candidates
            .into_values()
            .filter(|key| key.cost > 1 && !key.vars.is_empty())
            .take(MAX_HOISTS_PER_LOOP)
            .collect::<Vec<_>>();

        if selected.is_empty() {
            return (Vec::new(), body.clone());
        }

        let mut replacements = HashMap::<String, String>::new();
        let mut hoists = Vec::with_capacity(selected.len());
        for key in selected {
            let temp = self.next_hoist_temp();
            let value = replace_expr_keys(&key.expr, &replacements);
            replacements.insert(key.repr, temp.clone());
            hoists.push(Stmt::Assign {
                var: temp,
                value,
                span,
            });
        }

        let body = replace_block_expr_keys(body, &replacements);
        (hoists, body)
    }

    fn optimize_expr_without_cse(&mut self, expr: &Expr) -> Expr {
        self.optimize_expr_with_cse(expr, &HashMap::new())
    }

    fn optimize_expr_with_cse(&mut self, expr: &Expr, cse: &HashMap<String, CseEntry>) -> Expr {
        let expr = match expr {
            Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
                op: *op,
                operand: Box::new(self.optimize_expr_with_cse(operand, cse)),
                span: *span,
            },
            Expr::BinaryOp {
                op,
                left,
                right,
                span,
            } => Expr::BinaryOp {
                op: *op,
                left: Box::new(self.optimize_expr_with_cse(left, cse)),
                right: Box::new(self.optimize_expr_with_cse(right, cse)),
                span: *span,
            },
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            } => Expr::Call {
                function: function.clone(),
                args: args
                    .iter()
                    .map(|arg| self.optimize_expr_with_cse(arg, cse))
                    .collect(),
                kwargs: kwargs
                    .iter()
                    .map(|(name, value)| (name.clone(), self.optimize_expr_with_cse(value, cse)))
                    .collect(),
                splat_mask: splat_mask.clone(),
                kwargs_splat_mask: kwargs_splat_mask.clone(),
                span: *span,
            },
            Expr::Builtin { name, args, span } => Expr::Builtin {
                name: *name,
                args: args
                    .iter()
                    .map(|arg| self.optimize_expr_with_cse(arg, cse))
                    .collect(),
                span: *span,
            },
            Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            } => Expr::ModuleCall {
                module: module.clone(),
                function: function.clone(),
                args: args
                    .iter()
                    .map(|arg| self.optimize_expr_with_cse(arg, cse))
                    .collect(),
                kwargs: kwargs
                    .iter()
                    .map(|(name, value)| (name.clone(), self.optimize_expr_with_cse(value, cse)))
                    .collect(),
                splat_mask: splat_mask.clone(),
                kwargs_splat_mask: kwargs_splat_mask.clone(),
                span: *span,
            },
            Expr::TupleLiteral { elements, span } => Expr::TupleLiteral {
                elements: elements
                    .iter()
                    .map(|expr| self.optimize_expr_with_cse(expr, cse))
                    .collect(),
                span: *span,
            },
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                span,
            } => Expr::Ternary {
                condition: Box::new(self.optimize_expr_with_cse(condition, cse)),
                then_expr: Box::new(self.optimize_expr_with_cse(then_expr, cse)),
                else_expr: Box::new(self.optimize_expr_with_cse(else_expr, cse)),
                span: *span,
            },
            _ => expr.clone(),
        };

        let Some(key) = pure_expr_key(&expr) else {
            return expr;
        };
        if key.cost <= 1 {
            return expr;
        }
        cse.get(&key.repr)
            .map(|entry| Expr::Var(entry.local.clone(), expr_span(&expr)))
            .unwrap_or(expr)
    }

    fn next_hoist_temp(&mut self) -> String {
        let temp = format!("{HOIST_TEMP_PREFIX}{}", self.next_temp);
        self.next_temp += 1;
        temp
    }
}

#[derive(Debug, Clone)]
struct CseEntry {
    local: String,
    vars: BTreeSet<String>,
}

fn invalidate_var(cse: &mut HashMap<String, CseEntry>, var: &str) {
    cse.retain(|_, entry| entry.local != var && !entry.vars.contains(var));
}

fn loop_mutation_set(stmt: &Stmt) -> (HashSet<String>, HashSet<String>) {
    let mut mutated = HashSet::new();
    let mut loop_vars = HashSet::new();
    match stmt {
        Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
            mutated.insert(var.clone());
            loop_vars.insert(var.clone());
            collect_assigned_vars_block(body, &mut mutated);
        }
        Stmt::ForEachTuple { vars, body, .. } => {
            for var in vars {
                mutated.insert(var.clone());
                loop_vars.insert(var.clone());
            }
            collect_assigned_vars_block(body, &mut mutated);
        }
        Stmt::While { body, .. } => {
            collect_assigned_vars_block(body, &mut mutated);
        }
        _ => {}
    }
    (mutated, loop_vars)
}

fn mutation_set_after_nested_optimization(
    mut mutated: HashSet<String>,
    body: &Block,
) -> HashSet<String> {
    collect_assigned_vars_block(body, &mut mutated);
    mutated
}

fn collect_assigned_vars_block(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_assigned_vars_stmt(stmt, out);
    }
}

fn collect_assigned_vars_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
            out.insert(var.clone());
            collect_mutated_vars_expr(value, out);
        }
        Stmt::FieldAssign { object, value, .. } => {
            out.insert(object.clone());
            collect_mutated_vars_expr(value, out);
        }
        Stmt::IndexAssign {
            array,
            indices,
            value,
            ..
        } => {
            out.insert(array.clone());
            for index in indices {
                collect_mutated_vars_expr(index, out);
            }
            collect_mutated_vars_expr(value, out);
        }
        Stmt::DictAssign {
            dict, key, value, ..
        } => {
            out.insert(dict.clone());
            collect_mutated_vars_expr(key, out);
            collect_mutated_vars_expr(value, out);
        }
        Stmt::DestructuringAssign { targets, .. } => {
            out.extend(targets.iter().cloned());
        }
        Stmt::Expr { expr, .. } => {
            collect_mutated_vars_expr(expr, out);
        }
        Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
            out.insert(var.clone());
            collect_assigned_vars_block(body, out);
        }
        Stmt::ForEachTuple { vars, body, .. } => {
            out.extend(vars.iter().cloned());
            collect_assigned_vars_block(body, out);
        }
        Stmt::While { body, .. } | Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_assigned_vars_block(body, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_mutated_vars_expr(condition, out);
            collect_assigned_vars_block(then_branch, out);
            if let Some(block) = else_branch {
                collect_assigned_vars_block(block, out);
            }
        }
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_assigned_vars_block(try_block, out);
            if let Some(var) = catch_var {
                out.insert(var.clone());
            }
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                collect_assigned_vars_block(block, out);
            }
        }
        Stmt::Block(block) => collect_assigned_vars_block(block, out),
        _ => {}
    }
}

fn collect_mutated_vars_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            if mutating_call_name(function) {
                for arg in args {
                    if let Expr::Var(name, _) = arg {
                        out.insert(name.clone());
                    }
                }
            }
            for arg in args {
                collect_mutated_vars_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_mutated_vars_expr(value, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                collect_mutated_vars_expr(arg, out);
            }
        }
        Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_mutated_vars_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_mutated_vars_expr(value, out);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_mutated_vars_expr(left, out);
            collect_mutated_vars_expr(right, out);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::FieldAccess {
            object: operand, ..
        } => {
            collect_mutated_vars_expr(operand, out);
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for element in elements {
                collect_mutated_vars_expr(element, out);
            }
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_mutated_vars_expr(value, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_mutated_vars_expr(key, out);
            collect_mutated_vars_expr(value, out);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_mutated_vars_expr(key, out);
                collect_mutated_vars_expr(value, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_mutated_vars_expr(array, out);
            for index in indices {
                collect_mutated_vars_expr(index, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_mutated_vars_expr(start, out);
            if let Some(step) = step {
                collect_mutated_vars_expr(step, out);
            }
            collect_mutated_vars_expr(stop, out);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_mutated_vars_expr(condition, out);
            collect_mutated_vars_expr(then_expr, out);
            collect_mutated_vars_expr(else_expr, out);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_mutated_vars_expr(value, out);
            }
            collect_assigned_vars_block(body, out);
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_mutated_vars_expr(part, out);
            }
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_mutated_vars_expr(body, out);
            collect_mutated_vars_expr(iter, out);
            if let Some(filter) = filter {
                collect_mutated_vars_expr(filter, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_mutated_vars_expr(body, out);
            for (_, iter) in iterations {
                collect_mutated_vars_expr(iter, out);
            }
            if let Some(filter) = filter {
                collect_mutated_vars_expr(filter, out);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_mutated_vars_expr(base_expr, out);
            }
            for arg in type_args {
                collect_mutated_vars_expr(arg, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            collect_mutated_vars_expr(constructor, out);
        }
        Expr::AssignExpr { var, value, .. } => {
            out.insert(var.clone());
            collect_mutated_vars_expr(value, out);
        }
        Expr::ReturnExpr { value, .. } => {
            if let Some(value) = value {
                collect_mutated_vars_expr(value, out);
            }
        }
        Expr::Literal(_, _)
        | Expr::Var(_, _)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

fn mutating_call_name(function: &str) -> bool {
    matches!(
        function,
        "push!" | "pop!" | "append!" | "deleteat!" | "setindex!" | "empty!" | "resize!" | "fill!"
    )
}

fn collect_loop_candidates_block(
    block: &Block,
    mutated: &HashSet<String>,
    out: &mut BTreeMap<String, ExprKey>,
) {
    for stmt in &block.stmts {
        collect_loop_candidates_stmt(stmt, mutated, out);
    }
}

fn collect_loop_candidates_stmt(
    stmt: &Stmt,
    mutated: &HashSet<String>,
    out: &mut BTreeMap<String, ExprKey>,
) {
    match stmt {
        Stmt::Assign { value, .. } | Stmt::Expr { expr: value, .. } => {
            collect_loop_candidates_expr(value, mutated, out);
        }
        Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::AddAssign { value, .. }
        | Stmt::Test {
            condition: value, ..
        } => {
            collect_loop_candidates_expr(value, mutated, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_loop_candidates_expr(condition, mutated, out);
            collect_loop_candidates_block(then_branch, mutated, out);
            if let Some(block) = else_branch {
                collect_loop_candidates_block(block, mutated, out);
            }
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            collect_loop_candidates_block(block, mutated, out);
        }
        _ => {}
    }
}

fn collect_loop_candidates_expr(
    expr: &Expr,
    mutated: &HashSet<String>,
    out: &mut BTreeMap<String, ExprKey>,
) {
    match expr {
        Expr::UnaryOp { operand, .. } => collect_loop_candidates_expr(operand, mutated, out),
        Expr::BinaryOp { left, right, .. } => {
            collect_loop_candidates_expr(left, mutated, out);
            collect_loop_candidates_expr(right, mutated, out);
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_loop_candidates_expr(arg, mutated, out);
            }
            for (_, value) in kwargs {
                collect_loop_candidates_expr(value, mutated, out);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_loop_candidates_expr(arg, mutated, out);
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_loop_candidates_expr(element, mutated, out);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_loop_candidates_expr(condition, mutated, out);
            collect_loop_candidates_expr(then_expr, mutated, out);
            collect_loop_candidates_expr(else_expr, mutated, out);
        }
        _ => {}
    }

    let Some(key) = pure_expr_key(expr) else {
        return;
    };
    if key.cost <= 1 || key.vars.iter().any(|var| mutated.contains(var)) {
        return;
    }
    out.entry(key.repr.clone()).or_insert(key);
}

fn replace_block_expr_keys(block: &Block, replacements: &HashMap<String, String>) -> Block {
    Block {
        stmts: block
            .stmts
            .iter()
            .map(|stmt| replace_stmt_expr_keys(stmt, replacements))
            .collect(),
        span: block.span,
    }
}

fn replace_stmt_expr_keys(stmt: &Stmt, replacements: &HashMap<String, String>) -> Stmt {
    match stmt {
        Stmt::Assign { var, value, span } => Stmt::Assign {
            var: var.clone(),
            value: replace_expr_keys(value, replacements),
            span: *span,
        },
        Stmt::Expr { expr, span } => Stmt::Expr {
            expr: replace_expr_keys(expr, replacements),
            span: *span,
        },
        Stmt::Return { value, span } => Stmt::Return {
            value: value
                .as_ref()
                .map(|expr| replace_expr_keys(expr, replacements)),
            span: *span,
        },
        Stmt::AddAssign { var, value, span } => Stmt::AddAssign {
            var: var.clone(),
            value: replace_expr_keys(value, replacements),
            span: *span,
        },
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => Stmt::If {
            condition: replace_expr_keys(condition, replacements),
            then_branch: replace_block_expr_keys(then_branch, replacements),
            else_branch: else_branch
                .as_ref()
                .map(|block| replace_block_expr_keys(block, replacements)),
            span: *span,
        },
        Stmt::Block(block) => Stmt::Block(replace_block_expr_keys(block, replacements)),
        _ => stmt.clone(),
    }
}

fn replace_expr_keys(expr: &Expr, replacements: &HashMap<String, String>) -> Expr {
    if let Some(key) = pure_expr_key(expr) {
        if let Some(temp) = replacements.get(&key.repr) {
            return Expr::Var(temp.clone(), expr_span(expr));
        }
    }

    match expr {
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op: *op,
            operand: Box::new(replace_expr_keys(operand, replacements)),
            span: *span,
        },
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Expr::BinaryOp {
            op: *op,
            left: Box::new(replace_expr_keys(left, replacements)),
            right: Box::new(replace_expr_keys(right, replacements)),
            span: *span,
        },
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Expr::Call {
            function: function.clone(),
            args: args
                .iter()
                .map(|arg| replace_expr_keys(arg, replacements))
                .collect(),
            kwargs: kwargs
                .iter()
                .map(|(name, value)| (name.clone(), replace_expr_keys(value, replacements)))
                .collect(),
            splat_mask: splat_mask.clone(),
            kwargs_splat_mask: kwargs_splat_mask.clone(),
            span: *span,
        },
        Expr::Builtin { name, args, span } => Expr::Builtin {
            name: *name,
            args: args
                .iter()
                .map(|arg| replace_expr_keys(arg, replacements))
                .collect(),
            span: *span,
        },
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Expr::ModuleCall {
            module: module.clone(),
            function: function.clone(),
            args: args
                .iter()
                .map(|arg| replace_expr_keys(arg, replacements))
                .collect(),
            kwargs: kwargs
                .iter()
                .map(|(name, value)| (name.clone(), replace_expr_keys(value, replacements)))
                .collect(),
            splat_mask: splat_mask.clone(),
            kwargs_splat_mask: kwargs_splat_mask.clone(),
            span: *span,
        },
        Expr::TupleLiteral { elements, span } => Expr::TupleLiteral {
            elements: elements
                .iter()
                .map(|expr| replace_expr_keys(expr, replacements))
                .collect(),
            span: *span,
        },
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            span,
        } => Expr::Ternary {
            condition: Box::new(replace_expr_keys(condition, replacements)),
            then_expr: Box::new(replace_expr_keys(then_expr, replacements)),
            else_expr: Box::new(replace_expr_keys(else_expr, replacements)),
            span: *span,
        },
        _ => expr.clone(),
    }
}

fn pure_expr_key(expr: &Expr) -> Option<ExprKey> {
    match expr {
        Expr::Literal(lit, _) if pure_literal(lit) => Some(ExprKey {
            repr: format!("lit:{lit:?}"),
            vars: BTreeSet::new(),
            cost: 1,
            expr: expr.clone(),
        }),
        Expr::Var(name, _) => Some(ExprKey {
            repr: format!("var:{name}"),
            vars: BTreeSet::from([name.clone()]),
            cost: 1,
            expr: expr.clone(),
        }),
        Expr::UnaryOp { op, operand, .. } if pure_unary_op(*op) => {
            let key = pure_expr_key(operand)?;
            Some(ExprKey {
                repr: format!("un:{op:?}({})", key.repr),
                vars: key.vars,
                cost: key.cost + 1,
                expr: expr.clone(),
            })
        }
        Expr::BinaryOp {
            op, left, right, ..
        } if pure_nothrow_binary_op(*op) => {
            let left = pure_expr_key(left)?;
            let right = pure_expr_key(right)?;
            let mut vars = left.vars.clone();
            vars.extend(right.vars.iter().cloned());
            Some(ExprKey {
                repr: format!("bin:{op:?}({}, {})", left.repr, right.repr),
                vars,
                cost: left.cost + right.cost + 1,
                expr: expr.clone(),
            })
        }
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if kwargs.is_empty()
            && splat_mask.iter().all(|splat| !*splat)
            && kwargs_splat_mask.iter().all(|splat| !*splat)
            && effect_inference::infer_expr_effects(expr).is_pure() =>
        {
            expr_key_from_args(format!("call:{function}"), args, expr)
        }
        Expr::Builtin { name, args, .. }
            if effect_inference::infer_expr_effects(expr).is_pure() =>
        {
            expr_key_from_args(format!("builtin:{name:?}"), args, expr)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            let condition = pure_expr_key(condition)?;
            let then_expr = pure_expr_key(then_expr)?;
            let else_expr = pure_expr_key(else_expr)?;
            let mut vars = condition.vars.clone();
            vars.extend(then_expr.vars.iter().cloned());
            vars.extend(else_expr.vars.iter().cloned());
            Some(ExprKey {
                repr: format!(
                    "if:{}?{}:{}",
                    condition.repr, then_expr.repr, else_expr.repr
                ),
                vars,
                cost: condition.cost + then_expr.cost + else_expr.cost + 1,
                expr: expr.clone(),
            })
        }
        _ => None,
    }
}

fn expr_key_from_args(prefix: String, args: &[Expr], expr: &Expr) -> Option<ExprKey> {
    let mut vars = BTreeSet::new();
    let mut cost = 1;
    let mut arg_reprs = Vec::with_capacity(args.len());
    for arg in args {
        let arg_key = pure_expr_key(arg)?;
        vars.extend(arg_key.vars.iter().cloned());
        cost += arg_key.cost;
        arg_reprs.push(arg_key.repr);
    }
    Some(ExprKey {
        repr: format!("{prefix}({})", arg_reprs.join(", ")),
        vars,
        cost,
        expr: expr.clone(),
    })
}

fn pure_literal(lit: &Literal) -> bool {
    matches!(
        lit,
        Literal::Int(_)
            | Literal::Int128(_)
            | Literal::Float(_)
            | Literal::Float32(_)
            | Literal::Float16(_)
            | Literal::Bool(_)
            | Literal::Char(_)
            | Literal::Nothing
            | Literal::Missing
    )
}

fn pure_unary_op(op: UnaryOp) -> bool {
    matches!(op, UnaryOp::Neg | UnaryOp::Not | UnaryOp::Pos)
}

fn pure_nothrow_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Egal
            | BinaryOp::NotEgal
            | BinaryOp::Subtype
            | BinaryOp::And
            | BinaryOp::Or
    )
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::Literal(_, span)
        | Expr::Var(_, span)
        | Expr::BinaryOp { span, .. }
        | Expr::UnaryOp { span, .. }
        | Expr::Call { span, .. }
        | Expr::Builtin { span, .. }
        | Expr::ArrayLiteral { span, .. }
        | Expr::TypedEmptyArray { span, .. }
        | Expr::Index { span, .. }
        | Expr::Range { span, .. }
        | Expr::Comprehension { span, .. }
        | Expr::MultiComprehension { span, .. }
        | Expr::Generator { span, .. }
        | Expr::SliceAll { span, .. }
        | Expr::FieldAccess { span, .. }
        | Expr::FunctionRef { span, .. }
        | Expr::TupleLiteral { span, .. }
        | Expr::NamedTupleLiteral { span, .. }
        | Expr::Pair { span, .. }
        | Expr::DictLiteral { span, .. }
        | Expr::LetBlock { span, .. }
        | Expr::StringConcat { span, .. }
        | Expr::ModuleCall { span, .. }
        | Expr::Ternary { span, .. }
        | Expr::New { span, .. }
        | Expr::DynamicTypeConstruct { span, .. }
        | Expr::QuoteLiteral { span, .. }
        | Expr::AssignExpr { span, .. }
        | Expr::ReturnExpr { span, .. }
        | Expr::BreakExpr { span }
        | Expr::ContinueExpr { span } => *span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string(), sp())
    }

    fn int(value: i64) -> Expr {
        Expr::Literal(Literal::Int(value), sp())
    }

    fn nothing() -> Expr {
        Expr::Literal(Literal::Nothing, sp())
    }

    fn add(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(left),
            right: Box::new(right),
            span: sp(),
        }
    }

    fn mul(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            op: BinaryOp::Mul,
            left: Box::new(left),
            right: Box::new(right),
            span: sp(),
        }
    }

    fn egal(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(left),
            right: Box::new(right),
            span: sp(),
        }
    }

    fn call(function: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            function: function.to_string(),
            args,
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: sp(),
        }
    }

    fn length_call(name: &str) -> Expr {
        call("length", vec![var(name)])
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: sp() }
    }

    fn expr_contains_var_prefix(expr: &Expr, prefix: &str) -> bool {
        match expr {
            Expr::Var(name, _) => name.starts_with(prefix),
            Expr::UnaryOp { operand, .. }
            | Expr::FieldAccess {
                object: operand, ..
            } => expr_contains_var_prefix(operand, prefix),
            Expr::BinaryOp { left, right, .. } => {
                expr_contains_var_prefix(left, prefix) || expr_contains_var_prefix(right, prefix)
            }
            Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
                args.iter().any(|arg| expr_contains_var_prefix(arg, prefix))
                    || kwargs
                        .iter()
                        .any(|(_, value)| expr_contains_var_prefix(value, prefix))
            }
            Expr::Builtin { args, .. } | Expr::New { args, .. } => {
                args.iter().any(|arg| expr_contains_var_prefix(arg, prefix))
            }
            Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => elements
                .iter()
                .any(|element| expr_contains_var_prefix(element, prefix)),
            Expr::NamedTupleLiteral { fields, .. } => fields
                .iter()
                .any(|(_, value)| expr_contains_var_prefix(value, prefix)),
            Expr::Pair { key, value, .. } => {
                expr_contains_var_prefix(key, prefix) || expr_contains_var_prefix(value, prefix)
            }
            Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
                expr_contains_var_prefix(key, prefix) || expr_contains_var_prefix(value, prefix)
            }),
            Expr::Index { array, indices, .. } => {
                expr_contains_var_prefix(array, prefix)
                    || indices
                        .iter()
                        .any(|index| expr_contains_var_prefix(index, prefix))
            }
            Expr::Range {
                start, step, stop, ..
            } => {
                expr_contains_var_prefix(start, prefix)
                    || step
                        .as_ref()
                        .is_some_and(|step| expr_contains_var_prefix(step, prefix))
                    || expr_contains_var_prefix(stop, prefix)
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                expr_contains_var_prefix(condition, prefix)
                    || expr_contains_var_prefix(then_expr, prefix)
                    || expr_contains_var_prefix(else_expr, prefix)
            }
            _ => false,
        }
    }

    #[test]
    fn straight_line_cse_reuses_prior_local_issue_5185() {
        let input = block(vec![
            Stmt::Assign {
                var: "a".to_string(),
                value: add(var("x"), int(1)),
                span: sp(),
            },
            Stmt::Assign {
                var: "b".to_string(),
                value: add(var("x"), int(1)),
                span: sp(),
            },
        ]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert!(matches!(
            &output.stmts[1],
            Stmt::Assign {
                var,
                value: Expr::Var(source, _),
                ..
            } if var == "b" && source == "a"
        ));
    }

    #[test]
    fn straight_line_cse_invalidates_mutated_inputs_issue_5185() {
        let input = block(vec![
            Stmt::Assign {
                var: "a".to_string(),
                value: add(var("x"), int(1)),
                span: sp(),
            },
            Stmt::Assign {
                var: "x".to_string(),
                value: int(10),
                span: sp(),
            },
            Stmt::Assign {
                var: "b".to_string(),
                value: add(var("x"), int(1)),
                span: sp(),
            },
        ]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert!(matches!(
            &output.stmts[2],
            Stmt::Assign {
                value: Expr::BinaryOp { .. },
                ..
            }
        ));
    }

    #[test]
    fn straight_line_cse_reuses_pure_call_issue_5185() {
        let input = block(vec![
            Stmt::Assign {
                var: "a".to_string(),
                value: length_call("xs"),
                span: sp(),
            },
            Stmt::Assign {
                var: "b".to_string(),
                value: length_call("xs"),
                span: sp(),
            },
        ]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert!(matches!(
            &output.stmts[1],
            Stmt::Assign {
                var,
                value: Expr::Var(source, _),
                ..
            } if var == "b" && source == "a"
        ));
    }

    #[test]
    fn loop_invariant_expression_is_hoisted_issue_5185() {
        let input = block(vec![Stmt::For {
            var: "i".to_string(),
            start: int(1),
            end: int(3),
            step: None,
            body: block(vec![Stmt::Assign {
                var: "y".to_string(),
                value: add(var("limit"), int(1)),
                span: sp(),
            }]),
            span: sp(),
        }]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert_eq!(output.stmts.len(), 2);
        assert!(matches!(
            &output.stmts[0],
            Stmt::Assign {
                var,
                value: Expr::BinaryOp { .. },
                ..
            } if var.starts_with(HOIST_TEMP_PREFIX)
        ));
        assert!(matches!(
            &output.stmts[1],
            Stmt::For {
                body: Block { stmts, .. },
                ..
            } if matches!(
                &stmts[0],
                Stmt::Assign {
                    value: Expr::Var(name, _),
                    ..
                } if name.starts_with(HOIST_TEMP_PREFIX)
            )
        ));
    }

    #[test]
    fn loop_invariant_arithmetic_is_not_hoisted_issue_5618() {
        let input = block(vec![Stmt::While {
            condition: egal(var("x"), nothing()),
            body: block(vec![Stmt::Return {
                value: Some(add(var("x"), int(1))),
                span: sp(),
            }]),
            span: sp(),
        }]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert_eq!(output.stmts.len(), 1);
        assert!(matches!(&output.stmts[0], Stmt::While { .. }));
    }

    #[test]
    fn loop_invariant_pure_call_is_hoisted_issue_5185() {
        let input = block(vec![Stmt::For {
            var: "i".to_string(),
            start: int(1),
            end: int(3),
            step: None,
            body: block(vec![Stmt::Assign {
                var: "n".to_string(),
                value: length_call("xs"),
                span: sp(),
            }]),
            span: sp(),
        }]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert_eq!(output.stmts.len(), 2);
        assert!(matches!(
            &output.stmts[0],
            Stmt::Assign {
                var,
                value: Expr::Call { function, .. },
                ..
            } if var.starts_with(HOIST_TEMP_PREFIX) && function == "length"
        ));
    }

    #[test]
    fn outer_licm_skips_nested_hoist_temp_dependencies_issue_5592() {
        let invariant = add(var("limit"), int(1));
        let input = block(vec![Stmt::For {
            var: "i".to_string(),
            start: int(1),
            end: int(3),
            step: None,
            body: block(vec![Stmt::For {
                var: "j".to_string(),
                start: int(1),
                end: int(3),
                step: None,
                body: block(vec![Stmt::Assign {
                    var: "y".to_string(),
                    value: mul(invariant.clone(), invariant),
                    span: sp(),
                }]),
                span: sp(),
            }]),
            span: sp(),
        }]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        for stmt in output
            .stmts
            .iter()
            .take_while(|stmt| !matches!(stmt, Stmt::For { var, .. } if var == "i"))
        {
            let Stmt::Assign { value, .. } = stmt else {
                continue;
            };
            assert!(
                !expr_contains_var_prefix(value, HOIST_TEMP_PREFIX),
                "outer LICM hoisted a value that depends on a nested hoist temp: {value:?}"
            );
        }
    }

    #[test]
    fn loop_invariant_call_skips_mutated_argument_issue_5185() {
        let input = block(vec![Stmt::For {
            var: "i".to_string(),
            start: int(1),
            end: int(3),
            step: None,
            body: block(vec![
                Stmt::Expr {
                    expr: call("push!", vec![var("xs"), var("i")]),
                    span: sp(),
                },
                Stmt::Assign {
                    var: "n".to_string(),
                    value: length_call("xs"),
                    span: sp(),
                },
            ]),
            span: sp(),
        }]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert_eq!(output.stmts.len(), 1);
        assert!(matches!(&output.stmts[0], Stmt::For { .. }));
    }

    #[test]
    fn loop_invariant_hoist_skips_loop_var_and_mutated_inputs_issue_5185() {
        let input = block(vec![Stmt::For {
            var: "i".to_string(),
            start: int(1),
            end: int(3),
            step: None,
            body: block(vec![
                Stmt::Assign {
                    var: "a".to_string(),
                    value: add(var("i"), int(1)),
                    span: sp(),
                },
                Stmt::Assign {
                    var: "limit".to_string(),
                    value: add(var("limit"), int(1)),
                    span: sp(),
                },
                Stmt::Assign {
                    var: "b".to_string(),
                    value: add(var("limit"), int(2)),
                    span: sp(),
                },
            ]),
            span: sp(),
        }]);

        let output = IrOptimizer { next_temp: 0 }.optimize_block(&input);

        assert_eq!(output.stmts.len(), 1);
        assert!(matches!(&output.stmts[0], Stmt::For { .. }));
    }
}
