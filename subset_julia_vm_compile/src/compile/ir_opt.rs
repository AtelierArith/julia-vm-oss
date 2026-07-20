//! IR-level pure expression consumers for Effects-driven optimizations.
//!
//! This pass covers the conservative slice needed by Issue #5185:
//! - straight-line CSE reuses an earlier local binding for the same pure value;
//! - loop-invariant pure expressions are hoisted into a generated temp before
//!   `for`/`foreach`/`while` loops when they do not depend on loop-mutated vars.

use crate::compile::effects::inference as effect_inference;
use crate::compile::effects::{EffectBit, Effects};
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
    let base_function_count = base_function_count.min(program.functions.len());
    let assumed_effects = collect_assumed_effects(program, base_function_count);
    let mut optimizer = IrOptimizer::new(assumed_effects);

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
    assumed_effects: HashMap<String, Effects>,
}

impl IrOptimizer {
    fn new(assumed_effects: HashMap<String, Effects>) -> Self {
        Self {
            next_temp: 0,
            assumed_effects,
        }
    }

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
                    if let Some(key) = pure_expr_key(&value, &self.assumed_effects) {
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
                    if pure_expr_key(&expr, &self.assumed_effects).is_none() {
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
        collect_loop_candidates_block(body, mutated, &self.assumed_effects, &mut candidates);

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
            let value = replace_expr_keys(&key.expr, &replacements, &self.assumed_effects);
            replacements.insert(key.repr, temp.clone());
            hoists.push(Stmt::Assign {
                var: temp,
                value,
                span,
            });
        }

        let body = replace_block_expr_keys(body, &replacements, &self.assumed_effects);
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
                function: *function,
                args: args
                    .iter()
                    .map(|arg| self.optimize_expr_with_cse(arg, cse))
                    .collect(),
                kwargs: kwargs
                    .iter()
                    .map(|(name, value)| (*name, self.optimize_expr_with_cse(value, cse)))
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
                module: *module,
                function: *function,
                args: args
                    .iter()
                    .map(|arg| self.optimize_expr_with_cse(arg, cse))
                    .collect(),
                kwargs: kwargs
                    .iter()
                    .map(|(name, value)| (*name, self.optimize_expr_with_cse(value, cse)))
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

        let Some(key) = pure_expr_key(&expr, &self.assumed_effects) else {
            return expr;
        };
        if key.cost <= 1 {
            return expr;
        }
        cse.get(&key.repr)
            .map(|entry| Expr::Var(entry.local.clone().into(), expr_span(&expr)))
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

fn collect_assumed_effects(
    program: &Program,
    base_function_count: usize,
) -> HashMap<String, Effects> {
    let mut effects = HashMap::new();
    for func in &program.functions[base_function_count..] {
        collect_function_assumed_effects(func, None, &mut effects);
    }
    for module in &program.modules {
        collect_module_assumed_effects(module, None, &mut effects);
    }
    effects
}

fn collect_module_assumed_effects(
    module: &Module,
    parent_path: Option<&str>,
    out: &mut HashMap<String, Effects>,
) {
    let module_path = parent_path
        .map(|parent| format!("{parent}.{}", module.name))
        .unwrap_or_else(|| module.name.clone());
    for func in &module.functions {
        collect_function_assumed_effects(func, Some(&module_path), out);
    }
    for submodule in &module.submodules {
        collect_module_assumed_effects(submodule, Some(&module_path), out);
    }
}

fn collect_function_assumed_effects(
    func: &Function,
    module_path: Option<&str>,
    out: &mut HashMap<String, Effects>,
) {
    let mut purity = 0u16;
    for stmt in &func.body.stmts {
        let Stmt::Meta { annotation, .. } = stmt else {
            break;
        };
        if annotation.name == "assume_effects" {
            for arg in &annotation.args {
                purity |= super::assume_effects_purity_bits(arg);
            }
        }
    }

    let Some(effects) = effects_from_assume_effects_purity(purity) else {
        return;
    };
    if !effects.is_pure() || !effects.terminates {
        return;
    }

    out.insert(func.name.clone(), effects);
    if let Some(module_path) = module_path {
        out.insert(format!("{module_path}.{}", func.name), effects);
    }
}

fn effects_from_assume_effects_purity(purity: u16) -> Option<Effects> {
    if purity == 0 {
        return None;
    }
    Some(Effects {
        consistent: effect_bit_from_purity(purity, 1),
        effect_free: effect_bit_from_purity(purity, 2),
        nothrow: purity & 4 != 0,
        terminates: purity & (8 | 16) != 0,
        notaskstate: purity & 32 != 0,
        inaccessiblememonly: purity & 64 != 0,
        // `@assume_effects` is a user-authored binary assertion (present or
        // its negation), so it only ever asserts AlwaysTrue/AlwaysFalse, never
        // the `Conditional`/`NOUB_IF_NOINBOUNDS`-equivalent refinement (Issue
        // #9496) — matching how `consistent`/`effect_free` already decode
        // through `effect_bit_from_purity` above.
        noub: effect_bit_from_purity(purity, 128),
        nonoverlayed: true,
        nortcall: purity & 1024 != 0,
    })
}

fn effect_bit_from_purity(purity: u16, mask: u16) -> EffectBit {
    if purity & mask != 0 {
        EffectBit::AlwaysTrue
    } else {
        EffectBit::AlwaysFalse
    }
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
            out.insert(var.to_string());
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
            out.insert(var.to_string());
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
                out.insert(var.to_string());
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
                        out.insert(name.to_string());
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
        | Expr::Convert { operand, .. }
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
            out.insert(var.to_string());
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
    assumed_effects: &HashMap<String, Effects>,
    out: &mut BTreeMap<String, ExprKey>,
) {
    for stmt in &block.stmts {
        collect_loop_candidates_stmt(stmt, mutated, assumed_effects, out);
    }
}

fn collect_loop_candidates_stmt(
    stmt: &Stmt,
    mutated: &HashSet<String>,
    assumed_effects: &HashMap<String, Effects>,
    out: &mut BTreeMap<String, ExprKey>,
) {
    match stmt {
        Stmt::Assign { value, .. } | Stmt::Expr { expr: value, .. } => {
            collect_loop_candidates_expr(value, mutated, assumed_effects, out);
        }
        Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::AddAssign { value, .. }
        | Stmt::Test {
            condition: value, ..
        } => {
            collect_loop_candidates_expr(value, mutated, assumed_effects, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_loop_candidates_expr(condition, mutated, assumed_effects, out);
            collect_loop_candidates_block(then_branch, mutated, assumed_effects, out);
            if let Some(block) = else_branch {
                collect_loop_candidates_block(block, mutated, assumed_effects, out);
            }
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            collect_loop_candidates_block(block, mutated, assumed_effects, out);
        }
        _ => {}
    }
}

fn collect_loop_candidates_expr(
    expr: &Expr,
    mutated: &HashSet<String>,
    assumed_effects: &HashMap<String, Effects>,
    out: &mut BTreeMap<String, ExprKey>,
) {
    match expr {
        Expr::UnaryOp { operand, .. } => {
            collect_loop_candidates_expr(operand, mutated, assumed_effects, out)
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_loop_candidates_expr(left, mutated, assumed_effects, out);
            collect_loop_candidates_expr(right, mutated, assumed_effects, out);
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_loop_candidates_expr(arg, mutated, assumed_effects, out);
            }
            for (_, value) in kwargs {
                collect_loop_candidates_expr(value, mutated, assumed_effects, out);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_loop_candidates_expr(arg, mutated, assumed_effects, out);
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_loop_candidates_expr(element, mutated, assumed_effects, out);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_loop_candidates_expr(condition, mutated, assumed_effects, out);
            collect_loop_candidates_expr(then_expr, mutated, assumed_effects, out);
            collect_loop_candidates_expr(else_expr, mutated, assumed_effects, out);
        }
        _ => {}
    }

    let Some(key) = pure_expr_key(expr, assumed_effects) else {
        return;
    };
    if key.cost <= 1 || key.vars.iter().any(|var| mutated.contains(var)) {
        return;
    }
    out.entry(key.repr.clone()).or_insert(key);
}

fn replace_block_expr_keys(
    block: &Block,
    replacements: &HashMap<String, String>,
    assumed_effects: &HashMap<String, Effects>,
) -> Block {
    Block {
        stmts: block
            .stmts
            .iter()
            .map(|stmt| replace_stmt_expr_keys(stmt, replacements, assumed_effects))
            .collect(),
        span: block.span,
    }
}

fn replace_stmt_expr_keys(
    stmt: &Stmt,
    replacements: &HashMap<String, String>,
    assumed_effects: &HashMap<String, Effects>,
) -> Stmt {
    match stmt {
        Stmt::Assign { var, value, span } => Stmt::Assign {
            var: var.clone(),
            value: replace_expr_keys(value, replacements, assumed_effects),
            span: *span,
        },
        Stmt::Expr { expr, span } => Stmt::Expr {
            expr: replace_expr_keys(expr, replacements, assumed_effects),
            span: *span,
        },
        Stmt::Return { value, span } => Stmt::Return {
            value: value
                .as_ref()
                .map(|expr| replace_expr_keys(expr, replacements, assumed_effects)),
            span: *span,
        },
        Stmt::AddAssign { var, value, span } => Stmt::AddAssign {
            var: var.clone(),
            value: replace_expr_keys(value, replacements, assumed_effects),
            span: *span,
        },
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => Stmt::If {
            condition: replace_expr_keys(condition, replacements, assumed_effects),
            then_branch: replace_block_expr_keys(then_branch, replacements, assumed_effects),
            else_branch: else_branch
                .as_ref()
                .map(|block| replace_block_expr_keys(block, replacements, assumed_effects)),
            span: *span,
        },
        Stmt::Block(block) => Stmt::Block(replace_block_expr_keys(
            block,
            replacements,
            assumed_effects,
        )),
        _ => stmt.clone(),
    }
}

fn replace_expr_keys(
    expr: &Expr,
    replacements: &HashMap<String, String>,
    assumed_effects: &HashMap<String, Effects>,
) -> Expr {
    if let Some(key) = pure_expr_key(expr, assumed_effects) {
        if let Some(temp) = replacements.get(&key.repr) {
            return Expr::Var(temp.clone().into(), expr_span(expr));
        }
    }

    match expr {
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op: *op,
            operand: Box::new(replace_expr_keys(operand, replacements, assumed_effects)),
            span: *span,
        },
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Expr::BinaryOp {
            op: *op,
            left: Box::new(replace_expr_keys(left, replacements, assumed_effects)),
            right: Box::new(replace_expr_keys(right, replacements, assumed_effects)),
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
            function: *function,
            args: args
                .iter()
                .map(|arg| replace_expr_keys(arg, replacements, assumed_effects))
                .collect(),
            kwargs: kwargs
                .iter()
                .map(|(name, value)| {
                    (
                        *name,
                        replace_expr_keys(value, replacements, assumed_effects),
                    )
                })
                .collect(),
            splat_mask: splat_mask.clone(),
            kwargs_splat_mask: kwargs_splat_mask.clone(),
            span: *span,
        },
        Expr::Builtin { name, args, span } => Expr::Builtin {
            name: *name,
            args: args
                .iter()
                .map(|arg| replace_expr_keys(arg, replacements, assumed_effects))
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
            module: *module,
            function: *function,
            args: args
                .iter()
                .map(|arg| replace_expr_keys(arg, replacements, assumed_effects))
                .collect(),
            kwargs: kwargs
                .iter()
                .map(|(name, value)| {
                    (
                        *name,
                        replace_expr_keys(value, replacements, assumed_effects),
                    )
                })
                .collect(),
            splat_mask: splat_mask.clone(),
            kwargs_splat_mask: kwargs_splat_mask.clone(),
            span: *span,
        },
        Expr::TupleLiteral { elements, span } => Expr::TupleLiteral {
            elements: elements
                .iter()
                .map(|expr| replace_expr_keys(expr, replacements, assumed_effects))
                .collect(),
            span: *span,
        },
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            span,
        } => Expr::Ternary {
            condition: Box::new(replace_expr_keys(condition, replacements, assumed_effects)),
            then_expr: Box::new(replace_expr_keys(then_expr, replacements, assumed_effects)),
            else_expr: Box::new(replace_expr_keys(else_expr, replacements, assumed_effects)),
            span: *span,
        },
        _ => expr.clone(),
    }
}

fn pure_expr_key(expr: &Expr, assumed_effects: &HashMap<String, Effects>) -> Option<ExprKey> {
    match expr {
        Expr::Literal(lit, _) if pure_literal(lit) => Some(ExprKey {
            repr: format!("lit:{lit:?}"),
            vars: BTreeSet::new(),
            cost: 1,
            expr: expr.clone(),
        }),
        Expr::Var(name, _) => Some(ExprKey {
            repr: format!("var:{name}"),
            vars: BTreeSet::from([name.to_string()]),
            cost: 1,
            expr: expr.clone(),
        }),
        Expr::UnaryOp { op, operand, .. } if pure_unary_op(*op) => {
            let key = pure_expr_key(operand, assumed_effects)?;
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
            let left = pure_expr_key(left, assumed_effects)?;
            let right = pure_expr_key(right, assumed_effects)?;
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
            && call_effects_are_pure(function, expr, assumed_effects) =>
        {
            expr_key_from_args(format!("call:{function}"), args, expr, assumed_effects)
        }
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if kwargs.is_empty()
            && splat_mask.iter().all(|splat| !*splat)
            && kwargs_splat_mask.iter().all(|splat| !*splat)
            && module_call_effects_are_pure(module, function, assumed_effects) =>
        {
            expr_key_from_args(
                format!("module-call:{module}.{function}"),
                args,
                expr,
                assumed_effects,
            )
        }
        Expr::Builtin { name, args, .. }
            if effect_inference::infer_expr_effects(expr).is_pure() =>
        {
            expr_key_from_args(format!("builtin:{name:?}"), args, expr, assumed_effects)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            let condition = pure_expr_key(condition, assumed_effects)?;
            let then_expr = pure_expr_key(then_expr, assumed_effects)?;
            let else_expr = pure_expr_key(else_expr, assumed_effects)?;
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

fn call_effects_are_pure(
    function: &str,
    expr: &Expr,
    assumed_effects: &HashMap<String, Effects>,
) -> bool {
    assumed_effects.get(function).is_some_and(Effects::is_pure)
        || effect_inference::infer_expr_effects(expr).is_pure()
}

fn module_call_effects_are_pure(
    module: &str,
    function: &str,
    assumed_effects: &HashMap<String, Effects>,
) -> bool {
    assumed_effects
        .get(&format!("{module}.{function}"))
        .is_some_and(Effects::is_pure)
}

fn expr_key_from_args(
    prefix: String,
    args: &[Expr],
    expr: &Expr,
    assumed_effects: &HashMap<String, Effects>,
) -> Option<ExprKey> {
    let mut vars = BTreeSet::new();
    let mut cost = 1;
    let mut arg_reprs = Vec::with_capacity(args.len());
    for arg in args {
        let arg_key = pure_expr_key(arg, assumed_effects)?;
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
        | Expr::ContinueExpr { span }
        | Expr::Convert { span, .. } => *span,
    }
}

#[cfg(test)]
mod tests;
