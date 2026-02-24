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

use crate::ir::core::{Block, Expr, Function, Stmt};

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

    // Analyze the function body
    analyze_block_free_vars(
        &func.body,
        outer_scope_vars,
        &mut local_vars,
        &mut free_vars,
    );

    free_vars
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
            // Analyze the value first (RHS may reference outer variables)
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
            // Then mark var as local (simple assignments create local bindings)
            local_vars.insert(var.clone());
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
        Stmt::FunctionDef { func, .. } => {
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
        // Statements that don't introduce variables or reference expressions
        Stmt::Break { .. }
        | Stmt::Continue { .. }
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
        Expr::Call { args, kwargs, .. } => {
            // function is a String, not an Expr
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
            comp_local.insert(var.clone());
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
        Expr::DynamicTypeConstruct { type_args, .. } => {
            for arg in type_args {
                analyze_expr_free_vars(arg, outer_scope_vars, local_vars, free_vars);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            analyze_expr_free_vars(constructor, outer_scope_vars, local_vars, free_vars);
        }
        Expr::AssignExpr { value, var, .. } => {
            analyze_expr_free_vars(value, outer_scope_vars, local_vars, free_vars);
            // var becomes local after assignment
            // But we need to check - is this intentionally not tracked?
            // For now, don't track it to be conservative
            let _ = var;
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
        assert!(result.contains("x"), "Expected 'x' in free vars, got {:?}", result);
        assert!(!result.contains("y"), "Expected 'y' NOT in free vars, got {:?}", result);
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
        assert!(result.is_empty(), "Expected no free vars (x is a param), got {:?}", result);
    }

    /// A local assignment shadows an outer variable for subsequent reads.
    #[test]
    fn test_local_assign_shadows_outer_var() {
        // body: x = 1; return x  -- x is assigned locally, not captured
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
        assert!(result.is_empty(), "Expected no free vars (x is locally assigned), got {:?}", result);
    }

    /// The RHS of an assignment is evaluated before the LHS becomes local.
    #[test]
    fn test_rhs_evaluated_before_assign() {
        // body: y = x + 1  -- x is used on RHS (outer), y becomes local
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
        let outer = outer(&["x", "y"]);
        let result = analyze_free_variables(&func, &outer);
        assert!(result.contains("x"), "Expected 'x' in free vars, got {:?}", result);
        // y is assigned locally, not captured
        assert!(!result.contains("y"), "Expected 'y' NOT in free vars, got {:?}", result);
    }

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
        assert!(result.contains("n"), "Expected 'n' in free vars, got {:?}", result);
        assert!(!result.contains("i"), "Expected 'i' NOT in free vars (loop var), got {:?}", result);
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
        assert!(!result.contains("z"), "Expected 'z' NOT in free vars (not in outer scope), got {:?}", result);
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
        assert!(result.contains("x"), "Expected 'x' in free vars, got {:?}", result);
        assert!(result.contains("y"), "Expected 'y' in free vars, got {:?}", result);
        assert!(!result.contains("z"), "Expected 'z' NOT in free vars (not referenced), got {:?}", result);
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
        assert!(!result.contains("x"), "Expected 'x' NOT in free vars (comprehension var), got {:?}", result);
        assert!(result.contains("n"), "Expected 'n' in free vars, got {:?}", result);
        assert!(result.contains("arr"), "Expected 'arr' in free vars, got {:?}", result);
    }
}
