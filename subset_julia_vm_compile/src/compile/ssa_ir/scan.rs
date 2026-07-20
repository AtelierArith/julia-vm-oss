//! Syntactic read/write scanners used by the Core IR → SSA conversion
//! (Issue #8550).
//!
//! These walkers back the opaque-barrier model of `build.rs`:
//!
//! * **Reads** over-approximate the variable names an expression/statement may
//!   read. Over-approximation is sound: a spurious read only adds an operand
//!   to an opaque op; names with no local binding are skipped at resolution.
//! * **Writes** over-approximate the local bindings a statement may rebind.
//!   Over-approximation is sound (an extra `BarrierReload` merely loses
//!   precision); *under*-approximation would break SSA dominance, so every
//!   construct that can syntactically rebind an enclosing-scope name must be
//!   covered here. Construct-scoped binders (loop variables, `let` bindings,
//!   comprehension variables, `catch` variables) are subtracted because Julia
//!   scopes them to the construct.
//!
//! All matches are exhaustive (no `_` arm) so future Core IR variants force a
//! review of both scanners.

use std::collections::BTreeSet;

use crate::ir::core::{decode_tuple_comprehension_binding, Block, Expr, Stmt};

/// Variable names possibly read by `expr`, in first-occurrence order.
pub(super) fn expr_read_names(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    expr_reads(expr, &mut out);
    dedup_in_order(out)
}

/// Variable names possibly read by `stmt`, in first-occurrence order.
pub(super) fn stmt_read_names(stmt: &Stmt) -> Vec<String> {
    let mut out = Vec::new();
    stmt_reads(stmt, &mut out);
    dedup_in_order(out)
}

fn dedup_in_order(names: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

fn block_reads(block: &Block, out: &mut Vec<String>) {
    for stmt in &block.stmts {
        stmt_reads(stmt, out);
    }
}

fn stmt_reads(stmt: &Stmt, out: &mut Vec<String>) {
    match stmt {
        Stmt::Block(block) => block_reads(block, out),
        Stmt::Assign { value, .. } => expr_reads(value, out),
        Stmt::AddAssign { var, value, .. } => {
            out.push(var.clone());
            expr_reads(value, out);
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            var: _,
            span: _,
        } => {
            expr_reads(start, out);
            expr_reads(end, out);
            if let Some(step) = step {
                expr_reads(step, out);
            }
            block_reads(body, out);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            expr_reads(iterable, out);
            block_reads(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            expr_reads(condition, out);
            block_reads(body, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_reads(condition, out);
            block_reads(then_branch, out);
            if let Some(else_branch) = else_branch {
                block_reads(else_branch, out);
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            catch_var: _,
            span: _,
        } => {
            block_reads(try_block, out);
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                block_reads(block, out);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                expr_reads(value, out);
            }
        }
        Stmt::Expr { expr, .. } => expr_reads(expr, out),
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => block_reads(body, out),
        Stmt::Test { condition, .. } => expr_reads(condition, out),
        Stmt::TestThrows { expr, .. } => expr_reads(expr, out),
        Stmt::IndexAssign {
            array,
            indices,
            value,
            ..
        } => {
            out.push(array.clone());
            for index in indices {
                expr_reads(index, out);
            }
            expr_reads(value, out);
        }
        Stmt::FieldAssign { object, value, .. } => {
            out.push(object.clone());
            expr_reads(value, out);
        }
        Stmt::DestructuringAssign { value, .. } => expr_reads(value, out),
        Stmt::DictAssign {
            dict, key, value, ..
        } => {
            out.push(dict.clone());
            expr_reads(key, out);
            expr_reads(value, out);
        }
        // A nested function definition captures enclosing locals it reads;
        // over-approximate them as reads of the definition statement.
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            block_reads(&func.body, out)
        }
        Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::RuntimeNominalDef { .. }
        | Stmt::Global { .. } => {}
    }
}

fn expr_reads(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Var(name, _) => out.push(name.to_string()),
        Expr::Literal(..)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        // FunctionRef names are resolved to function indices at compile time,
        // not local variable reads.
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
        Expr::BinaryOp { left, right, .. } | Expr::Pair { key: left, value: right, .. } => {
            expr_reads(left, out);
            expr_reads(right, out);
        }
        Expr::UnaryOp { operand, .. } | Expr::Convert { operand, .. } => expr_reads(operand, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                expr_reads(arg, out);
            }
            for (_, value) in kwargs {
                expr_reads(value, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                expr_reads(arg, out);
            }
        }
        Expr::ArrayLiteral { elements, .. }
        | Expr::TupleLiteral { elements, .. }
        | Expr::StringConcat { parts: elements, .. } => {
            for element in elements {
                expr_reads(element, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            expr_reads(array, out);
            for index in indices {
                expr_reads(index, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_reads(start, out);
            if let Some(step) = step {
                expr_reads(step, out);
            }
            expr_reads(stop, out);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            expr_reads(body, out);
            expr_reads(iter, out);
            if let Some(filter) = filter {
                expr_reads(filter, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            expr_reads(body, out);
            for (_, iter) in iterations {
                expr_reads(iter, out);
            }
            if let Some(filter) = filter {
                expr_reads(filter, out);
            }
        }
        Expr::FieldAccess { object, .. } => expr_reads(object, out),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                expr_reads(value, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                expr_reads(key, out);
                expr_reads(value, out);
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                expr_reads(value, out);
            }
            block_reads(body, out);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_reads(condition, out);
            expr_reads(then_expr, out);
            expr_reads(else_expr, out);
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                expr_reads(base_expr, out);
            }
            for type_arg in type_args {
                expr_reads(type_arg, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => expr_reads(constructor, out),
        Expr::AssignExpr { value, .. } => expr_reads(value, out),
        Expr::ReturnExpr { value, .. } => {
            if let Some(value) = value {
                expr_reads(value, out);
            }
        }
    }
}

/// Local binding names possibly (re)bound by the statements of `block`.
pub(super) fn block_write_names(block: &Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        stmt_write_names(stmt, out);
    }
}

/// Local binding names possibly (re)bound by `stmt`.
pub(super) fn stmt_write_names(stmt: &Stmt, out: &mut BTreeSet<String>) {
    match stmt {
        Stmt::Block(block) => block_write_names(block, out),
        Stmt::Assign { var, value, .. } => {
            insert_assign_targets(var, out);
            expr_write_names(value, out);
        }
        Stmt::AddAssign { var, value, .. } => {
            out.insert(var.to_string());
            expr_write_names(value, out);
        }
        Stmt::For {
            var,
            start,
            end,
            step,
            body,
            ..
        } => {
            expr_write_names(start, out);
            expr_write_names(end, out);
            if let Some(step) = step {
                expr_write_names(step, out);
            }
            merge_minus(out, scoped_block_writes(body), &[var.as_str()]);
        }
        Stmt::ForEach {
            var,
            iterable,
            body,
            ..
        } => {
            expr_write_names(iterable, out);
            merge_minus(out, scoped_block_writes(body), &[var.as_str()]);
        }
        Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            ..
        } => {
            expr_write_names(iterable, out);
            let bound: Vec<&str> = vars.iter().map(String::as_str).collect();
            merge_minus(out, scoped_block_writes(body), &bound);
        }
        Stmt::While {
            condition, body, ..
        } => {
            expr_write_names(condition, out);
            block_write_names(body, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_write_names(condition, out);
            block_write_names(then_branch, out);
            if let Some(else_branch) = else_branch {
                block_write_names(else_branch, out);
            }
        }
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            span: _,
        } => {
            block_write_names(try_block, out);
            if let Some(catch_block) = catch_block {
                let bound: Vec<&str> = catch_var.iter().map(String::as_str).collect();
                merge_minus(out, scoped_block_writes(catch_block), &bound);
            }
            for block in [else_block, finally_block].into_iter().flatten() {
                block_write_names(block, out);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                expr_write_names(value, out);
            }
        }
        Stmt::Expr { expr, .. } => expr_write_names(expr, out),
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => block_write_names(body, out),
        Stmt::Test { condition, .. } => expr_write_names(condition, out),
        Stmt::TestThrows { expr, .. } => expr_write_names(expr, out),
        // Mutation statements update contents, not the local binding; only
        // their sub-expressions can rebind (via embedded AssignExpr).
        Stmt::IndexAssign { indices, value, .. } => {
            for index in indices {
                expr_write_names(index, out);
            }
            expr_write_names(value, out);
        }
        Stmt::FieldAssign { value, .. } => expr_write_names(value, out),
        Stmt::DestructuringAssign { targets, value, .. } => {
            for target in targets {
                out.insert(target.clone());
            }
            expr_write_names(value, out);
        }
        Stmt::DictAssign { key, value, .. } => {
            expr_write_names(key, out);
            expr_write_names(value, out);
        }
        // A nested definition binds the function name. Its body is a separate
        // scope; rebinding of captured variables happens at *call* sites,
        // which this slice does not model (see docs/vm/SSA_IR.md).
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            out.insert(func.name.clone());
        }
        Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::RuntimeNominalDef { .. }
        // `global x` routes writes to the global frame; it binds nothing
        // locally (build.rs collects the declared names separately).
        | Stmt::Global { .. } => {}
    }
}

/// Local binding names possibly (re)bound while evaluating `expr`
/// (embedded `AssignExpr`s and statement blocks in expression position).
pub(super) fn expr_write_names(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::AssignExpr { var, value, .. } => {
            out.insert(var.to_string());
            expr_write_names(value, out);
        }
        Expr::Literal(..)
        | Expr::Var(..)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
        Expr::BinaryOp { left, right, .. }
        | Expr::Pair {
            key: left,
            value: right,
            ..
        } => {
            expr_write_names(left, out);
            expr_write_names(right, out);
        }
        Expr::UnaryOp { operand, .. } | Expr::Convert { operand, .. } => {
            expr_write_names(operand, out)
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                expr_write_names(arg, out);
            }
            for (_, value) in kwargs {
                expr_write_names(value, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                expr_write_names(arg, out);
            }
        }
        Expr::ArrayLiteral { elements, .. }
        | Expr::TupleLiteral { elements, .. }
        | Expr::StringConcat {
            parts: elements, ..
        } => {
            for element in elements {
                expr_write_names(element, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            expr_write_names(array, out);
            for index in indices {
                expr_write_names(index, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_write_names(start, out);
            if let Some(step) = step {
                expr_write_names(step, out);
            }
            expr_write_names(stop, out);
        }
        // Comprehension/generator binding variables are scoped to the
        // comprehension; assignments in body/filter otherwise leak to the
        // enclosing soft scope, so they are kept.
        Expr::Comprehension {
            body,
            var,
            iter,
            filter,
            ..
        }
        | Expr::Generator {
            body,
            var,
            iter,
            filter,
            ..
        } => {
            expr_write_names(iter, out);
            let mut inner = BTreeSet::new();
            expr_write_names(body, &mut inner);
            if let Some(filter) = filter {
                expr_write_names(filter, &mut inner);
            }
            merge_minus(out, inner, &[var.as_str()]);
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            for (_, iter) in iterations {
                expr_write_names(iter, out);
            }
            let mut inner = BTreeSet::new();
            expr_write_names(body, &mut inner);
            if let Some(filter) = filter {
                expr_write_names(filter, &mut inner);
            }
            let bound: Vec<&str> = iterations.iter().map(|(var, _)| var.as_str()).collect();
            merge_minus(out, inner, &bound);
        }
        Expr::FieldAccess { object, .. } => expr_write_names(object, out),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                expr_write_names(value, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                expr_write_names(key, out);
                expr_write_names(value, out);
            }
        }
        // `let` bindings are scoped to the let block; other assignments in
        // the body leak to the enclosing soft scope.
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                expr_write_names(value, out);
            }
            let bound: Vec<&str> = bindings.iter().map(|(name, _)| name.as_str()).collect();
            merge_minus(out, scoped_block_writes(body), &bound);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_write_names(condition, out);
            expr_write_names(then_expr, out);
            expr_write_names(else_expr, out);
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                expr_write_names(base_expr, out);
            }
            for type_arg in type_args {
                expr_write_names(type_arg, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => expr_write_names(constructor, out),
        Expr::ReturnExpr { value, .. } => {
            if let Some(value) = value {
                expr_write_names(value, out);
            }
        }
    }
}

fn scoped_block_writes(block: &Block) -> BTreeSet<String> {
    let mut inner = BTreeSet::new();
    block_write_names(block, &mut inner);
    inner
}

fn merge_minus(out: &mut BTreeSet<String>, inner: BTreeSet<String>, bound: &[&str]) {
    for name in inner {
        if !bound.contains(&name.as_str()) {
            out.insert(name);
        }
    }
}

fn insert_assign_targets(var: &str, out: &mut BTreeSet<String>) {
    // Lowering encodes tuple comprehension bindings as a single Assign with a
    // prefixed variable holding all target names.
    if let Some(vars) = decode_tuple_comprehension_binding(var) {
        out.extend(vars);
    } else {
        out.insert(var.to_string());
    }
}

/// Collect names declared `global` anywhere in the statement-level blocks of
/// `block`. Nested function definitions are separate scopes and are skipped;
/// `global` inside expression-position `let` bodies is not tracked in this
/// slice (documented in docs/vm/SSA_IR.md).
pub(super) fn collect_global_decls(block: &Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        collect_global_decls_stmt(stmt, out);
    }
}

fn collect_global_decls_stmt(stmt: &Stmt, out: &mut BTreeSet<String>) {
    match stmt {
        Stmt::Global { names, .. } => out.extend(names.iter().cloned()),
        Stmt::Block(block) => collect_global_decls(block, out),
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Timed { body, .. }
        | Stmt::TestSet { body, .. } => collect_global_decls(body, out),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_global_decls(then_branch, out);
            if let Some(else_branch) = else_branch {
                collect_global_decls(else_branch, out);
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_global_decls(try_block, out);
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                collect_global_decls(block, out);
            }
        }
        Stmt::Assign { .. }
        | Stmt::AddAssign { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Return { .. }
        | Stmt::Expr { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Test { .. }
        | Stmt::TestThrows { .. }
        | Stmt::IndexAssign { .. }
        | Stmt::FieldAssign { .. }
        | Stmt::DestructuringAssign { .. }
        | Stmt::DictAssign { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::FunctionDef { .. }
        | Stmt::EvalFunctionDef { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::RuntimeNominalDef { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::test_helpers::{call_expr, int_lit, var_expr, zero_span};

    fn block_of(stmts: Vec<Stmt>) -> Block {
        Block {
            stmts,
            span: zero_span(),
        }
    }

    #[test]
    fn expr_read_names_dedups_in_order() {
        let expr = call_expr("f", vec![var_expr("b"), var_expr("a"), var_expr("b")]);
        assert_eq!(expr_read_names(&expr), vec!["b", "a"]);
    }

    #[test]
    fn expr_read_names_skips_assign_target_but_visits_value() {
        let expr = Expr::AssignExpr {
            var: "x".to_string().into(),
            value: Box::new(var_expr("y")),
            span: zero_span(),
        };
        assert_eq!(expr_read_names(&expr), vec!["y"]);
    }

    #[test]
    fn stmt_write_names_covers_nested_branches() {
        let stmt = Stmt::If {
            condition: var_expr("c"),
            then_branch: block_of(vec![Stmt::Assign {
                var: "x".to_string(),
                value: int_lit(1),
                span: zero_span(),
            }]),
            else_branch: Some(block_of(vec![Stmt::AddAssign {
                var: "y".to_string(),
                value: int_lit(2),
                span: zero_span(),
            }])),
            span: zero_span(),
        };
        let mut out = BTreeSet::new();
        stmt_write_names(&stmt, &mut out);
        assert_eq!(
            out.into_iter().collect::<Vec<_>>(),
            vec!["x".to_string(), "y".to_string()]
        );
    }

    #[test]
    fn stmt_write_names_scopes_loop_variable() {
        let stmt = Stmt::ForEach {
            var: "i".to_string(),
            iterable: var_expr("xs"),
            body: block_of(vec![Stmt::Assign {
                var: "acc".to_string(),
                value: var_expr("i"),
                span: zero_span(),
            }]),
            span: zero_span(),
        };
        let mut out = BTreeSet::new();
        stmt_write_names(&stmt, &mut out);
        assert_eq!(out.into_iter().collect::<Vec<_>>(), vec!["acc".to_string()]);
    }

    #[test]
    fn collect_global_decls_recurses_statement_blocks() {
        let block = block_of(vec![Stmt::While {
            condition: var_expr("c"),
            body: block_of(vec![Stmt::Global {
                names: vec!["g".to_string()],
                span: zero_span(),
            }]),
            span: zero_span(),
        }]);
        let mut out = BTreeSet::new();
        collect_global_decls(&block, &mut out);
        assert_eq!(out.into_iter().collect::<Vec<_>>(), vec!["g".to_string()]);
    }

    #[test]
    fn collect_global_decls_empty_block_is_empty() {
        let block = block_of(vec![]);
        let mut out = BTreeSet::new();
        collect_global_decls(&block, &mut out);
        assert!(out.is_empty());
    }
}
