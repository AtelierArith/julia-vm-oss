//! Reverse the #9103 / #9127 generator lift for the AoT backend
//! (Issues #9179 / #9292).
//!
//! The lowering phase (`lower_generator_expr` / `lift_generator_as_nested` in
//! `crate::lowering::expr::collection`) rewrites a generator the compiler cannot
//! lower lazily inline into a nested-function form so the VM can build a lazy,
//! runtime-callable generator that participates in closure analysis:
//!
//! ```text
//! (body for var in iter if pred)
//! =>
//! let
//!     function __gen_body_N(var); return body; end
//!     function __gen_pred_N(var); return pred; end
//!     (__gen_body_N(var) for var in iter if __gen_pred_N(var))
//! end
//! ```
//!
//! The #9103 lift covered filterless generators with a non-trivial body; the
//! #9127 lift extended it to filtered generators (lifting the predicate into
//! `__gen_pred_N`) and to tuple-destructuring bindings (`(a, b) in pairs`), which
//! take a single synthetic tuple parameter and a destructuring prologue
//! (`a = arg[1]; b = arg[2]`) injected into each synthetic function.
//!
//! In expression position (a `collect(...)` / `sum(...)` argument) this
//! `Expr::LetBlock` reaches the AoT IR converter, which rejects a
//! multi-statement expression-position `let` block with the #7014 diagnostic,
//! and — even once accepted — AoT type inference does not descend into the
//! generator to specialize `__gen_body_N` by the loop-variable type, so the
//! surrounding binding widens to `Any`. A dangling `__gen_pred_N(var)` filter
//! whose definition was dropped is worse still: the AoT converter sees an
//! `Any`-typed control-flow condition and rejects it (#9292).
//!
//! This module reverses the lift for AoT by inlining the trivial calls to the
//! nested body/predicate functions back into the generator (substituting the
//! tuple-destructuring prologue with inline index expressions), restoring the
//! pre-lift inline generator shape. Running it as a whole-program pass *before*
//! type inference lets both inference and codegen see the concrete element type.
//! The VM path is unaffected — it keeps the lifted form it relies on.

use crate::ir::core::{Block, Expr, Function, Literal, Program, Stmt};
use std::collections::HashMap;

/// Reverse every expression-position generator-body lift in `program` in place,
/// across the main block and all top-level / module function bodies.
///
/// Intended to run before AoT type inference so the reversed inline generator is
/// visible to both inference and IR conversion.
pub fn reverse_generator_lifts_in_program(program: &mut Program) {
    reverse_in_block(&mut program.main);
    // `program.functions` holds `Arc<Function>`; `Arc::make_mut` gives a mutable
    // (copy-on-write) view so a lift inside a user-function body is reversed too.
    for func in &mut program.functions {
        reverse_in_block(&mut std::sync::Arc::make_mut(func).body);
    }
    for module in &mut program.modules {
        reverse_generator_lifts_in_module(module);
    }
}

fn reverse_generator_lifts_in_module(module: &mut crate::ir::core::Module) {
    reverse_in_block(&mut module.body);
    for func in &mut module.functions {
        reverse_in_block(&mut func.body);
    }
    for submodule in &mut module.submodules {
        reverse_generator_lifts_in_module(submodule);
    }
}

fn reverse_in_block(block: &mut Block) {
    for stmt in &mut block.stmts {
        reverse_in_stmt(stmt);
    }
}

fn reverse_in_stmt(stmt: &mut Stmt) {
    match stmt {
        Stmt::Block(block) => reverse_in_block(block),
        Stmt::Assign { value, .. } => reverse_in_expr(value),
        Stmt::AddAssign { value, .. } => reverse_in_expr(value),
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            reverse_in_expr(start);
            reverse_in_expr(end);
            if let Some(step) = step {
                reverse_in_expr(step);
            }
            reverse_in_block(body);
        }
        Stmt::ForEach { iterable, body, .. } => {
            reverse_in_expr(iterable);
            reverse_in_block(body);
        }
        Stmt::ForEachTuple { iterable, body, .. } => {
            reverse_in_expr(iterable);
            reverse_in_block(body);
        }
        Stmt::While {
            condition, body, ..
        } => {
            reverse_in_expr(condition);
            reverse_in_block(body);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            reverse_in_expr(condition);
            reverse_in_block(then_branch);
            if let Some(else_branch) = else_branch {
                reverse_in_block(else_branch);
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            finally_block,
            else_block,
            ..
        } => {
            reverse_in_block(try_block);
            if let Some(catch_block) = catch_block {
                reverse_in_block(catch_block);
            }
            if let Some(finally_block) = finally_block {
                reverse_in_block(finally_block);
            }
            if let Some(else_block) = else_block {
                reverse_in_block(else_block);
            }
        }
        Stmt::Return {
            value: Some(value), ..
        } => reverse_in_expr(value),
        Stmt::Expr { expr, .. } => reverse_in_expr(expr),
        Stmt::Timed { body, .. } => reverse_in_block(body),
        // `array` is the target name (a `String`), only the indices and value
        // are expressions.
        Stmt::IndexAssign { indices, value, .. } => {
            for index in indices {
                reverse_in_expr(index);
            }
            reverse_in_expr(value);
        }
        // `object` is the target name (a `String`).
        Stmt::FieldAssign { value, .. } => reverse_in_expr(value),
        Stmt::DestructuringAssign { value, .. } => reverse_in_expr(value),
        // `dict` is the target name (a `String`).
        Stmt::DictAssign { key, value, .. } => {
            reverse_in_expr(key);
            reverse_in_expr(value);
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            reverse_in_block(&mut func.body);
        }
        // Remaining statement forms carry no expression-position generator lift.
        _ => {}
    }
}

/// Recurse into all sub-expressions of `expr`, then reverse `expr` itself if it
/// is a lift-shaped `let` block.
fn reverse_in_expr(expr: &mut Expr) {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            reverse_in_expr(left);
            reverse_in_expr(right);
        }
        Expr::UnaryOp { operand, .. } => reverse_in_expr(operand),
        Expr::Call { args, kwargs, .. } => {
            for arg in args {
                reverse_in_expr(arg);
            }
            for (_, value) in kwargs {
                reverse_in_expr(value);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                reverse_in_expr(arg);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                reverse_in_expr(element);
            }
        }
        Expr::Index { array, indices, .. } => {
            reverse_in_expr(array);
            for index in indices {
                reverse_in_expr(index);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            reverse_in_expr(start);
            if let Some(step) = step {
                reverse_in_expr(step);
            }
            reverse_in_expr(stop);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            reverse_in_expr(body);
            reverse_in_expr(iter);
            if let Some(filter) = filter {
                reverse_in_expr(filter);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            reverse_in_expr(body);
            for (_, iter) in iterations {
                reverse_in_expr(iter);
            }
            if let Some(filter) = filter {
                reverse_in_expr(filter);
            }
        }
        Expr::FieldAccess { object, .. } => reverse_in_expr(object),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                reverse_in_expr(value);
            }
        }
        Expr::Pair { key, value, .. } => {
            reverse_in_expr(key);
            reverse_in_expr(value);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                reverse_in_expr(key);
                reverse_in_expr(value);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                reverse_in_expr(part);
            }
        }
        Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                reverse_in_expr(arg);
            }
            for (_, value) in kwargs {
                reverse_in_expr(value);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            reverse_in_expr(condition);
            reverse_in_expr(then_expr);
            reverse_in_expr(else_expr);
        }
        Expr::New { args, .. } => {
            for arg in args {
                reverse_in_expr(arg);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                reverse_in_expr(base_expr);
            }
            for type_arg in type_args {
                reverse_in_expr(type_arg);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => reverse_in_expr(constructor),
        Expr::AssignExpr { value, .. } => reverse_in_expr(value),
        Expr::ReturnExpr {
            value: Some(value), ..
        } => reverse_in_expr(value),
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                reverse_in_expr(value);
            }
            reverse_in_block(body);
        }
        // Leaf forms with no sub-expressions.
        _ => {}
    }

    // After normalizing children, reverse this node if it is a lift-shaped block.
    if let Expr::LetBlock { bindings, body, .. } = expr {
        if let Some(rewritten) = reverse_lifted_letblock(bindings, body) {
            *expr = rewritten;
        }
    }

    // The Issue #9200 (S2) simple-generator desugar's identity form is a bare
    // `Generator(identity, iter)` call (no `let` block); reverse it here too.
    if let Some(rewritten) = reverse_identity_generator_call(expr) {
        *expr = rewritten;
    }
}

/// Reverse the #9103 / #9127 generator lift when its `let` block appears in
/// expression position (Issues #9179 / #9292).
///
/// Recognizes a bindings-free `let` block whose body is one or more nested
/// function definitions followed by a single trailing generator (or bare call)
/// — the exact shape `lift_generator_as_nested` emits. Returns the trailing
/// generator with the trivial calls to those nested body/predicate functions
/// inlined, so a caller can use it directly (the nested definitions are dropped,
/// avoiding a dead standalone function that would trip `-D warnings`).
///
/// Returns `None` when the block is not of this shape or a nested call cannot be
/// inlined safely, in which case the AoT converter emits the #7014 diagnostic.
pub(crate) fn reverse_lifted_letblock(
    bindings: &[(crate::ir::core::InternedStr, Expr)],
    body: &Block,
) -> Option<Expr> {
    if !bindings.is_empty() {
        return None;
    }
    let (last, prefix) = body.stmts.split_last()?;
    // The leading statements must all be nested function definitions, and there
    // must be at least one (the single-trailing-expression case is handled by an
    // earlier fast path in the converter).
    if prefix.is_empty()
        || !prefix
            .iter()
            .all(|stmt| matches!(stmt, Stmt::FunctionDef { .. }))
    {
        return None;
    }
    let mut nested: HashMap<&str, &Function> = HashMap::new();
    for stmt in prefix {
        if let Stmt::FunctionDef { func, .. } = stmt {
            nested.insert(func.name.as_str(), func.as_ref());
        }
    }
    // The trailing statement must be a single value expression.
    let Stmt::Expr { expr: trailing, .. } = last else {
        return None;
    };

    match trailing {
        // The lift's trailing expression is a generator whose body is the call to
        // the nested `__gen_body_N` function and, for a filtered generator
        // (Issue #9127), whose filter is the call to the nested `__gen_pred_N`
        // function. Inline both calls so the reversed generator carries inline
        // body/filter expressions — the eager/inline shape AoT supports — instead
        // of a dangling call to the dropped nested predicate, which the AoT
        // converter would otherwise see as an `Any`-typed control-flow condition
        // (Issue #9292).
        Expr::Generator {
            body: gen_body,
            var,
            iter,
            filter,
            span,
        } => {
            let Expr::Call { function, args, .. } = gen_body.as_ref() else {
                return None;
            };
            let inlined_body = inline_trivial_nested_call(function, args, &nested)?;
            let inlined_filter = match filter {
                Some(filter_expr) => Some(Box::new(inline_lifted_predicate(filter_expr, &nested)?)),
                None => None,
            };
            Some(Expr::Generator {
                body: Box::new(inlined_body),
                var: *var,
                iter: iter.clone(),
                filter: inlined_filter,
                span: *span,
            })
        }
        // The Issue #9200 (S2) simple-generator desugar's trailing expression is a
        // `Generator(__gen_body_N, iter)` call whose first argument is the nested
        // lifted body function passed by value. Reverse it to an inline
        // `Expr::Generator` whose body is that function's returned expression and
        // whose loop variable is its single parameter, so AoT inference / codegen
        // see the concrete element type instead of the #7014-rejected `let` block.
        Expr::Call {
            function,
            args,
            span,
            ..
        } if is_generator_ctor_name(function) && args.len() == 2 => {
            // Issue #9200 (S3): the FILTERED desugar's trailing expression is
            // `Generator(map, Filter(__gen_pred_N, base))`. Reverse it to an
            // inline FILTERED `Expr::Generator` (body + filter + base) before it
            // continues to the simple (unfiltered) reversal below.
            if let Some(filtered) =
                reverse_generator_over_filter(&args[0], &args[1], &nested, *span)
            {
                return Some(filtered);
            }
            let fname = match &args[0] {
                Expr::Var(name, _) | Expr::FunctionRef { name, .. } => name.as_str(),
                _ => return None,
            };
            let func = nested.get(fname)?;
            // Only the simple scalar-body shape is desugared this way: exactly one
            // parameter and a single trailing `return <expr>` with no prologue.
            let [param] = func.params.as_slice() else {
                return None;
            };
            let (last, prologue) = func.body.stmts.split_last()?;
            if !prologue.is_empty() {
                return None;
            }
            let Stmt::Return {
                value: Some(body_expr),
                ..
            } = last
            else {
                return None;
            };
            Some(Expr::Generator {
                body: Box::new(body_expr.clone()),
                var: param.name.clone().into(),
                iter: Box::new(args[1].clone()),
                filter: None,
                span: *span,
            })
        }
        // Also accept a bare trailing call to a nested function.
        Expr::Call { function, args, .. } => inline_trivial_nested_call(function, args, &nested),
        _ => None,
    }
}

/// Whether `name` refers to the `Base.Generator` constructor (Issue #9200 S2).
/// The simple-generator desugar emits the unqualified `Generator`; a hand-written
/// `Base.Generator(...)` is accepted too.
fn is_generator_ctor_name(name: &str) -> bool {
    name == "Generator" || name == "Base.Generator"
}

/// Whether `name` refers to the `Iterators.Filter` constructor (Issue #9200 S3).
fn is_filter_ctor_name(name: &str) -> bool {
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    base == "Filter"
}

/// Reverse the Issue #9200 (S3) FILTERED-generator desugar
/// `Generator(map, Filter(__gen_pred_N, base))` back to an inline FILTERED
/// `Expr::Generator { body, var, iter: base, filter }`.
///
/// `map` is either `identity` (the body is the loop variable) or a lifted
/// `__gen_body_N` passed by value; `__gen_pred_N` is the lifted predicate. Both
/// lifted functions share the single scalar loop-variable parameter (no
/// destructuring prologue for the S3 single-scalar-binding shape). Returns `None`
/// when `iter_arg` is not a `Filter(...)` call of this exact lifted shape, so the
/// caller falls through to the unfiltered reversal / the #7014 diagnostic.
fn reverse_generator_over_filter(
    map_arg: &Expr,
    iter_arg: &Expr,
    nested: &HashMap<&str, &Function>,
    span: crate::span::Span,
) -> Option<Expr> {
    // The iterator argument must be a `Filter(pred, base)` construction call.
    let Expr::Call {
        function,
        args: filter_args,
        ..
    } = iter_arg
    else {
        return None;
    };
    if !is_filter_ctor_name(function) || filter_args.len() != 2 {
        return None;
    }
    let pred_name = callable_ref_name(&filter_args[0])?;
    let (pred_param, pred_body) = nested_scalar_lift_body(pred_name, nested)?;
    let base = &filter_args[1];

    // Resolve the map body: `identity` -> the loop variable itself; otherwise a
    // lifted `__gen_body_N` whose single parameter is the same loop variable. The
    // desugar (`desugar_filtered_generator`) always gives the body and predicate
    // functions the identical loop-variable parameter, so a spelling mismatch is
    // not the S3 shape — bail to the #7014 diagnostic instead of substituting.
    let (var, body) = match callable_ref_name(map_arg) {
        Some("identity") => (
            pred_param.clone(),
            Expr::Var(pred_param.clone().into(), span),
        ),
        Some(map_name) => {
            let (map_param, map_body) = nested_scalar_lift_body(map_name, nested)?;
            if map_param != pred_param {
                return None;
            }
            (map_param, map_body)
        }
        None => return None,
    };

    Some(Expr::Generator {
        body: Box::new(body),
        var: var.into(),
        iter: Box::new(base.clone()),
        filter: Some(Box::new(pred_body)),
        span,
    })
}

/// Callable name of a by-value function reference (`Var` / `FunctionRef`).
fn callable_ref_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Var(name, _) | Expr::FunctionRef { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

/// Extract the single scalar parameter name and single-`return` body expression of
/// a lifted `__gen_body_N` / `__gen_pred_N` function (Issue #9200 S3). Returns
/// `None` for any other shape (multiple params, a destructuring prologue, or a
/// non-`return` tail).
fn nested_scalar_lift_body(
    name: &str,
    nested: &HashMap<&str, &Function>,
) -> Option<(String, Expr)> {
    let func = nested.get(name)?;
    let [param] = func.params.as_slice() else {
        return None;
    };
    let (last, prologue) = func.body.stmts.split_last()?;
    if !prologue.is_empty() {
        return None;
    }
    let Stmt::Return {
        value: Some(body_expr),
        ..
    } = last
    else {
        return None;
    };
    Some((param.name.clone(), body_expr.clone()))
}

/// Reverse a desugared *identity* generator `Generator(identity, iter)` (Issue
/// #9200 S2) in expression position back to an inline `Expr::Generator`.
///
/// The identity form carries no lifted body function (there is no `let` block), so
/// `reverse_lifted_letblock` never sees it; it must be recognized on the bare call.
/// A fresh scalar loop variable is synthesized — it is bound only in the reversed
/// generator's body (a bare reference to itself), so it cannot collide with the
/// source iterator's free variables.
fn reverse_identity_generator_call(expr: &Expr) -> Option<Expr> {
    let Expr::Call {
        function,
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    } = expr
    else {
        return None;
    };
    if !is_generator_ctor_name(function)
        || args.len() != 2
        || !kwargs.is_empty()
        || splat_mask.iter().any(|&b| b)
        || kwargs_splat_mask.iter().any(|&b| b)
    {
        return None;
    }
    let is_identity = matches!(
        &args[0],
        Expr::Var(name, _) | Expr::FunctionRef { name, .. } if name == "identity"
    );
    if !is_identity {
        return None;
    }
    let var = "__gen_identity_var".to_string();
    Some(Expr::Generator {
        body: Box::new(Expr::Var(var.clone().into(), *span)),
        var: var.into(),
        iter: Box::new(args[1].clone()),
        filter: None,
        span: *span,
    })
}

/// Inline a lifted filter predicate (`__gen_pred_N(var)`) the same way the
/// generator body is inlined (Issue #9127 / #9292).
///
/// When the filter is a trivial call to one of the nested lift functions, inline
/// it back into the inline boolean expression AoT expects. A filter that is *not*
/// such a call (a predicate the compiler never lifted) is kept verbatim. Returns
/// `None` only when the filter is a call to a nested lift function that cannot be
/// inlined, so the caller falls back to the #7014 diagnostic.
fn inline_lifted_predicate(filter: &Expr, nested: &HashMap<&str, &Function>) -> Option<Expr> {
    if let Expr::Call { function, args, .. } = filter {
        if nested.contains_key(function.as_str()) {
            return inline_trivial_nested_call(function, args, nested);
        }
    }
    Some(filter.clone())
}

/// Inline a trivial call `f(p1, p2, …)` to a nested lift function, returning the
/// function's body with the parameters bound to the arguments.
///
/// Two lift shapes are inlined, both called as an *identity* call (each argument
/// is exactly the matching parameter as a variable reference):
///
/// * the #9103 scalar-body lift — a single-`return` body, inlined as a verbatim
///   clone of the returned expression;
/// * the #9127 tuple-destructuring lift — a pure destructuring prologue
///   (`bound_i = arg[idx_i]`, one plain literal index into the single tuple
///   parameter per bound name) followed by a single `return`. Because each
///   prologue binding is a pure index into the loop element, it is inlined by
///   substituting every free occurrence of `bound_i` in the returned expression
///   with its `arg[idx_i]` index expression. The reversed generator then binds
///   the element directly to `arg` and indexes it inline — a shape AoT supports.
///
/// Any other shape (a non-identity call, a prologue statement that is not a pure
/// tuple index, or a body whose scoping the substitution cannot model soundly)
/// returns `None`, so the caller falls back to the #7014 diagnostic.
fn inline_trivial_nested_call(
    function: &str,
    args: &[Expr],
    nested: &HashMap<&str, &Function>,
) -> Option<Expr> {
    let func = nested.get(function)?;
    if func.params.len() != args.len() {
        return None;
    }
    for (param, arg) in func.params.iter().zip(args) {
        match arg {
            Expr::Var(name, _) if name == &param.name => {}
            _ => return None,
        }
    }
    // The body is an optional pure destructuring prologue followed by a single
    // trailing `return <expr>`.
    let (last, prologue) = func.body.stmts.split_last()?;
    let Stmt::Return {
        value: Some(body_expr),
        ..
    } = last
    else {
        return None;
    };
    if prologue.is_empty() {
        // #9103 scalar-body lift: verbatim clone, cannot capture.
        return Some(body_expr.clone());
    }

    // #9127 tuple-destructuring lift: only the single-tuple-parameter shape
    // reaches here (the lift always emits exactly one synthetic parameter).
    let [param] = func.params.as_slice() else {
        return None;
    };
    let mut subst: HashMap<String, Expr> = HashMap::new();
    for stmt in prologue {
        // Each prologue statement must be `bound = arg[<int literal>]`, a pure
        // index into the tuple parameter. Anything else is not a destructuring
        // prologue and must not be inlined (it could carry side effects).
        let Stmt::Assign { var, value, .. } = stmt else {
            return None;
        };
        let Expr::Index { array, indices, .. } = value else {
            return None;
        };
        match array.as_ref() {
            Expr::Var(name, _) if name == &param.name => {}
            _ => return None,
        }
        if indices.len() != 1 || !matches!(&indices[0], Expr::Literal(Literal::Int(_), _)) {
            return None;
        }
        subst.insert(var.clone(), value.clone());
    }

    let mut inlined = body_expr.clone();
    if substitute_free_vars(&mut inlined, &subst) {
        Some(inlined)
    } else {
        None
    }
}

/// Substitute every free occurrence of the destructuring-bound names in `subst`
/// with their (pure) index expressions, in place.
///
/// The replacement expressions reference only the synthetic tuple parameter
/// (`__gen_arg_N`), which no user construct shadows, so capture *into* a
/// replacement is impossible. The only soundness concern is capture *of* a bound
/// name by an inner binder that reuses the same identifier; those scopes drop the
/// shadowed name before recursing.
///
/// Returns `false` (bail) when the expression contains a binding/assignment
/// construct whose scoping this pass does not model, so the caller can fall back
/// to the #7014 diagnostic instead of risking an unsound substitution.
fn substitute_free_vars(expr: &mut Expr, subst: &HashMap<String, Expr>) -> bool {
    match expr {
        Expr::Var(name, _) => {
            if let Some(replacement) = subst.get(name.as_str()) {
                *expr = replacement.clone();
            }
            true
        }
        Expr::BinaryOp { left, right, .. } => {
            substitute_free_vars(left, subst) && substitute_free_vars(right, subst)
        }
        Expr::UnaryOp { operand, .. } => substitute_free_vars(operand, subst),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            args.iter_mut().all(|arg| substitute_free_vars(arg, subst))
                && kwargs
                    .iter_mut()
                    .all(|(_, value)| substitute_free_vars(value, subst))
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            args.iter_mut().all(|arg| substitute_free_vars(arg, subst))
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => elements
            .iter_mut()
            .all(|element| substitute_free_vars(element, subst)),
        Expr::Index { array, indices, .. } => {
            substitute_free_vars(array, subst)
                && indices
                    .iter_mut()
                    .all(|index| substitute_free_vars(index, subst))
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            substitute_free_vars(start, subst)
                && step
                    .as_mut()
                    .map(|s| substitute_free_vars(s, subst))
                    .unwrap_or(true)
                && substitute_free_vars(stop, subst)
        }
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
            // The loop variable binds in the body and filter but not the source
            // iterator; drop it from the active substitution for the scoped parts.
            if !substitute_free_vars(iter, subst) {
                return false;
            }
            let mut shadowed;
            let inner = if subst.contains_key(var.as_str()) {
                shadowed = subst.clone();
                shadowed.remove(var.as_str());
                &shadowed
            } else {
                subst
            };
            substitute_free_vars(body, inner)
                && filter
                    .as_mut()
                    .map(|f| substitute_free_vars(f, inner))
                    .unwrap_or(true)
        }
        // A multi-clause comprehension that rebinds a substituted name: its
        // per-clause scoping is not modeled here — bail to the #7014 diagnostic.
        Expr::MultiComprehension { iterations, .. }
            if iterations
                .iter()
                .any(|(var, _)| subst.contains_key(var.as_str())) =>
        {
            false
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            iterations
                .iter_mut()
                .all(|(_, iter)| substitute_free_vars(iter, subst))
                && substitute_free_vars(body, subst)
                && filter
                    .as_mut()
                    .map(|f| substitute_free_vars(f, subst))
                    .unwrap_or(true)
        }
        Expr::FieldAccess { object, .. } => substitute_free_vars(object, subst),
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter_mut()
            .all(|(_, value)| substitute_free_vars(value, subst)),
        Expr::Pair { key, value, .. } => {
            substitute_free_vars(key, subst) && substitute_free_vars(value, subst)
        }
        Expr::DictLiteral { pairs, .. } => pairs.iter_mut().all(|(key, value)| {
            substitute_free_vars(key, subst) && substitute_free_vars(value, subst)
        }),
        Expr::StringConcat { parts, .. } => parts
            .iter_mut()
            .all(|part| substitute_free_vars(part, subst)),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_free_vars(condition, subst)
                && substitute_free_vars(then_expr, subst)
                && substitute_free_vars(else_expr, subst)
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr
                .as_mut()
                .map(|b| substitute_free_vars(b, subst))
                .unwrap_or(true)
                && type_args
                    .iter_mut()
                    .all(|type_arg| substitute_free_vars(type_arg, subst))
        }
        Expr::QuoteLiteral { constructor, .. } => substitute_free_vars(constructor, subst),
        Expr::ReturnExpr {
            value: Some(value), ..
        } => substitute_free_vars(value, subst),
        // Binding / assignment / statement-bearing constructs whose scoping this
        // pass does not model soundly — bail to the #7014 diagnostic. `LetBlock`
        // and `AssignExpr` can rebind a substituted name mid-expression.
        Expr::LetBlock { .. } | Expr::AssignExpr { .. } => false,
        // Leaf forms (literals, `SliceAll`, `FunctionRef`, `TypedEmptyArray`,
        // break/continue, and value-less returns) carry no substitutable var.
        _ => true,
    }
}
