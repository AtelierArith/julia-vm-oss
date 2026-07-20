//! Small pure-function IR inlining.
//!
//! This pass is deliberately narrow. It inlines only user-defined top-level
//! functions that have a single visible method, no varargs/kwargs/type params,
//! no return conversion, and a small side-effect-free expression body. Arguments
//! are first bound in a generated let block so each call argument is evaluated
//! exactly once before the callee body is expanded.

use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Module, Program, Stmt};
use crate::span::Span;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

const MAX_INLINE_EXPR_COST: usize = 12;
const INLINE_TEMP_PREFIX: &str = "__sjulia_inline_arg_";

#[derive(Debug, Clone)]
struct InlineCandidate {
    params: Vec<String>,
    body: Expr,
    /// Source position where this top-level method becomes visible. `None` for
    /// a prior REPL definition replayed into a later full compile, whose byte
    /// offsets belong to a different input fragment and are not comparable.
    visible_from_source_start: Option<usize>,
}

#[cfg(test)]
pub(super) fn inline_small_pure_functions(
    program: &Program,
    base_function_count: usize,
) -> Program {
    inline_small_pure_functions_cow(program, base_function_count, None).into_owned()
}

pub(super) fn inline_small_pure_functions_cow(
    program: &Program,
    base_function_count: usize,
    current_source_function_count: Option<usize>,
) -> Cow<'_, Program> {
    let candidates =
        collect_inline_candidates(program, base_function_count, current_source_function_count);
    if candidates.is_empty() {
        return Cow::Borrowed(program);
    }

    let mut inliner = Inliner {
        candidates,
        next_temp: 0,
        local_scopes: Vec::new(),
        module_stack: Vec::new(),
    };
    Cow::Owned(inliner.inline_program(program, base_function_count))
}

fn collect_inline_candidates(
    program: &Program,
    base_function_count: usize,
    current_source_function_count: Option<usize>,
) -> HashMap<String, InlineCandidate> {
    let base_names: HashSet<&str> = program
        .functions
        .iter()
        .take(base_function_count)
        .map(|func| func.name.as_str())
        .collect();
    // A REPL fragment may interpose marker-less helpers before or between its
    // source methods. Count only non-helper methods toward the current-input
    // source budget; merged prior methods after that budget stay immediately
    // visible to the inliner (Issue #9784).
    let mut selected_source_function_count = 0usize;
    let mut user_functions: Vec<(String, &Function, Option<usize>)> = program
        .functions
        .iter()
        .skip(base_function_count)
        .map(|func| {
            let source_ordered = match current_source_function_count {
                None => true,
                Some(_) if is_markerless_lowered_function(func) => false,
                Some(current_count) => {
                    let selected = selected_source_function_count < current_count;
                    selected_source_function_count += 1;
                    selected
                }
            };
            (
                func.name.clone(),
                func.as_ref(),
                source_ordered.then_some(func.span.start),
            )
        })
        .collect();
    for module in &program.modules {
        collect_module_functions(module, "", &mut user_functions);
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (key, _, _) in &user_functions {
        *counts.entry(key.clone()).or_insert(0) += 1;
    }

    let callable_typeof_blocklist = callable_typeof_inline_blocklist(program, &user_functions);
    let runtime_eval_blocklist = runtime_eval_inline_blocklist(program);
    let mut candidates = HashMap::new();
    for (key, func, visible_from_source_start) in user_functions {
        if base_names.contains(func.name.as_str())
            || counts[&key] != 1
            || callable_typeof_blocklist.contains(&key)
            || callable_typeof_blocklist.contains(func.name.as_str())
            || runtime_eval_blocklist.contains(&key)
            || runtime_eval_blocklist.contains(func.name.as_str())
        {
            continue;
        }
        if let Some(mut candidate) = candidate_for_function(func) {
            candidate.visible_from_source_start = visible_from_source_start;
            candidates.insert(key, candidate);
        }
    }
    candidates
}

fn runtime_eval_inline_blocklist(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_runtime_eval_names_block(&program.main, &mut names);
    for func in &program.functions {
        collect_runtime_eval_names_block(&func.body, &mut names);
    }
    for module in &program.modules {
        collect_runtime_eval_names_module(module, &mut names);
    }
    names
}

fn collect_runtime_eval_names_module(module: &Module, names: &mut HashSet<String>) {
    for func in &module.functions {
        collect_runtime_eval_names_block(&func.body, names);
    }
    for submodule in &module.submodules {
        collect_runtime_eval_names_module(submodule, names);
    }
}

fn collect_runtime_eval_names_block(block: &Block, names: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::EvalFunctionDef { func, .. } => {
                names.insert(func.name.clone());
                collect_runtime_eval_names_block(&func.body, names);
            }
            Stmt::Block(block)
            | Stmt::For { body: block, .. }
            | Stmt::ForEach { body: block, .. }
            | Stmt::ForEachTuple { body: block, .. }
            | Stmt::While { body: block, .. }
            | Stmt::Timed { body: block, .. }
            | Stmt::TestSet { body: block, .. } => collect_runtime_eval_names_block(block, names),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_runtime_eval_names_block(then_branch, names);
                if let Some(else_branch) = else_branch {
                    collect_runtime_eval_names_block(else_branch, names);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_runtime_eval_names_block(try_block, names);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_runtime_eval_names_block(block, names);
                }
            }
            Stmt::FunctionDef { func, .. } => collect_runtime_eval_names_block(&func.body, names),
            _ => {}
        }
    }
}

fn callable_typeof_inline_blocklist(
    program: &Program,
    user_functions: &[(String, &Function, Option<usize>)],
) -> HashSet<String> {
    let callable_names: HashSet<String> = user_functions
        .iter()
        .flat_map(|(key, func, _)| [key.clone(), func.name.clone()])
        .collect();
    let mut callable_bindings: HashMap<String, String> = callable_names
        .iter()
        .map(|name| (name.clone(), name.clone()))
        .collect();
    let mut blocklist = HashSet::new();

    for stmt in &program.main.stmts {
        let Stmt::Assign { var, value, .. } = stmt else {
            continue;
        };

        if let Some(callable_name) = typeof_callable_target(value, &callable_bindings) {
            blocklist.insert(callable_name);
            continue;
        }

        if let Some(callable_name) = callable_binding_target(value, &callable_bindings) {
            callable_bindings.insert(var.clone(), callable_name);
        }
    }

    blocklist
}

fn callable_binding_target(
    expr: &Expr,
    callable_bindings: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        Expr::FunctionRef { name, .. } => callable_bindings.get(name.as_str()).cloned(),
        Expr::Var(name, _) => callable_bindings.get(name.as_str()).cloned(),
        _ => None,
    }
}

fn typeof_callable_target(
    expr: &Expr,
    callable_bindings: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        Expr::Builtin {
            name: crate::ir::core::BuiltinOp::TypeOf,
            args,
            ..
        } => args
            .first()
            .and_then(|arg| callable_binding_target(arg, callable_bindings)),
        Expr::Call { function, args, .. } if function == "typeof" => args
            .first()
            .and_then(|arg| callable_binding_target(arg, callable_bindings)),
        _ => None,
    }
}

fn collect_module_functions<'a>(
    module: &'a Module,
    prefix: &str,
    functions: &mut Vec<(String, &'a Function, Option<usize>)>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    for func in &module.functions {
        functions.push((format!("{}.{}", module_path, func.name), func, None));
    }
    for submodule in &module.submodules {
        collect_module_functions(submodule, &module_path, functions);
    }
}

fn candidate_for_function(func: &Function) -> Option<InlineCandidate> {
    if !func.kwparams.is_empty()
        || !func.type_params.is_empty()
        || func.return_type.is_some()
        || func.params.iter().any(|param| param.is_varargs)
        || leading_noinline(func)
        || leading_generated(func)
    {
        return None;
    }
    if func
        .params
        .iter()
        .any(|param| !matches!(param.effective_type(), crate::types::JuliaType::Any))
    {
        // Issue #5984: this IR pass runs before method-table dispatch. Inlining a
        // typed single method like `h(::String)` into `g(x::Any)=h(x)` erases the
        // runtime MethodError path for non-String values.
        return None;
    }

    let expr = single_expression_body(&func.body)?;
    let allowed_vars: HashSet<&str> = func
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect();
    if !is_small_pure_expr(expr, &allowed_vars, 0)? {
        return None;
    }

    Some(InlineCandidate {
        params: func.params.iter().map(|param| param.name.clone()).collect(),
        body: expr.clone(),
        visible_from_source_start: None,
    })
}

fn single_expression_body(body: &Block) -> Option<&Expr> {
    let stmts: Vec<&Stmt> = body
        .stmts
        .iter()
        .filter(|stmt| !matches!(stmt, Stmt::Meta { .. }))
        .collect();
    match stmts.as_slice() {
        [Stmt::Return {
            value: Some(expr), ..
        }] => Some(expr),
        [Stmt::Expr { expr, .. }] => Some(expr),
        _ => None,
    }
}

fn leading_noinline(func: &Function) -> bool {
    for stmt in &func.body.stmts {
        let Stmt::Meta { annotation, .. } = stmt else {
            break;
        };
        match annotation.name.as_str() {
            "noinline" => return true,
            "inline" | "propagate_inbounds" => return false,
            _ => {}
        }
    }
    false
}

fn leading_generated(func: &Function) -> bool {
    for stmt in &func.body.stmts {
        let Stmt::Meta { annotation, .. } = stmt else {
            break;
        };
        if annotation.name == "generated" {
            return true;
        }
    }
    false
}

fn is_small_pure_expr(expr: &Expr, allowed_vars: &HashSet<&str>, cost: usize) -> Option<bool> {
    if cost > MAX_INLINE_EXPR_COST {
        return None;
    }

    match expr {
        Expr::Literal(lit, _) => Some(is_pure_literal(lit)),
        Expr::Var(name, _) => Some(allowed_vars.contains(name.as_str())),
        Expr::UnaryOp { operand, .. } => is_small_pure_expr(operand, allowed_vars, cost + 1),
        Expr::BinaryOp {
            op, left, right, ..
        } if is_pure_binary_op(*op) => Some(
            is_small_pure_expr(left, allowed_vars, cost + 1)?
                && is_small_pure_expr(right, allowed_vars, cost + 1)?,
        ),
        Expr::TupleLiteral { elements, .. } => {
            for (offset, element) in elements.iter().enumerate() {
                let current_cost = cost + 1 + offset;
                if !is_small_pure_expr(element, allowed_vars, current_cost)? {
                    return Some(false);
                }
            }
            Some(true)
        }
        // Do not inline branchy expressions until this pass can also eliminate
        // statically unreachable arms. Otherwise `f(nothing)` may inline a
        // nullable ternary and still type-check the unreachable arithmetic arm
        // against `Nothing` (Issue #5589).
        Expr::Ternary { .. } => Some(false),
        _ => Some(false),
    }
}

fn is_pure_literal(lit: &Literal) -> bool {
    matches!(
        lit,
        Literal::Int(_)
            | Literal::Int128(_)
            | Literal::BigInt(_)
            | Literal::BigFloat(_)
            | Literal::Float(_)
            | Literal::Float32(_)
            | Literal::Float16(_)
            | Literal::Bool(_)
            | Literal::Str(_)
            | Literal::Char(_)
            | Literal::Nothing
            | Literal::Missing
            | Literal::Undef
            | Literal::Symbol(_)
    )
}

fn is_pure_binary_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::IntDiv
            | BinaryOp::Mod
            | BinaryOp::Pow
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

struct Inliner {
    candidates: HashMap<String, InlineCandidate>,
    next_temp: usize,
    local_scopes: Vec<HashSet<String>>,
    module_stack: Vec<String>,
}

impl Inliner {
    fn inline_program(&mut self, program: &Program, base_function_count: usize) -> Program {
        let base_function_count = base_function_count.min(program.functions.len());
        let mut functions = Vec::with_capacity(program.functions.len());
        functions.extend_from_slice(&program.functions[..base_function_count]);
        functions.extend(
            program.functions[base_function_count..]
                .iter()
                .map(|func| std::sync::Arc::new(self.inline_function(func))),
        );

        Program {
            abstract_types: program.abstract_types.clone(),
            primitive_types: program.primitive_types.clone(),
            type_aliases: program.type_aliases.clone(),
            structs: program.structs.clone(),
            functions,
            base_function_count: program.base_function_count,
            modules: program
                .modules
                .iter()
                .map(|module| self.inline_module(module))
                .collect(),
            usings: program.usings.clone(),
            macros: program.macros.clone(),
            enums: program.enums.clone(),
            main: self.inline_block(&program.main),
        }
    }

    fn inline_module(&mut self, module: &Module) -> Module {
        let module_path = if let Some(parent) = self.module_stack.last() {
            format!("{}.{}", parent, module.name)
        } else {
            module.name.clone()
        };
        self.module_stack.push(module_path);

        let mut result = module.clone();
        result.functions = module
            .functions
            .iter()
            .map(|func| self.inline_function(func))
            .collect();
        result.submodules = module
            .submodules
            .iter()
            .map(|submodule| self.inline_module(submodule))
            .collect();
        result.body = self.inline_block(&module.body);

        self.module_stack.pop();
        result
    }

    fn inline_function(&mut self, func: &Function) -> Function {
        let mut result = func.clone();
        let mut scope: HashSet<String> =
            func.params.iter().map(|param| param.name.clone()).collect();
        scope.extend(func.kwparams.iter().map(|param| param.name.clone()));
        self.local_scopes.push(scope);
        result.body = self.inline_block(&func.body);
        self.local_scopes.pop();
        result
    }

    fn inline_block(&mut self, block: &Block) -> Block {
        // A nested function definition introduces a LOCAL binding for its name in
        // the enclosing scope, shadowing any same-named global throughout that
        // scope (Julia hoists local function definitions to the top of the scope).
        // Register every directly-nested NAMED function as a local binding BEFORE
        // inlining the block's statements so a call to that name — whether it
        // precedes or follows the textual definition — is never inlined to the
        // global body (Issue #8105). `add_local_binding` is a no-op at the top
        // level (empty scope stack), so top-level inlining is unaffected.
        //
        // Compiler-generated anonymous functions (`__lambda_*`, `__do_block_*`)
        // carry unique names that can never collide with a user global, so they
        // must NOT be registered: doing so wrongly marks the lambda passed to a
        // HOF (e.g. `reduce((acc,x)->acc+x*0.5, xs)`) as a local binding and
        // blocks its inlining, which breaks higher-order-function return-type
        // propagation (Issue #5094 regression introduced by #8105; fixed #8129).
        for stmt in &block.stmts {
            if let Stmt::FunctionDef { func, .. } = stmt {
                if !is_markerless_lowered_function(func) {
                    self.add_local_binding(&func.name);
                }
            }
        }
        Block {
            stmts: block
                .stmts
                .iter()
                .map(|stmt| self.inline_stmt(stmt))
                .collect(),
            span: block.span,
        }
    }

    fn inline_stmt(&mut self, stmt: &Stmt) -> Stmt {
        match stmt {
            Stmt::Block(block) => Stmt::Block(self.inline_block(block)),
            Stmt::Assign { var, value, span } => Stmt::Assign {
                var: var.clone(),
                value: {
                    let value = self.inline_expr(value);
                    self.add_local_binding(var);
                    value
                },
                span: *span,
            },
            Stmt::AddAssign { var, value, span } => Stmt::AddAssign {
                var: var.clone(),
                value: self.inline_expr(value),
                span: *span,
            },
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                span,
            } => {
                let start = self.inline_expr(start);
                let end = self.inline_expr(end);
                let step = step.as_ref().map(|expr| self.inline_expr(expr));
                self.local_scopes.push(HashSet::from([var.clone()]));
                let body = self.inline_block(body);
                self.local_scopes.pop();
                Stmt::For {
                    var: var.clone(),
                    start,
                    end,
                    step,
                    body,
                    span: *span,
                }
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                span,
            } => {
                let iterable = self.inline_expr(iterable);
                self.local_scopes.push(HashSet::from([var.clone()]));
                let body = self.inline_block(body);
                self.local_scopes.pop();
                Stmt::ForEach {
                    var: var.clone(),
                    iterable,
                    body,
                    span: *span,
                }
            }
            Stmt::ForEachTuple {
                vars,
                iterable,
                body,
                span,
            } => {
                let iterable = self.inline_expr(iterable);
                self.local_scopes
                    .push(vars.iter().cloned().collect::<HashSet<_>>());
                let body = self.inline_block(body);
                self.local_scopes.pop();
                Stmt::ForEachTuple {
                    vars: vars.clone(),
                    iterable,
                    body,
                    span: *span,
                }
            }
            Stmt::While {
                condition,
                body,
                span,
            } => Stmt::While {
                condition: self.inline_expr(condition),
                body: self.inline_block(body),
                span: *span,
            },
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Stmt::If {
                condition: self.inline_expr(condition),
                then_branch: self.inline_block(then_branch),
                else_branch: else_branch.as_ref().map(|block| self.inline_block(block)),
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
                try_block: self.inline_block(try_block),
                catch_var: catch_var.clone(),
                catch_block: catch_block.as_ref().map(|block| {
                    if let Some(catch_var) = catch_var {
                        self.local_scopes.push(HashSet::from([catch_var.clone()]));
                        let block = self.inline_block(block);
                        self.local_scopes.pop();
                        block
                    } else {
                        self.inline_block(block)
                    }
                }),
                else_block: else_block.as_ref().map(|block| self.inline_block(block)),
                finally_block: finally_block.as_ref().map(|block| self.inline_block(block)),
                span: *span,
            },
            Stmt::Return { value, span } => Stmt::Return {
                value: value.as_ref().map(|expr| self.inline_expr(expr)),
                span: *span,
            },
            Stmt::Expr { expr, span } => Stmt::Expr {
                expr: self.inline_expr(expr),
                span: *span,
            },
            Stmt::Timed { body, span } => Stmt::Timed {
                body: self.inline_block(body),
                span: *span,
            },
            Stmt::Test {
                condition,
                message,
                span,
            } => Stmt::Test {
                condition: self.inline_expr(condition),
                message: message.clone(),
                span: *span,
            },
            Stmt::TestSet { name, body, span } => Stmt::TestSet {
                name: name.clone(),
                body: self.inline_block(body),
                span: *span,
            },
            Stmt::TestThrows {
                exception_type,
                expr,
                span,
            } => Stmt::TestThrows {
                exception_type: exception_type.clone(),
                expr: Box::new(self.inline_expr(expr)),
                span: *span,
            },
            Stmt::IndexAssign {
                array,
                indices,
                value,
                span,
            } => Stmt::IndexAssign {
                array: array.clone(),
                indices: indices.iter().map(|expr| self.inline_expr(expr)).collect(),
                value: self.inline_expr(value),
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
                value: self.inline_expr(value),
                span: *span,
            },
            Stmt::DestructuringAssign {
                targets,
                value,
                span,
            } => Stmt::DestructuringAssign {
                targets: targets.clone(),
                value: self.inline_expr(value),
                span: *span,
            },
            Stmt::DictAssign {
                dict,
                key,
                value,
                span,
            } => Stmt::DictAssign {
                dict: dict.clone(),
                key: self.inline_expr(key),
                value: self.inline_expr(value),
                span: *span,
            },
            Stmt::FunctionDef { func, span } => Stmt::FunctionDef {
                func: Box::new(self.inline_function(func)),
                span: *span,
            },
            Stmt::EvalFunctionDef { func, span } => Stmt::EvalFunctionDef {
                func: Box::new(self.inline_function(func)),
                span: *span,
            },
            _ => stmt.clone(),
        }
    }

    fn inline_expr(&mut self, expr: &Expr) -> Expr {
        match expr {
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            } => {
                let args: Vec<Expr> = args.iter().map(|arg| self.inline_expr(arg)).collect();
                let kwargs: Vec<(crate::ir::core::InternedStr, Expr)> = kwargs
                    .iter()
                    .map(|(name, value)| (*name, self.inline_expr(value)))
                    .collect();
                if kwargs.is_empty()
                    && splat_mask.iter().all(|splat| !*splat)
                    && kwargs_splat_mask.iter().all(|splat| !*splat)
                {
                    if let Some(inlined) = self.inline_unqualified_call(function, &args, *span) {
                        return inlined;
                    }
                }
                Expr::Call {
                    function: *function,
                    args,
                    kwargs,
                    splat_mask: splat_mask.clone(),
                    kwargs_splat_mask: kwargs_splat_mask.clone(),
                    span: *span,
                }
            }
            Expr::BinaryOp {
                op,
                left,
                right,
                span,
            } => Expr::BinaryOp {
                op: *op,
                left: Box::new(self.inline_expr(left)),
                right: Box::new(self.inline_expr(right)),
                span: *span,
            },
            Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
                op: *op,
                operand: Box::new(self.inline_expr(operand)),
                span: *span,
            },
            Expr::TupleLiteral { elements, span } => Expr::TupleLiteral {
                elements: elements.iter().map(|expr| self.inline_expr(expr)).collect(),
                span: *span,
            },
            Expr::LetBlock {
                bindings,
                body,
                span,
            } => {
                let bindings: Vec<(crate::ir::core::InternedStr, Expr)> = bindings
                    .iter()
                    .map(|(name, value)| (*name, self.inline_expr(value)))
                    .collect();
                self.local_scopes
                    .push(bindings.iter().map(|(name, _)| name.to_string()).collect());
                let body = self.inline_block(body);
                self.local_scopes.pop();
                Expr::LetBlock {
                    bindings,
                    body,
                    span: *span,
                }
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                span,
            } => Expr::Ternary {
                condition: Box::new(self.inline_expr(condition)),
                then_expr: Box::new(self.inline_expr(then_expr)),
                else_expr: Box::new(self.inline_expr(else_expr)),
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
            } => {
                let args: Vec<Expr> = args.iter().map(|arg| self.inline_expr(arg)).collect();
                let kwargs: Vec<(crate::ir::core::InternedStr, Expr)> = kwargs
                    .iter()
                    .map(|(name, value)| (*name, self.inline_expr(value)))
                    .collect();
                if kwargs.is_empty()
                    && splat_mask.iter().all(|splat| !*splat)
                    && kwargs_splat_mask.iter().all(|splat| !*splat)
                {
                    if let Some(key) = self.lexically_visible_module_call_key(module, function) {
                        if let Some(inlined) = self.inline_call(&key, &args, *span) {
                            return inlined;
                        }
                    }
                }
                Expr::ModuleCall {
                    module: *module,
                    function: *function,
                    args,
                    kwargs,
                    splat_mask: splat_mask.clone(),
                    kwargs_splat_mask: kwargs_splat_mask.clone(),
                    span: *span,
                }
            }
            _ => expr.clone(),
        }
    }

    fn inline_call(&mut self, function: &str, args: &[Expr], span: Span) -> Option<Expr> {
        if self.is_local_binding(function) {
            return None;
        }
        let candidate = self.candidates.get(function)?.clone();
        if candidate
            .visible_from_source_start
            .is_some_and(|definition_start| span.start < definition_start)
        {
            return None;
        }
        if candidate.params.len() != args.len() {
            return None;
        }

        let mut substitutions = HashMap::new();
        let mut bindings = Vec::with_capacity(args.len());
        for (param, arg) in candidate.params.iter().zip(args) {
            if matches!(arg, Expr::Literal(..)) {
                substitutions.insert(param.clone(), arg.clone());
            } else {
                let temp = self.next_temp_name(function);
                substitutions.insert(param.clone(), Expr::Var(temp.clone().into(), arg.span()));
                bindings.push((temp.into(), arg.clone()));
            }
        }

        let body_expr = substitute_params(&candidate.body, &substitutions);
        Some(Expr::LetBlock {
            bindings,
            body: Block {
                stmts: vec![Stmt::Expr {
                    expr: body_expr,
                    span,
                }],
                span,
            },
            span,
        })
    }

    /// Return the candidate key for a module-qualified call only when the
    /// syntactic module root is bound by the current module itself or one of
    /// its direct submodules.  Imported/aliased calls are conservatively left
    /// for the scope-aware compiler; inlining them is an optimization.
    fn lexically_visible_module_call_key(&self, module: &str, function: &str) -> Option<String> {
        let root = module.split('.').next().unwrap_or(module);
        if self.is_local_binding(root) {
            return None;
        }
        let Some(current) = self.module_stack.last() else {
            return Some(format!("{module}.{function}"));
        };
        let rest = module.strip_prefix(root).unwrap_or_default();
        let current_short = current.rsplit('.').next().unwrap_or(current);

        if root == current_short {
            let candidate = format!("{current}{rest}.{function}");
            if self.candidates.contains_key(&candidate) {
                return Some(candidate);
            }
        }

        let nested = format!("{current}.{module}.{function}");
        self.candidates.contains_key(&nested).then_some(nested)
    }

    fn inline_unqualified_call(
        &mut self,
        function: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Expr> {
        if !function.contains('.') {
            if let Some(module_path) = self.module_stack.last() {
                let qualified = format!("{}.{}", module_path, function);
                if let Some(inlined) = self.inline_call(&qualified, args, span) {
                    return Some(inlined);
                }
            }
        }
        self.inline_call(function, args, span)
    }

    fn next_temp_name(&mut self, function: &str) -> String {
        let name = format!(
            "{INLINE_TEMP_PREFIX}{}_{}",
            sanitize_identifier_fragment(function),
            self.next_temp
        );
        self.next_temp += 1;
        name
    }

    fn add_local_binding(&mut self, name: &str) {
        if let Some(scope) = self.local_scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_local_binding(&self, name: &str) -> bool {
        self.local_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
    }
}

fn sanitize_identifier_fragment(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn substitute_params(expr: &Expr, substitutions: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Var(name, span) => substitutions
            .get(name.as_str())
            .map(|replacement| match replacement {
                Expr::Var(replacement_name, _) => Expr::Var(*replacement_name, *span),
                replacement => replacement.clone(),
            })
            .unwrap_or_else(|| expr.clone()),
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op: *op,
            operand: Box::new(substitute_params(operand, substitutions)),
            span: *span,
        },
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Expr::BinaryOp {
            op: *op,
            left: Box::new(substitute_params(left, substitutions)),
            right: Box::new(substitute_params(right, substitutions)),
            span: *span,
        },
        Expr::TupleLiteral { elements, span } => Expr::TupleLiteral {
            elements: elements
                .iter()
                .map(|element| substitute_params(element, substitutions))
                .collect(),
            span: *span,
        },
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            span,
        } => Expr::Ternary {
            condition: Box::new(substitute_params(condition, substitutions)),
            then_expr: Box::new(substitute_params(then_expr, substitutions)),
            else_expr: Box::new(substitute_params(else_expr, substitutions)),
            span: *span,
        },
        _ => expr.clone(),
    }
}

/// `true` for a callable introduced by lowering rather than by a source-level
/// Julia definition. Lowering stamps these helpers explicitly; zero remains a
/// valid legacy/unstamped source order and therefore cannot identify helpers.
/// Provenance, rather than a reserved-looking spelling, keeps a user method
/// such as `__gen_body_0(x) = x` source-visible (Issues #8129/#9784/#11685).
pub(crate) fn is_markerless_lowered_function(function: &Function) -> bool {
    function.is_lowering_helper()
}

#[cfg(test)]
mod markerless_lowered_function_tests {
    use super::is_markerless_lowered_function;
    use crate::ir::core::{Block, Function};
    use crate::span::Span;

    #[test]
    fn provenance_not_name_marks_lowered_helpers_9784() {
        let function = |name: &str, definition_order| {
            let mut span = Span::new(0, 0, 1, 1, 1, 1);
            span.definition_order = definition_order;
            Function {
                name: name.to_string(),
                params: Vec::new(),
                kwparams: Vec::new(),
                type_params: Vec::new(),
                return_type: None,
                body: Block {
                    stmts: Vec::new(),
                    span,
                },
                is_base_extension: false,
                is_runtime_eval: false,
                new_struct_name: None,
                span,
            }
        };
        assert!(!is_markerless_lowered_function(&function(
            "ordinary_name",
            0
        )));
        assert!(!is_markerless_lowered_function(&function(
            "__gen_body_0",
            1
        )));
        assert!(is_markerless_lowered_function(
            &function("ordinary_name", 0).into_lowering_helper()
        ));
    }
}

#[cfg(test)]
mod repl_hof_helper_9784_tests {
    use super::*;
    use crate::ir::core::{Program, TypedParam};

    fn test_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn pure_add_const(name: &str, value: i64) -> Function {
        Function {
            name: name.to_string(),
            params: vec![TypedParam::untyped("x".to_string(), test_span())],
            kwparams: Vec::new(),
            type_params: Vec::new(),
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string().into(), test_span())),
                        right: Box::new(Expr::Literal(Literal::Int(value), test_span())),
                        span: test_span(),
                    }),
                    span: test_span(),
                }],
                span: test_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: test_span(),
            new_struct_name: None,
        }
    }

    fn test_program(functions: Vec<Function>) -> Program {
        Program {
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            type_aliases: Vec::new(),
            structs: Vec::new(),
            functions: functions.into_iter().map(std::sync::Arc::new).collect(),
            base_function_count: 0,
            modules: Vec::new(),
            usings: Vec::new(),
            macros: Vec::new(),
            enums: Vec::new(),
            main: Block {
                stmts: Vec::new(),
                span: test_span(),
            },
        }
    }

    #[test]
    fn repl_source_order_visibility_skips_interposed_helpers_9784() -> Result<(), String> {
        let mut helper = pure_add_const("__helper_before_9784", 1);
        helper.span.start = 1;
        helper = helper.into_lowering_helper();
        let mut source = pure_add_const("source_9784", 2);
        source.span.start = 10;
        source.span.definition_order = 1;
        let mut prior = pure_add_const("prior_9784", 3);
        prior.span.start = 20;
        prior.span.definition_order = 2;
        let input = test_program(vec![helper, source, prior]);

        let candidates = collect_inline_candidates(&input, 0, Some(1));
        let helper = candidates
            .get("__helper_before_9784")
            .ok_or_else(|| "missing helper candidate".to_string())?;
        let source = candidates
            .get("source_9784")
            .ok_or_else(|| "missing source candidate".to_string())?;
        let prior = candidates
            .get("prior_9784")
            .ok_or_else(|| "missing prior candidate".to_string())?;
        assert_eq!(
            helper.visible_from_source_start, None,
            "a lowering helper must not consume the source-method budget"
        );
        assert_eq!(
            source.visible_from_source_start,
            Some(10),
            "the first Julia-visible source method remains source-ordered"
        );
        assert_eq!(
            prior.visible_from_source_start, None,
            "a merged prior method after the source budget stays immediately visible"
        );
        Ok(())
    }
}

#[cfg(test)]
mod issue_11281_local_decl_tests {
    use super::*;
    use crate::ir::core::LocalDeclKind;

    #[test]
    fn typed_declarations_prevent_single_expression_inlining() {
        let span = Span::new(0, 0, 1, 1, 0, 0);
        for kind in [LocalDeclKind::Explicit, LocalDeclKind::CompilerEnclosing] {
            let body = Block {
                stmts: vec![
                    Stmt::LocalDecl {
                        var: "x".into(),
                        kind,
                        span,
                    },
                    Stmt::Return {
                        value: Some(Expr::Literal(Literal::Int(1), span)),
                        span,
                    },
                ],
                span,
            };
            assert!(single_expression_body(&body).is_none());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::core::{MetaAnnotation, TypedParam, UnaryOp};

    fn sp() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn program(functions: Vec<Function>, main_stmts: Vec<Stmt>) -> Program {
        Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            structs: vec![],
            functions: functions.into_iter().map(std::sync::Arc::new).collect(),
            base_function_count: 0,
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: Block {
                stmts: main_stmts,
                span: sp(),
            },
        }
    }

    fn empty_module(name: &str, functions: Vec<Function>) -> Module {
        Module {
            name: name.to_string(),
            is_bare: false,
            is_package_origin: false,
            is_base_origin: false,
            functions,
            structs: vec![],
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            submodules: vec![],
            usings: vec![],
            macros: vec![],
            exports: vec![],
            publics: vec![],
            body: Block {
                stmts: vec![],
                span: sp(),
            },
            span: sp(),
        }
    }

    fn pure_add_one() -> Function {
        pure_add_const("addone", 1)
    }

    fn pure_add_const(name: &str, value: i64) -> Function {
        Function {
            name: name.to_string(),
            params: vec![TypedParam::untyped("x".to_string(), sp())],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string().into(), sp())),
                        right: Box::new(Expr::Literal(Literal::Int(value), sp())),
                        span: sp(),
                    }),
                    span: sp(),
                }],
                span: sp(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: sp(),
            new_struct_name: None,
        }
    }

    #[test]
    fn inline_small_pure_function_rewrites_call_to_let_issue_5184() {
        let input = program(
            vec![pure_add_one()],
            vec![Stmt::Assign {
                var: "y".to_string(),
                value: Expr::Call {
                    function: "addone".to_string().into(),
                    args: vec![Expr::Literal(Literal::Int(41), sp())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );

        let output = inline_small_pure_functions(&input, 0);
        let Stmt::Assign { value, .. } = &output.main.stmts[0] else {
            panic!("expected assignment");
        };
        let Expr::LetBlock { bindings, body, .. } = value else {
            panic!("expected inlined let block");
        };

        assert!(bindings.is_empty());
        assert!(matches!(
            &body.stmts[0],
            Stmt::Expr {
                expr: Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left,
                    ..
                },
                ..
            } if matches!(left.as_ref(), Expr::Literal(Literal::Int(41), _))
        ));
    }

    #[test]
    fn inline_small_pure_function_skips_typeof_callable_alias_issue_4309() {
        let input = program(
            vec![pure_add_one()],
            vec![
                Stmt::Assign {
                    var: "TAddOne".to_string(),
                    value: Expr::Builtin {
                        name: crate::ir::core::BuiltinOp::TypeOf,
                        args: vec![Expr::Var("addone".to_string().into(), sp())],
                        span: sp(),
                    },
                    span: sp(),
                },
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "addone".to_string().into(),
                        args: vec![Expr::Literal(Literal::Str("a".to_string()), sp())],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span: sp(),
                    },
                    span: sp(),
                },
            ],
        );

        let output = inline_small_pure_functions(&input, 0);
        let Stmt::Expr { expr, .. } = &output.main.stmts[1] else {
            panic!("expected expression");
        };
        assert!(
            matches!(expr, Expr::Call { function, .. } if function == "addone"),
            "typeof(callable) aliases must keep calls dispatchable"
        );
    }

    #[test]
    fn inline_small_pure_function_binds_argument_once_issue_5184() {
        let input = program(
            vec![Function {
                name: "twice".to_string(),
                params: vec![TypedParam::untyped("x".to_string(), sp())],
                kwparams: vec![],
                type_params: vec![],
                return_type: None,
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Var("x".to_string().into(), sp())),
                            right: Box::new(Expr::Var("x".to_string().into(), sp())),
                            span: sp(),
                        }),
                        span: sp(),
                    }],
                    span: sp(),
                },
                is_base_extension: false,
                is_runtime_eval: false,
                span: sp(),
                new_struct_name: None,
            }],
            vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "twice".to_string().into(),
                    args: vec![Expr::Call {
                        function: "source".to_string().into(),
                        args: vec![],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span: sp(),
                    }],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );

        let output = inline_small_pure_functions(&input, 0);
        let Stmt::Expr {
            expr: Expr::LetBlock { bindings, .. },
            ..
        } = &output.main.stmts[0]
        else {
            panic!("expected inlined let block");
        };

        assert_eq!(bindings.len(), 1);
        assert!(matches!(bindings[0].1, Expr::Call { .. }));
    }

    #[test]
    fn inline_literal_argument_avoids_lexical_temp_for_const_folding_11569() {
        let input = program(
            vec![Function {
                name: "addone".to_string(),
                params: vec![TypedParam::untyped("x".to_string(), sp())],
                kwparams: vec![],
                type_params: vec![],
                return_type: None,
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Var("x".to_string().into(), sp())),
                            right: Box::new(Expr::Literal(Literal::Int(1), sp())),
                            span: sp(),
                        }),
                        span: sp(),
                    }],
                    span: sp(),
                },
                is_base_extension: false,
                is_runtime_eval: false,
                span: sp(),
                new_struct_name: None,
            }],
            vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "addone".to_string().into(),
                    args: vec![Expr::Literal(Literal::Int(41), sp())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );

        let output = inline_small_pure_functions(&input, 0);
        assert!(matches!(
            &output.main.stmts[0],
            Stmt::Expr {
                expr: Expr::LetBlock { .. },
                ..
            }
        ));
        let Stmt::Expr {
            expr: Expr::LetBlock { bindings, body, .. },
            ..
        } = &output.main.stmts[0]
        else {
            return;
        };
        assert!(bindings.is_empty());
        assert!(matches!(
            &body.stmts[0],
            Stmt::Expr {
                expr: Expr::BinaryOp { left, .. },
                ..
            } if matches!(left.as_ref(), Expr::Literal(Literal::Int(41), _))
        ));
    }

    #[test]
    fn inline_small_pure_function_skips_ternary_body_issue_5589() {
        let input = program(
            vec![Function {
                name: "nullable_addone".to_string(),
                params: vec![TypedParam::untyped("x".to_string(), sp())],
                kwparams: vec![],
                type_params: vec![],
                return_type: None,
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Ternary {
                            condition: Box::new(Expr::BinaryOp {
                                op: BinaryOp::Egal,
                                left: Box::new(Expr::Var("x".to_string().into(), sp())),
                                right: Box::new(Expr::Literal(Literal::Nothing, sp())),
                                span: sp(),
                            }),
                            then_expr: Box::new(Expr::Literal(Literal::Int(0), sp())),
                            else_expr: Box::new(Expr::BinaryOp {
                                op: BinaryOp::Add,
                                left: Box::new(Expr::Var("x".to_string().into(), sp())),
                                right: Box::new(Expr::Literal(Literal::Int(1), sp())),
                                span: sp(),
                            }),
                            span: sp(),
                        }),
                        span: sp(),
                    }],
                    span: sp(),
                },
                is_base_extension: false,
                is_runtime_eval: false,
                span: sp(),
                new_struct_name: None,
            }],
            vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "nullable_addone".to_string().into(),
                    args: vec![Expr::Literal(Literal::Nothing, sp())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );

        let output = inline_small_pure_functions(&input, 0);
        assert!(matches!(
            &output.main.stmts[0],
            Stmt::Expr {
                expr: Expr::Call { function, .. },
                ..
            } if function == "nullable_addone"
        ));
    }

    #[test]
    fn inline_small_pure_function_respects_bound_call_target_issue_5612() {
        let input = program(
            vec![
                Function {
                    name: "f".to_string(),
                    params: vec![TypedParam::untyped("x".to_string(), sp())],
                    kwparams: vec![],
                    type_params: vec![],
                    return_type: None,
                    body: Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::BinaryOp {
                                op: BinaryOp::Add,
                                left: Box::new(Expr::Var("x".to_string().into(), sp())),
                                right: Box::new(Expr::Literal(Literal::Int(1), sp())),
                                span: sp(),
                            }),
                            span: sp(),
                        }],
                        span: sp(),
                    },
                    is_base_extension: false,
                    is_runtime_eval: false,
                    span: sp(),
                    new_struct_name: None,
                },
                Function {
                    name: "apply".to_string(),
                    params: vec![
                        TypedParam::untyped("f".to_string(), sp()),
                        TypedParam::untyped("x".to_string(), sp()),
                    ],
                    kwparams: vec![],
                    type_params: vec![],
                    return_type: None,
                    body: Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::Call {
                                function: "f".to_string().into(),
                                args: vec![Expr::Var("x".to_string().into(), sp())],
                                kwargs: vec![],
                                splat_mask: vec![],
                                kwargs_splat_mask: vec![],
                                span: sp(),
                            }),
                            span: sp(),
                        }],
                        span: sp(),
                    },
                    is_base_extension: false,
                    is_runtime_eval: false,
                    span: sp(),
                    new_struct_name: None,
                },
            ],
            vec![],
        );

        let output = inline_small_pure_functions(&input, 0);
        let apply = output
            .functions
            .iter()
            .find(|func| func.name == "apply")
            .expect("apply function should remain present");
        let Stmt::Return {
            value: Some(Expr::Call { function, .. }),
            ..
        } = &apply.body.stmts[0]
        else {
            panic!("expected bound f call to remain uninlined");
        };
        assert_eq!(function, "f");
    }

    #[test]
    fn inline_small_pure_function_skips_impure_body_issue_5184() {
        let mut func = pure_add_one();
        func.name = "negate_call".to_string();
        func.body.stmts = vec![Stmt::Return {
            value: Some(Expr::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(Expr::Call {
                    function: "source".to_string().into(),
                    args: vec![],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                }),
                span: sp(),
            }),
            span: sp(),
        }];
        let input = program(
            vec![func],
            vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "negate_call".to_string().into(),
                    args: vec![Expr::Literal(Literal::Int(1), sp())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );

        let output = inline_small_pure_functions(&input, 0);

        assert!(matches!(
            &output.main.stmts[0],
            Stmt::Expr {
                expr: Expr::Call { .. },
                ..
            }
        ));
    }

    #[test]
    fn inline_small_pure_function_respects_noinline_meta_issue_5184() {
        let mut func = pure_add_one();
        func.body.stmts.insert(
            0,
            Stmt::Meta {
                annotation: MetaAnnotation {
                    name: "noinline".to_string(),
                    args: vec![],
                },
                span: sp(),
            },
        );
        let input = program(
            vec![func],
            vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "addone".to_string().into(),
                    args: vec![Expr::Literal(Literal::Int(41), sp())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );

        let output = inline_small_pure_functions(&input, 0);

        assert!(matches!(
            &output.main.stmts[0],
            Stmt::Expr {
                expr: Expr::Call { .. },
                ..
            }
        ));
    }

    #[test]
    fn inline_small_pure_function_skips_generated_meta_issue_6214() {
        let mut func = pure_add_one();
        func.body.stmts.insert(
            0,
            Stmt::Meta {
                annotation: MetaAnnotation {
                    name: "generated".to_string(),
                    args: vec![],
                },
                span: sp(),
            },
        );
        let input = program(
            vec![func],
            vec![Stmt::Expr {
                expr: Expr::Call {
                    function: "addone".to_string().into(),
                    args: vec![Expr::Literal(Literal::Int(41), sp())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );

        let output = inline_small_pure_functions(&input, 0);

        assert!(matches!(
            &output.main.stmts[0],
            Stmt::Expr {
                expr: Expr::Call { .. },
                ..
            }
        ));
    }

    #[test]
    fn inline_small_pure_module_bare_call_prefers_local_function_issue_10771() {
        let driver = Function {
            name: "driver".to_string(),
            params: vec![TypedParam::untyped("x".to_string(), sp())],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "inner".to_string().into(),
                        args: vec![Expr::Var("x".to_string().into(), sp())],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span: sp(),
                    }),
                    span: sp(),
                }],
                span: sp(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: sp(),
            new_struct_name: None,
        };
        let mut input = program(vec![pure_add_const("inner", 1000)], vec![]);
        input
            .modules
            .push(empty_module("M", vec![pure_add_const("inner", 2), driver]));

        let output = inline_small_pure_functions(&input, 0);
        let driver = &output.modules[0].functions[1];
        let Stmt::Return {
            value: Some(Expr::LetBlock { body, .. }),
            ..
        } = &driver.body.stmts[0]
        else {
            panic!("expected module-local inner call to be inlined");
        };
        let Stmt::Expr {
            expr: Expr::BinaryOp { right, .. },
            ..
        } = &body.stmts[0]
        else {
            panic!("expected inlined module-local add body");
        };
        assert!(matches!(right.as_ref(), Expr::Literal(Literal::Int(2), _)));
    }

    /// Coverage for Issue #10840 (prevention for #10771): extend the bare-call
    /// module-scope regression to a TWO-level nested module (`A.B`) with a
    /// same-named candidate at every level (top-level, `A`, and `A.B`), so a
    /// same-name-across-levels tie can only resolve correctly if `Expr::Call`
    /// still routes through the scope-aware `inline_unqualified_call` policy
    /// helper. If `inline_expr`'s `Expr::Call` arm ever reverts to calling
    /// `inline_call(function, ...)` directly on the bare (unqualified) name,
    /// this test fails: `inline_call` performs a flat `HashMap` lookup keyed
    /// by the literal string `"inner"`, which the top-level candidate (value
    /// 1000) shadows unconditionally regardless of lexical module nesting.
    #[test]
    fn inline_small_pure_nested_module_bare_call_prefers_innermost_function_issue_10840() {
        let driver = Function {
            name: "driver".to_string(),
            params: vec![TypedParam::untyped("x".to_string(), sp())],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "inner".to_string().into(),
                        args: vec![Expr::Var("x".to_string().into(), sp())],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span: sp(),
                    }),
                    span: sp(),
                }],
                span: sp(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: sp(),
            new_struct_name: None,
        };

        let inner_module_b = empty_module("B", vec![pure_add_const("inner", 2), driver]);
        let mut outer_module_a = empty_module("A", vec![pure_add_const("inner", 100)]);
        outer_module_a.submodules = vec![inner_module_b];

        let mut input = program(vec![pure_add_const("inner", 1000)], vec![]);
        input.modules.push(outer_module_a);

        let output = inline_small_pure_functions(&input, 0);
        let module_a = &output.modules[0];
        let module_b = &module_a.submodules[0];
        let driver = &module_b.functions[1];
        let Stmt::Return {
            value: Some(Expr::LetBlock { body, .. }),
            ..
        } = &driver.body.stmts[0]
        else {
            panic!("expected nested module-local inner call to be inlined");
        };
        let Stmt::Expr {
            expr: Expr::BinaryOp { right, .. },
            ..
        } = &body.stmts[0]
        else {
            panic!("expected inlined nested module-local add body");
        };
        assert!(matches!(right.as_ref(), Expr::Literal(Literal::Int(2), _)));
    }

    #[test]
    fn inline_small_pure_module_function_issue_5184() {
        let mut input = program(
            vec![],
            vec![Stmt::Expr {
                expr: Expr::ModuleCall {
                    module: "M".to_string().into(),
                    function: "addone".to_string().into(),
                    args: vec![Expr::Literal(Literal::Int(41), sp())],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span: sp(),
                },
                span: sp(),
            }],
        );
        input.modules.push(empty_module("M", vec![pure_add_one()]));

        let output = inline_small_pure_functions(&input, 0);

        assert!(matches!(
            &output.main.stmts[0],
            Stmt::Expr {
                expr: Expr::LetBlock { .. },
                ..
            }
        ));
    }
}
