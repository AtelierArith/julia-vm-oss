//! File/module-mode soft-scope resolution (Issue #9210).
//!
//! Upstream Julia distinguishes two flavours of the "soft scope" that a
//! top-level `for`/`while`/`try` body introduces:
//!
//! * **Interactive (REPL/Jupyter)** — the REPL wraps each top-level expression
//!   in `Expr(:block, Expr(:softscope, true), ex)` (`stdlib/REPL/src/REPL.jl`,
//!   `softscope`). In this *lenient* mode an assignment to a name that already
//!   exists as a global re-uses the global binding, so `total += 1` mutates the
//!   global. This is sjulia's REPL behaviour (Issues #8691 / #8715) and is left
//!   untouched — the REPL lowers through `Lowering`, never through this pass.
//!
//! * **File / module (non-interactive)** — no `softscope` marker is injected,
//!   so the loop body is a *strict* soft scope. An assignment to a name that is
//!   an existing global is ambiguous: the name becomes a **new local**, a
//!   soft-scope warning is printed, and a read-before-write (`+=`) raises
//!   `UndefVarError`. Assignment to the global requires an explicit `global`.
//!   See `julia/src/julia-syntax.scm` (`resolve-scopes-`, the
//!   `Assignment to \`x\` in soft scope is ambiguous` warning).
//!
//! sjulia already binds a *fresh* loop-local for any loop-body assignment whose
//! name is **not** a pre-existing top-level global (an un-initialised slot read
//! raises `UndefVarError`, matching upstream). The only divergence is that a
//! name which *is* a top-level global assigned **before** the loop shares the
//! global's slot, so the loop mutates it. This pass closes that gap by renaming
//! such soft-scope-captured globals to fresh loop-locals (which are then
//! un-initialised, exactly reproducing upstream's new-local semantics) and
//! emitting the upstream-format warning. Explicit `global` declarations and
//! names that are not pre-existing globals are left unchanged.

use std::collections::HashSet;

use crate::ir::core::{Block, Expr, Stmt};
use crate::span::Span;

#[doc(hidden)]
pub use super::scope_bindings::{collect_scope_level_bindings, ScopeBindingInventory};

/// Source-ordered module bindings visible to later strict soft scopes.
///
/// Julia treats an existing mutable global as ambiguous inside a file-mode
/// soft scope, but a same-named `const` assignment becomes a fresh local
/// without that warning (#11305). Keep both facts in the same inventory so
/// clause and loop consumers cannot infer constness from a bare name set.
#[derive(Clone, Default)]
struct ToplevelBindingInventory {
    globals: HashSet<String>,
    consts: HashSet<String>,
    /// Names consumed by an earlier clause-local slot. They are not globals,
    /// but a later soft scope must mint a new internal slot rather than reuse
    /// whole-program compiler metadata under the same bare spelling (#11322).
    retired_clause_locals: HashSet<String>,
}

impl ToplevelBindingInventory {
    fn contains(&self, name: &str) -> bool {
        self.globals.contains(name)
    }

    fn is_const(&self, name: &str) -> bool {
        self.consts.contains(name)
    }

    fn is_retired_clause_local(&self, name: &str) -> bool {
        self.retired_clause_locals.contains(name)
    }

    fn record_global(&mut self, name: impl Into<String>) {
        let name = name.into();
        self.retired_clause_locals.remove(&name);
        self.globals.insert(name);
    }

    fn record_const(&mut self, name: impl Into<String>) {
        let name = name.into();
        self.retired_clause_locals.remove(&name);
        self.globals.insert(name.clone());
        self.consts.insert(name);
    }

    fn retire_clause_local(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !self.globals.contains(&name) {
            self.retired_clause_locals.insert(name);
        }
    }
}

/// Apply upstream file/module soft-scope rules to a top-level `main` block.
///
/// Call this on the freshly lowered **user** program (before the prelude/Base
/// merge) on the strict, non-interactive surfaces only —
/// `pipeline::parse_and_lower_with_base_dir_mode(.., SoftScopeMode::Strict, ..)`
/// and its `parse_and_lower_strict` shortcut. These back the CLI
/// (`sjulia file.jl` / `-e` / piped stdin), the C ABI editor entries
/// (`compile_and_run*` / `compile_and_run_detailed` / `compile_and_run_streaming`),
/// and the WASM `run_from_source` host (Issue #9283). The interactive REPL lowers
/// through `Lowering` and never reaches this pass, so its lenient soft scope
/// (Issues #8691 / #8715) is preserved; the internal lenient entry points
/// (`parse_and_lower` / `parse_and_lower_with_base_dir`, used by prelude/Base
/// cache compilation and the fixture harness) also skip it.
///
/// `script_path` is the file the source came from, used only to render the
/// soft-scope warning location (`└ @ /abs/path/to/script.jl:<line>`, matching
/// `julia file.jl`, Issue #9283). Pass `None` for `-e` / piped stdin / host
/// buffers, where upstream itself prints `└ @ none:<line>`.
pub fn apply_file_mode_soft_scope(main: &mut Block, script_path: Option<&str>) {
    let mut counter = FreshCounter { next: 0 };
    let mut globals_before = ToplevelBindingInventory::default();
    let enclosing_locals = HashSet::new();
    process_toplevel_stmts(
        &mut main.stmts,
        &mut globals_before,
        &enclosing_locals,
        &mut counter,
        script_path,
    );
}

#[cfg(test)]
mod issue_11281_binding_tests {
    use super::*;
    use crate::ir::core::{Literal, LocalDeclKind};

    fn span() -> Span {
        Span::new(0, 0, 1, 1, 0, 0)
    }

    #[test]
    fn inventory_preserves_typed_and_expression_binding_provenance() {
        let span = span();
        let body = Block {
            stmts: vec![
                Stmt::LocalDecl {
                    var: "explicit".into(),
                    kind: LocalDeclKind::Explicit,
                    span,
                },
                Stmt::Expr {
                    expr: Expr::AssignExpr {
                        var: "expression".into(),
                        value: Box::new(Expr::Literal(Literal::Int(1), span)),
                        span,
                    },
                    span,
                },
                Stmt::Global {
                    names: vec!["module".into()],
                    span,
                },
                Stmt::DictAssign {
                    dict: "d".into(),
                    key: Expr::AssignExpr {
                        var: "key_expression".into(),
                        value: Box::new(Expr::Literal(Literal::Int(1), span)),
                        span,
                    },
                    value: Expr::Literal(Literal::Int(2), span),
                    span,
                },
            ],
            span,
        };
        let inventory = ScopeBindingInventory::collect(&body);
        assert!(inventory.explicit_locals.contains("explicit"));
        assert!(inventory.soft_bindings.contains("expression"));
        assert!(inventory.assignment_bindings.contains("expression"));
        assert!(inventory.soft_bindings.contains("key_expression"));
        assert!(inventory.assignment_bindings.contains("key_expression"));
        assert!(inventory.globals.contains("module"));
    }

    #[test]
    fn value_try_declares_its_result_as_compiler_enclosing() {
        let span = span();
        let stmt = Stmt::Try {
            try_block: Block {
                stmts: vec![Stmt::Expr {
                    expr: Expr::Literal(Literal::Int(1), span),
                    span,
                }],
                span,
            },
            catch_var: None,
            catch_block: None,
            else_block: None,
            finally_block: None,
            span,
        };
        let result = crate::lowering::expr::try_stmt_into_value_expr(stmt, span);
        assert!(
            matches!(result, Some(Expr::LetBlock { .. })),
            "try value must use a transparent wrapper"
        );
        let Some(Expr::LetBlock { body, .. }) = result else {
            return;
        };
        let inventory = ScopeBindingInventory::collect(&body);
        assert_eq!(inventory.compiler_enclosing.len(), 1);
        assert!(inventory
            .compiler_enclosing
            .iter()
            .all(|name| inventory.soft_bindings.contains(name)));
    }

    #[test]
    fn transparent_inventory_skips_nested_try_bindings() {
        let span = span();
        let body = Block {
            stmts: vec![
                Stmt::Assign {
                    var: "outer".into(),
                    value: Expr::Literal(Literal::Int(1), span),
                    span,
                },
                Stmt::Try {
                    try_block: Block {
                        stmts: vec![Stmt::Assign {
                            var: "clause".into(),
                            value: Expr::Literal(Literal::Int(2), span),
                            span,
                        }],
                        span,
                    },
                    catch_var: None,
                    catch_block: None,
                    else_block: None,
                    finally_block: None,
                    span,
                },
            ],
            span,
        };
        let inventory = ScopeBindingInventory::collect(&body);
        assert!(inventory.soft_bindings.contains("outer"));
        assert!(!inventory.soft_bindings.contains("clause"));
    }

    #[test]
    fn comprehension_filter_assignment_is_not_a_current_scope_binding() {
        let span = span();
        let assign = |name: &str| Expr::AssignExpr {
            var: name.into(),
            value: Box::new(Expr::Literal(Literal::Int(1), span)),
            span,
        };
        let body = Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::Comprehension {
                        body: Box::new(assign("body_local")),
                        var: "i".into(),
                        iter: Box::new(assign("outer_iter")),
                        filter: Some(Box::new(assign("filter_local"))),
                        span,
                    },
                    span,
                },
                Stmt::Expr {
                    expr: Expr::MultiComprehension {
                        body: Box::new(assign("multi_body_local")),
                        iterations: vec![
                            ("i".into(), assign("multi_outer_iter")),
                            ("j".into(), assign("multi_inner_iter")),
                        ],
                        filter: Some(Box::new(assign("multi_filter_local"))),
                        flatten: true,
                        span,
                    },
                    span,
                },
            ],
            span,
        };
        let inventory = ScopeBindingInventory::collect(&body);
        assert!(inventory.soft_bindings.contains("outer_iter"));
        assert!(!inventory.soft_bindings.contains("body_local"));
        assert!(!inventory.soft_bindings.contains("filter_local"));
        assert!(inventory.soft_bindings.contains("multi_outer_iter"));
        assert!(!inventory.soft_bindings.contains("multi_inner_iter"));
        assert!(!inventory.soft_bindings.contains("multi_body_local"));
        assert!(!inventory.soft_bindings.contains("multi_filter_local"));
    }
}

struct FreshCounter {
    next: usize,
}

impl FreshCounter {
    /// A fresh loop-local name for `base`. The `##softlocal` infix cannot occur
    /// in user source, so it can never collide with a real binding.
    fn fresh(&mut self, base: &str) -> String {
        let name = format!("{base}##softlocal{}", self.next);
        self.next += 1;
        name
    }

    /// A fresh hard-scope (`let`) loop-local name for `base` (Issue #9284). Uses
    /// a distinct `##letlocal` infix from [`Self::fresh`] so the two passes never
    /// mint colliding names; both markers are stripped from user-facing
    /// `UndefVarError` messages (`bytecode::error::strip_softlocal_suffix`).
    fn fresh_let(&mut self, base: &str) -> String {
        let name = format!("{base}##letlocal{}", self.next);
        self.next += 1;
        name
    }
}

/// Hard-scope `let` localization (Issue #9284).
///
/// A `let ... end` is a **hard** local scope: unlike the top-level soft scope
/// (which is REPL-vs-file dependent, see [`apply_file_mode_soft_scope`]), a
/// `for`/`while` loop nested inside a `let` that assigns a name resolving ONLY
/// to a module global must bind a **fresh loop-local**, so a read-before-write
/// (`+=`) raises `UndefVarError` with **no** soft-scope warning. This holds in
/// every execution mode — `julia file.jl` *and* the REPL both error — so this
/// pass runs unconditionally (it is not gated on
/// [`crate::pipeline::SoftScopeMode`]).
///
/// sjulia already binds a fresh loop-local for any loop-body assignment whose
/// name is not a pre-existing global (an un-initialised slot read raises
/// `UndefVarError`). The only divergence is a name that IS a pre-existing global
/// assigned before the `let`: sjulia leniently read through to that global and
/// wrote a `let`-local, so the loop left the global untouched and printed its
/// old value. This pass renames such loop-captured globals to fresh hard-scope
/// locals (then un-initialised), reproducing upstream's new-local semantics.
/// Names bound by the `let` (its bindings or a let-body-level assignment),
/// explicit `global` declarations, and loop variables are left untouched.
pub fn apply_hard_scope_let_localization(main: &mut Block) {
    let mut counter = FreshCounter { next: 0 };
    let mut globals_before = ToplevelBindingInventory::default();
    walk_toplevel_for_hard_lets(&mut main.stmts, &mut globals_before, &mut counter);
}

/// Walk a module-scope statement list in source order, localizing every
/// hard-scope `let` reached while accumulating the running set of names already
/// bound as globals (`globals_before`), exactly as [`process_toplevel_stmts`]
/// tracks them for the soft-scope pass.
fn walk_toplevel_for_hard_lets(
    stmts: &mut [Stmt],
    globals_before: &mut ToplevelBindingInventory,
    counter: &mut FreshCounter,
) {
    for stmt in stmts.iter_mut() {
        process_toplevel_stmt_for_hard_lets(stmt, globals_before, counter);
        record_toplevel_globals(stmt, globals_before);
    }
}

fn process_toplevel_stmt_for_hard_lets(
    stmt: &mut Stmt,
    globals_before: &mut ToplevelBindingInventory,
    counter: &mut FreshCounter,
) {
    match stmt {
        // Statement-position `let ... end`, or a scope-transparent
        // `begin`/`@time` wrapper (empty-bindings `LetBlock`) that may contain a
        // nested hard `let`.
        Stmt::Expr { expr, .. } => {
            process_toplevel_expr_for_hard_lets(expr, globals_before, counter)
        }
        // `x = let ... end` / `x = begin … let … end … end`: the loop lives in a
        // value-position `LetBlock`.
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            process_toplevel_expr_for_hard_lets(value, globals_before, counter)
        }
        // Scope-transparent control flow at module scope: recurse to find nested
        // top-level `let`s (their globals are shared with the module).
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            walk_toplevel_for_hard_lets(&mut then_branch.stmts, globals_before, counter);
            if let Some(block) = else_branch {
                walk_toplevel_for_hard_lets(&mut block.stmts, globals_before, counter);
            }
        }
        Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
            walk_toplevel_for_hard_lets(&mut block.stmts, globals_before, counter);
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            let mut try_bindings = globals_before.clone();
            walk_toplevel_for_hard_lets(&mut try_block.stmts, &mut try_bindings, counter);
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                let mut clause_bindings = globals_before.clone();
                walk_toplevel_for_hard_lets(&mut block.stmts, &mut clause_bindings, counter);
            }
        }
        _ => {}
    }
}

fn process_toplevel_expr_for_hard_lets(
    expr: &mut Expr,
    globals_before: &mut ToplevelBindingInventory,
    counter: &mut FreshCounter,
) {
    if let Expr::LetBlock { bindings, body, .. } = expr {
        if bindings.is_empty() {
            // Scope-transparent `begin`/`@time` block: its statements run at
            // module scope, so recurse (a nested hard `let` is still top-level).
            walk_toplevel_for_hard_lets(&mut body.stmts, globals_before, counter);
        } else {
            // A real hard-scope `let`.
            let enclosing_locals = HashSet::new();
            process_hard_scope_let(bindings, body, &enclosing_locals, globals_before, counter);
        }
    }
}

/// Localize a single hard-scope `let`: rename any loop-body assignment to a
/// pre-existing global that is NOT a scope local (a `let` binding, a
/// let-body-level assignment, or a let-body-level `global` declaration) to a
/// fresh hard-scope local, and recurse into nested `let`s (new hard scopes).
fn process_hard_scope_let(
    bindings: &[(crate::ir::core::InternedStr, Expr)],
    body: &mut Block,
    enclosing_locals: &HashSet<String>,
    globals_before: &ToplevelBindingInventory,
    counter: &mut FreshCounter,
) {
    // Names local to (or explicitly global within) this hard scope, which must
    // NOT be localized: enclosing-scope locals, this `let`'s own bindings, and
    // every name bound or `global`-declared directly in the let body (outside
    // nested loops / functions / nested `let`s).
    let mut scope_locals = enclosing_locals.clone();
    for (name, _) in bindings {
        scope_locals.insert(name.to_string());
    }
    let inventory = ScopeBindingInventory::collect(body);
    scope_locals.extend(inventory.binding_names().cloned());
    // `binding_names` intentionally excludes globals for compiler lexical
    // routing. Hard-let localization needs them as a separate exclusion set:
    // a let-body-level `global x` applies to its nested loops and must prevent
    // the `##letlocal` rewrite (Issue #9284).
    scope_locals.extend(inventory.globals);

    process_hardscope_body_stmts(
        &mut body.stmts,
        &scope_locals,
        globals_before,
        false,
        counter,
        None,
    );
}

/// Extend an enclosing hard-scope inventory with bindings owned by one nested
/// clause. Loops inside a `try`/`catch`/`else`/`finally` clause resolve names
/// against that whole clause, including declarations that occur before them;
/// sibling clauses must not share those declarations.
fn hard_scope_clause_bindings(
    enclosing: &HashSet<String>,
    block: &Block,
    binder: Option<&String>,
) -> HashSet<String> {
    let inventory = ScopeBindingInventory::collect(block);
    let mut bindings = enclosing.clone();
    bindings.extend(inventory.binding_names().cloned());
    bindings.extend(inventory.globals);
    if let Some(binder) = binder {
        bindings.insert(binder.clone());
    }
    bindings
}

/// Apply the soft-scope decision to assignments owned by one
/// try/catch/else/finally clause, then process its nested loops and clauses.
///
/// A clause is a distinct lexical owner but, at module level, it is still a
/// Julia soft scope: a fresh name becomes clause-local without warning, an
/// existing global becomes clause-local *with* the strict-file warning, and an
/// explicit `global` keeps the module binding. Function definitions are not
/// assignment slots and remain tracked by #11319's generic-identity work.
fn process_clause_scope(
    block: &mut Block,
    binder: Option<&String>,
    enclosing: &HashSet<String>,
    globals_before: &ToplevelBindingInventory,
    warn: bool,
    counter: &mut FreshCounter,
    script_path: Option<&str>,
) {
    let inventory = ScopeBindingInventory::collect(block);
    let mut to_localize: Vec<String> = inventory
        .assignment_bindings
        .iter()
        .filter(|name| {
            (globals_before.is_retired_clause_local(name) || globals_before.contains(name))
                && !enclosing.contains(*name)
                && binder != Some(*name)
                && !inventory.explicit_locals.contains(*name)
                && !inventory.compiler_enclosing.contains(*name)
                && !inventory.globals.contains(*name)
        })
        .cloned()
        .collect();
    to_localize.sort_by(|a, b| {
        let key = |name: &str| first_assign_span_block(block, name).map(|span| span.start);
        key(a).cmp(&key(b)).then_with(|| a.cmp(b))
    });

    for name in &to_localize {
        let fresh = if warn {
            counter.fresh(name)
        } else {
            counter.fresh_let(name)
        };
        if warn
            && globals_before.contains(name)
            && !globals_before.is_const(name)
            && !globals_before.is_retired_clause_local(name)
        {
            emit_soft_scope_warning(name, first_assign_span_block(block, name), script_path);
        }
        rename_name_block(block, name, &fresh);
    }

    let clause_scope = hard_scope_clause_bindings(enclosing, block, binder);
    process_hardscope_body_stmts(
        &mut block.stmts,
        &clause_scope,
        globals_before,
        warn,
        counter,
        script_path,
    );
}

/// Process nested scopes after the current scope's own localization: localize
/// child loops, preserve per-clause provenance, and recurse into nested lets.
fn process_hardscope_body_stmts(
    stmts: &mut [Stmt],
    scope_locals: &HashSet<String>,
    globals_before: &ToplevelBindingInventory,
    warn: bool,
    counter: &mut FreshCounter,
    script_path: Option<&str>,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
                let loop_vars = [var.clone()];
                localize_loop_ex(
                    body,
                    &loop_vars,
                    globals_before,
                    scope_locals,
                    warn,
                    counter,
                    script_path,
                );
            }
            Stmt::ForEachTuple { vars, body, .. } => {
                let loop_vars = vars.clone();
                localize_loop_ex(
                    body,
                    &loop_vars,
                    globals_before,
                    scope_locals,
                    warn,
                    counter,
                    script_path,
                );
            }
            Stmt::While { body, .. } => {
                localize_loop_ex(
                    body,
                    &[],
                    globals_before,
                    scope_locals,
                    warn,
                    counter,
                    script_path,
                );
            }
            // Scope-transparent control flow inside the `let`: same hard scope.
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                process_hardscope_body_stmts(
                    &mut then_branch.stmts,
                    scope_locals,
                    globals_before,
                    warn,
                    counter,
                    script_path,
                );
                if let Some(block) = else_branch {
                    process_hardscope_body_stmts(
                        &mut block.stmts,
                        scope_locals,
                        globals_before,
                        warn,
                        counter,
                        script_path,
                    );
                }
            }
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
                process_hardscope_body_stmts(
                    &mut block.stmts,
                    scope_locals,
                    globals_before,
                    warn,
                    counter,
                    script_path,
                );
            }
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                process_clause_scope(
                    try_block,
                    None,
                    scope_locals,
                    globals_before,
                    warn,
                    counter,
                    script_path,
                );
                if let Some(block) = catch_block {
                    process_clause_scope(
                        block,
                        catch_var.as_ref(),
                        scope_locals,
                        globals_before,
                        warn,
                        counter,
                        script_path,
                    );
                }
                for block in [else_block, finally_block].into_iter().flatten() {
                    process_clause_scope(
                        block,
                        None,
                        scope_locals,
                        globals_before,
                        warn,
                        counter,
                        script_path,
                    );
                }
            }
            // A nested hard `let` (or transparent block value) as a statement or
            // assignment value: a new hard scope whose enclosing locals include
            // this scope's locals.
            Stmt::Expr { expr, .. }
            | Stmt::Assign { value: expr, .. }
            | Stmt::AddAssign { value: expr, .. } => {
                if let Expr::LetBlock { bindings, body, .. } = expr {
                    if bindings.is_empty() {
                        process_hardscope_body_stmts(
                            &mut body.stmts,
                            scope_locals,
                            globals_before,
                            warn,
                            counter,
                            script_path,
                        );
                    } else {
                        process_hard_scope_let(
                            bindings,
                            body,
                            scope_locals,
                            globals_before,
                            counter,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// If `expr` is a scope-transparent `begin ... end` block, return its body.
///
/// A top-level `begin ... end` — and every `@time`/`@timed`/`@elapsed`-style
/// macro expansion that splices the user code into an escaped block — lowers to
/// an **empty-bindings** `Expr::LetBlock` (see
/// `lowering::expr::misc::lower_block_as_expr`). A real `let ... end` is a hard
/// scope and is *never* matched here: its lowering always injects at least one
/// synthetic `__sjulia_let_scope_*` binding (see
/// `lowering::expr::misc::lower_let_expr`), so `bindings` is non-empty. This is
/// the exact distinction upstream draws between the scope-transparent `begin`
/// and the scope-introducing `let`.
fn scope_transparent_block_mut(expr: &mut Expr) -> Option<&mut Block> {
    match expr {
        Expr::LetBlock { bindings, body, .. } if bindings.is_empty() => Some(body),
        _ => None,
    }
}

/// Immutable counterpart of [`scope_transparent_block_mut`].
fn scope_transparent_block(expr: &Expr) -> Option<&Block> {
    match expr {
        Expr::LetBlock { bindings, body, .. } if bindings.is_empty() => Some(body),
        _ => None,
    }
}

/// Extend inherited locals with declarations owned by one scope-transparent
/// block. Upstream resolves the whole `scope-block` at once: explicit and
/// compiler-generated locals therefore enclose nested `try`/loop soft scopes,
/// even when their declaration precedes the nested construct (#11415).
/// Ordinary assignments stay module globals, and an explicit `global` in the
/// same block wins over a same-spelled local declaration.
fn scope_locals_for_block(enclosing: &HashSet<String>, block: &Block) -> HashSet<String> {
    let mut locals = enclosing.clone();
    extend_scope_locals(&mut locals, block);
    locals
}

fn extend_scope_locals(locals: &mut HashSet<String>, block: &Block) {
    let inventory = ScopeBindingInventory::collect(block);
    locals.extend(
        inventory
            .explicit_locals
            .iter()
            .chain(inventory.compiler_enclosing.iter())
            .filter(|name| !inventory.globals.contains(*name))
            .cloned(),
    );
}

/// Walk the statements of a top-level (module) scope in source order. `for`,
/// `while`, and each `try` clause introduce soft scopes that get localized;
/// `if` and `begin` are scope-transparent at top level, so we recurse into them
/// while accumulating the running set of names already assigned as globals
/// (`globals_before`).
///
/// The scope-transparent shapes the real parse→lower pipeline actually produces
/// for `begin`/`@time` are **empty-bindings `Expr::LetBlock`s**, reached here
/// two ways (verified by an IR probe on `parse_cli_program`, Issue #9210):
///
/// * a top-level `begin ... end` — and the outer wrapper of an expanded
///   `@time ...` — is a `Stmt::Expr { expr: LetBlock { bindings: [], .. } }`;
/// * `@time` additionally nests the user loop in a *value-position* empty
///   `LetBlock` (`result = begin … end`), so we also look through the value of
///   a top-level `Assign`/`AddAssign`.
///
/// The `Stmt::Block`/`Stmt::Timed` arms below cover alternate/legacy IR (a
/// compound-statement `Stmt::Block`, or the backwards-compat `Timed` node that
/// current `@time` lowering no longer emits); recursing into them stays
/// correct, but they are NOT how `begin`/`@time` reach this pass.
fn process_toplevel_stmts(
    stmts: &mut [Stmt],
    globals_before: &mut ToplevelBindingInventory,
    enclosing_locals: &HashSet<String>,
    counter: &mut FreshCounter,
    script_path: Option<&str>,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
                let loop_vars = [var.clone()];
                localize_loop(
                    body,
                    &loop_vars,
                    globals_before,
                    enclosing_locals,
                    counter,
                    script_path,
                );
            }
            Stmt::ForEachTuple { vars, body, .. } => {
                let loop_vars = vars.clone();
                localize_loop(
                    body,
                    &loop_vars,
                    globals_before,
                    enclosing_locals,
                    counter,
                    script_path,
                );
            }
            Stmt::While { body, .. } => {
                localize_loop(
                    body,
                    &[],
                    globals_before,
                    enclosing_locals,
                    counter,
                    script_path,
                );
            }
            // Scope-transparent constructs: recurse so a loop nested inside a
            // top-level `if`/`begin`/`@time` is still treated as a top-level
            // soft scope (upstream lowers these without a new scope).
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut branch_locals = scope_locals_for_block(enclosing_locals, then_branch);
                if let Some(block) = else_branch.as_ref() {
                    extend_scope_locals(&mut branch_locals, block);
                }
                process_toplevel_stmts(
                    &mut then_branch.stmts,
                    globals_before,
                    &branch_locals,
                    counter,
                    script_path,
                );
                if let Some(else_branch) = else_branch {
                    process_toplevel_stmts(
                        &mut else_branch.stmts,
                        globals_before,
                        &branch_locals,
                        counter,
                        script_path,
                    );
                }
            }
            // Top-level `begin ... end` / `@time` wrapper (empty-bindings
            // `LetBlock` in statement position).
            Stmt::Expr { expr, .. } => {
                if let Some(body) = scope_transparent_block_mut(expr) {
                    let block_locals = scope_locals_for_block(enclosing_locals, body);
                    process_toplevel_stmts(
                        &mut body.stmts,
                        globals_before,
                        &block_locals,
                        counter,
                        script_path,
                    );
                }
            }
            // `@time`'s `result = begin … end` and a user `x = begin … end`:
            // the loop lives inside a value-position empty-bindings `LetBlock`.
            Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
                if let Some(body) = scope_transparent_block_mut(value) {
                    let block_locals = scope_locals_for_block(enclosing_locals, body);
                    process_toplevel_stmts(
                        &mut body.stmts,
                        globals_before,
                        &block_locals,
                        counter,
                        script_path,
                    );
                }
            }
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
                let block_locals = scope_locals_for_block(enclosing_locals, block);
                process_toplevel_stmts(
                    &mut block.stmts,
                    globals_before,
                    &block_locals,
                    counter,
                    script_path,
                );
            }
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                process_clause_scope(
                    try_block,
                    None,
                    enclosing_locals,
                    globals_before,
                    true,
                    counter,
                    script_path,
                );
                if let Some(block) = catch_block {
                    process_clause_scope(
                        block,
                        catch_var.as_ref(),
                        enclosing_locals,
                        globals_before,
                        true,
                        counter,
                        script_path,
                    );
                }
                if let Some(block) = else_block {
                    process_clause_scope(
                        block,
                        None,
                        enclosing_locals,
                        globals_before,
                        true,
                        counter,
                        script_path,
                    );
                }
                if let Some(block) = finally_block {
                    process_clause_scope(
                        block,
                        None,
                        enclosing_locals,
                        globals_before,
                        true,
                        counter,
                        script_path,
                    );
                }
            }
            _ => {}
        }
        // Record global bindings introduced by this statement so that a *later*
        // loop sees them as pre-existing globals. Mirrors upstream's
        // `defined-julia-global` check being evaluated in source order.
        record_toplevel_globals_excluding(stmt, globals_before, enclosing_locals);
    }
}

/// Localize soft-scope-captured globals inside a single top-level loop body.
fn localize_loop(
    body: &mut Block,
    loop_vars: &[String],
    globals_before: &ToplevelBindingInventory,
    enclosing_locals: &HashSet<String>,
    counter: &mut FreshCounter,
    script_path: Option<&str>,
) {
    localize_loop_ex(
        body,
        loop_vars,
        globals_before,
        enclosing_locals,
        true,
        counter,
        script_path,
    );
}

/// Localize loop-captured globals inside a single loop body.
///
/// Shared by the top-level soft-scope pass (`warn = true`, no extra exclusions,
/// `##softlocal` names) and the hard-scope `let` pass (Issue #9284; `warn =
/// false`, `extra_excluded` = the enclosing hard scope's locals/`global`
/// declarations, `##letlocal` names). For every name assigned in the loop that
/// is a pre-existing global, is not explicitly `global`-declared inside the
/// loop, is not a loop variable, and is not in `extra_excluded`, all loop-body
/// occurrences (reads and writes) are renamed to one fresh local so a
/// read-before-write hits an un-initialised slot (`UndefVarError`).
fn localize_loop_ex(
    body: &mut Block,
    loop_vars: &[String],
    globals_before: &ToplevelBindingInventory,
    extra_excluded: &HashSet<String>,
    warn: bool,
    counter: &mut FreshCounter,
    script_path: Option<&str>,
) {
    // Names assigned somewhere in the body (soft-scope level; not descending
    // into nested function definitions).
    let mut assigned: HashSet<String> = HashSet::new();
    collect_soft_assigned_block(body, &mut assigned);

    // Names explicitly declared `global` inside the body: those resolve to the
    // module binding and must not be localized.
    let mut global_decls: HashSet<String> = HashSet::new();
    collect_global_decls_block(body, &mut global_decls);

    let loop_var_set: HashSet<&str> = loop_vars.iter().map(String::as_str).collect();

    let mut to_localize: Vec<String> = assigned
        .iter()
        .filter(|name| {
            (globals_before.contains(name) || globals_before.is_retired_clause_local(name))
                && !global_decls.contains(*name)
                && !loop_var_set.contains(name.as_str())
                && !extra_excluded.contains(*name)
        })
        .cloned()
        .collect();

    // Upstream warns (and gensyms) in SOURCE order, not alphabetical order
    // (Issue #9283). Order each captured name by the byte offset of its first
    // in-body assignment; fall back to the name itself for the (rare) same-span
    // case so numbering stays deterministic across HashSet iteration orders.
    to_localize.sort_by(|a, b| {
        let key = |name: &str| first_assign_span_block(body, name).map(|s| s.start);
        key(a).cmp(&key(b)).then_with(|| a.cmp(b))
    });

    for name in &to_localize {
        let fresh = if warn {
            counter.fresh(name)
        } else {
            counter.fresh_let(name)
        };
        if warn
            && globals_before.contains(name)
            && !globals_before.is_const(name)
            && !globals_before.is_retired_clause_local(name)
        {
            emit_soft_scope_warning(name, first_assign_span_block(body, name), script_path);
        }
        rename_name_block(body, name, &fresh);
    }

    // Nested loops and hard try clauses are separate provenance scopes. Walk
    // them only after this loop's own bindings have received their final names,
    // so children inherit localized parents while sibling clauses remain
    // isolated from one another (Issue #11316).
    let inventory = ScopeBindingInventory::collect(body);
    let mut nested_scope = extra_excluded.clone();
    nested_scope.extend(loop_vars.iter().cloned());
    nested_scope.extend(inventory.binding_names().cloned());
    nested_scope.extend(inventory.globals);
    process_hardscope_body_stmts(
        &mut body.stmts,
        &nested_scope,
        globals_before,
        warn,
        counter,
        script_path,
    );
}

/// Render the upstream-format soft-scope ambiguity warning as a two-line string.
///
/// `script_path` renders the location line: `└ @ /abs/path/to/script.jl:<line>`
/// for `julia file.jl`, or `└ @ none:<line>` when `None` (upstream itself prints
/// `none` for `-e` / piped stdin and host buffers). Pure so it can be unit tested
/// without capturing stderr (Issue #9283).
fn format_soft_scope_warning(name: &str, span: Option<Span>, script_path: Option<&str>) -> String {
    let line = span.map(|s| s.start_line).unwrap_or(0);
    let location = script_path.unwrap_or("none");
    format!(
        "┌ Warning: Assignment to `{name}` in soft scope is ambiguous because a global \
variable by the same name exists: `{name}` will be treated as a new local. Disambiguate \
by using `local {name}` to suppress this warning or `global {name}` to assign to the \
existing global variable.\n└ @ {location}:{line}"
    )
}

/// Print the upstream-format soft-scope ambiguity warning to stderr.
///
/// This only runs on the strict, non-interactive script path
/// (`SoftScopeMode::Strict`), a stderr-capable context; the lenient host/REPL
/// default never reaches it. `clippy::print_stderr` is allowed here for the same
/// reason as the other diagnostic reporters in this crate (`compile::profile`,
/// `compile::budget_metrics`).
#[allow(clippy::print_stderr)]
fn emit_soft_scope_warning(name: &str, span: Option<Span>, script_path: Option<&str>) {
    eprintln!("{}", format_soft_scope_warning(name, span, script_path));
}

// === Global-binding recording ===============================================

/// Record top-level global bindings introduced by `stmt`. Descends through
/// scope-transparent `if`/`begin` bodies. Try clauses own their ordinary
/// assignments, so only explicit `global` declarations cross that boundary.
/// Loop bodies and function definitions do not introduce implicit top-level
/// globals.
fn record_toplevel_globals(stmt: &Stmt, out: &mut ToplevelBindingInventory) {
    record_toplevel_globals_excluding(stmt, out, &HashSet::new());
}

/// Scope-aware counterpart used while walking one top-level expression.
/// Hygienic macro locals live in a transparent block rather than a hard
/// `let`; their assignments must not be promoted into module-global
/// provenance, and nested clause bindings must not retire those outer locals
/// (#11415).
fn record_toplevel_globals_excluding(
    stmt: &Stmt,
    out: &mut ToplevelBindingInventory,
    enclosing_locals: &HashSet<String>,
) {
    match stmt {
        Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
            if !enclosing_locals.contains(var.as_str()) {
                out.record_global(var.to_string());
            }
            // `x = begin … end`: the transparent block's own statements run at
            // module scope, so any globals they assign are pre-existing for a
            // later loop (mirrors the statement-position `begin` arm below).
            if let Some(block) = scope_transparent_block(value) {
                let block_locals = scope_locals_for_block(enclosing_locals, block);
                for s in &block.stmts {
                    record_toplevel_globals_excluding(s, out, &block_locals);
                }
            }
        }
        Stmt::DestructuringAssign { targets, .. } => {
            for target in targets {
                if !enclosing_locals.contains(target) {
                    out.record_global(target.clone());
                }
            }
        }
        Stmt::Global { names, .. } => {
            for name in names {
                out.record_global(name.clone());
            }
        }
        // A top-level `begin … end` (or `@time` wrapper) is scope-transparent,
        // so globals it assigns become pre-existing for later top-level loops.
        Stmt::Expr { expr, .. } => {
            if let Some(block) = scope_transparent_block(expr) {
                let block_locals = scope_locals_for_block(enclosing_locals, block);
                for s in &block.stmts {
                    record_toplevel_globals_excluding(s, out, &block_locals);
                }
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            let mut branch_locals = scope_locals_for_block(enclosing_locals, then_branch);
            if let Some(block) = else_branch.as_ref() {
                extend_scope_locals(&mut branch_locals, block);
            }
            for s in &then_branch.stmts {
                record_toplevel_globals_excluding(s, out, &branch_locals);
            }
            if let Some(else_branch) = else_branch {
                for s in &else_branch.stmts {
                    record_toplevel_globals_excluding(s, out, &branch_locals);
                }
            }
        }
        Stmt::Block(block) => {
            if let Some(name) = lowered_const_declaration_name(block) {
                if !enclosing_locals.contains(name) {
                    out.record_const(name);
                }
            } else {
                let block_locals = scope_locals_for_block(enclosing_locals, block);
                for stmt in &block.stmts {
                    record_toplevel_globals_excluding(stmt, out, &block_locals);
                }
            }
        }
        Stmt::Timed { body, .. } => {
            let block_locals = scope_locals_for_block(enclosing_locals, body);
            for stmt in &body.stmts {
                record_toplevel_globals_excluding(stmt, out, &block_locals);
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
            for block in [
                Some(try_block),
                catch_block.as_ref(),
                else_block.as_ref(),
                finally_block.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                record_explicit_globals_block(block, out);
                retire_clause_bindings(block, out, enclosing_locals);
            }
            if let Some(binder) = catch_var {
                if !enclosing_locals.contains(binder) {
                    out.retire_clause_local(binder.clone());
                }
            }
        }
        _ => {}
    }
}

/// Preserve the fact that a bare spelling has already belonged to a completed
/// clause-local slot. Later scopes must allocate a new slot for that spelling,
/// but this fact is not a module global and must never trigger a warning.
fn retire_clause_bindings(
    block: &Block,
    out: &mut ToplevelBindingInventory,
    enclosing_locals: &HashSet<String>,
) {
    let inventory = ScopeBindingInventory::collect(block);
    for name in inventory.assignment_bindings {
        if !inventory.globals.contains(&name) && !enclosing_locals.contains(&name) {
            out.retire_clause_local(name);
        }
    }

    for stmt in &block.stmts {
        retire_nested_try_bindings(stmt, out, enclosing_locals);
    }
}

fn retire_nested_try_bindings(
    stmt: &Stmt,
    out: &mut ToplevelBindingInventory,
    enclosing_locals: &HashSet<String>,
) {
    match stmt {
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            retire_clause_bindings(try_block, out, enclosing_locals);
            for clause in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                retire_clause_bindings(clause, out, enclosing_locals);
            }
            if let Some(binder) = catch_var {
                if !enclosing_locals.contains(binder) {
                    out.retire_clause_local(binder.clone());
                }
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            let mut branch_locals = scope_locals_for_block(enclosing_locals, then_branch);
            if let Some(block) = else_branch.as_ref() {
                extend_scope_locals(&mut branch_locals, block);
            }
            for nested in &then_branch.stmts {
                retire_nested_try_bindings(nested, out, &branch_locals);
            }
            if let Some(block) = else_branch {
                for nested in &block.stmts {
                    retire_nested_try_bindings(nested, out, &branch_locals);
                }
            }
        }
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Block(body)
        | Stmt::Timed { body, .. }
        | Stmt::TestSet { body, .. } => {
            let block_locals = scope_locals_for_block(enclosing_locals, body);
            for nested in &body.stmts {
                retire_nested_try_bindings(nested, out, &block_locals);
            }
        }
        Stmt::Expr {
            expr: Expr::LetBlock { body, .. },
            ..
        }
        | Stmt::Assign {
            value: Expr::LetBlock { body, .. },
            ..
        }
        | Stmt::AddAssign {
            value: Expr::LetBlock { body, .. },
            ..
        } => {
            let block_locals = scope_locals_for_block(enclosing_locals, body);
            for nested in &body.stmts {
                retire_nested_try_bindings(nested, out, &block_locals);
            }
        }
        _ => {}
    }
}

/// `const x = value` lowers to `Block([declare_const("x"), Assign(x, ...)])`.
/// Recover that preserved provenance instead of flattening the block into an
/// indistinguishable mutable assignment (#11305).
fn lowered_const_declaration_name(block: &Block) -> Option<&str> {
    match block.stmts.first() {
        Some(Stmt::Expr {
            expr: Expr::Call { function, args, .. },
            ..
        }) if function == "#__sjulia_declare_const__" => match args.first() {
            Some(Expr::Literal(crate::ir::core::Literal::Str(name), _)) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

/// Collect explicit module effects from a prior clause without collapsing its
/// fresh lexical bindings into module-global provenance. Nested executable
/// scopes may contain `global`, but function bodies and quoted expressions do
/// not execute while the containing statement is evaluated.
fn record_explicit_globals_block(block: &Block, out: &mut ToplevelBindingInventory) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Global { names, .. } => {
                for name in names {
                    out.record_global(name.clone());
                }
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                record_explicit_globals_block(then_branch, out);
                if let Some(block) = else_branch {
                    record_explicit_globals_block(block, out);
                }
            }
            Stmt::For { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachTuple { body, .. }
            | Stmt::While { body, .. }
            | Stmt::Timed { body, .. }
            | Stmt::TestSet { body, .. } => record_explicit_globals_block(body, out),
            Stmt::Block(body) => {
                if let Some(name) = lowered_const_declaration_name(body) {
                    if out.contains(name) {
                        out.record_const(name);
                    }
                } else {
                    record_explicit_globals_block(body, out);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                record_explicit_globals_block(try_block, out);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    record_explicit_globals_block(block, out);
                }
            }
            Stmt::Expr {
                expr: Expr::LetBlock { body, .. },
                ..
            }
            | Stmt::Assign {
                value: Expr::LetBlock { body, .. },
                ..
            }
            | Stmt::AddAssign {
                value: Expr::LetBlock { body, .. },
                ..
            } => record_explicit_globals_block(body, out),
            _ => {}
        }
    }
}

// === Assigned-name / global-decl collection =================================

/// Collect plain assignment targets (`x = …`, `x += …`, `(a, b) = …`, and
/// expression-position `x = …`) reachable inside a loop body's soft scope.
/// Descends through scope-transparent control flow and nested loops, but stops
/// at function definitions and hard `try` clause boundaries. Clause assignments
/// are localized by their own provenance-aware walk, so siblings cannot affect
/// one another (Issue #11316). Hard binders are intentionally excluded.
fn collect_soft_assigned_block(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_soft_assigned_stmt(stmt, out);
    }
}

fn collect_soft_assigned_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
            out.insert(var.to_string());
            collect_soft_assigned_expr(value, out);
        }
        Stmt::DestructuringAssign { targets, value, .. } => {
            out.extend(targets.iter().cloned());
            collect_soft_assigned_expr(value, out);
        }
        Stmt::For {
            start, end, body, ..
        } => {
            collect_soft_assigned_expr(start, out);
            collect_soft_assigned_expr(end, out);
            collect_soft_assigned_block(body, out);
        }
        Stmt::ForEach { iterable, body, .. } => {
            collect_soft_assigned_expr(iterable, out);
            collect_soft_assigned_block(body, out);
        }
        Stmt::ForEachTuple { iterable, body, .. } => {
            collect_soft_assigned_expr(iterable, out);
            collect_soft_assigned_block(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_soft_assigned_expr(condition, out);
            collect_soft_assigned_block(body, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_soft_assigned_expr(condition, out);
            collect_soft_assigned_block(then_branch, out);
            if let Some(else_branch) = else_branch {
                collect_soft_assigned_block(else_branch, out);
            }
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            collect_soft_assigned_block(block, out);
        }
        Stmt::Expr { expr, .. } => collect_soft_assigned_expr(expr, out),
        Stmt::Return {
            value: Some(value), ..
        } => collect_soft_assigned_expr(value, out),
        Stmt::IndexAssign { value, .. }
        | Stmt::FieldAssign { value, .. }
        | Stmt::DictAssign { value, .. } => collect_soft_assigned_expr(value, out),
        // Function definitions are hard scopes: their assignments do not leak.
        _ => {}
    }
}

fn collect_soft_assigned_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::AssignExpr { var, value, .. } => {
            out.insert(var.to_string());
            collect_soft_assigned_expr(value, out);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_soft_assigned_expr(left, out);
            collect_soft_assigned_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_soft_assigned_expr(operand, out),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_soft_assigned_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_soft_assigned_expr(value, out);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_soft_assigned_expr(arg, out);
            }
        }
        Expr::ArrayLiteral { elements, .. }
        | Expr::TupleLiteral { elements, .. }
        | Expr::StringConcat {
            parts: elements, ..
        } => {
            for element in elements {
                collect_soft_assigned_expr(element, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_soft_assigned_expr(array, out);
            for index in indices {
                collect_soft_assigned_expr(index, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_soft_assigned_expr(start, out);
            if let Some(step) = step {
                collect_soft_assigned_expr(step, out);
            }
            collect_soft_assigned_expr(stop, out);
        }
        Expr::FieldAccess { object, .. } => collect_soft_assigned_expr(object, out),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_soft_assigned_expr(value, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_soft_assigned_expr(key, out);
            collect_soft_assigned_expr(value, out);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_soft_assigned_expr(key, out);
                collect_soft_assigned_expr(value, out);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_soft_assigned_expr(condition, out);
            collect_soft_assigned_expr(then_expr, out);
            collect_soft_assigned_expr(else_expr, out);
        }
        Expr::ReturnExpr {
            value: Some(value), ..
        } => collect_soft_assigned_expr(value, out),
        // Comprehensions/generators/let/quotes introduce their own bindings or
        // quoted code; their inner assignments do not localize the outer loop's
        // globals, so we do not descend for assignment-target collection.
        _ => {}
    }
}

/// Collect names declared `global` inside one loop soft scope, descending
/// through scope-transparent control flow and nested loops but stopping at
/// function and hard `try` clause boundaries.
///
/// Nested-loop descent is required for correctness (Issue #9493): the assigned-
/// name collection ([`collect_soft_assigned_block`]) descends into nested loops,
/// so a `global val` declared next to a `val += 1` in an inner loop must be
/// visible too, or the pass localizes an explicitly-global name and a program
/// that runs under upstream `julia file.jl` raises `UndefVarError`. Upstream
/// resolves the inner scope's `global` declaration to the module binding, so a
/// declaration at any loop depth exempts the name. (Known single-pass
/// approximation, documented in Issue #9493: a name bare-assigned at one level
/// and `global`-declared only at another within the same top-level loop nest is
/// exempted uniformly, where upstream would localize only the undeclared
/// scope's assignment.)
fn collect_global_decls_block(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_global_decls_stmt(stmt, out);
    }
}

fn collect_global_decls_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Global { names, .. } => out.extend(names.iter().cloned()),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_global_decls_block(then_branch, out);
            if let Some(else_branch) = else_branch {
                collect_global_decls_block(else_branch, out);
            }
        }
        // Nested loops: a `global` declaration inside an inner loop still
        // resolves the name to the module binding (Issue #9493) — keep this
        // collection symmetric with `collect_soft_assigned_stmt`, which also
        // descends into nested loop bodies.
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. } => {
            collect_global_decls_block(body, out);
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            collect_global_decls_block(block, out);
        }
        _ => {}
    }
}

/// Line span of the first `name = …` / `name += …` assignment inside `block`.
fn first_assign_span_block(block: &Block, name: &str) -> Option<Span> {
    for stmt in &block.stmts {
        if let Some(span) = first_assign_span_stmt(stmt, name) {
            return Some(span);
        }
    }
    None
}

fn first_assign_span_stmt(stmt: &Stmt, name: &str) -> Option<Span> {
    match stmt {
        Stmt::Assign { var, span, .. } | Stmt::AddAssign { var, span, .. } if var == name => {
            Some(*span)
        }
        Stmt::DestructuringAssign { targets, span, .. } if targets.iter().any(|t| t == name) => {
            Some(*span)
        }
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Timed { body, .. }
        | Stmt::TestSet { body, .. } => first_assign_span_block(body, name),
        Stmt::Block(block) => first_assign_span_block(block, name),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => first_assign_span_block(then_branch, name).or_else(|| {
            else_branch
                .as_ref()
                .and_then(|block| first_assign_span_block(block, name))
        }),
        _ => None,
    }
}

// === Renaming ===============================================================

/// Rename every free occurrence of `from` to `to` inside `block`, EXCEPT inside
/// nested function definitions (hard scopes) and quoted metaprogramming code.
/// Both reads (`Expr::Var`) and assignment targets are rewritten so that a
/// read-before-write of a freshly localized name hits an un-initialised slot.
fn rename_name_block(block: &mut Block, from: &str, to: &str) {
    for stmt in &mut block.stmts {
        rename_name_stmt(stmt, from, to);
    }
}

fn clause_owns_name(block: &Block, binder: Option<&str>, name: &str) -> bool {
    if binder == Some(name) {
        return true;
    }
    let inventory = ScopeBindingInventory::collect(block);
    let function_only_binding =
        inventory.soft_bindings.contains(name) && !inventory.assignment_bindings.contains(name);
    inventory.globals.contains(name)
        || inventory.explicit_locals.contains(name)
        || inventory.compiler_enclosing.contains(name)
        || function_only_binding
}

fn rename_name_in_clause(block: &mut Block, binder: Option<&str>, from: &str, to: &str) {
    // Explicit locals/globals, catch binders, and function-only identities
    // shadow the enclosing binding throughout the clause. An ordinary
    // assignment does not: when an enclosing soft scope already owns the name,
    // a nested try reuses that slot (#11159). Otherwise references cross the
    // boundary and must follow an enclosing binding that localization renamed
    // (Issue #11316).
    if !clause_owns_name(block, binder, from) {
        rename_name_block(block, from, to);
    }
}

fn rename_name_stmt(stmt: &mut Stmt, from: &str, to: &str) {
    match stmt {
        Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
            if var == from {
                *var = to.to_string();
            }
            rename_name_expr(value, from, to);
        }
        Stmt::DestructuringAssign { targets, value, .. } => {
            for target in targets.iter_mut() {
                if target == from {
                    *target = to.to_string();
                }
            }
            rename_name_expr(value, from, to);
        }
        Stmt::For {
            var,
            start,
            end,
            step,
            body,
            ..
        } => {
            rename_name_expr(start, from, to);
            rename_name_expr(end, from, to);
            if let Some(step) = step {
                rename_name_expr(step, from, to);
            }
            if var == from {
                *var = to.to_string();
            }
            rename_name_block(body, from, to);
        }
        Stmt::ForEach {
            var,
            iterable,
            body,
            ..
        } => {
            rename_name_expr(iterable, from, to);
            if var == from {
                *var = to.to_string();
            }
            rename_name_block(body, from, to);
        }
        Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            ..
        } => {
            rename_name_expr(iterable, from, to);
            for var in vars.iter_mut() {
                if var == from {
                    *var = to.to_string();
                }
            }
            rename_name_block(body, from, to);
        }
        Stmt::While {
            condition, body, ..
        } => {
            rename_name_expr(condition, from, to);
            rename_name_block(body, from, to);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            rename_name_expr(condition, from, to);
            rename_name_block(then_branch, from, to);
            if let Some(else_branch) = else_branch {
                rename_name_block(else_branch, from, to);
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
            rename_name_in_clause(try_block, None, from, to);
            if let Some(block) = catch_block {
                rename_name_in_clause(block, catch_var.as_deref(), from, to);
            }
            if let Some(block) = else_block {
                rename_name_in_clause(block, None, from, to);
            }
            if let Some(block) = finally_block {
                rename_name_in_clause(block, None, from, to);
            }
        }
        Stmt::Return {
            value: Some(value), ..
        } => rename_name_expr(value, from, to),
        Stmt::Expr { expr, .. } => rename_name_expr(expr, from, to),
        Stmt::IndexAssign {
            array,
            indices,
            value,
            ..
        } => {
            if array == from {
                *array = to.to_string();
            }
            for index in indices.iter_mut() {
                rename_name_expr(index, from, to);
            }
            rename_name_expr(value, from, to);
        }
        Stmt::FieldAssign { object, value, .. } => {
            if object == from {
                *object = to.to_string();
            }
            rename_name_expr(value, from, to);
        }
        Stmt::DictAssign {
            dict, key, value, ..
        } => {
            if dict == from {
                *dict = to.to_string();
            }
            rename_name_expr(key, from, to);
            rename_name_expr(value, from, to);
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            rename_name_block(block, from, to);
        }
        Stmt::Test { condition, .. } => rename_name_expr(condition, from, to),
        Stmt::TestThrows { expr, .. } => rename_name_expr(expr, from, to),
        // Do NOT touch: `global`/function definitions/labels/quoted code — hard
        // scopes and non-variable statements.
        _ => {}
    }
}

fn rename_name_expr(expr: &mut Expr, from: &str, to: &str) {
    match expr {
        Expr::Var(name, _) if name == from => {
            *name = to.to_string().into();
        }
        Expr::BinaryOp { left, right, .. } => {
            rename_name_expr(left, from, to);
            rename_name_expr(right, from, to);
        }
        Expr::UnaryOp { operand, .. } => rename_name_expr(operand, from, to),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args.iter_mut() {
                rename_name_expr(arg, from, to);
            }
            for (_, value) in kwargs.iter_mut() {
                rename_name_expr(value, from, to);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args.iter_mut() {
                rename_name_expr(arg, from, to);
            }
        }
        Expr::ArrayLiteral { elements, .. }
        | Expr::TupleLiteral { elements, .. }
        | Expr::StringConcat {
            parts: elements, ..
        } => {
            for element in elements.iter_mut() {
                rename_name_expr(element, from, to);
            }
        }
        Expr::Index { array, indices, .. } => {
            rename_name_expr(array, from, to);
            for index in indices.iter_mut() {
                rename_name_expr(index, from, to);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            rename_name_expr(start, from, to);
            if let Some(step) = step {
                rename_name_expr(step, from, to);
            }
            rename_name_expr(stop, from, to);
        }
        Expr::FieldAccess { object, .. } => rename_name_expr(object, from, to),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields.iter_mut() {
                rename_name_expr(value, from, to);
            }
        }
        Expr::Pair { key, value, .. } => {
            rename_name_expr(key, from, to);
            rename_name_expr(value, from, to);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs.iter_mut() {
                rename_name_expr(key, from, to);
                rename_name_expr(value, from, to);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            rename_name_expr(condition, from, to);
            rename_name_expr(then_expr, from, to);
            rename_name_expr(else_expr, from, to);
        }
        Expr::New { args, .. }
        | Expr::DynamicTypeConstruct {
            type_args: args, ..
        } => {
            for arg in args.iter_mut() {
                rename_name_expr(arg, from, to);
            }
        }
        Expr::AssignExpr { var, value, .. } => {
            if var == from {
                *var = to.to_string().into();
            }
            rename_name_expr(value, from, to);
        }
        Expr::ReturnExpr {
            value: Some(value), ..
        } => rename_name_expr(value, from, to),
        // Comprehension/Generator/MultiComprehension bind their own variables
        // and LetBlock introduces its own scope; if the bound name shadows
        // `from`, occurrences inside refer to that inner binding. We rename the
        // outer-scope iterator/binding expressions but only descend into the
        // body when the inner binder does NOT shadow `from`.
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
            rename_name_expr(iter, from, to);
            if var != from {
                rename_name_expr(body, from, to);
                if let Some(filter) = filter {
                    rename_name_expr(filter, from, to);
                }
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            let mut shadowed = false;
            for (var, iter) in iterations.iter_mut() {
                if !shadowed {
                    rename_name_expr(iter, from, to);
                }
                if var == from {
                    shadowed = true;
                }
            }
            if !shadowed {
                rename_name_expr(body, from, to);
                if let Some(filter) = filter {
                    rename_name_expr(filter, from, to);
                }
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            let mut shadowed = false;
            for (name, value) in bindings.iter_mut() {
                if !shadowed {
                    rename_name_expr(value, from, to);
                }
                if name == from {
                    shadowed = true;
                }
            }
            if !shadowed {
                rename_name_block(body, from, to);
            }
        }
        // Literals, function refs, quoted code, slices, etc.: no free variables
        // to rewrite here.
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use crate::ir::core::Literal;

    fn dummy_span() -> Span {
        Span::new(0, 0, 1, 1, 0, 0)
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block {
            stmts,
            span: dummy_span(),
        }
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string().into(), dummy_span())
    }

    fn int_lit(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), dummy_span())
    }

    /// `total += 1` inside a top-level loop, with `total` a pre-existing global,
    /// renames the loop-body occurrences to a fresh local (leaving the outer
    /// binding untouched).
    #[test]
    fn localizes_soft_scope_captured_global() {
        // total = 0
        // for i in 1:1
        //     total = total + 1
        // end
        let inner = Stmt::Assign {
            var: "total".to_string(),
            value: Expr::BinaryOp {
                op: crate::ir::core::BinaryOp::Add,
                left: Box::new(var("total")),
                right: Box::new(int_lit(1)),
                span: dummy_span(),
            },
            span: dummy_span(),
        };
        let loop_stmt = Stmt::For {
            var: "i".to_string(),
            start: int_lit(1),
            end: int_lit(1),
            step: None,
            body: block(vec![inner]),
            span: dummy_span(),
        };
        let mut main = block(vec![
            Stmt::Assign {
                var: "total".to_string(),
                value: int_lit(0),
                span: dummy_span(),
            },
            loop_stmt,
        ]);

        apply_file_mode_soft_scope(&mut main, None);

        // Outer binding stays `total`; loop body target is renamed.
        let Stmt::Assign { var: outer_var, .. } = &main.stmts[0] else {
            panic!("expected outer assign");
        };
        assert_eq!(outer_var, "total");

        let Stmt::For { body, .. } = &main.stmts[1] else {
            panic!("expected for loop");
        };
        let Stmt::Assign {
            var: inner_var,
            value,
            ..
        } = &body.stmts[0]
        else {
            panic!("expected inner assign");
        };
        assert_ne!(inner_var, "total", "loop-body target must be localized");
        assert!(inner_var.contains("##softlocal"));
        // The read side is renamed to the SAME fresh local, so `+=` reads an
        // un-initialised local (UndefVarError at runtime).
        let Expr::BinaryOp { left, .. } = value else {
            panic!("expected binary op");
        };
        let Expr::Var(read_name, _) = left.as_ref() else {
            panic!("expected var read");
        };
        assert_eq!(read_name, inner_var);
    }

    /// An explicit `global total` inside the loop keeps `total` bound to the
    /// module global (no rename).
    #[test]
    fn explicit_global_is_not_localized() {
        let body = block(vec![
            Stmt::Global {
                names: vec!["total".to_string()],
                span: dummy_span(),
            },
            Stmt::Assign {
                var: "total".to_string(),
                value: Expr::BinaryOp {
                    op: crate::ir::core::BinaryOp::Add,
                    left: Box::new(var("total")),
                    right: Box::new(int_lit(1)),
                    span: dummy_span(),
                },
                span: dummy_span(),
            },
        ]);
        let mut main = block(vec![
            Stmt::Assign {
                var: "total".to_string(),
                value: int_lit(0),
                span: dummy_span(),
            },
            Stmt::While {
                condition: Expr::Literal(Literal::Bool(false), dummy_span()),
                body,
                span: dummy_span(),
            },
        ]);

        apply_file_mode_soft_scope(&mut main, None);

        let Stmt::While { body, .. } = &main.stmts[1] else {
            panic!("expected while loop");
        };
        let Stmt::Assign { var: inner_var, .. } = &body.stmts[1] else {
            panic!("expected inner assign");
        };
        assert_eq!(inner_var, "total", "explicit global must not be localized");
    }

    // === Real parse→lower pipeline coverage (Issue #9210 fix-forward) =======
    //
    // The hand-constructed IR tests above pass even if the pass never descends
    // into the shapes the *real* pipeline emits. These parse actual source
    // through `parse_source_with_include` (the same lowering the CLI strict path
    // uses) and assert the pass reaches loops nested in `begin`/`@time`, which
    // lower to empty-bindings `LetBlock`s — the gap the adversarial review found.

    fn parse_main(src: &str) -> Block {
        crate::pipeline::parse_source_with_include(src, None)
            .expect("parse user program")
            .main
    }

    /// A loop nested in a top-level `begin … end` (empty-bindings `LetBlock`)
    /// must be localized on the real pipeline, not only on hand-built IR.
    #[test]
    fn real_pipeline_begin_block_localizes_loop_global() {
        let mut main = parse_main(
            "total = 0\nbegin\n    for i in 1:3\n        total += 1\n    end\nend\nprintln(total)\n",
        );
        apply_file_mode_soft_scope(&mut main, None);
        let dump = format!("{main:#?}");
        assert!(
            dump.contains("total##softlocal"),
            "begin-nested loop global was not localized on the real pipeline: {dump}"
        );
    }

    /// A loop under `@time` (whose expansion nests the loop in a value-position
    /// empty-bindings `LetBlock`) must be localized on the real pipeline. The
    /// integration crate owns VM-backed macro expansion, so this lowering-crate
    /// unit test parses the real loop and embeds it in the exact post-expansion
    /// IR shape exercised here.
    #[test]
    fn real_pipeline_time_macro_localizes_loop_global() {
        let mut main = parse_main("total = 0\nfor i in 1:3\n    total += 1\nend\nprintln(total)\n");
        let loop_stmt = main.stmts.remove(1);
        let span = dummy_span();
        main.stmts.insert(
            1,
            Stmt::Expr {
                expr: Expr::LetBlock {
                    bindings: Vec::new(),
                    body: block(vec![Stmt::Assign {
                        var: "result".to_string(),
                        value: Expr::LetBlock {
                            bindings: Vec::new(),
                            body: block(vec![loop_stmt]),
                            span,
                        },
                        span,
                    }]),
                    span,
                },
                span,
            },
        );
        apply_file_mode_soft_scope(&mut main, None);
        let dump = format!("{main:#?}");
        assert!(
            dump.contains("total##softlocal"),
            "@time-nested loop global was not localized on the real pipeline: {dump}"
        );
    }

    /// `let … end` is a hard scope: its lowering injects a synthetic
    /// `__sjulia_let_scope_*` binding, so the empty-bindings `begin`/`@time`
    /// fast path must NOT treat it as transparent (which would emit a spurious
    /// soft-scope warning). Guards against over-application.
    #[test]
    fn real_pipeline_hard_scope_let_is_not_localized() {
        let mut main =
            parse_main("total = 0\nlet\n    for i in 1:3\n        total += 1\n    end\nend\n");
        apply_file_mode_soft_scope(&mut main, None);
        let dump = format!("{main:#?}");
        assert!(
            !dump.contains("softlocal"),
            "hard-scope `let` must not be localized by the begin/@time fast path: {dump}"
        );
    }

    // === Hard-scope `let` localization coverage (Issue #9284) ===============

    /// The reported MWE: a loop inside an (empty) `let` that `+=`s a pre-existing
    /// global is localized to a fresh `##letlocal` name so the read-before-write
    /// raises `UndefVarError`.
    #[test]
    fn hard_let_localizes_loop_global() {
        let mut main = parse_main(
            "total = 0\nlet\n    for i in 1:3\n        total += 1\n    end\nend\nprintln(total)\n",
        );
        apply_hard_scope_let_localization(&mut main);
        let dump = format!("{main:#?}");
        assert!(
            dump.contains("total##letlocal"),
            "hard-scope `let` loop global was not localized: {dump}"
        );
        // The outer global assignment (`total = 0`) keeps its name.
        let Stmt::Assign { var, .. } = &main.stmts[0] else {
            panic!("expected outer assign");
        };
        assert_eq!(var, "total");
    }

    /// The bound `let x = 10` variant is localized the same way.
    #[test]
    fn hard_let_bound_binding_localizes_loop_global() {
        let mut main = parse_main(
            "total = 0\nlet x = 10\n    for i in 1:3\n        total += 1\n    end\nend\n",
        );
        apply_hard_scope_let_localization(&mut main);
        assert!(
            format!("{main:#?}").contains("total##letlocal"),
            "bound `let` loop global was not localized"
        );
    }

    /// A `let` binding of the SAME name shadows the global, so the loop mutates
    /// the let-local — nothing is localized.
    #[test]
    fn hard_let_same_name_binding_is_not_localized() {
        let mut main = parse_main(
            "total = 0\nlet total = 100\n    for i in 1:3\n        total += 1\n    end\n    total\nend\n",
        );
        apply_hard_scope_let_localization(&mut main);
        assert!(
            !format!("{main:#?}").contains("letlocal"),
            "a `let`-bound same-name variable must not be localized"
        );
    }

    /// A let-body-level assignment of the name makes it a let-local, so the loop
    /// refers to that local — not localized.
    #[test]
    fn hard_let_body_level_assign_is_not_localized() {
        let mut main = parse_main(
            "total = 0\nlet\n    total = 100\n    for i in 1:3\n        total += 1\n    end\nend\n",
        );
        apply_hard_scope_let_localization(&mut main);
        assert!(
            !format!("{main:#?}").contains("letlocal"),
            "a let-body-level assignment target must not be localized"
        );
    }

    /// An explicit `global total` inside the loop keeps the module binding —
    /// not localized.
    #[test]
    fn hard_let_explicit_global_in_loop_is_not_localized() {
        let mut main = parse_main(
            "total = 0\nlet\n    for i in 1:3\n        global total\n        total += 1\n    end\nend\n",
        );
        apply_hard_scope_let_localization(&mut main);
        assert!(
            !format!("{main:#?}").contains("letlocal"),
            "an explicit `global` in the loop must not be localized"
        );
    }

    /// A let-body-level `global total` (outside the loop) also keeps the module
    /// binding — not localized.
    #[test]
    fn hard_let_body_level_global_decl_is_not_localized() {
        let mut main = parse_main(
            "total = 0\nlet\n    global total\n    for i in 1:3\n        total += 1\n    end\nend\n",
        );
        apply_hard_scope_let_localization(&mut main);
        assert!(
            !format!("{main:#?}").contains("letlocal"),
            "a let-body-level `global` decl must not be localized"
        );
    }

    /// A loop-body name that is NOT a pre-existing global is left unchanged
    /// (sjulia already binds it as a fresh local through the existing path).
    #[test]
    fn hard_let_non_preexisting_name_is_untouched() {
        let mut main = parse_main("let\n    for i in 1:3\n        acc += 1\n    end\nend\n");
        apply_hard_scope_let_localization(&mut main);
        assert!(
            !format!("{main:#?}").contains("letlocal"),
            "a non-global name is already a fresh loop-local; it must not be renamed"
        );
    }

    /// A nested `let` inside a `let`: the inner loop over a global is localized.
    #[test]
    fn hard_let_nested_localizes_inner_loop_global() {
        let mut main = parse_main(
            "total = 0\nlet x = 1\n    let y = 2\n        for i in 1:3\n            total += 1\n        end\n    end\nend\n",
        );
        apply_hard_scope_let_localization(&mut main);
        assert!(
            format!("{main:#?}").contains("total##letlocal"),
            "a nested-`let` inner loop global was not localized"
        );
    }

    /// The hard-scope pass must NOT touch a top-level `for` loop (that is the
    /// soft-scope pass's job, and it is REPL-lenient).
    #[test]
    fn hard_let_pass_leaves_toplevel_loop_untouched() {
        let mut main = parse_main("total = 0\nfor i in 1:3\n    total += 1\nend\nprintln(total)\n");
        apply_hard_scope_let_localization(&mut main);
        assert!(
            !format!("{main:#?}").contains("letlocal"),
            "a top-level loop must not be touched by the hard-scope `let` pass"
        );
    }

    /// A loop-body name that is NOT a pre-existing global is left unchanged
    /// (sjulia already binds it as a fresh local through the existing path).
    #[test]
    fn non_global_name_is_untouched() {
        let body = block(vec![Stmt::Assign {
            var: "acc".to_string(),
            value: int_lit(1),
            span: dummy_span(),
        }]);
        let mut main = block(vec![Stmt::While {
            condition: Expr::Literal(Literal::Bool(false), dummy_span()),
            body,
            span: dummy_span(),
        }]);

        apply_file_mode_soft_scope(&mut main, None);

        let Stmt::While { body, .. } = &main.stmts[0] else {
            panic!("expected while loop");
        };
        let Stmt::Assign { var: inner_var, .. } = &body.stmts[0] else {
            panic!("expected inner assign");
        };
        assert_eq!(inner_var, "acc");
    }

    /// A fresh assignment owned by a top-level try clause is not a module
    /// binding. A later loop still needs a distinct internal slot, but that
    /// rename is silent rather than a global-ambiguity warning (#11322).
    #[test]
    fn try_clause_fresh_binding_does_not_seed_later_soft_scope_11322() {
        let mut main = parse_main(
            "try\n    ghost11322 = 1\ncatch\nend\nfor i in 1:1\n    ghost11322 = 2\nend\n",
        );

        apply_file_mode_soft_scope(&mut main, None);

        let dump = format!("{main:#?}");
        assert!(
            dump.contains("ghost11322##softlocal"),
            "a later scope must not reuse the retired clause-local slot: {dump}"
        );
    }

    /// A real top-level assignment after a retired clause local becomes the
    /// authoritative mutable-global fact. The retired marker must not suppress
    /// localization (and its warning) in a later loop.
    #[test]
    fn later_global_supersedes_retired_clause_local() {
        let mut main = parse_main(
            "try\n    mixedprov = 1\ncatch\nend\nmixedprov = 0\nfor i in 1:1\n    mixedprov = 2\nend\n",
        );

        apply_file_mode_soft_scope(&mut main, None);

        let dump = format!("{main:#?}");
        assert!(
            dump.contains("mixedprov##softlocal"),
            "the later mutable global must supersede retired-local provenance: {dump}"
        );
    }

    /// An existing mutable global is different: strict file mode localizes the
    /// try-clause assignment itself, preserving the outer value (Issue #11335).
    #[test]
    fn try_clause_existing_global_is_soft_localized_11335() {
        let mut main = parse_main("existing11335 = 1\ntry\n    existing11335 = 2\ncatch\nend\n");

        apply_file_mode_soft_scope(&mut main, None);

        let dump = format!("{main:#?}");
        assert!(
            dump.contains("existing11335##softlocal"),
            "an existing mutable global must be localized at the clause boundary: {dump}"
        );
    }

    /// Once an outer clause localizes an existing global, an ordinary
    /// assignment in a nested try reuses that enclosing local. It must not
    /// allocate a second soft-local slot (#11159).
    #[test]
    fn nested_try_assignment_reuses_enclosing_clause_local_11159() {
        let mut main = parse_main(
            "nestedreuse11159 = 0\ntry\n    nestedreuse11159 = 1\n    try\n        nestedreuse11159 = 2\n    catch\n    end\ncatch\nend\n",
        );

        apply_file_mode_soft_scope(&mut main, None);

        let dump = format!("{main:#?}");
        assert!(
            dump.contains("nestedreuse11159##softlocal0"),
            "the outer clause assignment must be localized: {dump}"
        );
        assert!(
            !dump.contains("nestedreuse11159##softlocal1"),
            "the nested clause must reuse the enclosing localized slot: {dump}"
        );
    }

    /// A hygienic macro expansion is represented as a scope-transparent block
    /// whose explicit locals enclose every nested soft scope.  In particular,
    /// the `try` used by `Test.@test` must reuse the generated result slot
    /// instead of localizing it as an ambiguous module global (#11415).
    #[test]
    fn transparent_macro_local_encloses_try_clause_11415() {
        use crate::ir::core::LocalDeclKind;

        let span = dummy_span();
        let mut main = block(vec![
            Stmt::Expr {
                expr: Expr::LetBlock {
                    bindings: Vec::new(),
                    body: block(vec![
                        Stmt::LocalDecl {
                            var: "result11415".into(),
                            kind: LocalDeclKind::Explicit,
                            span,
                        },
                        Stmt::Assign {
                            var: "result11415".into(),
                            value: Expr::Literal(Literal::Bool(false), span),
                            span,
                        },
                        Stmt::Try {
                            try_block: block(vec![Stmt::Assign {
                                var: "result11415".into(),
                                value: Expr::Literal(Literal::Bool(true), span),
                                span,
                            }]),
                            catch_var: None,
                            catch_block: None,
                            else_block: None,
                            finally_block: None,
                            span,
                        },
                    ]),
                    span,
                },
                span,
            },
            // Once the transparent expression ends, the same spelling is a
            // fresh loop-local. The completed macro scope must not leak either
            // global or retired-clause provenance into this later expression.
            Stmt::For {
                var: "i11415".into(),
                start: int_lit(1),
                end: int_lit(1),
                step: None,
                body: block(vec![Stmt::Assign {
                    var: "result11415".into(),
                    value: int_lit(2),
                    span,
                }]),
                span,
            },
        ]);

        apply_file_mode_soft_scope(&mut main, None);

        let dump = format!("{main:#?}");
        assert!(
            !dump.contains("result11415##softlocal"),
            "the try clause must reuse its enclosing hygienic local: {dump}"
        );
    }

    /// `global` inside a clause still establishes a module binding. A later
    /// loop must see it and follow the normal strict soft-scope path.
    #[test]
    fn try_clause_explicit_global_seeds_later_soft_scope_11322() {
        let mut main = parse_main(
            "try\n    global explicit11322 = 1\ncatch\nend\nfor i in 1:1\n    explicit11322 = 2\nend\n",
        );

        apply_file_mode_soft_scope(&mut main, None);

        let dump = format!("{main:#?}");
        assert!(
            dump.contains("explicit11322##softlocal"),
            "an explicit clause global must remain visible to the later loop: {dump}"
        );
    }

    /// A nested try clause has the same ownership boundary as its parent. Its
    /// fresh binding retires silently instead of becoming a module global.
    #[test]
    fn nested_try_clause_fresh_binding_does_not_seed_global_11322() {
        let mut main = parse_main(
            "try\n    try\n        nested11322 = 1\n    catch\n    end\ncatch\nend\nfor i in 1:1\n    nested11322 = 2\nend\n",
        );

        apply_file_mode_soft_scope(&mut main, None);

        let dump = format!("{main:#?}");
        assert!(
            dump.contains("nested11322##softlocal"),
            "later scope must allocate separately from a nested retired clause local: {dump}"
        );
    }

    /// Current main already preserves a same-named const through a try nested
    /// in a loop. Pin that #11305 behavior while fixing the shared walker.
    #[test]
    fn loop_nested_try_const_shadow_stays_clause_local_11305() {
        let mut main = parse_main(
            "const const11305 = 1\nfor i in 1:1\n    try\n        const11305 = 2\n    catch\n    end\nend\n",
        );

        apply_file_mode_soft_scope(&mut main, None);

        assert!(
            format!("{main:#?}").contains("const11305##softlocal"),
            "the nested clause must use a fresh internal slot without warning"
        );
    }

    /// The same const provenance applies to a direct top-level loop: the loop
    /// receives a fresh internal slot without a mutable-global warning.
    #[test]
    fn top_level_loop_const_shadow_is_silently_localized_11305() {
        let mut main = parse_main(
            "const direct_const11305 = 1\nfor i in 1:1\n    direct_const11305 = 2\nend\n",
        );

        apply_file_mode_soft_scope(&mut main, None);

        assert!(
            format!("{main:#?}").contains("direct_const11305##softlocal"),
            "const shadow must use a fresh internal slot without warning"
        );
    }

    // === Diagnostic-text parity (Issue #9283) ===============================

    /// With a script path the warning location is the absolute path + line,
    /// matching `julia file.jl`.
    #[test]
    fn warning_uses_script_path_location() {
        let span = Span::new(0, 0, 4, 4, 0, 0);
        let msg = format_soft_scope_warning("total", Some(span), Some("/abs/path/to/script.jl"));
        assert!(
            msg.ends_with("└ @ /abs/path/to/script.jl:4"),
            "warning must locate at the script path + line: {msg}"
        );
        assert!(msg.contains("Assignment to `total` in soft scope is ambiguous"));
    }

    /// Without a script path (`-e` / stdin / host buffer) the location is
    /// `none:<line>`, matching upstream `julia -e`.
    #[test]
    fn warning_uses_none_location_without_path() {
        let span = Span::new(0, 0, 4, 4, 0, 0);
        let msg = format_soft_scope_warning("total", Some(span), None);
        assert!(
            msg.ends_with("└ @ none:4"),
            "no script path must render `none:<line>`: {msg}"
        );
    }

    /// A `global` declaration inside a NESTED loop exempts the name from
    /// localization (Issue #9493): `global val += 1` in an inner loop runs under
    /// upstream `julia file.jl`, so the pass must see the declaration even
    /// though it sits below another loop level.
    #[test]
    fn global_decl_in_nested_loop_is_not_localized() {
        let mut main = parse_main(
            "val = 1\nfor i in 1:2\n    for j in 1:3\n        global val += 1\n    end\nend\nprintln(val)\n",
        );
        apply_file_mode_soft_scope(&mut main, None);
        let dump = format!("{main:#?}");
        assert!(
            !dump.contains("softlocal"),
            "a global decl in a nested loop must exempt the name: {dump}"
        );
    }

    /// Same for a bare `global val` declaration line in the inner loop.
    #[test]
    fn bare_global_decl_in_nested_loop_is_not_localized() {
        let mut main = parse_main(
            "val = 1\nfor i in 1:2\n    for j in 1:3\n        global val\n        val += 1\n    end\nend\n",
        );
        apply_file_mode_soft_scope(&mut main, None);
        assert!(
            !format!("{main:#?}").contains("softlocal"),
            "a bare global decl in a nested loop must exempt the name"
        );
    }

    /// Multiple captured names are localized (and thus warned + gensym-numbered)
    /// in SOURCE order, not alphabetical order (`zebra` before `apple`). The
    /// first source-order name gets `##softlocal0`.
    #[test]
    fn multiple_captured_names_localized_in_source_order() {
        // Parse real source so the assignment spans carry byte offsets.
        let mut main =
            parse_main("zebra = 0\napple = 0\nfor i in 1:3\n    zebra += 1\n    apple += 1\nend\n");
        apply_file_mode_soft_scope(&mut main, None);
        let dump = format!("{main:#?}");
        // `zebra` appears first in source, so it is localized first → softlocal0;
        // `apple` second → softlocal1. Alphabetical order would invert these.
        assert!(
            dump.contains("zebra##softlocal0"),
            "first source-order name must be softlocal0: {dump}"
        );
        assert!(
            dump.contains("apple##softlocal1"),
            "second source-order name must be softlocal1: {dump}"
        );
    }
}
