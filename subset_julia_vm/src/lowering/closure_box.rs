//! Closure variable boxing (Issue #6262).
//!
//! A scalar local that is **captured by a closure** and **reassigned** in its
//! defining scope must share a single mutable cell between the scope and the
//! closure, so the closure observes later reassignments (Julia `Core.Box`
//! semantics). sjulia closures capture by value-snapshot (`CreateClosure`), so
//! `counter = 0; f = () -> counter; counter = 5; f()` returned the stale `0`.
//!
//! This post-lowering pass boxes such locals as `Ref`: the binding is rewritten
//! to `v = Ref(init)`, reads `v` become `v[]`, and reassignments `v = x` become
//! `v[] = x` — in both the defining scope and the capturing closure bodies.
//! Because the `Ref` binding is never rebound (only its contents mutate), the
//! closure's value-snapshot of the `Ref` still points at the shared cell, and
//! sjulia's `Ref` is already reference-semantic on capture.
//!
//! The pass is deliberately conservative: a candidate variable is boxed only
//! when every one of its uses is a plain read or a top-level rebind. Any
//! shadowing, compound assignment, use as a callee/array/field target, or a
//! reassignment outside the defining scope's top level causes the variable to
//! be left unboxed (the snapshot bug remains for that rare shape, but nothing
//! is miscompiled). The exhaustive `match`es mirror `compile::free_vars` so the
//! compiler enforces that every IR node is considered.

use std::collections::{HashMap, HashSet};

use crate::compile::analyze_free_variables;
use crate::ir::core::{Block, Expr, Function, Literal, Stmt};
use crate::span::Span;

/// Box captured-and-reassigned scalar locals across `main` and every function.
/// Each function body and `main` is a defining scope; the pass also recurses
/// into nested scopes (closure bodies, `@testset`/`@time` blocks, bare blocks,
/// and empty-binding `let` blocks — the lowering of `@testset`/`@test`/bare
/// `begin` bodies, Issue #6281).
///
/// A closure that captures the local appears at this stage either inline (an
/// `Stmt::FunctionDef`, e.g. an arrow lambda lowered into a `LetBlock`, rewritten
/// transitively with the scope body) or as a separately lifted function in
/// `functions` referenced by `FunctionRef` (e.g. lambdas defined at top level /
/// inside `@testset`). The latter are rewritten by index in a final pass, using
/// a read-only snapshot of `functions` for capture analysis so the live tree can
/// be mutated freely meanwhile.
pub(crate) fn box_captured_reassigned_locals(functions: &mut [Function], main: &mut [Stmt]) {
    let snapshot = functions.to_vec();
    let name_to_idx: HashMap<String, usize> = snapshot
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    let mut ctx = BoxCtx {
        funcs: &snapshot,
        name_to_idx: &name_to_idx,
        ref_rewrites: Vec::new(),
    };
    // `main` is the global / top-level scope; the function bodies are not.
    ctx.box_scope(main, &[], true);
    for f in functions.iter_mut() {
        let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
        ctx.box_scope(&mut f.body.stmts, &params, false);
    }
    let ref_rewrites = ctx.ref_rewrites;
    for (idx, var) in ref_rewrites {
        rewrite_reads_block(&mut functions[idx].body, &var);
    }
}

struct BoxCtx<'a> {
    funcs: &'a [Function],
    name_to_idx: &'a HashMap<String, usize>,
    /// `(function index, captured var)` reads to rewrite to `var[]` in a lifted
    /// closure once all scopes are processed.
    ref_rewrites: Vec<(usize, String)>,
}

impl BoxCtx<'_> {
    /// Box the eligible locals of one scope, then recurse into its nested scopes.
    fn box_scope(&mut self, stmts: &mut [Stmt], params: &[String], global_scope: bool) {
        for (var, ref_closures) in self.compute_box_vars(stmts, params, global_scope) {
            box_in_parent_stmts(stmts, &var);
            for c in ref_closures {
                self.ref_rewrites.push((c, var.clone()));
            }
        }
        for stmt in stmts.iter_mut() {
            self.recurse_scopes_stmt(stmt, global_scope);
        }
    }

    /// Recurse into the scope-introducing children of `stmt` (closure bodies,
    /// `@testset`/`@time`/bare blocks, and empty-binding `let` blocks become
    /// their own scopes; control-flow blocks are descended without starting a
    /// new scope).
    fn recurse_scopes_stmt(&mut self, stmt: &mut Stmt, global_scope: bool) {
        match stmt {
            Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
                // Entering a function body: no longer the global scope.
                let params: Vec<String> = func.params.iter().map(|p| p.name.clone()).collect();
                self.box_scope(&mut func.body.stmts, &params, false);
            }
            Stmt::TestSet { body, .. } | Stmt::Timed { body, .. } | Stmt::Block(body) => {
                // These do not introduce a function boundary — inherit globalness.
                self.box_scope(&mut body.stmts, &[], global_scope);
            }
            // An empty-binding `let` block (`let; …; end`) is how `@testset` /
            // `@test` / bare `begin … end` bodies are lowered — nested
            // `Stmt::Expr(LetBlock { bindings: [], … })`, not `Stmt::Block`. It
            // introduces no new bindings, so its body is a defining scope just
            // like a bare block: descend so a captured-and-reassigned local that
            // lives inside it (e.g. a `counter` local to an `@testset`) is boxed
            // (Issue #6281). A `let` *with* bindings is a real binding scope and
            // is left alone here.
            Stmt::Expr {
                expr: Expr::LetBlock { bindings, body, .. },
                ..
            } if bindings.is_empty() => {
                self.box_scope(&mut body.stmts, &[], global_scope);
            }
            // `@time` / `@elapsed` capture their body's value into an assignment
            // whose value is an empty-binding `let` block (`#result# = let … end`).
            // That `let` introduces no bindings, so its body is a defining scope:
            // descend so a captured-and-reassigned local inside a `@time` block is
            // boxed like the `@testset` case (Issues #6281 / #6288).
            Stmt::Assign {
                value: Expr::LetBlock { bindings, body, .. },
                ..
            } if bindings.is_empty() => {
                self.box_scope(&mut body.stmts, &[], global_scope);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                for s in &mut then_branch.stmts {
                    self.recurse_scopes_stmt(s, global_scope);
                }
                if let Some(b) = else_branch {
                    for s in &mut b.stmts {
                        self.recurse_scopes_stmt(s, global_scope);
                    }
                }
            }
            Stmt::For { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachTuple { body, .. }
            | Stmt::While { body, .. } => {
                for s in &mut body.stmts {
                    self.recurse_scopes_stmt(s, global_scope);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                for s in &mut try_block.stmts {
                    self.recurse_scopes_stmt(s, global_scope);
                }
                for b in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    for s in &mut b.stmts {
                        self.recurse_scopes_stmt(s, global_scope);
                    }
                }
            }
            _ => {}
        }
    }

    /// Determine which locals of one scope to box, with the lifted closures that
    /// capture each: reassigned at least twice at the scope's top level, captured
    /// by some closure (inline or lifted), and used only in box-safe shapes.
    fn compute_box_vars(
        &self,
        stmts: &[Stmt],
        params: &[String],
        global_scope: bool,
    ) -> Vec<(String, Vec<usize>)> {
        // Names bound in this scope (params + every assignment target), used to
        // decide which closure reads are captures of *this* scope.
        let mut scope_locals: HashSet<String> = params.iter().cloned().collect();
        for stmt in stmts {
            collect_assigned_names_stmt(stmt, &mut scope_locals);
        }

        let mut inline_funcs: Vec<&Function> = Vec::new();
        for stmt in stmts {
            collect_inline_funcs_stmt(stmt, &mut inline_funcs);
        }
        let mut ref_names: HashSet<String> = HashSet::new();
        for stmt in stmts {
            collect_function_refs_stmt(stmt, &mut ref_names);
        }
        let ref_closures: Vec<usize> = ref_names
            .iter()
            .filter_map(|n| self.name_to_idx.get(n).copied())
            .collect();
        if inline_funcs.is_empty() && ref_closures.is_empty() {
            return Vec::new();
        }

        let mut candidates: HashSet<String> = HashSet::new();
        let mut mutated_captures: HashSet<String> = HashSet::new();
        for f in &inline_funcs {
            candidates.extend(analyze_free_variables(f, &scope_locals));
            mutated_captures.extend(assigned_captures(f, &scope_locals));
        }
        for &c in &ref_closures {
            candidates.extend(analyze_free_variables(&self.funcs[c], &scope_locals));
            mutated_captures.extend(assigned_captures(&self.funcs[c], &scope_locals));
        }
        candidates.extend(mutated_captures.iter().cloned());

        let param_set: HashSet<&String> = params.iter().collect();
        let mut result = Vec::new();
        for var in candidates {
            if param_set.contains(&var) {
                continue; // a parameter is bound, not a reassigned local
            }
            // Must have a binding in this scope. Pure parent-scope reassignment
            // needs >= 2 top-level assignments (init + reassignment). A nested
            // closure assignment mutates the captured binding, so one parent init
            // plus the closure-side assignment is enough to require boxing.
            let toplevel_assigns = stmts
                .iter()
                .filter(|s| matches!(s, Stmt::Assign { var: v, .. } if *v == var))
                .count();
            let closure_mutates = mutated_captures.contains(&var);
            // The "one parent init + closure-side mutation" shortcut applies only
            // inside FUNCTION scopes: there an assignment to an enclosing local
            // captures it. At the global / top-level scope a closure assigning a
            // same-named global creates a fresh local (no capture), so a single
            // top-level binding must NOT be boxed — otherwise e.g. a top-level
            // `do` block that assigns a name also assigned later at top level would
            // be made to capture an as-yet-undefined global (Issue #7759).
            let capture_on_assign = closure_mutates && toplevel_assigns >= 1 && !global_scope;
            if toplevel_assigns < 2 && !capture_on_assign {
                continue;
            }
            // Scope-body safety also covers inline closures (it recurses through
            // `FunctionDef`), so a shadowing/reassigning inline closure bails here.
            if !parent_scope_box_safe(stmts, &var, closure_mutates) {
                continue;
            }
            let one: HashSet<String> = std::iter::once(var.clone()).collect();
            // Lifted closures that capture this var; bail if any reassigns/shadows
            // it (a partial box would be inconsistent).
            let mut capturing_refs = Vec::new();
            let mut ref_unsafe = false;
            for &c in &ref_closures {
                let closure_assigns = assigned_captures(&self.funcs[c], &one);
                let captures_by_read = !analyze_free_variables(&self.funcs[c], &one).is_empty();
                let captures_by_assignment = closure_assigns.contains(&var);
                if !captures_by_read && !captures_by_assignment {
                    continue;
                }
                if closure_body_box_safe(&self.funcs[c], &var, captures_by_assignment) {
                    capturing_refs.push(c);
                } else {
                    ref_unsafe = true;
                    break;
                }
            }
            if ref_unsafe {
                continue;
            }
            let inline_captures = inline_funcs.iter().any(|f| {
                !analyze_free_variables(f, &one).is_empty()
                    || assigned_captures(f, &one).contains(&var)
            });
            if capturing_refs.is_empty() && !inline_captures {
                continue; // not actually captured by any closure
            }
            result.push((var, capturing_refs));
        }
        result
    }
}

fn collect_inline_funcs_block<'a>(block: &'a Block, out: &mut Vec<&'a Function>) {
    for s in &block.stmts {
        collect_inline_funcs_stmt(s, out);
    }
}

fn assigned_captures(func: &Function, scope_locals: &HashSet<String>) -> HashSet<String> {
    // A name assigned in the closure that already exists in the enclosing scope
    // (`scope_locals`) is a *capture-on-assign*: the assignment rebinds the outer
    // local (Julia soft-scope), so the closure mutates the captured binding and it
    // must be boxed. Only HARD binders shadow such a name — parameters, loop
    // variables and catch variables — so those are excluded. (Earlier this used
    // every `Stmt::Assign` target as a "local", which excluded the very
    // capture-on-assign names it should report, leaving the
    // one-parent-init + closure-mutation boxing path dead — Issues #7619/#7618.)
    let params: HashSet<&str> = func.params.iter().map(|p| p.name.as_str()).collect();
    let mut hard_locals: HashSet<String> = func.params.iter().map(|p| p.name.clone()).collect();
    collect_hard_local_binding_names_block(&func.body, &mut hard_locals);
    let mut assigned = HashSet::new();
    collect_assigned_names_block(&func.body, &mut assigned);
    assigned
        .into_iter()
        .filter(|name| {
            scope_locals.contains(name)
                && !params.contains(name.as_str())
                && !hard_locals.contains(name)
        })
        .collect()
}

/// Collect only the **hard** local binders of a function body — loop variables
/// and catch variables — which shadow an enclosing same-named local. Plain
/// `Stmt::Assign` targets are deliberately NOT collected: a bare assignment to a
/// name that exists in an enclosing scope captures it (Julia soft-scope), it does
/// not introduce a fresh local. Used to keep capture-on-assign names visible to
/// the boxing analysis (Issue #7619).
fn collect_hard_local_binding_names_block(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_hard_local_binding_names_stmt(stmt, out);
    }
}

fn collect_hard_local_binding_names_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
            out.insert(var.clone());
            collect_hard_local_binding_names_block(body, out);
        }
        Stmt::ForEachTuple { vars, body, .. } => {
            out.extend(vars.iter().cloned());
            collect_hard_local_binding_names_block(body, out);
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. }
        | Stmt::While { body: block, .. } => collect_hard_local_binding_names_block(block, out),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_hard_local_binding_names_block(then_branch, out);
            if let Some(block) = else_branch {
                collect_hard_local_binding_names_block(block, out);
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
            collect_hard_local_binding_names_block(try_block, out);
            if let Some(var) = catch_var {
                out.insert(var.clone());
            }
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                collect_hard_local_binding_names_block(block, out);
            }
        }
        _ => {}
    }
}

/// Collect every inline `FunctionDef` function reachable from `stmt` (including
/// closures nested in `LetBlock`s and inside other inline closures).
fn collect_inline_funcs_stmt<'a>(stmt: &'a Stmt, out: &mut Vec<&'a Function>) {
    if let Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } = stmt {
        out.push(func);
        collect_inline_funcs_block(&func.body, out);
    }
    match stmt {
        Stmt::Block(block) => collect_inline_funcs_block(block, out),
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            collect_inline_funcs_expr(value, out)
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                collect_inline_funcs_expr(e, out);
            }
        }
        Stmt::Expr { expr, .. } => collect_inline_funcs_expr(expr, out),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_inline_funcs_expr(condition, out);
            collect_inline_funcs_block(then_branch, out);
            if let Some(b) = else_branch {
                collect_inline_funcs_block(b, out);
            }
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_inline_funcs_expr(start, out);
            collect_inline_funcs_expr(end, out);
            if let Some(s) = step {
                collect_inline_funcs_expr(s, out);
            }
            collect_inline_funcs_block(body, out);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_inline_funcs_expr(iterable, out);
            collect_inline_funcs_block(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_inline_funcs_expr(condition, out);
            collect_inline_funcs_block(body, out);
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_inline_funcs_block(try_block, out);
            for b in [
                catch_block.as_ref(),
                else_block.as_ref(),
                finally_block.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                collect_inline_funcs_block(b, out);
            }
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_inline_funcs_expr(key, out);
            collect_inline_funcs_expr(value, out);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for e in indices {
                collect_inline_funcs_expr(e, out);
            }
            collect_inline_funcs_expr(value, out);
        }
        Stmt::FieldAssign { value, .. } => collect_inline_funcs_expr(value, out),
        Stmt::DestructuringAssign { value, .. } => collect_inline_funcs_expr(value, out),
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_inline_funcs_block(body, out)
        }
        Stmt::Test { condition, .. } => collect_inline_funcs_expr(condition, out),
        Stmt::TestThrows { expr, .. } => collect_inline_funcs_expr(expr, out),
        // `FunctionDef` body already handled above; nothing else binds funcs.
        Stmt::FunctionDef { .. }
        | Stmt::EvalFunctionDef { .. }
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

fn collect_inline_funcs_expr<'a>(expr: &'a Expr, out: &mut Vec<&'a Function>) {
    match expr {
        Expr::Var(_, _) | Expr::Literal(_, _) => {}
        Expr::BinaryOp { left, right, .. } => {
            collect_inline_funcs_expr(left, out);
            collect_inline_funcs_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_inline_funcs_expr(operand, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for a in args {
                collect_inline_funcs_expr(a, out);
            }
            for (_, e) in kwargs {
                collect_inline_funcs_expr(e, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for a in args {
                collect_inline_funcs_expr(a, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_inline_funcs_expr(array, out);
            for e in indices {
                collect_inline_funcs_expr(e, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for e in elements {
                collect_inline_funcs_expr(e, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_inline_funcs_expr(start, out);
            collect_inline_funcs_expr(stop, out);
            if let Some(s) = step {
                collect_inline_funcs_expr(s, out);
            }
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_inline_funcs_expr(body, out);
            collect_inline_funcs_expr(iter, out);
            if let Some(f) = filter {
                collect_inline_funcs_expr(f, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_inline_funcs_expr(body, out);
            for (_, e) in iterations {
                collect_inline_funcs_expr(e, out);
            }
            if let Some(f) = filter {
                collect_inline_funcs_expr(f, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_inline_funcs_expr(object, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_inline_funcs_expr(condition, out);
            collect_inline_funcs_expr(then_expr, out);
            collect_inline_funcs_expr(else_expr, out);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, e) in bindings {
                collect_inline_funcs_expr(e, out);
            }
            collect_inline_funcs_block(body, out);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, e) in fields {
                collect_inline_funcs_expr(e, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (k, val) in pairs {
                collect_inline_funcs_expr(k, out);
                collect_inline_funcs_expr(val, out);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for p in parts {
                collect_inline_funcs_expr(p, out);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(b) = base_expr {
                collect_inline_funcs_expr(b, out);
            }
            for a in type_args {
                collect_inline_funcs_expr(a, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => collect_inline_funcs_expr(constructor, out),
        Expr::AssignExpr { value, .. } => collect_inline_funcs_expr(value, out),
        Expr::ReturnExpr { value, .. } => {
            if let Some(e) = value {
                collect_inline_funcs_expr(e, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_inline_funcs_expr(key, out);
            collect_inline_funcs_expr(value, out);
        }
        Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

/// Rewrite a defining scope's statements: the first top-level `v = init` becomes
/// `v = Ref(init)`, later top-level `v = x` become `v[] = x`, and all reads of
/// `v` (in those values and every other statement) become `v[]`.
fn box_in_parent_stmts(stmts: &mut [Stmt], v: &str) {
    let mut seen_init = false;
    for stmt in stmts.iter_mut() {
        if let Stmt::Assign { var, value, span } = stmt {
            if var == v {
                rewrite_reads_expr(value, v);
                let value = std::mem::replace(value, Expr::Literal(Literal::Nothing, *span));
                let span = *span;
                if !seen_init {
                    seen_init = true;
                    *stmt = Stmt::Assign {
                        var: v.to_string(),
                        value: make_ref_call(value, span),
                        span,
                    };
                } else {
                    *stmt = Stmt::IndexAssign {
                        array: v.to_string(),
                        indices: Vec::new(),
                        value,
                        span,
                    };
                }
                continue;
            }
        }
        rewrite_reads_stmt(stmt, v);
    }
}

/// `Ref(value)`
fn make_ref_call(value: Expr, span: Span) -> Expr {
    Expr::Call {
        function: "Ref".to_string(),
        args: vec![value],
        kwargs: Vec::new(),
        splat_mask: vec![false],
        kwargs_splat_mask: Vec::new(),
        span,
    }
}

/// `v[]` (Ref deref read) reusing the original variable span.
fn make_deref(v: &str, span: Span) -> Expr {
    Expr::Index {
        array: Box::new(Expr::Var(v.to_string(), span)),
        indices: Vec::new(),
        span,
    }
}

// ===================== safety analysis =====================

/// The defining scope is safe to box `v` when, for each top-level statement:
/// a `v = …` assignment's value contains no unsafe use of `v`, and every other
/// statement (recursively) contains no unsafe use of `v` — where "unsafe"
/// includes any reassignment, binder/shadow, or use as a callee/array/field.
fn parent_scope_box_safe(stmts: &[Stmt], v: &str, allow_nested_assignment: bool) -> bool {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, .. } if var == v => {
                if expr_unsafe_for_box(value, v, false) {
                    return false;
                }
            }
            _ => {
                if stmt_unsafe_for_box(stmt, v, allow_nested_assignment) {
                    return false;
                }
            }
        }
    }
    true
}

/// A lifted (separate) capturing closure is safe when its body never shadows or
/// reassigns `v` (it only reads it), so rewriting its reads to `v[]` is correct.
fn closure_body_box_safe(func: &Function, v: &str, allow_assignment: bool) -> bool {
    if func.params.iter().any(|p| p.name == v) {
        return false;
    }
    !block_unsafe_for_box(&func.body, v, allow_assignment)
}

fn block_unsafe_for_box(block: &Block, v: &str, allow_assignment: bool) -> bool {
    block
        .stmts
        .iter()
        .any(|s| stmt_unsafe_for_box(s, v, allow_assignment))
}

/// True if `stmt` uses `v` in any way other than a plain read (reassignment,
/// binder/shadow, callee/array/field target, …). Exhaustive over `Stmt`.
fn stmt_unsafe_for_box(stmt: &Stmt, v: &str, allow_assignment: bool) -> bool {
    match stmt {
        Stmt::Block(block) => block_unsafe_for_box(block, v, allow_assignment),
        // Any assignment to `v` outside the handled top-level path is unsafe.
        Stmt::Assign { var, value, .. } => {
            (var == v && !allow_assignment) || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Stmt::AddAssign { var, value, .. } => {
            var == v || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_some_and(|e| expr_unsafe_for_box(e, v, allow_assignment)),
        Stmt::Expr { expr, .. } => expr_unsafe_for_box(expr, v, allow_assignment),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_unsafe_for_box(condition, v, allow_assignment)
                || block_unsafe_for_box(then_branch, v, allow_assignment)
                || else_branch
                    .as_ref()
                    .is_some_and(|b| block_unsafe_for_box(b, v, allow_assignment))
        }
        Stmt::For {
            var,
            start,
            end,
            step,
            body,
            ..
        } => {
            var == v
                || expr_unsafe_for_box(start, v, allow_assignment)
                || expr_unsafe_for_box(end, v, allow_assignment)
                || step
                    .as_ref()
                    .is_some_and(|s| expr_unsafe_for_box(s, v, allow_assignment))
                || block_unsafe_for_box(body, v, allow_assignment)
        }
        Stmt::ForEach {
            var,
            iterable,
            body,
            ..
        } => {
            var == v
                || expr_unsafe_for_box(iterable, v, allow_assignment)
                || block_unsafe_for_box(body, v, allow_assignment)
        }
        Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            ..
        } => {
            vars.iter().any(|x| x == v)
                || expr_unsafe_for_box(iterable, v, allow_assignment)
                || block_unsafe_for_box(body, v, allow_assignment)
        }
        Stmt::While {
            condition, body, ..
        } => {
            expr_unsafe_for_box(condition, v, allow_assignment)
                || block_unsafe_for_box(body, v, allow_assignment)
        }
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            catch_var.as_ref().is_some_and(|c| c == v)
                || block_unsafe_for_box(try_block, v, allow_assignment)
                || catch_block
                    .as_ref()
                    .is_some_and(|b| block_unsafe_for_box(b, v, allow_assignment))
                || else_block
                    .as_ref()
                    .is_some_and(|b| block_unsafe_for_box(b, v, allow_assignment))
                || finally_block
                    .as_ref()
                    .is_some_and(|b| block_unsafe_for_box(b, v, allow_assignment))
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            func.name == v
                || func.params.iter().any(|p| p.name == v)
                || block_unsafe_for_box(&func.body, v, allow_assignment)
        }
        Stmt::DictAssign { key, value, .. } => {
            expr_unsafe_for_box(key, v, allow_assignment)
                || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Stmt::IndexAssign {
            array,
            indices,
            value,
            ..
        } => {
            array == v
                || indices
                    .iter()
                    .any(|e| expr_unsafe_for_box(e, v, allow_assignment))
                || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Stmt::FieldAssign { object, value, .. } => {
            object == v || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Stmt::DestructuringAssign { targets, value, .. } => {
            targets.iter().any(|t| t == v) || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            block_unsafe_for_box(body, v, allow_assignment)
        }
        Stmt::Test { condition, .. } => expr_unsafe_for_box(condition, v, allow_assignment),
        Stmt::TestThrows { expr, .. } => expr_unsafe_for_box(expr, v, allow_assignment),
        Stmt::Global { names, .. } => names.iter().any(|n| n == v),
        Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. } => false,
    }
}

/// True if `expr` uses `v` other than as a plain read (called as a function, or
/// shadowed by a comprehension/generator/let/assign binder). Exhaustive.
fn expr_unsafe_for_box(expr: &Expr, v: &str, allow_assignment: bool) -> bool {
    match expr {
        Expr::Var(_, _) => false, // a plain read is fine
        Expr::Literal(_, _) => false,
        Expr::BinaryOp { left, right, .. } => {
            expr_unsafe_for_box(left, v, allow_assignment)
                || expr_unsafe_for_box(right, v, allow_assignment)
        }
        Expr::UnaryOp { operand, .. } => expr_unsafe_for_box(operand, v, allow_assignment),
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            function == v
                || args
                    .iter()
                    .any(|a| expr_unsafe_for_box(a, v, allow_assignment))
                || kwargs
                    .iter()
                    .any(|(_, e)| expr_unsafe_for_box(e, v, allow_assignment))
        }
        Expr::Builtin { args, .. } => args
            .iter()
            .any(|a| expr_unsafe_for_box(a, v, allow_assignment)),
        Expr::Index { array, indices, .. } => {
            expr_unsafe_for_box(array, v, allow_assignment)
                || indices
                    .iter()
                    .any(|e| expr_unsafe_for_box(e, v, allow_assignment))
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => elements
            .iter()
            .any(|e| expr_unsafe_for_box(e, v, allow_assignment)),
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_unsafe_for_box(start, v, allow_assignment)
                || expr_unsafe_for_box(stop, v, allow_assignment)
                || step
                    .as_ref()
                    .is_some_and(|s| expr_unsafe_for_box(s, v, allow_assignment))
        }
        Expr::Comprehension {
            body,
            var,
            iter,
            filter,
            ..
        } => {
            comprehension_binds(var, v)
                || expr_unsafe_for_box(body, v, allow_assignment)
                || expr_unsafe_for_box(iter, v, allow_assignment)
                || filter
                    .as_ref()
                    .is_some_and(|f| expr_unsafe_for_box(f, v, allow_assignment))
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            iterations.iter().any(|(var, _)| var == v)
                || expr_unsafe_for_box(body, v, allow_assignment)
                || iterations
                    .iter()
                    .any(|(_, e)| expr_unsafe_for_box(e, v, allow_assignment))
                || filter
                    .as_ref()
                    .is_some_and(|f| expr_unsafe_for_box(f, v, allow_assignment))
        }
        Expr::Generator {
            body,
            var,
            iter,
            filter,
            ..
        } => {
            var == v
                || expr_unsafe_for_box(body, v, allow_assignment)
                || expr_unsafe_for_box(iter, v, allow_assignment)
                || filter
                    .as_ref()
                    .is_some_and(|f| expr_unsafe_for_box(f, v, allow_assignment))
        }
        Expr::FieldAccess { object, .. } => expr_unsafe_for_box(object, v, allow_assignment),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_unsafe_for_box(condition, v, allow_assignment)
                || expr_unsafe_for_box(then_expr, v, allow_assignment)
                || expr_unsafe_for_box(else_expr, v, allow_assignment)
        }
        Expr::LetBlock { bindings, body, .. } => {
            bindings.iter().any(|(name, _)| name == v)
                || bindings
                    .iter()
                    .any(|(_, e)| expr_unsafe_for_box(e, v, allow_assignment))
                || block_unsafe_for_box(body, v, allow_assignment)
        }
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .any(|(_, e)| expr_unsafe_for_box(e, v, allow_assignment)),
        Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(k, val)| {
            expr_unsafe_for_box(k, v, allow_assignment)
                || expr_unsafe_for_box(val, v, allow_assignment)
        }),
        Expr::StringConcat { parts, .. } => parts
            .iter()
            .any(|p| expr_unsafe_for_box(p, v, allow_assignment)),
        Expr::ModuleCall { args, kwargs, .. } => {
            args.iter()
                .any(|a| expr_unsafe_for_box(a, v, allow_assignment))
                || kwargs
                    .iter()
                    .any(|(_, e)| expr_unsafe_for_box(e, v, allow_assignment))
        }
        Expr::New { args, .. } => args
            .iter()
            .any(|a| expr_unsafe_for_box(a, v, allow_assignment)),
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr
                .as_ref()
                .is_some_and(|b| expr_unsafe_for_box(b, v, allow_assignment))
                || type_args
                    .iter()
                    .any(|a| expr_unsafe_for_box(a, v, allow_assignment))
        }
        Expr::QuoteLiteral { constructor, .. } => {
            expr_unsafe_for_box(constructor, v, allow_assignment)
        }
        Expr::AssignExpr { value, var, .. } => {
            (var == v && !allow_assignment) || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Expr::ReturnExpr { value, .. } => value
            .as_ref()
            .is_some_and(|e| expr_unsafe_for_box(e, v, allow_assignment)),
        Expr::Pair { key, value, .. } => {
            expr_unsafe_for_box(key, v, allow_assignment)
                || expr_unsafe_for_box(value, v, allow_assignment)
        }
        Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => false,
    }
}

fn comprehension_binds(var: &str, v: &str) -> bool {
    // `var` may be a single name or a tuple-destructuring spelling; treat any
    // exact match (or containment) conservatively as a binder of `v`.
    var == v
        || var
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|p| p == v)
}

// ===================== closure / binding collection =====================

fn collect_function_refs_block(block: &Block, out: &mut HashSet<String>) {
    for s in &block.stmts {
        collect_function_refs_stmt(s, out);
    }
}

/// Collect every `FunctionRef` name reachable from `stmt`. Exhaustive over `Stmt`.
fn collect_function_refs_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Block(block) => collect_function_refs_block(block, out),
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            collect_function_refs_expr(value, out)
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                collect_function_refs_expr(e, out);
            }
        }
        Stmt::Expr { expr, .. } => collect_function_refs_expr(expr, out),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_function_refs_expr(condition, out);
            collect_function_refs_block(then_branch, out);
            if let Some(b) = else_branch {
                collect_function_refs_block(b, out);
            }
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_function_refs_expr(start, out);
            collect_function_refs_expr(end, out);
            if let Some(s) = step {
                collect_function_refs_expr(s, out);
            }
            collect_function_refs_block(body, out);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_function_refs_expr(iterable, out);
            collect_function_refs_block(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_function_refs_expr(condition, out);
            collect_function_refs_block(body, out);
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_function_refs_block(try_block, out);
            for b in [
                catch_block.as_ref(),
                else_block.as_ref(),
                finally_block.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                collect_function_refs_block(b, out);
            }
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            collect_function_refs_block(&func.body, out)
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_function_refs_expr(key, out);
            collect_function_refs_expr(value, out);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for e in indices {
                collect_function_refs_expr(e, out);
            }
            collect_function_refs_expr(value, out);
        }
        Stmt::FieldAssign { value, .. } => collect_function_refs_expr(value, out),
        Stmt::DestructuringAssign { value, .. } => collect_function_refs_expr(value, out),
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_function_refs_block(body, out)
        }
        Stmt::Test { condition, .. } => collect_function_refs_expr(condition, out),
        Stmt::TestThrows { expr, .. } => collect_function_refs_expr(expr, out),
        Stmt::Break { .. }
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

/// Collect every `FunctionRef` name reachable from `expr`. Exhaustive over `Expr`.
fn collect_function_refs_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::FunctionRef { name, .. } => {
            out.insert(name.clone());
        }
        Expr::Var(_, _) | Expr::Literal(_, _) => {}
        Expr::BinaryOp { left, right, .. } => {
            collect_function_refs_expr(left, out);
            collect_function_refs_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_function_refs_expr(operand, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for a in args {
                collect_function_refs_expr(a, out);
            }
            for (_, e) in kwargs {
                collect_function_refs_expr(e, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for a in args {
                collect_function_refs_expr(a, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_function_refs_expr(array, out);
            for e in indices {
                collect_function_refs_expr(e, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for e in elements {
                collect_function_refs_expr(e, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_function_refs_expr(start, out);
            collect_function_refs_expr(stop, out);
            if let Some(s) = step {
                collect_function_refs_expr(s, out);
            }
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_function_refs_expr(body, out);
            collect_function_refs_expr(iter, out);
            if let Some(f) = filter {
                collect_function_refs_expr(f, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_function_refs_expr(body, out);
            for (_, e) in iterations {
                collect_function_refs_expr(e, out);
            }
            if let Some(f) = filter {
                collect_function_refs_expr(f, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_function_refs_expr(object, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_function_refs_expr(condition, out);
            collect_function_refs_expr(then_expr, out);
            collect_function_refs_expr(else_expr, out);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, e) in bindings {
                collect_function_refs_expr(e, out);
            }
            collect_function_refs_block(body, out);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, e) in fields {
                collect_function_refs_expr(e, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (k, val) in pairs {
                collect_function_refs_expr(k, out);
                collect_function_refs_expr(val, out);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for p in parts {
                collect_function_refs_expr(p, out);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(b) = base_expr {
                collect_function_refs_expr(b, out);
            }
            for a in type_args {
                collect_function_refs_expr(a, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => collect_function_refs_expr(constructor, out),
        Expr::AssignExpr { value, .. } => collect_function_refs_expr(value, out),
        Expr::ReturnExpr { value, .. } => {
            if let Some(e) = value {
                collect_function_refs_expr(e, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_function_refs_expr(key, out);
            collect_function_refs_expr(value, out);
        }
        Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

// ===================== read rewriting =====================

fn collect_assigned_names_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => {
            out.insert(var.clone());
        }
        Stmt::Return {
            value: Some(value), ..
        } => collect_assigned_names_expr(value, out),
        Stmt::Return { value: None, .. } => {}
        Stmt::Expr { expr, .. } => collect_assigned_names_expr(expr, out),
        Stmt::DestructuringAssign { targets, .. } => {
            for t in targets {
                out.insert(t.clone());
            }
        }
        Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
            out.insert(var.clone());
            for s in &body.stmts {
                collect_assigned_names_stmt(s, out);
            }
        }
        Stmt::ForEachTuple { vars, body, .. } => {
            for x in vars {
                out.insert(x.clone());
            }
            for s in &body.stmts {
                collect_assigned_names_stmt(s, out);
            }
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. }
        | Stmt::While { body: block, .. } => {
            for s in &block.stmts {
                collect_assigned_names_stmt(s, out);
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            for s in &then_branch.stmts {
                collect_assigned_names_stmt(s, out);
            }
            if let Some(b) = else_branch {
                for s in &b.stmts {
                    collect_assigned_names_stmt(s, out);
                }
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            for blk in [
                Some(try_block),
                catch_block.as_ref(),
                else_block.as_ref(),
                finally_block.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                for s in &blk.stmts {
                    collect_assigned_names_stmt(s, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_assigned_names_block(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_assigned_names_stmt(stmt, out);
    }
}

fn collect_assigned_names_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::AssignExpr { var, value, .. } => {
            out.insert(var.clone());
            collect_assigned_names_expr(value, out);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_assigned_names_expr(left, out);
            collect_assigned_names_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_assigned_names_expr(operand, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_assigned_names_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_assigned_names_expr(value, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                collect_assigned_names_expr(arg, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_assigned_names_expr(array, out);
            for index in indices {
                collect_assigned_names_expr(index, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_assigned_names_expr(element, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_assigned_names_expr(start, out);
            if let Some(step) = step {
                collect_assigned_names_expr(step, out);
            }
            collect_assigned_names_expr(stop, out);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_assigned_names_expr(body, out);
            collect_assigned_names_expr(iter, out);
            if let Some(filter) = filter {
                collect_assigned_names_expr(filter, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_assigned_names_expr(body, out);
            for (_, iter) in iterations {
                collect_assigned_names_expr(iter, out);
            }
            if let Some(filter) = filter {
                collect_assigned_names_expr(filter, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_assigned_names_expr(object, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_assigned_names_expr(condition, out);
            collect_assigned_names_expr(then_expr, out);
            collect_assigned_names_expr(else_expr, out);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_assigned_names_expr(value, out);
            }
            collect_assigned_names_block(body, out);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_assigned_names_expr(value, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_assigned_names_expr(key, out);
                collect_assigned_names_expr(value, out);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_assigned_names_expr(part, out);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_assigned_names_expr(base_expr, out);
            }
            for arg in type_args {
                collect_assigned_names_expr(arg, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => collect_assigned_names_expr(constructor, out),
        Expr::ReturnExpr { value, .. } => {
            if let Some(value) = value {
                collect_assigned_names_expr(value, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_assigned_names_expr(key, out);
            collect_assigned_names_expr(value, out);
        }
        Expr::Var(_, _)
        | Expr::Literal(_, _)
        | Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

fn rewrite_reads_block(block: &mut Block, v: &str) {
    for stmt in &mut block.stmts {
        rewrite_reads_stmt(stmt, v);
    }
}

/// Rewrite every plain read `v` to `v[]` inside `stmt`. Does NOT convert
/// `v = …` bindings (the defining scope handles those); here a `v = x` simply
/// has its value's reads rewritten. Exhaustive over `Stmt`.
fn rewrite_reads_stmt(stmt: &mut Stmt, v: &str) {
    match stmt {
        Stmt::Block(block) => rewrite_reads_block(block, v),
        Stmt::Assign { var, value, span } if var == v => {
            rewrite_reads_expr(value, v);
            let value = std::mem::replace(value, Expr::Literal(Literal::Nothing, *span));
            *stmt = Stmt::IndexAssign {
                array: v.to_string(),
                indices: Vec::new(),
                value,
                span: *span,
            };
        }
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => rewrite_reads_expr(value, v),
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                rewrite_reads_expr(e, v);
            }
        }
        Stmt::Expr { expr, .. } => rewrite_reads_expr(expr, v),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            rewrite_reads_expr(condition, v);
            rewrite_reads_block(then_branch, v);
            if let Some(b) = else_branch {
                rewrite_reads_block(b, v);
            }
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            rewrite_reads_expr(start, v);
            rewrite_reads_expr(end, v);
            if let Some(s) = step {
                rewrite_reads_expr(s, v);
            }
            rewrite_reads_block(body, v);
        }
        Stmt::ForEach { iterable, body, .. } => {
            rewrite_reads_expr(iterable, v);
            rewrite_reads_block(body, v);
        }
        Stmt::ForEachTuple { iterable, body, .. } => {
            rewrite_reads_expr(iterable, v);
            rewrite_reads_block(body, v);
        }
        Stmt::While {
            condition, body, ..
        } => {
            rewrite_reads_expr(condition, v);
            rewrite_reads_block(body, v);
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            rewrite_reads_block(try_block, v);
            if let Some(b) = catch_block {
                rewrite_reads_block(b, v);
            }
            if let Some(b) = else_block {
                rewrite_reads_block(b, v);
            }
            if let Some(b) = finally_block {
                rewrite_reads_block(b, v);
            }
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            rewrite_reads_block(&mut func.body, v)
        }
        Stmt::DictAssign { key, value, .. } => {
            rewrite_reads_expr(key, v);
            rewrite_reads_expr(value, v);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for e in indices {
                rewrite_reads_expr(e, v);
            }
            rewrite_reads_expr(value, v);
        }
        Stmt::FieldAssign { value, .. } => rewrite_reads_expr(value, v),
        Stmt::DestructuringAssign { value, .. } => rewrite_reads_expr(value, v),
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => rewrite_reads_block(body, v),
        Stmt::Test { condition, .. } => rewrite_reads_expr(condition, v),
        Stmt::TestThrows { expr, .. } => rewrite_reads_expr(expr, v),
        Stmt::Break { .. }
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

/// Replace each plain read `Expr::Var(v)` with `v[]`. Exhaustive over `Expr`.
fn rewrite_reads_expr(expr: &mut Expr, v: &str) {
    match expr {
        Expr::Var(name, span) => {
            if name == v {
                let span = *span;
                *expr = make_deref(v, span);
            }
        }
        Expr::Literal(_, _) => {}
        Expr::BinaryOp { left, right, .. } => {
            rewrite_reads_expr(left, v);
            rewrite_reads_expr(right, v);
        }
        Expr::UnaryOp { operand, .. } => rewrite_reads_expr(operand, v),
        Expr::Call { args, kwargs, .. } => {
            for a in args {
                rewrite_reads_expr(a, v);
            }
            for (_, e) in kwargs {
                rewrite_reads_expr(e, v);
            }
        }
        Expr::Builtin { args, .. } => {
            for a in args {
                rewrite_reads_expr(a, v);
            }
        }
        Expr::Index { array, indices, .. } => {
            rewrite_reads_expr(array, v);
            for e in indices {
                rewrite_reads_expr(e, v);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for e in elements {
                rewrite_reads_expr(e, v);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            rewrite_reads_expr(start, v);
            rewrite_reads_expr(stop, v);
            if let Some(s) = step {
                rewrite_reads_expr(s, v);
            }
        }
        Expr::Comprehension {
            body, iter, filter, ..
        } => {
            rewrite_reads_expr(body, v);
            rewrite_reads_expr(iter, v);
            if let Some(f) = filter {
                rewrite_reads_expr(f, v);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            rewrite_reads_expr(body, v);
            for (_, e) in iterations {
                rewrite_reads_expr(e, v);
            }
            if let Some(f) = filter {
                rewrite_reads_expr(f, v);
            }
        }
        Expr::Generator {
            body, iter, filter, ..
        } => {
            rewrite_reads_expr(body, v);
            rewrite_reads_expr(iter, v);
            if let Some(f) = filter {
                rewrite_reads_expr(f, v);
            }
        }
        Expr::FieldAccess { object, .. } => rewrite_reads_expr(object, v),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_reads_expr(condition, v);
            rewrite_reads_expr(then_expr, v);
            rewrite_reads_expr(else_expr, v);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, e) in bindings {
                rewrite_reads_expr(e, v);
            }
            rewrite_reads_block(body, v);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, e) in fields {
                rewrite_reads_expr(e, v);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (k, val) in pairs {
                rewrite_reads_expr(k, v);
                rewrite_reads_expr(val, v);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for p in parts {
                rewrite_reads_expr(p, v);
            }
        }
        Expr::ModuleCall { args, kwargs, .. } => {
            for a in args {
                rewrite_reads_expr(a, v);
            }
            for (_, e) in kwargs {
                rewrite_reads_expr(e, v);
            }
        }
        Expr::New { args, .. } => {
            for a in args {
                rewrite_reads_expr(a, v);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(b) = base_expr {
                rewrite_reads_expr(b, v);
            }
            for a in type_args {
                rewrite_reads_expr(a, v);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => rewrite_reads_expr(constructor, v),
        Expr::AssignExpr { var, value, span } if var == v => {
            rewrite_reads_expr(value, v);
            let span = *span;
            let temp = format!("#box_assign#{}_{}", v, span.start);
            let value = std::mem::replace(value.as_mut(), Expr::Literal(Literal::Nothing, span));
            *expr = Expr::LetBlock {
                bindings: vec![],
                body: Block {
                    stmts: vec![
                        Stmt::Assign {
                            var: temp.clone(),
                            value,
                            span,
                        },
                        Stmt::IndexAssign {
                            array: v.to_string(),
                            indices: Vec::new(),
                            value: Expr::Var(temp.clone(), span),
                            span,
                        },
                        Stmt::Expr {
                            expr: Expr::Var(temp, span),
                            span,
                        },
                    ],
                    span,
                },
                span,
            };
        }
        Expr::AssignExpr { value, .. } => rewrite_reads_expr(value, v),
        Expr::ReturnExpr { value, .. } => {
            if let Some(e) = value {
                rewrite_reads_expr(e, v);
            }
        }
        Expr::Pair { key, value, .. } => {
            rewrite_reads_expr(key, v);
            rewrite_reads_expr(value, v);
        }
        Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}
