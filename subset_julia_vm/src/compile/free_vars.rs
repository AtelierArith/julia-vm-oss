//! Free variable analysis for closure capture detection.
//!
//! This module provides pure functions for analyzing which variables from an outer
//! scope are referenced (and thus need to be captured) by a nested function body.
//! These functions are independent of `CoreCompiler` and operate only on the Core IR.
//!
//! The four functions are mutually recursive:
//! - `analyze_free_variables` — entry point, analyzes a `Function`
//! - `analyze_block_free_vars` — analyzes a `Block`
//! - `analyze_stmt_free_vars` — analyzes a `Stmt`
//! - `analyze_expr_free_vars` — analyzes an `Expr`

use std::collections::HashSet;

use crate::ir::core::{decode_tuple_comprehension_binding, Block, Expr, Function, Stmt};

/// Analyze which variables from `outer_scope_vars` are used as free variables
/// in the given function body. These are variables that need to be captured
/// for closure creation.
///
/// # Arguments
/// * `func` - The function to analyze
/// * `outer_scope_vars` - Variables available in the outer scope that could be captured
///
/// # Returns
/// Set of variable names from outer_scope_vars that are actually used in the function body
pub(crate) fn analyze_free_variables(
    func: &Function,
    outer_scope_vars: &HashSet<String>,
) -> HashSet<String> {
    let mut free_vars = HashSet::new();
    let mut local_vars = HashSet::new();

    // Collect function parameters as local variables
    for param in &func.params {
        local_vars.insert(param.name.clone());
    }
    collect_function_local_bindings_block(&func.body, &mut local_vars);

    // Analyze the function body
    analyze_block_free_vars(
        &func.body,
        outer_scope_vars,
        &mut local_vars,
        &mut free_vars,
    );
    free_vars.retain(|var| !local_vars.contains(var));

    // Capture-on-assign (Julia soft-scope rule; Issues #7618/#7619/#7685): a bare
    // assignment to a name that already exists in an enclosing local scope rebinds
    // (captures) that outer variable rather than introducing a fresh local — e.g.
    // `function f(); x=0; g()=(x=1); g(); x; end` returns `1`, and a `count = count+1`
    // accumulator captures the outer `count`. The pre-pass above hoists every
    // assigned name into `local_vars` so a read-before-write of a *fresh* local is
    // not spuriously captured, but that also over-localizes outer names that must be
    // captured; re-add those here. Only genuine soft assignments capture —
    // parameters (which shadow) and hard binders (loop / catch / comprehension
    // variables, never soft-assigned) are excluded.
    let mut soft_assigned = HashSet::new();
    collect_soft_assigned_names_block(&func.body, &mut soft_assigned);
    for name in &soft_assigned {
        if outer_scope_vars.contains(name) && !func.params.iter().any(|param| &param.name == name) {
            free_vars.insert(name.clone());
        }
    }

    free_vars
}

/// Collect the names referenced by `func`'s body via `Expr::FunctionRef` or a
/// bare `Expr::Var` (do-block / arrow lambdas are lifted to top-level
/// `__lambda_N` functions and referenced from their enclosing lambda by name).
///
/// Used to recover the *nesting* relationship between flat lifted lambdas so
/// that a nested do-block can capture its enclosing do-block's params / locals
/// (Issue #7600). The full free-variable walk deliberately ignores
/// `FunctionRef`, so a dedicated walk is required here.
pub(crate) fn collect_referenced_names(func: &Function) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_referenced_names_block(&func.body, &mut names);
    names
}

fn collect_referenced_names_block(block: &Block, names: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_referenced_names_stmt(stmt, names);
    }
}

fn collect_referenced_names_stmt(stmt: &Stmt, names: &mut HashSet<String>) {
    match stmt {
        Stmt::Block(block) => collect_referenced_names_block(block, names),
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            collect_referenced_names_expr(value, names)
        }
        Stmt::Return {
            value: Some(value), ..
        } => collect_referenced_names_expr(value, names),
        Stmt::Expr { expr, .. } => collect_referenced_names_expr(expr, names),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_referenced_names_expr(condition, names);
            collect_referenced_names_block(then_branch, names);
            if let Some(b) = else_branch {
                collect_referenced_names_block(b, names);
            }
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_referenced_names_expr(start, names);
            collect_referenced_names_expr(end, names);
            if let Some(s) = step {
                collect_referenced_names_expr(s, names);
            }
            collect_referenced_names_block(body, names);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_referenced_names_expr(iterable, names);
            collect_referenced_names_block(body, names);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_referenced_names_expr(condition, names);
            collect_referenced_names_block(body, names);
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_referenced_names_block(try_block, names);
            for b in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                collect_referenced_names_block(b, names);
            }
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            collect_referenced_names_block(&func.body, names)
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_referenced_names_expr(key, names);
            collect_referenced_names_expr(value, names);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for e in indices {
                collect_referenced_names_expr(e, names);
            }
            collect_referenced_names_expr(value, names);
        }
        Stmt::FieldAssign { value, .. } | Stmt::DestructuringAssign { value, .. } => {
            collect_referenced_names_expr(value, names)
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_referenced_names_block(body, names)
        }
        Stmt::Test { condition, .. } => collect_referenced_names_expr(condition, names),
        Stmt::TestThrows { expr, .. } => collect_referenced_names_expr(expr, names),
        Stmt::Return { value: None, .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::Global { .. }
        | Stmt::EnumDef { .. } => {}
    }
}

fn collect_referenced_names_expr(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::FunctionRef { name, .. } => {
            names.insert(name.clone());
        }
        Expr::Var(name, _) => {
            names.insert(name.clone());
        }
        Expr::Literal(_, _) => {}
        Expr::BinaryOp { left, right, .. } => {
            collect_referenced_names_expr(left, names);
            collect_referenced_names_expr(right, names);
        }
        Expr::UnaryOp { operand, .. } => collect_referenced_names_expr(operand, names),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for a in args {
                collect_referenced_names_expr(a, names);
            }
            for (_, e) in kwargs {
                collect_referenced_names_expr(e, names);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for a in args {
                collect_referenced_names_expr(a, names);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_referenced_names_expr(array, names);
            for i in indices {
                collect_referenced_names_expr(i, names);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_referenced_names_expr(start, names);
            if let Some(s) = step {
                collect_referenced_names_expr(s, names);
            }
            collect_referenced_names_expr(stop, names);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_referenced_names_expr(body, names);
            collect_referenced_names_expr(iter, names);
            if let Some(f) = filter {
                collect_referenced_names_expr(f, names);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_referenced_names_expr(body, names);
            for (_, it) in iterations {
                collect_referenced_names_expr(it, names);
            }
            if let Some(f) = filter {
                collect_referenced_names_expr(f, names);
            }
        }
        Expr::FieldAccess { object, .. } => collect_referenced_names_expr(object, names),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_referenced_names_expr(condition, names);
            collect_referenced_names_expr(then_expr, names);
            collect_referenced_names_expr(else_expr, names);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_referenced_names_expr(value, names);
            }
            collect_referenced_names_block(body, names);
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for e in elements {
                collect_referenced_names_expr(e, names);
            }
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, v) in fields {
                collect_referenced_names_expr(v, names);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (k, v) in pairs {
                collect_referenced_names_expr(k, names);
                collect_referenced_names_expr(v, names);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for p in parts {
                collect_referenced_names_expr(p, names);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(b) = base_expr {
                collect_referenced_names_expr(b, names);
            }
            for a in type_args {
                collect_referenced_names_expr(a, names);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => collect_referenced_names_expr(constructor, names),
        Expr::AssignExpr { value, .. } => collect_referenced_names_expr(value, names),
        Expr::ReturnExpr {
            value: Some(value), ..
        } => collect_referenced_names_expr(value, names),
        Expr::Pair { key, value, .. } => {
            collect_referenced_names_expr(key, names);
            collect_referenced_names_expr(value, names);
        }
        Expr::ReturnExpr { value: None, .. }
        | Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

/// Public wrapper (Issue #8118): collect every name bound (assigned, loop var,
/// catch var, comprehension var, etc.) anywhere in `block`. Used to recover the
/// enclosing function's full local-variable set as the outer scope when
/// pre-scanning a block's nested closures for transitive sibling captures —
/// before the body's statements have been compiled (so `locals` is not yet
/// populated). Over-collecting is safe here: `analyze_free_variables` only
/// returns names the nested function actually reads that are in this set.
pub(crate) fn collect_block_local_bindings(block: &Block) -> HashSet<String> {
    let mut local_vars = HashSet::new();
    collect_function_local_bindings_block(block, &mut local_vars);
    local_vars
}

fn collect_function_local_bindings_block(block: &Block, local_vars: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_function_local_bindings_stmt(stmt, local_vars);
    }
}

fn collect_function_local_bindings_stmt(stmt: &Stmt, local_vars: &mut HashSet<String>) {
    match stmt {
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            collect_function_local_bindings_block(block, local_vars);
        }
        Stmt::Assign { var, value, .. } => {
            local_vars.insert(var.clone());
            collect_function_local_bindings_expr(value, local_vars);
        }
        Stmt::Return {
            value: Some(value), ..
        } => {
            collect_function_local_bindings_expr(value, local_vars);
        }
        Stmt::Return { value: None, .. } => {}
        Stmt::Expr { expr, .. } => {
            collect_function_local_bindings_expr(expr, local_vars);
        }
        Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
            local_vars.insert(var.clone());
            collect_function_local_bindings_block(body, local_vars);
        }
        Stmt::ForEachTuple { vars, body, .. } => {
            local_vars.extend(vars.iter().cloned());
            collect_function_local_bindings_block(body, local_vars);
        }
        Stmt::While { body, .. } => collect_function_local_bindings_block(body, local_vars),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_function_local_bindings_block(then_branch, local_vars);
            if let Some(block) = else_branch {
                collect_function_local_bindings_block(block, local_vars);
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
            collect_function_local_bindings_block(try_block, local_vars);
            if let Some(var) = catch_var {
                local_vars.insert(var.clone());
            }
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                collect_function_local_bindings_block(block, local_vars);
            }
        }
        Stmt::DestructuringAssign { targets, .. } => {
            local_vars.extend(targets.iter().cloned());
        }
        _ => {}
    }
}

fn collect_function_local_bindings_expr(expr: &Expr, local_vars: &mut HashSet<String>) {
    match expr {
        Expr::AssignExpr { var, value, .. } => {
            local_vars.insert(var.clone());
            collect_function_local_bindings_expr(value, local_vars);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_function_local_bindings_expr(left, local_vars);
            collect_function_local_bindings_expr(right, local_vars);
        }
        Expr::UnaryOp { operand, .. } => collect_function_local_bindings_expr(operand, local_vars),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_function_local_bindings_expr(arg, local_vars);
            }
            for (_, value) in kwargs {
                collect_function_local_bindings_expr(value, local_vars);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                collect_function_local_bindings_expr(arg, local_vars);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_function_local_bindings_expr(array, local_vars);
            for idx in indices {
                collect_function_local_bindings_expr(idx, local_vars);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for elem in elements {
                collect_function_local_bindings_expr(elem, local_vars);
            }
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_function_local_bindings_expr(value, local_vars);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_function_local_bindings_expr(start, local_vars);
            if let Some(step) = step {
                collect_function_local_bindings_expr(step, local_vars);
            }
            collect_function_local_bindings_expr(stop, local_vars);
        }
        Expr::Comprehension { iter, filter, .. } | Expr::Generator { iter, filter, .. } => {
            collect_function_local_bindings_expr(iter, local_vars);
            if let Some(filter) = filter {
                collect_function_local_bindings_expr(filter, local_vars);
            }
        }
        Expr::MultiComprehension {
            iterations, filter, ..
        } => {
            for (_, iter) in iterations {
                collect_function_local_bindings_expr(iter, local_vars);
            }
            if let Some(filter) = filter {
                collect_function_local_bindings_expr(filter, local_vars);
            }
        }
        Expr::FieldAccess { object, .. } => {
            collect_function_local_bindings_expr(object, local_vars)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_function_local_bindings_expr(condition, local_vars);
            collect_function_local_bindings_expr(then_expr, local_vars);
            collect_function_local_bindings_expr(else_expr, local_vars);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_function_local_bindings_expr(value, local_vars);
            }
            if bindings.is_empty() {
                collect_function_local_bindings_block(body, local_vars);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_function_local_bindings_expr(part, local_vars);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_function_local_bindings_expr(key, local_vars);
                collect_function_local_bindings_expr(value, local_vars);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            collect_function_local_bindings_expr(constructor, local_vars);
        }
        Expr::ReturnExpr { value, .. } => {
            if let Some(value) = value {
                collect_function_local_bindings_expr(value, local_vars);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_function_local_bindings_expr(key, local_vars);
            collect_function_local_bindings_expr(value, local_vars);
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_function_local_bindings_expr(base_expr, local_vars);
            }
            for arg in type_args {
                collect_function_local_bindings_expr(arg, local_vars);
            }
        }
        Expr::Var(_, _)
        | Expr::Literal(_, _)
        | Expr::FunctionRef { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

/// Collect the names *soft-assigned* (`x = …`, destructuring `(a, b) = …`, or an
/// `AssignExpr`) anywhere in this function's own hard scope, WITHOUT descending
/// into nested function/closure definitions. Loop, catch and comprehension
/// variables are hard binders that always shadow and are deliberately not
/// collected here — only soft assignments capture an enclosing local (Julia
/// soft-scope rule; Issues #7618/#7619/#7685). Under-collecting is safe (it only
/// means a deeply-nested assignment is not treated as a capture); over-collecting
/// is not, so this never inserts a hard-binder name.
fn collect_soft_assigned_names_block(block: &Block, names: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_soft_assigned_names_stmt(stmt, names);
    }
}

fn collect_soft_assigned_names_stmt(stmt: &Stmt, names: &mut HashSet<String>) {
    match stmt {
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => collect_soft_assigned_names_block(block, names),
        Stmt::Assign { var, value, .. } => {
            names.insert(var.clone());
            collect_soft_assigned_names_expr(value, names);
        }
        Stmt::Return {
            value: Some(value), ..
        } => collect_soft_assigned_names_expr(value, names),
        Stmt::Expr { expr, .. } => collect_soft_assigned_names_expr(expr, names),
        Stmt::For { body, .. } | Stmt::ForEach { body, .. } | Stmt::ForEachTuple { body, .. } => {
            collect_soft_assigned_names_block(body, names)
        }
        Stmt::While { body, .. } => collect_soft_assigned_names_block(body, names),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_soft_assigned_names_block(then_branch, names);
            if let Some(block) = else_branch {
                collect_soft_assigned_names_block(block, names);
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_soft_assigned_names_block(try_block, names);
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                collect_soft_assigned_names_block(block, names);
            }
        }
        Stmt::DestructuringAssign { targets, .. } => {
            names.extend(targets.iter().cloned());
        }
        _ => {}
    }
}

fn collect_soft_assigned_names_expr(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::AssignExpr { var, value, .. } => {
            names.insert(var.clone());
            collect_soft_assigned_names_expr(value, names);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_soft_assigned_names_expr(left, names);
            collect_soft_assigned_names_expr(right, names);
        }
        Expr::UnaryOp { operand, .. } => collect_soft_assigned_names_expr(operand, names),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_soft_assigned_names_expr(arg, names);
            }
            for (_, value) in kwargs {
                collect_soft_assigned_names_expr(value, names);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                collect_soft_assigned_names_expr(arg, names);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_soft_assigned_names_expr(array, names);
            for idx in indices {
                collect_soft_assigned_names_expr(idx, names);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_soft_assigned_names_expr(condition, names);
            collect_soft_assigned_names_expr(then_expr, names);
            collect_soft_assigned_names_expr(else_expr, names);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_soft_assigned_names_expr(value, names);
            }
            if bindings.is_empty() {
                collect_soft_assigned_names_block(body, names);
            }
        }
        _ => {}
    }
}

/// Analyze free variables in a block.
fn analyze_block_free_vars(
    block: &Block,
    outer_scope_vars: &HashSet<String>,
    local_vars: &mut HashSet<String>,
    free_vars: &mut HashSet<String>,
) {
    for stmt in &block.stmts {
        analyze_stmt_free_vars(stmt, outer_scope_vars, local_vars, free_vars);
    }
}

/// Analyze free variables in a statement.
fn analyze_stmt_free_vars(
    stmt: &Stmt,
    outer_scope_vars: &HashSet<String>,
    local_vars: &mut HashSet<String>,
    free_vars: &mut HashSet<String>,
) {
    match stmt {
        Stmt::Block(block) => {
            analyze_block_free_vars(block, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::Assign { var, value, .. } => {
            // Simple assignment creates a local binding for the whole function
            // hard scope, even when an outer variable has the same name.
            local_vars.insert(var.clone());
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::AddAssign { var, value, .. } => {
            // var must already exist - check if it's from outer scope
            if !local_vars.contains(var) && outer_scope_vars.contains(var) {
                free_vars.insert(var.clone());
            }
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::Return { value, .. } => {
            if let Some(expr) = value {
                analyze_expr_free_vars(expr, outer_scope_vars, local_vars, free_vars);
            }
        }
        Stmt::Expr { expr, .. } => {
            analyze_expr_free_vars(expr, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            analyze_expr_free_vars(condition, outer_scope_vars, local_vars, free_vars);
            analyze_block_free_vars(then_branch, outer_scope_vars, local_vars, free_vars);
            if let Some(block) = else_branch {
                analyze_block_free_vars(block, outer_scope_vars, local_vars, free_vars);
            }
        }
        Stmt::For {
            var,
            start,
            end,
            step,
            body,
            ..
        } => {
            analyze_expr_free_vars(start, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(end, outer_scope_vars, local_vars, free_vars);
            if let Some(s) = step {
                analyze_expr_free_vars(s, outer_scope_vars, local_vars, free_vars);
            }
            local_vars.insert(var.clone());
            analyze_block_free_vars(body, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::ForEach {
            var,
            iterable,
            body,
            ..
        } => {
            analyze_expr_free_vars(iterable, outer_scope_vars, local_vars, free_vars);
            local_vars.insert(var.clone());
            analyze_block_free_vars(body, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            ..
        } => {
            analyze_expr_free_vars(iterable, outer_scope_vars, local_vars, free_vars);
            for var in vars {
                local_vars.insert(var.clone());
            }
            analyze_block_free_vars(body, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::While {
            condition, body, ..
        } => {
            analyze_expr_free_vars(condition, outer_scope_vars, local_vars, free_vars);
            analyze_block_free_vars(body, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            analyze_block_free_vars(try_block, outer_scope_vars, local_vars, free_vars);
            if let Some(var) = catch_var {
                local_vars.insert(var.clone());
            }
            if let Some(block) = catch_block {
                analyze_block_free_vars(block, outer_scope_vars, local_vars, free_vars);
            }
            if let Some(block) = else_block {
                analyze_block_free_vars(block, outer_scope_vars, local_vars, free_vars);
            }
            if let Some(block) = finally_block {
                analyze_block_free_vars(block, outer_scope_vars, local_vars, free_vars);
            }
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            // Issue #1744: For deeply nested closures, we need to capture variables
            // that nested functions use from ancestor scopes. Analyze the nested function
            // and add any free variables it needs that are from our outer scope.
            //
            // Example: make_outer(x) -> middle() -> inner() -> uses x
            // When analyzing middle, we need to see that inner uses x, so middle
            // must also capture x to pass it down to inner.
            let nested_free_vars = analyze_free_variables(func, outer_scope_vars);
            for var in nested_free_vars {
                // If the nested function needs a variable from our outer scope,
                // we must capture it too (to pass it down)
                if !local_vars.contains(&var) && outer_scope_vars.contains(&var) {
                    free_vars.insert(var);
                }
            }
        }
        Stmt::DictAssign { key, value, .. } => {
            analyze_expr_free_vars(key, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::IndexAssign {
            array,
            indices,
            value,
            ..
        } => {
            // array is a String, check if it's from outer scope
            if !local_vars.contains(array) && outer_scope_vars.contains(array) {
                free_vars.insert(array.clone());
            }
            for idx in indices {
                analyze_expr_free_vars(idx, outer_scope_vars, local_vars, free_vars);
            }
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::FieldAssign { object, value, .. } => {
            // object is a String, check if it's from outer scope
            if !local_vars.contains(object) && outer_scope_vars.contains(object) {
                free_vars.insert(object.clone());
            }
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::DestructuringAssign { targets, value, .. } => {
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
            for target in targets {
                local_vars.insert(target.clone());
            }
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            analyze_block_free_vars(body, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::Test { condition, .. } => {
            analyze_expr_free_vars(condition, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::TestThrows { expr, .. } => {
            analyze_expr_free_vars(expr, outer_scope_vars, local_vars, free_vars);
        }
        Stmt::Global { names, .. } => {
            // A `global x` declaration binds `x` to the module-level binding, so
            // it is NOT captured from the enclosing function scope. Treat it as
            // local here so reads resolve to the global rather than a closure
            // capture (Issues #5548, #5549).
            for name in names {
                local_vars.insert(name.clone());
            }
        }
        // Statements that don't introduce variables or reference expressions
        Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. } => {}
    }
}

/// Analyze free variables in an expression.
fn analyze_expr_free_vars(
    expr: &Expr,
    outer_scope_vars: &HashSet<String>,
    local_vars: &HashSet<String>,
    free_vars: &mut HashSet<String>,
) {
    match expr {
        Expr::Var(name, _) => {
            // If the variable is not local but is in outer scope, it's a free variable
            if !local_vars.contains(name) && outer_scope_vars.contains(name) {
                free_vars.insert(name.clone());
            }
        }
        Expr::Literal(_, _) => {}
        Expr::BinaryOp { left, right, .. } => {
            analyze_expr_free_vars(left, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(right, outer_scope_vars, local_vars, free_vars);
        }
        Expr::UnaryOp { operand, .. } => {
            analyze_expr_free_vars(operand, outer_scope_vars, local_vars, free_vars);
        }
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            // `function` is a String (the callee name). When it names a variable
            // from the enclosing scope rather than a local or a global function,
            // it is a captured free variable holding a callable — e.g. the `f` in
            // `makeapply(f) = x -> f(x)`. Without capturing it, the closure body
            // errors with "Unknown function: f" (Issue #5723). Globals (`abs`,
            // user functions) are not in `outer_scope_vars`, so they are excluded.
            if !local_vars.contains(function) && outer_scope_vars.contains(function) {
                free_vars.insert(function.clone());
            }
            for arg in args {
                analyze_expr_free_vars(arg, outer_scope_vars, local_vars, free_vars);
            }
            for (_, value) in kwargs {
                analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                analyze_expr_free_vars(arg, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::Index { array, indices, .. } => {
            analyze_expr_free_vars(array, outer_scope_vars, local_vars, free_vars);
            for idx in indices {
                analyze_expr_free_vars(idx, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                analyze_expr_free_vars(elem, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for elem in elements {
                analyze_expr_free_vars(elem, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            analyze_expr_free_vars(start, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(stop, outer_scope_vars, local_vars, free_vars);
            if let Some(s) = step {
                analyze_expr_free_vars(s, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::Comprehension {
            body,
            var,
            iter,
            filter,
            ..
        } => {
            // Comprehension variable is local to the comprehension
            let mut comp_local = local_vars.clone();
            if let Some(tuple_vars) = decode_tuple_comprehension_binding(var) {
                comp_local.extend(tuple_vars);
            } else {
                comp_local.insert(var.clone());
            }
            analyze_expr_free_vars(iter, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(body, outer_scope_vars, &comp_local, free_vars);
            if let Some(f) = filter {
                analyze_expr_free_vars(f, outer_scope_vars, &comp_local, free_vars);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            // All iteration variables are local to the comprehension
            let mut comp_local = local_vars.clone();
            for (var, _) in iterations {
                comp_local.insert(var.clone());
            }
            for (_, iter_expr) in iterations {
                analyze_expr_free_vars(iter_expr, outer_scope_vars, local_vars, free_vars);
            }
            analyze_expr_free_vars(body, outer_scope_vars, &comp_local, free_vars);
            if let Some(f) = filter {
                analyze_expr_free_vars(f, outer_scope_vars, &comp_local, free_vars);
            }
        }
        Expr::Generator {
            body,
            var,
            iter,
            filter,
            ..
        } => {
            let mut gen_local = local_vars.clone();
            gen_local.insert(var.clone());
            analyze_expr_free_vars(iter, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(body, outer_scope_vars, &gen_local, free_vars);
            if let Some(f) = filter {
                analyze_expr_free_vars(f, outer_scope_vars, &gen_local, free_vars);
            }
        }
        Expr::FieldAccess { object, .. } => {
            analyze_expr_free_vars(object, outer_scope_vars, local_vars, free_vars);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            analyze_expr_free_vars(condition, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(then_expr, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(else_expr, outer_scope_vars, local_vars, free_vars);
        }
        Expr::LetBlock { bindings, body, .. } => {
            let mut let_local = local_vars.clone();
            for (name, value) in bindings {
                analyze_expr_free_vars(value, outer_scope_vars, &let_local, free_vars);
                let_local.insert(name.clone());
            }
            analyze_block_free_vars(body, outer_scope_vars, &mut let_local, free_vars);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                analyze_expr_free_vars(key, outer_scope_vars, local_vars, free_vars);
                analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                analyze_expr_free_vars(part, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                analyze_expr_free_vars(arg, outer_scope_vars, local_vars, free_vars);
            }
            for (_, value) in kwargs {
                analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::New { args, .. } => {
            for arg in args {
                analyze_expr_free_vars(arg, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                analyze_expr_free_vars(base_expr, outer_scope_vars, local_vars, free_vars);
            }
            for arg in type_args {
                analyze_expr_free_vars(arg, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            analyze_expr_free_vars(constructor, outer_scope_vars, local_vars, free_vars);
        }
        Expr::AssignExpr { value, var, .. } => {
            if !local_vars.contains(var) && outer_scope_vars.contains(var) {
                free_vars.insert(var.clone());
            }
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
        }
        Expr::ReturnExpr { value, .. } => {
            if let Some(v) = value {
                analyze_expr_free_vars(v, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::Pair { key, value, .. } => {
            analyze_expr_free_vars(key, outer_scope_vars, local_vars, free_vars);
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
        }
        // Expressions without sub-expressions that could reference variables
        Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Literal, Stmt, TypedParam};
    use crate::span::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    fn make_func(params: &[&str], body: Vec<Stmt>) -> Function {
        Function {
            name: "test_fn".to_string(),
            params: params
                .iter()
                .map(|name| TypedParam::untyped(name.to_string(), span()))
                .collect(),
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: body,
                span: span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: span(),
        }
    }

    fn outer(vars: &[&str]) -> HashSet<String> {
        vars.iter().map(|s| s.to_string()).collect()
    }

    fn var_expr(name: &str) -> Expr {
        Expr::Var(name.to_string(), span())
    }

    fn int_lit(n: i64) -> Expr {
        Expr::Literal(Literal::Int(n), span())
    }

    /// A function with no body references no free variables.
    #[test]
    fn test_empty_body_no_free_vars() {
        let func = make_func(&[], vec![]);
        let outer = outer(&["x", "y"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(result.is_empty(), "Expected no free vars, got {:?}", result);
    }

    /// A function that references an outer variable captures it.
    #[test]
    fn test_references_outer_var() {
        // body: return x
        let func = make_func(
            &[],
            vec![Stmt::Return {
                value: Some(var_expr("x")),
                span: span(),
            }],
        );
        let outer = outer(&["x", "y"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            result.contains("x"),
            "Expected 'x' in free vars, got {:?}",
            result
        );
        assert!(
            !result.contains("y"),
            "Expected 'y' NOT in free vars, got {:?}",
            result
        );
    }

    /// A function parameter with the same name as an outer variable does NOT capture it.
    #[test]
    fn test_param_shadows_outer_var() {
        // fn(x) -> return x   -- x is a param, not a free var
        let func = make_func(
            &["x"],
            vec![Stmt::Return {
                value: Some(var_expr("x")),
                span: span(),
            }],
        );
        let outer = outer(&["x"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            result.is_empty(),
            "Expected no free vars (x is a param), got {:?}",
            result
        );
    }

    /// A bare assignment to a name that is a local of an enclosing function
    /// captures (and mutates) that enclosing binding — it does NOT introduce a
    /// fresh local. This mirrors upstream Julia scoping: inside
    /// `function f(); x=0; g()=(x=1); g(); x; end`, the `x=1` in `g` rebinds
    /// `f`'s `x`, so `f()` returns `1` (Issues #7618, #7685). Only an explicit
    /// `local x` (lowered to `Stmt::Global`-style hard scope) forces a fresh
    /// local, which is exercised separately.
    #[test]
    fn test_assign_to_outer_var_captures() {
        // body: x = 1; return x  -- x exists in the enclosing scope, so the
        // assignment captures and mutates it.
        let func = make_func(
            &[],
            vec![
                Stmt::Assign {
                    var: "x".to_string(),
                    value: int_lit(1),
                    span: span(),
                },
                Stmt::Return {
                    value: Some(var_expr("x")),
                    span: span(),
                },
            ],
        );
        let outer = outer(&["x"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            result.contains("x"),
            "Expected 'x' captured (assigned name exists in outer scope), got {:?}",
            result
        );
    }

    /// An assignment to a name that does NOT exist in the enclosing scope
    /// introduces a fresh local — it is never captured, even when its RHS
    /// reads a captured outer variable.
    #[test]
    fn test_fresh_local_assign_not_captured() {
        // body: y = x + 1  -- x is read from the outer scope (captured); y is a
        // brand-new local (not present in outer_scope_vars) and stays local.
        let func = make_func(
            &[],
            vec![Stmt::Assign {
                var: "y".to_string(),
                value: Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(var_expr("x")),
                    right: Box::new(int_lit(1)),
                    span: span(),
                },
                span: span(),
            }],
        );
        // Only `x` is in the enclosing scope; `y` is introduced fresh.
        let outer = outer(&["x"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            result.contains("x"),
            "Expected 'x' in free vars, got {:?}",
            result
        );
        // y is a fresh local, not captured
        assert!(
            !result.contains("y"),
            "Expected 'y' NOT in free vars (fresh local), got {:?}",
            result
        );
    }

    /// Regression for the capture-on-assign + Base-name accumulator pattern
    /// (Issue #7619): a closure that reads-then-assigns an enclosing local
    /// captures it. The body `count = count + 1; return count` with `count` in
    /// the enclosing scope must report `count` as free so the closure captures
    /// and mutates the outer binding (rather than resolving `count` to the
    /// `Base.count` function).
    #[test]
    fn test_read_then_assign_outer_var_captures() {
        let func = make_func(
            &[],
            vec![
                Stmt::Assign {
                    var: "count".to_string(),
                    value: Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(var_expr("count")),
                        right: Box::new(int_lit(1)),
                        span: span(),
                    },
                    span: span(),
                },
                Stmt::Return {
                    value: Some(var_expr("count")),
                    span: span(),
                },
            ],
        );
        let outer = outer(&["count"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            result.contains("count"),
            "Expected 'count' captured (read-then-assign of outer local), got {:?}",
            result
        );
    }

    // NOTE: A test asserting `x = x + 1` SHADOWS an enclosing `x` (added by
    // 63ca645a8 alongside the `collect_function_local_bindings` shadow pre-pass)
    // was removed here: it directly contradicts `test_read_then_assign_outer_var_captures`
    // (the structurally identical `count = count + 1`) and upstream Julia, which
    // CAPTURES — `function f(); x=10; g()=(x=x+1); g(); x; end` returns 11.
    // Capture-on-assign is the correct semantics (Issues #7618/#7619/#7685).

    /// For loop variable is treated as local, not captured.
    #[test]
    fn test_for_loop_var_is_local() {
        // body: for i in 1:n; end  -- i is local, n is captured
        let func = make_func(
            &[],
            vec![Stmt::For {
                var: "i".to_string(),
                start: int_lit(1),
                end: var_expr("n"),
                step: None,
                body: Block {
                    stmts: vec![],
                    span: span(),
                },
                span: span(),
            }],
        );
        let outer = outer(&["i", "n"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            result.contains("n"),
            "Expected 'n' in free vars, got {:?}",
            result
        );
        assert!(
            !result.contains("i"),
            "Expected 'i' NOT in free vars (loop var), got {:?}",
            result
        );
    }

    /// Variables not in outer_scope_vars are never reported as free variables.
    #[test]
    fn test_unknown_var_not_captured() {
        // body: return z  -- z is not in outer scope
        let func = make_func(
            &[],
            vec![Stmt::Return {
                value: Some(var_expr("z")),
                span: span(),
            }],
        );
        let outer = outer(&["x", "y"]); // z is NOT in outer scope
        let result = analyze_free_variables(&func, &outer);
        assert!(
            !result.contains("z"),
            "Expected 'z' NOT in free vars (not in outer scope), got {:?}",
            result
        );
    }

    /// Multiple outer variables can be captured simultaneously.
    #[test]
    fn test_multiple_captures() {
        // body: return x + y  -- both x and y are captured
        let func = make_func(
            &[],
            vec![Stmt::Return {
                value: Some(Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(var_expr("x")),
                    right: Box::new(var_expr("y")),
                    span: span(),
                }),
                span: span(),
            }],
        );
        let outer = outer(&["x", "y", "z"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            result.contains("x"),
            "Expected 'x' in free vars, got {:?}",
            result
        );
        assert!(
            result.contains("y"),
            "Expected 'y' in free vars, got {:?}",
            result
        );
        assert!(
            !result.contains("z"),
            "Expected 'z' NOT in free vars (not referenced), got {:?}",
            result
        );
    }

    /// Comprehension variable is local inside the comprehension body.
    #[test]
    fn test_comprehension_var_is_local() {
        // body: expr = [x * n for x in arr]  -- x is local, n and arr are captured
        let func = make_func(
            &[],
            vec![Stmt::Expr {
                expr: Expr::Comprehension {
                    body: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(var_expr("x")),
                        right: Box::new(var_expr("n")),
                        span: span(),
                    }),
                    var: "x".to_string(),
                    iter: Box::new(var_expr("arr")),
                    filter: None,
                    span: span(),
                },
                span: span(),
            }],
        );
        let outer = outer(&["x", "n", "arr"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(
            !result.contains("x"),
            "Expected 'x' NOT in free vars (comprehension var), got {:?}",
            result
        );
        assert!(
            result.contains("n"),
            "Expected 'n' in free vars, got {:?}",
            result
        );
        assert!(
            result.contains("arr"),
            "Expected 'arr' in free vars, got {:?}",
            result
        );
    }
}
