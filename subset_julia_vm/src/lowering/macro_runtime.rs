//! Expansion-time execution for user-defined macros.
//!
//! Upstream Julia lowers `macro m(args...) ... end` to a callable method that
//! receives `__source__`, `__module__`, and the unevaluated AST arguments. Macro
//! expansion invokes that method, then lowers the returned AST. This module
//! mirrors that model for sjulia instead of statically substituting macro
//! parameters in the lowering pass.

use std::collections::{HashMap, HashSet};

use crate::compile::{compile_with_cache, CompileError};
use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::expr_heads::ExprHead;
use crate::ir::core::{
    BinaryOp, Block, BuiltinOp, Expr, Function, InnerConstructor, Literal, MetaAnnotation, Module,
    Program, Stmt, StructDef, StructField, TypedParam, UnaryOp, UsingImport,
};
use crate::lowering::expr::quote::cst_to_macro_arg_constructor;
use crate::lowering::stmt::{lower_destructuring_from_targets, DestructureTarget};
use crate::lowering::{
    LambdaContext, LowerResult, MacroHygieneInfo, MacroParamType, StoredMacroDef,
};
use crate::parser::cst::{CstWalker, Node};
use crate::rng::StableRng;
use crate::span::Span;
use crate::types::{JuliaType, TypeExpr, TypeParam};
use crate::vm::error::VmError;
use crate::vm::value::{ExprValue, GlobalRefValue, StructInstance, SymbolValue, TupleValue, Value};
use crate::vm::Vm;

/// A module-defined macro (bundled package or user `module`) carries its own
/// hygiene frame that qualifies non-`esc` member calls; quote-local function
/// gensym is applied only to plain top-level user macros, whose bare names are
/// always freshly introduced (Issue #8064).
fn macro_has_module_hygiene(
    lambda_ctx: &LambdaContext,
    macro_name: &str,
    macro_def: &StoredMacroDef,
) -> bool {
    macro_def.hygiene.is_some() || lambda_ctx.macro_hygiene_entry(macro_name).is_some()
}

/// Whether the macro's result is produced by a *direct* `quote ... end`
/// construction (its tail expression is a top-level `Expr(...)` builder), as
/// opposed to an `esc(...)` wrapper or a helper-function call that returns an
/// expression — e.g. MacroTools' `@qq`, which escapes the names it introduces.
///
/// Quote-local function gensym (Issue #8064) is applied only to such
/// plain-quote macros. When the result instead comes from `esc`/`@qq`, the
/// introduced names are meant to stay visible in the caller, and the escape
/// marker is already resolved by the time the expansion value reaches lowering,
/// so it can no longer be detected syntactically (the `esc` markers that DO
/// survive — `$(esc(x))` *inside* a plain quote — are still honored by the
/// per-subtree `esc_depth` tracking in [`collect_quote_local_function_names`]).
fn macro_body_returns_plain_quote(body: &Block) -> bool {
    let tail_expr = match body.stmts.last() {
        Some(Stmt::Expr { expr, .. }) => Some(expr),
        Some(Stmt::Return {
            value: Some(expr), ..
        }) => Some(expr),
        _ => None,
    };
    // A `quote ... end` macro body lowers to an `Expr::QuoteLiteral` whose
    // `constructor` builds the AST via `Expr(...)` (`BuiltinOp::ExprNew`). An
    // `esc(...)`/helper-call body (e.g. `@qq`) lowers to a `Builtin`/`Call`/
    // `ModuleCall` instead, so it is excluded.
    matches!(
        tail_expr,
        Some(Expr::QuoteLiteral { constructor, .. })
            if matches!(
                constructor.as_ref(),
                Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    ..
                }
            )
    )
}

/// Apply quote-local function-name hygiene (Issue #8064) to a freshly expanded
/// macro value when the macro is a plain top-level user macro returning a direct
/// `quote`. Module-defined macros (which carry their own member-qualification
/// hygiene) and `esc`/`@qq`-style macros are left untouched.
fn maybe_apply_quote_hygiene(
    value: Value,
    macro_name: &str,
    macro_def: &StoredMacroDef,
    span: Span,
    lambda_ctx: &LambdaContext,
) -> Value {
    if macro_has_module_hygiene(lambda_ctx, macro_name, macro_def)
        || !macro_body_returns_plain_quote(&macro_def.body)
    {
        return value;
    }
    apply_quote_function_hygiene(value, macro_name, span)
}

/// Make non-`esc` function names defined inside a (non-module) macro's `quote`
/// hygienic: each such name is replaced by a fresh, module-private gensym, so it
/// is NOT visible after the macrocall — matching upstream Julia, where a
/// non-`esc` quote-local `f(x) = ...` yields `UndefVarError: f` at the call site
/// (Issue #8064). `esc`'d names (whose definition callee is `Expr(:escape, ...)`,
/// not a bare `Symbol`) are left untouched, so they stay visible (Issue #8066).
/// Renaming both the definition and every non-`esc` reference within the same
/// expansion keeps internal calls/recursion working while stopping two macros
/// that share an internal helper name from merging into one global method table.
fn apply_quote_function_hygiene(value: Value, macro_name: &str, span: Span) -> Value {
    let mut names = HashSet::new();
    collect_quote_local_function_names(&value, 0, &mut names);
    if names.is_empty() {
        return value;
    }
    let rename: HashMap<String, String> = names
        .into_iter()
        .map(|name| {
            // `#` makes the gensym non-typable in source; macro name + call-site
            // offset make it unique per macro and per macrocall.
            let gensym = format!("{name}##{macro_name}#{}", span.start);
            (name, gensym)
        })
        .collect();
    rename_quote_local_symbols(value, &rename, 0)
}

/// Collect the bare-`Symbol` names of function definitions that appear outside
/// any `esc(...)` / quote subtree in a macro expansion (Issue #8064). A callee
/// that is escaped, qualified (`Mod.f`), parametric (`f{T}`), or interpolated is
/// not a freshly introduced local name, so it is skipped.
fn collect_quote_local_function_names(value: &Value, esc_depth: usize, out: &mut HashSet<String>) {
    let Value::Expr(expr) = value else {
        return;
    };
    let args = expr.args_snapshot();
    let head = ExprHead::from_expr(expr);
    // Quoted data is opaque to hygiene; do not collect names from it.
    if matches!(head, Some(ExprHead::Quote)) {
        return;
    }
    let inner_depth = match head {
        Some(ExprHead::Escape | ExprHead::HygienicScope) => esc_depth + 1,
        _ => esc_depth,
    };
    if esc_depth == 0 {
        match head {
            Some(ExprHead::Function) => {
                if let Some(name) = args.first().and_then(function_def_local_name) {
                    out.insert(name);
                }
            }
            Some(ExprHead::Assign) if args.len() == 2 => {
                if let Some(name) = function_def_local_name(&args[0]) {
                    out.insert(name);
                }
            }
            _ => {}
        }
    }
    for arg in &args {
        collect_quote_local_function_names(arg, inner_depth, out);
    }
}

/// Extract the bare-`Symbol` name of a function-definition signature, unwrapping
/// a `where` wrapper. Returns `None` when the callee is not a plain symbol
/// (escaped / qualified / parametric), so such names are left visible.
fn function_def_local_name(sig: &Value) -> Option<String> {
    let Value::Expr(expr) = sig else {
        return None;
    };
    match ExprHead::from_expr(expr)? {
        ExprHead::Where => expr
            .args_snapshot()
            .first()
            .and_then(function_def_local_name),
        ExprHead::Call => match expr.args_snapshot().first() {
            Some(Value::Symbol(name)) => Some(name.as_str().to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Rewrite every non-`esc` `Symbol` reference named in `rename` to its gensym,
/// leaving `esc(...)` subtrees and quoted data untouched (Issue #8064).
fn rename_quote_local_symbols(
    value: Value,
    rename: &HashMap<String, String>,
    esc_depth: usize,
) -> Value {
    match value {
        Value::Symbol(ref name) if esc_depth == 0 => match rename.get(name.as_str()) {
            Some(gensym) => Value::Symbol(SymbolValue::new(gensym)),
            None => value,
        },
        Value::Expr(expr) => {
            let head_str = expr.head.as_str().to_string();
            let head = ExprHead::from_name(&head_str);
            // Quoted data is opaque to hygiene; leave it verbatim.
            if matches!(head, Some(ExprHead::Quote)) {
                return Value::Expr(expr);
            }
            let inner_depth = match head {
                Some(ExprHead::Escape | ExprHead::HygienicScope) => esc_depth + 1,
                _ => esc_depth,
            };
            let new_args = expr
                .args_snapshot()
                .into_iter()
                .map(|arg| rename_quote_local_symbols(arg, rename, inner_depth))
                .collect::<Vec<_>>();
            Value::Expr(ExprValue::from_head(head_str, new_args))
        }
        other => other,
    }
}

pub(crate) fn expand_macro_to_stmt<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    macro_def: &StoredMacroDef,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let value = evaluate_macro(walker, macro_name, macro_def, args, span, lambda_ctx)?;
    let value = maybe_apply_quote_hygiene(value, macro_name, macro_def, span, lambda_ctx);
    // Qualify the macro's non-`esc` module-member call targets while converting
    // the returned AST (Issue #7355 / #7350 A4).
    let pushed = begin_macro_hygiene(lambda_ctx, macro_name, macro_def);
    // A value-producing macro expanded in statement position (e.g. a top-level
    // `@show f(3)` / `@time result = f(10)`) returns its result as the final
    // expression of an expansion block. Lowering that block to a bare `Stmt::Block`
    // discards the value at top-level program position (the statement result became
    // `nothing` / a wrong fallback) — Issue #7764. Convert a block value through the
    // value-preserving expression path and carry it in a `Stmt::Expr`, mirroring the
    // bundled-package path (`expand_bundled_package_macro`) — its trailing
    // non-LineNumberNode element becomes the statement value. We only do this for the
    // outermost block returned by a macro; the recursive `value_to_stmt` used for
    // nested blocks is left untouched so statement-only constructs keep working.
    let result = match &value {
        Value::Expr(expr)
            if ExprHead::is_expr(expr, ExprHead::Block)
                && !block_tail_requires_stmt_path(expr)
                && !block_contains_module_export_decl(expr) =>
        {
            value_to_branch_expr(value, span, walker, lambda_ctx)
                .map(|expr| Stmt::Expr { expr, span })
        }
        _ => value_to_stmt(value, span, walker, lambda_ctx),
    };
    if pushed {
        lambda_ctx.end_macro_hygiene();
    }
    result
}

fn block_tail_requires_stmt_path(expr: &ExprValue) -> bool {
    let args = expr.args_snapshot();
    let Some(tail) = args
        .iter()
        .rev()
        .find(|arg| !matches!(arg, Value::LineNumberNode(_)))
    else {
        return false;
    };
    value_requires_stmt_path_in_tail(tail)
}

fn value_requires_stmt_path_in_tail(value: &Value) -> bool {
    let Value::Expr(expr) = value else {
        return false;
    };
    let Some(head) = ExprHead::from_expr(expr) else {
        return false;
    };
    match head {
        ExprHead::Escape | ExprHead::HygienicScope => {
            let args = expr.args_snapshot();
            args.first()
                .map(value_requires_stmt_path_in_tail)
                .unwrap_or(false)
        }
        ExprHead::Block => block_tail_requires_stmt_path(expr),
        ExprHead::MacroCall => true,
        // A short-form function definition `f(args...) = body` round-trips as
        // `Expr(:(=), Expr(:call, ...), body)` (optionally `where`-wrapped). Its
        // head is `=`, so the generic `macro_return_to_expr` check below would
        // route it through the value-producing expression path and try to lower
        // it as an assignment expression — which has no notion of defining a
        // method. Force it through the statement path so it becomes a real
        // `Stmt::FunctionDef` (Issue #7933).
        ExprHead::Assign if assign_value_is_function_def(expr) => true,
        _ => head.spec().macro_return_to_stmt && !head.spec().macro_return_to_expr,
    }
}

/// A short-form function definition `f(args...) = body` round-trips through a
/// macro expansion as `Expr(:(=), Expr(:call, ...), body)`, optionally wrapped
/// in `Expr(:where, ...)` for `f(x::T) where {T} = body`. Although its head is
/// `=`, it is a method definition, not an assignment, and must flow through the
/// statement path so it registers as a `Stmt::FunctionDef` (Issue #7933).
fn assign_value_is_function_def(expr: &ExprValue) -> bool {
    if !ExprHead::is_expr(expr, ExprHead::Assign) {
        return false;
    }
    let args = expr.args_snapshot();
    if args.len() != 2 {
        return false;
    }
    matches!(
        macro_assignment_target(&args[0]),
        Value::Expr(target)
            if matches!(
                ExprHead::from_expr(&target),
                Some(ExprHead::Call | ExprHead::Where)
            )
    )
}

/// Returns `true` when a macro-returned block contains a module-level `export`
/// (or `public`) declaration anywhere — directly, inside a nested block, or
/// inside the branches of a top-level `if`/`elseif`.
///
/// Such declarations have a module-level side effect (recording into
/// `module.exports`) that is silently lost when the surrounding block is routed
/// through the value-producing expression path of `expand_macro_to_stmt`: there,
/// `Expr(:export, ...)` lowers to a bare `nothing` literal (`value_to_expr`),
/// so `collect_module_body_exports` never sees a `Stmt::Export`. Routing the
/// block through the statement path instead preserves the declaration as a real
/// `Stmt::Export`, which restores parity with upstream Julia for
/// macro-expanded/conditional exports such as AbstractAlgebra's `@alias`
/// (Issue #7959).
fn block_contains_module_export_decl(expr: &ExprValue) -> bool {
    expr.args_snapshot()
        .iter()
        .any(value_contains_module_export_decl)
}

fn value_contains_module_export_decl(value: &Value) -> bool {
    let Value::Expr(expr) = value else {
        return false;
    };
    let Some(head) = ExprHead::from_expr(expr) else {
        return false;
    };
    match head {
        ExprHead::Export | ExprHead::Public => true,
        ExprHead::Block => block_contains_module_export_decl(expr),
        ExprHead::Escape | ExprHead::HygienicScope => expr
            .args_snapshot()
            .first()
            .map(value_contains_module_export_decl)
            .unwrap_or(false),
        // `if`/`elseif` arguments are `[condition, then_branch, else_branch?]`;
        // a module-level export can live in either branch (e.g. the
        // `if ... export ... end` shape returned by `@alias`). The condition
        // itself can never be an export, so scanning all arguments is safe.
        ExprHead::If | ExprHead::ElseIf => expr
            .args_snapshot()
            .iter()
            .any(value_contains_module_export_decl),
        _ => false,
    }
}

pub(crate) fn expand_macro_to_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    macro_def: &StoredMacroDef,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let value = evaluate_macro(walker, macro_name, macro_def, args, span, lambda_ctx)?;
    let value = maybe_apply_quote_hygiene(value, macro_name, macro_def, span, lambda_ctx);
    // Qualify the macro's non-`esc` module-member call targets while converting
    // the returned AST (Issue #7355 / #7350 A4).
    let pushed = begin_macro_hygiene(lambda_ctx, macro_name, macro_def);
    let result = value_to_expr(value, span, walker, lambda_ctx);
    if pushed {
        lambda_ctx.end_macro_hygiene();
    }
    result
}

fn begin_macro_hygiene(
    lambda_ctx: &LambdaContext,
    macro_name: &str,
    macro_def: &StoredMacroDef,
) -> bool {
    if lambda_ctx.begin_macro_hygiene(macro_name) {
        return true;
    }
    if let Some(info) = &macro_def.hygiene {
        lambda_ctx.begin_macro_hygiene_frame(&info.module, info.members.clone());
        return true;
    }
    false
}

fn evaluate_macro<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    macro_def: &StoredMacroDef,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Value> {
    let macro_func_name = format!("__sjulia_macro_{}_{}", macro_name, span.start);
    let mut visible_functions = lambda_ctx.compile_time_functions();
    visible_functions.extend(macro_def.expansion_functions.iter().cloned());
    let visible_function_count = visible_functions.len();
    let mut visible_structs = lambda_ctx.compile_time_structs();
    visible_structs.extend(macro_def.expansion_structs.iter().cloned());
    let dependency_functions = macro_dependency_functions(&visible_functions, &macro_def.body);
    let referenced_modules = collect_referenced_modules_block(&macro_def.body);
    let selected_dependency_count = dependency_functions.len();
    let (mut functions, module_functions) =
        split_macro_dependency_functions(macro_name, macro_def, lambda_ctx, dependency_functions);
    functions.push(synthetic_macro_function(&macro_func_name, macro_def, span));
    let mut module_candidates = module_functions;
    module_candidates.extend(functions.iter().cloned());
    let compile_time_modules = compile_time_modules_for_macro(
        macro_name,
        macro_def,
        lambda_ctx,
        &module_candidates,
        &referenced_modules,
        span,
    );
    let compile_time_usings =
        compile_time_usings_for_macro(macro_name, macro_def, lambda_ctx, span);

    let mut call_args = vec![
        Expr::Literal(
            Literal::LineNumberNode {
                line: span_start_line_i64(span)?,
                file: source_file_literal(lambda_ctx),
            },
            span,
        ),
        Expr::Literal(Literal::Module(call_site_module_name(lambda_ctx)), span),
    ];
    for arg in args {
        call_args.push(cst_to_macro_arg_constructor(walker, *arg)?);
    }

    let splat_mask = vec![false; call_args.len()];
    let main = Block {
        stmts: vec![Stmt::Expr {
            expr: Expr::Call {
                function: macro_func_name.clone(),
                args: call_args,
                kwargs: vec![],
                splat_mask,
                kwargs_splat_mask: vec![],
                span,
            },
            span,
        }],
        span,
    };
    let program = Program {
        // User type definitions are included so a `compile_time_functions` member
        // that touches a user struct (e.g. `step!(l::Lorenz)` reading `l.x`) still
        // compiles during expansion — otherwise "Unknown field: …" (Issue #7272).
        abstract_types: lambda_ctx.compile_time_abstract_types(),
        primitive_types: lambda_ctx.compile_time_primitive_types(),
        type_aliases: vec![],
        structs: visible_structs.clone(),
        functions,
        base_function_count: 0,
        modules: compile_time_modules,
        usings: compile_time_usings,
        macros: vec![],
        enums: vec![],
        main,
    };

    let compiled = match compile_with_cache(&program) {
        Ok(compiled) => compiled,
        Err(err)
            if missing_splat_dependency_error(&err)
                && selected_dependency_count < visible_function_count =>
        {
            let (mut fallback_functions, fallback_module_functions) =
                split_macro_dependency_functions(
                    macro_name,
                    macro_def,
                    lambda_ctx,
                    visible_functions.clone(),
                );
            fallback_functions.push(synthetic_macro_function(&macro_func_name, macro_def, span));
            let mut fallback_module_candidates = fallback_module_functions;
            fallback_module_candidates.extend(fallback_functions.iter().cloned());
            let fallback_modules = compile_time_modules_for_macro(
                macro_name,
                macro_def,
                lambda_ctx,
                &fallback_module_candidates,
                &referenced_modules,
                span,
            );
            let fallback_usings =
                compile_time_usings_for_macro(macro_name, macro_def, lambda_ctx, span);
            let fallback_program = Program {
                abstract_types: lambda_ctx.compile_time_abstract_types(),
                primitive_types: lambda_ctx.compile_time_primitive_types(),
                type_aliases: vec![],
                structs: visible_structs.clone(),
                functions: fallback_functions,
                base_function_count: 0,
                modules: fallback_modules,
                usings: fallback_usings,
                macros: vec![],
                enums: vec![],
                main: program.main.clone(),
            };
            compile_with_cache(&fallback_program).map_err(|fallback_err| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "macro @{} expansion compile error after full dependency retry (Issue #7548): {:?}",
                        macro_name, fallback_err
                    ),
                )
            })?
        }
        Err(err) => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!("macro @{} expansion compile error: {:?}", macro_name, err),
                ),
            );
        }
    };
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let (value, struct_heap) = match vm.run() {
        Ok(value) => (value, vm.get_struct_heap().to_vec()),
        Err(err)
            if missing_runtime_dependency_error(&err)
                && program.functions.len() < visible_functions.len() + 1 =>
        {
            let mut fallback_functions = visible_functions;
            fallback_functions.push(synthetic_macro_function(&macro_func_name, macro_def, span));
            let fallback_modules = compile_time_modules_for_macro(
                macro_name,
                macro_def,
                lambda_ctx,
                &fallback_functions,
                &referenced_modules,
                span,
            );
            let fallback_usings =
                compile_time_usings_for_macro(macro_name, macro_def, lambda_ctx, span);
            let fallback_program = Program {
                abstract_types: lambda_ctx.compile_time_abstract_types(),
                primitive_types: lambda_ctx.compile_time_primitive_types(),
                type_aliases: vec![],
                structs: visible_structs.clone(),
                functions: fallback_functions,
                base_function_count: 0,
                modules: fallback_modules,
                usings: fallback_usings,
                macros: vec![],
                enums: vec![],
                main: program.main.clone(),
            };
            let fallback_compiled = compile_with_cache(&fallback_program).map_err(|fallback_err| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "macro @{} expansion compile error after full runtime-dependency retry (Issue #7569): {:?}",
                        macro_name, fallback_err
                    ),
                )
            })?;
            let mut fallback_vm = Vm::new_program(fallback_compiled, StableRng::new(0));
            let value = fallback_vm.run().map_err(|fallback_err| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "macro @{} expansion runtime error after full dependency retry (Issue #7569): {}",
                        macro_name, fallback_err
                    ),
                )
            })?;
            (value, fallback_vm.get_struct_heap().to_vec())
        }
        Err(err) => {
            let instr = vm
                .debug_current_instruction()
                .map(|(ip, instr)| {
                    let prev = ip
                        .checked_sub(1)
                        .and_then(|prev_ip| {
                            vm.debug_instruction_at(prev_ip)
                                .map(|prev_instr| format!(", prev {} ({:?})", prev_ip, prev_instr))
                        })
                        .unwrap_or_default();
                    format!(" at ip {} ({:?}{})", ip, instr, prev)
                })
                .unwrap_or_default();
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "macro @{} expansion runtime error{}: {}",
                        macro_name, instr, err
                    ),
                ),
            );
        }
    };
    Ok(resolve_macro_result_struct_refs(value, &struct_heap))
}

fn missing_splat_dependency_error(err: &CompileError) -> bool {
    match err {
        CompileError::Msg(msg) => {
            msg.contains("Cannot find function ") && msg.contains(" for splat call")
        }
        CompileError::Dispatch(_) => false,
    }
}

fn missing_runtime_dependency_error(err: &VmError) -> bool {
    err.to_string().contains("Unknown function:")
}

fn resolve_macro_result_struct_refs(value: Value, struct_heap: &[StructInstance]) -> Value {
    match value {
        Value::StructRef(idx) => struct_heap
            .get(idx)
            .cloned()
            .map(|instance| resolve_macro_result_struct_refs(Value::Struct(instance), struct_heap))
            .unwrap_or(Value::StructRef(idx)),
        Value::Struct(mut instance) => {
            instance.values = instance
                .values
                .into_iter()
                .map(|inner| resolve_macro_result_struct_refs(inner, struct_heap))
                .collect();
            Value::Struct(instance)
        }
        Value::Expr(expr) => {
            let args = expr
                .args_snapshot()
                .into_iter()
                .map(|arg| resolve_macro_result_struct_refs(arg, struct_heap))
                .collect();
            Value::Expr(ExprValue::new(expr.head.clone(), args))
        }
        Value::QuoteNode(inner) => Value::QuoteNode(Box::new(resolve_macro_result_struct_refs(
            *inner,
            struct_heap,
        ))),
        Value::Tuple(tuple) => Value::Tuple(TupleValue {
            elements: tuple
                .elements
                .into_iter()
                .map(|inner| resolve_macro_result_struct_refs(inner, struct_heap))
                .collect(),
        }),
        Value::SimpleVector(tuple) => Value::SimpleVector(TupleValue {
            elements: tuple
                .elements
                .into_iter()
                .map(|inner| resolve_macro_result_struct_refs(inner, struct_heap))
                .collect(),
        }),
        other => other,
    }
}

fn macro_dependency_functions(visible_functions: &[Function], body: &Block) -> Vec<Function> {
    let mut by_name: HashMap<&str, Vec<&Function>> = HashMap::new();
    for func in visible_functions {
        by_name.entry(&func.name).or_default().push(func);
    }

    let mut pending = HashSet::new();
    collect_called_functions_block(body, &mut pending);

    let mut selected = HashSet::new();
    let mut out = Vec::new();
    while let Some(name) = pending.iter().next().cloned() {
        pending.remove(&name);
        if !selected.insert(name.clone()) {
            continue;
        }
        let Some(funcs) = by_name.get(name.as_str()) else {
            continue;
        };
        for func in funcs {
            collect_called_functions_block(&func.body, &mut pending);
            for kwparam in &func.kwparams {
                collect_called_functions_expr(&kwparam.default, &mut pending);
            }
            out.push((*func).clone());
        }
    }

    out
}

fn split_macro_dependency_functions(
    macro_name: &str,
    macro_def: &StoredMacroDef,
    lambda_ctx: &LambdaContext,
    functions: Vec<Function>,
) -> (Vec<Function>, Vec<Function>) {
    let Some(info) = macro_hygiene_entry_for_macro(macro_name, macro_def, lambda_ctx) else {
        return (functions, Vec::new());
    };

    let mut top_level_functions = Vec::new();
    let mut module_functions = Vec::new();
    for func in functions {
        if info.members.contains(&func.name) {
            module_functions.push(func);
        } else {
            top_level_functions.push(func);
        }
    }
    (top_level_functions, module_functions)
}

fn compile_time_modules_for_macro(
    macro_name: &str,
    macro_def: &StoredMacroDef,
    lambda_ctx: &LambdaContext,
    functions: &[Function],
    referenced_modules: &HashSet<String>,
    span: Span,
) -> Vec<Module> {
    let mut modules = Vec::new();
    let mut module_members = Vec::new();
    if let Some(info) = macro_hygiene_entry_for_macro(macro_name, macro_def, lambda_ctx) {
        module_members.push(info);
    }
    for module_name in referenced_modules {
        if module_members
            .iter()
            .any(|info| info.module == *module_name)
        {
            continue;
        }
        if let Some(info) = lambda_ctx.macro_hygiene_info_for_module(module_name) {
            module_members.push(info);
        }
    }

    for info in module_members {
        let module_functions = functions
            .iter()
            .filter(|func| info.members.contains(&func.name))
            .cloned()
            .collect();
        modules.push(Module {
            name: info.module,
            is_bare: false,
            functions: module_functions,
            structs: lambda_ctx.compile_time_structs(),
            abstract_types: lambda_ctx.compile_time_abstract_types(),
            primitive_types: lambda_ctx.compile_time_primitive_types(),
            type_aliases: vec![],
            submodules: vec![],
            usings: vec![],
            macros: vec![],
            exports: info.exports.iter().cloned().collect(),
            publics: vec![],
            body: Block {
                stmts: Vec::new(),
                span,
            },
            span,
        });
    }
    modules
}

fn compile_time_usings_for_macro(
    macro_name: &str,
    macro_def: &StoredMacroDef,
    lambda_ctx: &LambdaContext,
    span: Span,
) -> Vec<UsingImport> {
    let Some(info) = macro_hygiene_entry_for_macro(macro_name, macro_def, lambda_ctx) else {
        return Vec::new();
    };
    let symbols = info.members.iter().cloned().collect();
    vec![UsingImport {
        module: info.module,
        symbols: Some(symbols),
        is_relative: true,
        relative_level: 1,
        alias_bindings: Vec::new(),
        span,
    }]
}

fn macro_hygiene_entry_for_macro(
    macro_name: &str,
    macro_def: &StoredMacroDef,
    lambda_ctx: &LambdaContext,
) -> Option<MacroHygieneInfo> {
    lambda_ctx
        .macro_hygiene_entry(macro_name)
        .or_else(|| macro_def.hygiene.clone())
}

fn collect_called_functions_block(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_called_functions_stmt(stmt, out);
    }
}

fn collect_called_functions_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Block(block) => collect_called_functions_block(block, out),
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            collect_called_functions_expr(value, out)
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                collect_called_functions_expr(e, out);
            }
        }
        Stmt::Expr { expr, .. } => collect_called_functions_expr(expr, out),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_called_functions_expr(condition, out);
            collect_called_functions_block(then_branch, out);
            if let Some(block) = else_branch {
                collect_called_functions_block(block, out);
            }
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_called_functions_expr(start, out);
            collect_called_functions_expr(end, out);
            if let Some(step) = step {
                collect_called_functions_expr(step, out);
            }
            collect_called_functions_block(body, out);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_called_functions_expr(iterable, out);
            collect_called_functions_block(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_called_functions_expr(condition, out);
            collect_called_functions_block(body, out);
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_called_functions_block(try_block, out);
            for block in [
                catch_block.as_ref(),
                else_block.as_ref(),
                finally_block.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                collect_called_functions_block(block, out);
            }
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            collect_called_functions_block(&func.body, out)
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_called_functions_expr(key, out);
            collect_called_functions_expr(value, out);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for index in indices {
                collect_called_functions_expr(index, out);
            }
            collect_called_functions_expr(value, out);
        }
        Stmt::FieldAssign { value, .. } => collect_called_functions_expr(value, out),
        Stmt::DestructuringAssign { value, .. } => collect_called_functions_expr(value, out),
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_called_functions_block(body, out)
        }
        Stmt::Test { condition, .. } => collect_called_functions_expr(condition, out),
        Stmt::TestThrows { expr, .. } => collect_called_functions_expr(expr, out),
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

fn collect_called_functions_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            out.insert(function.clone());
            collect_call_args(args, kwargs, out);
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            out.insert(function.clone());
            collect_call_args(args, kwargs, out);
        }
        Expr::FunctionRef { name, .. } => {
            out.insert(name.clone());
        }
        Expr::Var(name, _) => {
            out.insert(name.clone());
        }
        Expr::Literal(_, _) => {}
        Expr::BinaryOp { left, right, .. } => {
            collect_called_functions_expr(left, out);
            collect_called_functions_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_called_functions_expr(operand, out),
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                collect_called_functions_expr(arg, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_called_functions_expr(array, out);
            for index in indices {
                collect_called_functions_expr(index, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_called_functions_expr(element, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_called_functions_expr(start, out);
            if let Some(step) = step {
                collect_called_functions_expr(step, out);
            }
            collect_called_functions_expr(stop, out);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_called_functions_expr(body, out);
            collect_called_functions_expr(iter, out);
            if let Some(filter) = filter {
                collect_called_functions_expr(filter, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_called_functions_expr(body, out);
            for (_, iter) in iterations {
                collect_called_functions_expr(iter, out);
            }
            if let Some(filter) = filter {
                collect_called_functions_expr(filter, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_called_functions_expr(object, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_called_functions_expr(condition, out);
            collect_called_functions_expr(then_expr, out);
            collect_called_functions_expr(else_expr, out);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_called_functions_expr(value, out);
            }
            collect_called_functions_block(body, out);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_called_functions_expr(value, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_called_functions_expr(key, out);
                collect_called_functions_expr(value, out);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_called_functions_expr(part, out);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_called_functions_expr(base_expr, out);
            }
            for type_arg in type_args {
                collect_called_functions_expr(type_arg, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => collect_called_functions_expr(constructor, out),
        Expr::AssignExpr { value, .. } => collect_called_functions_expr(value, out),
        Expr::ReturnExpr { value, .. } => {
            if let Some(value) = value {
                collect_called_functions_expr(value, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_called_functions_expr(key, out);
            collect_called_functions_expr(value, out);
        }
        Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

fn collect_referenced_modules_block(block: &Block) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_referenced_modules_block_into(block, &mut out);
    out
}

fn collect_referenced_modules_block_into(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_referenced_modules_stmt(stmt, out);
    }
}

fn collect_referenced_modules_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Block(block) => collect_referenced_modules_block_into(block, out),
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            collect_referenced_modules_expr(value, out)
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                collect_referenced_modules_expr(e, out);
            }
        }
        Stmt::Expr { expr, .. } => collect_referenced_modules_expr(expr, out),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_referenced_modules_expr(condition, out);
            collect_referenced_modules_block_into(then_branch, out);
            if let Some(block) = else_branch {
                collect_referenced_modules_block_into(block, out);
            }
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_referenced_modules_expr(start, out);
            collect_referenced_modules_expr(end, out);
            if let Some(step) = step {
                collect_referenced_modules_expr(step, out);
            }
            collect_referenced_modules_block_into(body, out);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_referenced_modules_expr(iterable, out);
            collect_referenced_modules_block_into(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_referenced_modules_expr(condition, out);
            collect_referenced_modules_block_into(body, out);
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_referenced_modules_block_into(try_block, out);
            for block in [
                catch_block.as_ref(),
                else_block.as_ref(),
                finally_block.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                collect_referenced_modules_block_into(block, out);
            }
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            collect_referenced_modules_block_into(&func.body, out)
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_referenced_modules_expr(key, out);
            collect_referenced_modules_expr(value, out);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for index in indices {
                collect_referenced_modules_expr(index, out);
            }
            collect_referenced_modules_expr(value, out);
        }
        Stmt::FieldAssign { value, .. } => collect_referenced_modules_expr(value, out),
        Stmt::DestructuringAssign { value, .. } => collect_referenced_modules_expr(value, out),
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_referenced_modules_block_into(body, out)
        }
        Stmt::Test { condition, .. } => collect_referenced_modules_expr(condition, out),
        Stmt::TestThrows { expr, .. } => collect_referenced_modules_expr(expr, out),
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

fn collect_referenced_modules_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Call { args, kwargs, .. } => collect_referenced_module_call_args(args, kwargs, out),
        Expr::ModuleCall {
            module,
            args,
            kwargs,
            ..
        } => {
            out.insert(module.clone());
            collect_referenced_module_call_args(args, kwargs, out);
        }
        Expr::FunctionRef { .. } | Expr::Var(_, _) | Expr::Literal(_, _) => {}
        Expr::BinaryOp { left, right, .. } => {
            collect_referenced_modules_expr(left, out);
            collect_referenced_modules_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_referenced_modules_expr(operand, out),
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                collect_referenced_modules_expr(arg, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_referenced_modules_expr(array, out);
            for index in indices {
                collect_referenced_modules_expr(index, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_referenced_modules_expr(element, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_referenced_modules_expr(start, out);
            if let Some(step) = step {
                collect_referenced_modules_expr(step, out);
            }
            collect_referenced_modules_expr(stop, out);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_referenced_modules_expr(body, out);
            collect_referenced_modules_expr(iter, out);
            if let Some(filter) = filter {
                collect_referenced_modules_expr(filter, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_referenced_modules_expr(body, out);
            for (_, iter) in iterations {
                collect_referenced_modules_expr(iter, out);
            }
            if let Some(filter) = filter {
                collect_referenced_modules_expr(filter, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_referenced_modules_expr(object, out),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_referenced_modules_expr(condition, out);
            collect_referenced_modules_expr(then_expr, out);
            collect_referenced_modules_expr(else_expr, out);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_referenced_modules_expr(value, out);
            }
            collect_referenced_modules_block_into(body, out);
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_referenced_modules_expr(value, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_referenced_modules_expr(key, out);
                collect_referenced_modules_expr(value, out);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_referenced_modules_expr(part, out);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_referenced_modules_expr(base_expr, out);
            }
            for type_arg in type_args {
                collect_referenced_modules_expr(type_arg, out);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => collect_referenced_modules_expr(constructor, out),
        Expr::AssignExpr { value, .. } => collect_referenced_modules_expr(value, out),
        Expr::ReturnExpr { value, .. } => {
            if let Some(value) = value {
                collect_referenced_modules_expr(value, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_referenced_modules_expr(key, out);
            collect_referenced_modules_expr(value, out);
        }
        Expr::SliceAll { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

fn collect_referenced_module_call_args(
    args: &[Expr],
    kwargs: &[(String, Expr)],
    out: &mut HashSet<String>,
) {
    for arg in args {
        collect_referenced_modules_expr(arg, out);
    }
    for (_, value) in kwargs {
        collect_referenced_modules_expr(value, out);
    }
}

fn collect_call_args(args: &[Expr], kwargs: &[(String, Expr)], out: &mut HashSet<String>) {
    for arg in args {
        collect_called_functions_expr(arg, out);
    }
    for (_, value) in kwargs {
        collect_called_functions_expr(value, out);
    }
}

fn synthetic_macro_function(name: &str, macro_def: &StoredMacroDef, span: Span) -> Function {
    let mut params = vec![
        TypedParam::untyped("__source__".to_string(), span),
        TypedParam::untyped("__module__".to_string(), span),
    ];
    for (idx, param) in macro_def.params.iter().enumerate() {
        let is_varargs = macro_def.has_varargs && idx + 1 == macro_def.params.len();
        if is_varargs {
            params.push(TypedParam::varargs(param.clone(), None, span));
        } else {
            params.push(TypedParam::untyped(param.clone(), span));
        }
    }

    Function {
        name: name.to_string(),
        params,
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: macro_def.body.clone(),
        is_base_extension: false,
        is_runtime_eval: false,
        span,
    }
}

/// The module value bound to `__module__` during macro expansion: the call-site
/// module when expanding inside `module M ... end`, else `Main` at top level
/// (Issue #7919). sjulia represents a module value by its name, so this returns
/// the enclosing module name tracked by the lowering context.
fn call_site_module_name(lambda_ctx: &LambdaContext) -> String {
    lambda_ctx
        .current_module()
        .unwrap_or_else(|| "Main".to_string())
}

fn source_file_literal(lambda_ctx: &LambdaContext) -> Option<String> {
    let file = lambda_ctx.get_current_file();
    if file == "none" {
        None
    } else {
        Some(file)
    }
}

fn span_start_line_i64(span: Span) -> LowerResult<i64> {
    i64::try_from(span.start_line).map_err(|_| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion source line exceeds Int64 range")
    })
}

fn span_with_line_number(span: Span, line: i64) -> Span {
    let Ok(line) = usize::try_from(line) else {
        return span;
    };
    if line == 0 {
        return span;
    }
    Span {
        start_line: line,
        end_line: line,
        ..span
    }
}

fn values_to_stmts_with_line_spans<'a>(
    values: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Stmt>> {
    let mut current_span = span;
    let mut stmts = Vec::new();
    for value in values {
        match value {
            Value::LineNumberNode(line) => {
                current_span = span_with_line_number(span, line.line);
            }
            other => stmts.push(value_to_stmt(other, current_span, walker, lambda_ctx)?),
        }
    }
    Ok(stmts)
}

fn u64_to_i64(value: u64, span: Span) -> LowerResult<i64> {
    i64::try_from(value).map_err(|_| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion UInt64 value exceeds Int64 literal carrier range")
    })
}

fn u128_to_i128(value: u128, span: Span) -> LowerResult<i128> {
    i128::try_from(value).map_err(|_| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion UInt128 value exceeds Int128 literal carrier range")
    })
}

fn value_to_stmt<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    if let Value::Expr(expr) = &value {
        if let Some(head) = ExprHead::from_expr(expr) {
            debug_assert_eq!(
                head.spec().macro_return_to_stmt,
                macro_return_stmt_support(head)
            );
        }
    }
    match value {
        Value::Expr(expr)
            if matches!(
                ExprHead::from_expr(&expr),
                Some(ExprHead::Escape | ExprHead::HygienicScope)
            ) && !expr.args_snapshot().is_empty() =>
        {
            let args = expr.args_snapshot();
            lambda_ctx.enter_macro_esc();
            let result = value_to_stmt(args[0].clone(), span, walker, lambda_ctx);
            lambda_ctx.exit_macro_esc();
            result
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Block) => {
            let stmts =
                values_to_stmts_with_line_spans(expr.args_snapshot(), span, walker, lambda_ctx)?;
            Ok(Stmt::Block(Block { stmts, span }))
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::MacroCall) => {
            // Nested statement macrocalls such as
            // `Base.@__doc__(esc(Expr(:struct, ...)))` must re-enter the
            // statement conversion path so macro-expanded struct metadata is
            // registered, not lowered as an ordinary expression (Issue #7943).
            expand_macrocall_value_with(expr.args_snapshot(), span, lambda_ctx, |value| {
                value_to_stmt(value, span, walker, lambda_ctx)
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Function) => {
            function_stmt_from_values(expr.args_snapshot(), span, walker, lambda_ctx)
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Struct) => {
            let struct_def =
                struct_def_from_macro_values(expr.args_snapshot(), span, walker, lambda_ctx)?;
            lambda_ctx.add_macro_expanded_struct(struct_def);
            Ok(nothing_stmt(span))
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Meta) => {
            meta_stmt_from_args(expr.args_snapshot(), span)
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Assign) => {
            let args = expr.args_snapshot();
            if args.len() == 2 {
                let target_value = macro_assignment_target(&args[0]);
                match &target_value {
                    Value::Symbol(name) => {
                        return Ok(Stmt::Assign {
                            var: name.as_str().to_string(),
                            value: value_to_expr(args[1].clone(), span, walker, lambda_ctx)?,
                            span,
                        });
                    }
                    // Indexed assignment `a[i...] = v`, round-tripped as
                    // `Expr(:(=), Expr(:ref, a, i...), v)`. Without this the
                    // macro-expanded `a[i] = v` silently no-ops (Issue #7350 A2).
                    Value::Expr(target) if ExprHead::is_expr(target, ExprHead::Ref) => {
                        let target_args = target.args_snapshot();
                        if let Some((array, indices)) = target_args.split_first() {
                            return Ok(Stmt::IndexAssign {
                                array: symbol_arg(array, span)?,
                                indices: values_to_exprs(
                                    indices.to_vec(),
                                    span,
                                    walker,
                                    lambda_ctx,
                                )?,
                                value: value_to_expr(args[1].clone(), span, walker, lambda_ctx)?,
                                span,
                            });
                        }
                    }
                    // Field assignment `obj.field = v`, round-tripped as
                    // `Expr(:(=), Expr(:., obj, QuoteNode(:field)), v)`.
                    Value::Expr(target) if ExprHead::is_expr(target, ExprHead::Dot) => {
                        let target_args = target.args_snapshot();
                        if target_args.len() == 2 {
                            let field = match &target_args[1] {
                                Value::QuoteNode(inner) => symbol_arg(inner, span)?,
                                other => symbol_arg(other, span)?,
                            };
                            return Ok(Stmt::FieldAssign {
                                object: symbol_arg(&target_args[0], span)?,
                                field,
                                value: value_to_expr(args[1].clone(), span, walker, lambda_ctx)?,
                                span,
                            });
                        }
                    }
                    // Tuple destructuring `(a, b) = rhs`, round-tripped as
                    // `Expr(:(=), Expr(:tuple, a, b), rhs)`. Without this the
                    // assignment fell through to the expression path and lowered
                    // `=` as a call to the `=` operator → "Unknown function: ="
                    // (Issue #7900). Reuse the CST destructuring machinery.
                    Value::Expr(target) if ExprHead::is_expr(target, ExprHead::Tuple) => {
                        let patterns = target
                            .args_snapshot()
                            .iter()
                            .filter(|arg| !matches!(arg, Value::LineNumberNode(_)))
                            .map(|arg| macro_destructure_target(arg, span))
                            .collect::<LowerResult<Vec<_>>>()?;
                        let rhs = value_to_expr(args[1].clone(), span, walker, lambda_ctx)?;
                        return lower_destructuring_from_targets(patterns, rhs, span);
                    }
                    // Short-form function definition `f(args...) = body`,
                    // round-tripped as `Expr(:(=), Expr(:call, f, args...), body)`
                    // (optionally wrapped in `Expr(:where, ..., T...)` for
                    // `f(x::T) where {T} = body`). The interpolated-type variant
                    // `f(x::$T) = body` lands here too once `$T` is spliced.
                    // Without this the assignment fell through to
                    // `assignment_expr_from_values`, which errors with
                    // "unsupported assignment expression target" (Issue #7933).
                    // Route it to the same function-definition builder used for
                    // the full `Expr(:function, ...)` form.
                    Value::Expr(target)
                        if matches!(
                            ExprHead::from_expr(target),
                            Some(ExprHead::Call | ExprHead::Where)
                        ) =>
                    {
                        return function_stmt_from_values(
                            vec![target_value.clone(), args[1].clone()],
                            span,
                            walker,
                            lambda_ctx,
                        );
                    }
                    _ => {}
                }
            }
            Ok(Stmt::Expr {
                expr: value_to_expr(Value::Expr(expr), span, walker, lambda_ctx)?,
                span,
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Const) => {
            const_stmt_from_args(expr.args_snapshot(), span, walker, lambda_ctx)
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Export) => {
            let names = symbol_declaration_names(&expr.args_snapshot(), "export", span)?;
            Ok(Stmt::Export { names, span })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Public) => {
            validate_symbol_declaration_args(&expr.args_snapshot(), "public", span)?;
            Ok(nothing_stmt(span))
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::AddAssign) => {
            let args = expr.args_snapshot();
            if args.len() == 2 {
                let target_value = macro_assignment_target(&args[0]);
                if let Value::Symbol(name) = target_value {
                    return Ok(Stmt::AddAssign {
                        var: name.as_str().to_string(),
                        value: value_to_expr(args[1].clone(), span, walker, lambda_ctx)?,
                        span,
                    });
                }
            }
            Ok(Stmt::Expr {
                expr: value_to_expr(Value::Expr(expr), span, walker, lambda_ctx)?,
                span,
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Call) => {
            let args = expr.args_snapshot();
            if args.len() == 3 {
                if let Value::Symbol(op) = &args[0] {
                    if op.as_str() == "=" {
                        let target_value = macro_assignment_target(&args[1]);
                        match &target_value {
                            Value::Symbol(name) => {
                                return Ok(Stmt::Assign {
                                    var: name.as_str().to_string(),
                                    value: value_to_expr(
                                        args[2].clone(),
                                        span,
                                        walker,
                                        lambda_ctx,
                                    )?,
                                    span,
                                });
                            }
                            // Tuple destructuring `(a, b) = rhs` arrives from the
                            // macro-arg constructor as `Expr(:call, :(=),
                            // Expr(:tuple, a, b), rhs)`. Without this the assignment
                            // fell through and lowered `=` as a call to the `=`
                            // operator → "Unknown function: =" (Issue #7900).
                            Value::Expr(target) if ExprHead::is_expr(target, ExprHead::Tuple) => {
                                let patterns = target
                                    .args_snapshot()
                                    .iter()
                                    .filter(|arg| !matches!(arg, Value::LineNumberNode(_)))
                                    .map(|arg| macro_destructure_target(arg, span))
                                    .collect::<LowerResult<Vec<_>>>()?;
                                let rhs = value_to_expr(args[2].clone(), span, walker, lambda_ctx)?;
                                return lower_destructuring_from_targets(patterns, rhs, span);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Stmt::Expr {
                expr: expr_value_to_expr(expr, span, walker, lambda_ctx)?,
                span,
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::For) => {
            let args = expr.args_snapshot();
            if args.len() != 2 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro expansion returned malformed Expr(:for, ...)"),
                );
            }
            let (var, iterable) =
                for_binding_from_value(args[0].clone(), span, walker, lambda_ctx)?;
            Ok(Stmt::ForEach {
                var,
                iterable,
                body: value_to_block(args[1].clone(), span, walker, lambda_ctx)?,
                span,
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::If) => {
            let args = expr.args_snapshot();
            if args.len() < 2 || args.len() > 3 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro expansion returned malformed Expr(:if, ...)"),
                );
            }
            Ok(Stmt::If {
                condition: value_to_expr(args[0].clone(), span, walker, lambda_ctx)?,
                then_branch: value_to_block(args[1].clone(), span, walker, lambda_ctx)?,
                else_branch: if args.len() == 3 {
                    Some(value_to_block(args[2].clone(), span, walker, lambda_ctx)?)
                } else {
                    None
                },
                span,
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::ElseIf) => {
            let args = expr.args_snapshot();
            if args.len() < 2 || args.len() > 3 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro expansion returned malformed Expr(:elseif, ...)"),
                );
            }
            Ok(Stmt::If {
                condition: value_to_branch_expr(args[0].clone(), span, walker, lambda_ctx)?,
                then_branch: value_to_block(args[1].clone(), span, walker, lambda_ctx)?,
                else_branch: if args.len() == 3 {
                    Some(value_to_block(args[2].clone(), span, walker, lambda_ctx)?)
                } else {
                    None
                },
                span,
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Let) => Ok(Stmt::Expr {
            expr: let_expr_from_args(expr.args_snapshot(), span, walker, lambda_ctx)?,
            span,
        }),
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Try) => {
            try_stmt_from_values(expr.args_snapshot(), span, walker, lambda_ctx)
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Return) => {
            let args = expr.args_snapshot();
            Ok(Stmt::Return {
                value: args
                    .first()
                    .map(|arg| value_to_expr(arg.clone(), span, walker, lambda_ctx))
                    .transpose()?,
                span,
            })
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Local) => {
            let args = expr.args_snapshot();
            if let Some(inner) = args.first() {
                value_to_stmt(inner.clone(), span, walker, lambda_ctx)
            } else {
                Ok(Stmt::Expr {
                    expr: Expr::Literal(Literal::Nothing, span),
                    span,
                })
            }
        }
        other => Ok(Stmt::Expr {
            expr: value_to_expr(other, span, walker, lambda_ctx)?,
            span,
        }),
    }
}

fn macro_return_stmt_support(head: ExprHead) -> bool {
    matches!(
        head,
        ExprHead::Escape
            | ExprHead::HygienicScope
            | ExprHead::Block
            | ExprHead::Function
            | ExprHead::Struct
            | ExprHead::Meta
            | ExprHead::Assign
            | ExprHead::Const
            | ExprHead::Export
            | ExprHead::Public
            | ExprHead::AddAssign
            | ExprHead::Call
            | ExprHead::For
            | ExprHead::If
            | ExprHead::ElseIf
            | ExprHead::Let
            | ExprHead::Try
            | ExprHead::Return
            | ExprHead::Local
    )
}

fn meta_stmt_from_args(args: Vec<Value>, span: Span) -> LowerResult<Stmt> {
    let Some((name, rest)) = args.split_first() else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned empty Expr(:meta, ...)"),
        );
    };
    Ok(Stmt::Meta {
        annotation: MetaAnnotation {
            name: macro_meta_arg_to_string(name, span)?,
            args: rest
                .iter()
                .map(|arg| macro_meta_arg_to_string(arg, span))
                .collect::<LowerResult<Vec<_>>>()?,
        },
        span,
    })
}

fn macro_meta_arg_to_string(value: &Value, span: Span) -> LowerResult<String> {
    match value {
        Value::Symbol(name) => Ok(name.as_str().to_string()),
        Value::Str(value) => Ok(value.clone()),
        Value::I8(value) => Ok(value.to_string()),
        Value::I16(value) => Ok(value.to_string()),
        Value::I32(value) => Ok(value.to_string()),
        Value::I64(value) => Ok(value.to_string()),
        Value::I128(value) => Ok(value.to_string()),
        Value::U8(value) => Ok(value.to_string()),
        Value::U16(value) => Ok(value.to_string()),
        Value::U32(value) => Ok(value.to_string()),
        Value::U64(value) => Ok(value.to_string()),
        Value::U128(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported Expr(:meta, ...) argument {:?}",
                other.value_type()
            )),
        ),
    }
}

fn function_stmt_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    if args.len() != 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned malformed Expr(:function, ...)"),
        );
    }

    let mut iter = args.into_iter();
    let signature = iter.next().expect("len checked");
    let body = iter.next().expect("len checked");
    // Reuse the constructor-signature reader so a `where` wrapper
    // (`Expr(:where, Expr(:call, ...), T...)`) round-tripped through a macro
    // expansion contributes its type parameters instead of being dropped
    // (Issue #7933). For a plain `Expr(:call, ...)` it yields no type params.
    let (name, params, type_params) = constructor_signature_from_value(signature, span)?;
    Ok(Stmt::FunctionDef {
        func: Box::new(Function {
            name,
            params,
            kwparams: Vec::new(),
            type_params,
            return_type: None,
            body: value_to_block(body, span, walker, lambda_ctx)?,
            is_base_extension: false,
            is_runtime_eval: false,
            span,
        }),
        span,
    })
}

fn function_signature_from_value(
    signature: Value,
    span: Span,
) -> LowerResult<(String, Vec<TypedParam>)> {
    match signature {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Call) => {
            let args = expr.args_snapshot();
            let (callee, params) = args.split_first().ok_or_else(|| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned empty function signature")
            })?;
            let callee = macro_assignment_target(callee);
            let name = macro_callable_name_from_value(&callee, span)?;
            let params = params
                .iter()
                .map(|param| function_param_from_value(param, span))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok((name, params))
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported function signature {:?}",
                other.value_type()
            )),
        ),
    }
}

fn function_param_from_value(value: &Value, span: Span) -> LowerResult<TypedParam> {
    match value {
        Value::Symbol(name) => Ok(TypedParam::untyped(name.as_str().to_string(), span)),
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::TypeAssert) => {
            let args = expr.args_snapshot();
            if args.len() == 2 {
                return Ok(TypedParam::new(
                    symbol_arg(&args[0], span)?,
                    Some(julia_type_from_value(&args[1], span)?),
                    span,
                ));
            }
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed typed function parameter"),
            )
        }
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Splat) => {
            let args = expr.args_snapshot();
            if args.len() == 1 {
                let param = function_param_from_value(&args[0], span)?;
                return Ok(TypedParam::varargs(param.name, param.type_annotation, span));
            }
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed varargs function parameter"),
            )
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported function parameter {:?}",
                other.value_type()
            )),
        ),
    }
}

fn julia_type_from_value(value: &Value, span: Span) -> LowerResult<JuliaType> {
    match value {
        Value::Symbol(name) => Ok(JuliaType::from_name_or_struct(name.as_str())),
        Value::GlobalRef(_) => Ok(JuliaType::from_name_or_struct(&macro_type_name_from_value(
            value, span,
        )?)),
        Value::Expr(expr)
            if matches!(
                ExprHead::from_expr(expr),
                Some(ExprHead::Curly | ExprHead::Dot)
            ) =>
        {
            Ok(JuliaType::from_name_or_struct(&macro_type_name_from_value(
                value, span,
            )?))
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported function parameter type {:?}",
                other.value_type()
            )),
        ),
    }
}

fn macro_callable_name_from_value(value: &Value, span: Span) -> LowerResult<String> {
    match value {
        Value::Symbol(name) => Ok(name.as_str().to_string()),
        Value::GlobalRef(_) => macro_type_name_from_value(value, span),
        Value::Expr(expr)
            if matches!(
                ExprHead::from_expr(expr),
                Some(ExprHead::Curly | ExprHead::Dot)
            ) =>
        {
            macro_type_name_from_value(value, span)
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported function callee {:?}",
                other.value_type()
            )),
        ),
    }
}

fn struct_def_from_macro_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<StructDef> {
    if args.len() != 3 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned malformed Expr(:struct, ...)"),
        );
    }

    let is_mutable = match &args[0] {
        Value::Bool(value) => *value,
        other => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "macro-expanded struct mutability flag must be Bool, got {:?}",
                        other.value_type()
                    ),
                ),
            )
        }
    };

    let (name, type_params, parent_type) = struct_header_from_macro_value(&args[1], span)?;
    let (fields, inner_constructors) =
        struct_body_from_macro_value(&args[2], span, &name, &type_params, walker, lambda_ctx)?;
    let own_param_names: Vec<String> = type_params.iter().map(|param| param.name.clone()).collect();
    let parent_type = parent_type
        .map(|parent| crate::lowering::type_alias::expand_excluding(&parent, &own_param_names));

    Ok(StructDef {
        name,
        is_mutable,
        type_params,
        parent_type,
        fields,
        inner_constructors,
        span,
    })
}

fn struct_header_from_macro_value(
    value: &Value,
    span: Span,
) -> LowerResult<(String, Vec<TypeParam>, Option<String>)> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Subtype) => {
            let args = expr.args_snapshot();
            if args.len() != 2 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded struct subtype header must have two arguments"),
                );
            }
            let (name, type_params) = struct_name_params_from_macro_value(&args[0], span)?;
            let parent = macro_type_name_from_value(&args[1], span)?;
            Ok((name, type_params, Some(parent)))
        }
        other => {
            let (name, type_params) = struct_name_params_from_macro_value(other, span)?;
            Ok((name, type_params, None))
        }
    }
}

fn struct_name_params_from_macro_value(
    value: &Value,
    span: Span,
) -> LowerResult<(String, Vec<TypeParam>)> {
    match value {
        Value::Symbol(name) => Ok((name.as_str().to_string(), Vec::new())),
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Curly) => {
            let args = expr.args_snapshot();
            let Some((base, params)) = args.split_first() else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded struct curly header is empty"),
                );
            };
            let name = symbol_arg(base, span)?;
            let type_params = params
                .iter()
                .map(|param| struct_type_param_from_macro_value(param, span))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok((name, type_params))
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro-expanded struct header must be Symbol or Expr(:curly), got {:?}",
                other.value_type()
            )),
        ),
    }
}

fn struct_type_param_from_macro_value(value: &Value, span: Span) -> LowerResult<TypeParam> {
    match value {
        Value::Symbol(name) => Ok(TypeParam::new(name.as_str().to_string())),
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Subtype) => {
            let args = expr.args_snapshot();
            if args.len() != 2 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded struct type parameter bound is malformed"),
                );
            }
            Ok(TypeParam::with_upper_bound(
                symbol_arg(&args[0], span)?,
                macro_type_name_from_value(&args[1], span)?,
            ))
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro-expanded struct type parameter must be Symbol or Expr(:<:), got {:?}",
                other.value_type()
            )),
        ),
    }
}

fn struct_body_from_macro_value<'a>(
    value: &Value,
    span: Span,
    struct_name: &str,
    type_params: &[TypeParam],
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(Vec<StructField>, Vec<InnerConstructor>)> {
    let items = match value {
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Block) => expr.args_snapshot(),
        other => vec![other.clone()],
    };

    let mut fields = Vec::new();
    let mut inner_constructors = Vec::new();
    for item in items {
        if matches!(item, Value::LineNumberNode(_)) {
            continue;
        }
        match &item {
            Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Function) => {
                let Some(ctor) = inner_constructor_from_macro_function_values(
                    expr.args_snapshot(),
                    span,
                    struct_name,
                    walker,
                    lambda_ctx,
                )?
                else {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                            "macro-expanded struct body contains a non-constructor function",
                        ),
                    );
                };
                inner_constructors.push(ctor);
            }
            Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Assign) => {
                if let Some(ctor) = inner_constructor_from_macro_assignment(
                    expr.args_snapshot(),
                    span,
                    struct_name,
                    walker,
                    lambda_ctx,
                )? {
                    inner_constructors.push(ctor);
                } else {
                    fields.push(struct_field_from_macro_value(&item, span, type_params)?);
                }
            }
            _ => fields.push(struct_field_from_macro_value(&item, span, type_params)?),
        }
    }
    Ok((fields, inner_constructors))
}

fn inner_constructor_from_macro_function_values<'a>(
    args: Vec<Value>,
    span: Span,
    struct_name: &str,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<InnerConstructor>> {
    if args.len() != 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned malformed Expr(:function, ...)"),
        );
    }

    let mut iter = args.into_iter();
    let signature = iter.next().expect("len checked");
    let body = iter.next().expect("len checked");
    let (name, params, type_params) = constructor_signature_from_value(signature, span)?;
    if macro_constructor_base_name(&name) != struct_name {
        return Ok(None);
    }

    Ok(Some(InnerConstructor {
        params,
        kwparams: Vec::new(),
        type_params,
        body: value_to_block(body, span, walker, lambda_ctx)?,
        span,
    }))
}

fn inner_constructor_from_macro_assignment<'a>(
    args: Vec<Value>,
    span: Span,
    struct_name: &str,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<InnerConstructor>> {
    if args.len() != 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro-expanded short inner constructor assignment is malformed"),
        );
    }

    let (name, params, type_params) = constructor_signature_from_value(args[0].clone(), span)?;
    if macro_constructor_base_name(&name) != struct_name {
        return Ok(None);
    }
    let value = value_to_expr(args[1].clone(), span, walker, lambda_ctx)?;
    let body = Block {
        stmts: vec![Stmt::Return {
            value: Some(value),
            span,
        }],
        span,
    };

    Ok(Some(InnerConstructor {
        params,
        kwparams: Vec::new(),
        type_params,
        body,
        span,
    }))
}

fn constructor_signature_from_value(
    signature: Value,
    span: Span,
) -> LowerResult<(String, Vec<TypedParam>, Vec<TypeParam>)> {
    match signature {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Where) => {
            let args = expr.args_snapshot();
            let Some((body, params)) = args.split_first() else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded constructor where signature is empty"),
                );
            };
            let (name, sig_params, mut type_params) =
                constructor_signature_from_value(body.clone(), span)?;
            type_params.extend(
                params
                    .iter()
                    .map(|param| struct_type_param_from_macro_value(param, span))
                    .collect::<LowerResult<Vec<_>>>()?,
            );
            Ok((name, sig_params, type_params))
        }
        other => {
            let (name, params) = function_signature_from_value(other, span)?;
            Ok((name, params, Vec::new()))
        }
    }
}

fn macro_constructor_base_name(name: &str) -> &str {
    name.split_once('{').map(|(base, _)| base).unwrap_or(name)
}

fn struct_field_from_macro_value(
    value: &Value,
    span: Span,
    type_params: &[TypeParam],
) -> LowerResult<StructField> {
    match value {
        Value::Symbol(name) => Ok(StructField {
            name: name.as_str().to_string(),
            type_expr: None,
            span,
        }),
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::TypeAssert) => {
            let args = expr.args_snapshot();
            if args.len() != 2 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded struct typed field is malformed"),
                );
            }
            Ok(StructField {
                name: symbol_arg(&args[0], span)?,
                type_expr: Some(macro_type_expr_from_value(&args[1], span, type_params)?),
                span,
            })
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro-expanded struct body contains unsupported field item {:?}",
                other.value_type()
            )),
        ),
    }
}

fn macro_type_expr_from_value(
    value: &Value,
    span: Span,
    type_params: &[TypeParam],
) -> LowerResult<TypeExpr> {
    match value {
        Value::Symbol(name) => Ok(TypeExpr::from_name(name.as_str(), type_params)),
        Value::GlobalRef(GlobalRefValue { module, name }) => Ok(TypeExpr::from_name(
            &format!("{}.{}", module.as_str(), name.as_str()),
            type_params,
        )),
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Curly) => {
            let args = expr.args_snapshot();
            let Some((base, params)) = args.split_first() else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded parametric field type is empty"),
                );
            };
            Ok(TypeExpr::Parameterized {
                base: macro_type_name_from_value(base, span)?,
                params: params
                    .iter()
                    .map(|param| macro_type_expr_from_value(param, span, type_params))
                    .collect::<LowerResult<Vec<_>>>()?,
            })
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro-expanded struct field type is unsupported: {:?}",
                other.value_type()
            )),
        ),
    }
}

fn macro_type_name_from_value(value: &Value, span: Span) -> LowerResult<String> {
    match value {
        Value::Symbol(name) => Ok(name.as_str().to_string()),
        Value::GlobalRef(GlobalRefValue { module, name }) => {
            Ok(format!("{}.{}", module.as_str(), name.as_str()))
        }
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Curly) => {
            let args = expr.args_snapshot();
            let Some((base, params)) = args.split_first() else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded parametric type name is empty"),
                );
            };
            let rendered_params = params
                .iter()
                .map(|param| macro_type_name_from_value(param, span))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok(format!(
                "{}{{{}}}",
                macro_type_name_from_value(base, span)?,
                rendered_params.join(", ")
            ))
        }
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Dot) => {
            let args = expr.args_snapshot();
            if args.len() != 2 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro-expanded qualified type name is malformed"),
                );
            }
            let field = match &args[1] {
                Value::QuoteNode(inner) => symbol_arg(inner, span)?,
                other => symbol_arg(other, span)?,
            };
            Ok(format!(
                "{}.{}",
                macro_type_name_from_value(&args[0], span)?,
                field
            ))
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro-expanded type name is unsupported: {:?}",
                other.value_type()
            )),
        ),
    }
}

fn value_to_block<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Block> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Block) => {
            let stmts =
                values_to_stmts_with_line_spans(expr.args_snapshot(), span, walker, lambda_ctx)?;
            Ok(Block { stmts, span })
        }
        other => Ok(Block {
            stmts: vec![value_to_stmt(other, span, walker, lambda_ctx)?],
            span,
        }),
    }
}

fn value_to_value_block<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Block> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Block) => {
            let mut args: Vec<Value> = expr
                .args_snapshot()
                .into_iter()
                .filter(|arg| !matches!(arg, Value::LineNumberNode(_)))
                .collect();
            if let Some(last) = args.pop() {
                let mut stmts = args
                    .into_iter()
                    .map(|arg| value_to_stmt(arg, span, walker, lambda_ctx))
                    .collect::<LowerResult<Vec<_>>>()?;
                stmts.push(Stmt::Expr {
                    expr: value_to_expr(last, span, walker, lambda_ctx)?,
                    span,
                });
                Ok(Block { stmts, span })
            } else {
                Ok(Block {
                    stmts: Vec::new(),
                    span,
                })
            }
        }
        other => Ok(Block {
            stmts: vec![Stmt::Expr {
                expr: value_to_expr(other, span, walker, lambda_ctx)?,
                span,
            }],
            span,
        }),
    }
}

/// Convert a macro-produced branch value (the then/else arm of an `if` Expr used in
/// expression position) into a *value-producing* `Expr`, yielding the value of the
/// branch's last statement — mirroring `lower_block_as_expr` for source `if`
/// expressions (Issue #7350 A1). A bare expression value is lowered directly; a
/// `block` Expr yields its last non-`LineNumberNode` statement's value.
fn value_to_branch_expr<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Block) => {
            let mut args: Vec<Value> = expr
                .args_snapshot()
                .into_iter()
                .filter(|arg| !matches!(arg, Value::LineNumberNode(_)))
                .collect();
            match args.len() {
                0 => Ok(Expr::Literal(Literal::Nothing, span)),
                1 => value_to_expr(args.remove(0), span, walker, lambda_ctx),
                _ => {
                    let tail = args.pop().expect("non-empty after len check");
                    let stmts = args
                        .into_iter()
                        .map(|arg| value_to_stmt(arg, span, walker, lambda_ctx))
                        .collect::<LowerResult<Vec<_>>>()?;
                    let mut stmts = stmts;
                    if value_requires_stmt_path_in_tail(&tail) {
                        stmts.push(value_to_stmt(tail, span, walker, lambda_ctx)?);
                    } else {
                        stmts.push(Stmt::Expr {
                            expr: value_to_expr(tail, span, walker, lambda_ctx)?,
                            span,
                        });
                    }
                    Ok(Expr::LetBlock {
                        bindings: vec![],
                        body: Block { stmts, span },
                        span,
                    })
                }
            }
        }
        other => value_to_expr(other, span, walker, lambda_ctx),
    }
}

fn const_stmt_from_args<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let Some(inner) = args.first() else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned empty Expr(:const, ...)"),
        );
    };

    let mut stmts = Vec::new();
    match assignment_parts_from_value(inner.clone(), span, walker, lambda_ctx)? {
        Some((var, value)) => {
            stmts.push(const_declaration_stmt(&var, span));
            stmts.push(Stmt::Assign { var, value, span });
        }
        None => {
            let Value::Symbol(name) = inner else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro expansion returned malformed const declaration"),
                );
            };
            stmts.push(const_declaration_stmt(name.as_str(), span));
        }
    }

    Ok(Stmt::Block(Block { stmts, span }))
}

fn const_declaration_stmt(name: &str, span: Span) -> Stmt {
    Stmt::Expr {
        expr: Expr::Call {
            function: "#__sjulia_declare_const__".to_string(),
            args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        },
        span,
    }
}

fn optional_try_block_from_value<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<Block>> {
    match value {
        Value::Bool(false) => Ok(None),
        other => Ok(Some(value_to_block(other, span, walker, lambda_ctx)?)),
    }
}

fn try_stmt_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    if args.len() < 3 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned malformed Expr(:try, ...)"),
        );
    }

    let catch_var = match &args[1] {
        Value::Bool(false) => None,
        other => Some(symbol_arg(other, span)?),
    };
    Ok(Stmt::Try {
        try_block: value_to_block(args[0].clone(), span, walker, lambda_ctx)?,
        catch_var,
        catch_block: optional_try_block_from_value(args[2].clone(), span, walker, lambda_ctx)?,
        else_block: if args.len() >= 5 {
            optional_try_block_from_value(args[4].clone(), span, walker, lambda_ctx)?
        } else {
            None
        },
        finally_block: if args.len() >= 4 {
            optional_try_block_from_value(args[3].clone(), span, walker, lambda_ctx)?
        } else {
            None
        },
        span,
    })
}

fn try_expr_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let stmt = try_stmt_from_values(args, span, walker, lambda_ctx)?;
    crate::lowering::expr::try_stmt_into_value_expr(stmt, span).ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned malformed Expr(:try, ...)")
    })
}

fn assignment_parts_from_value<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<(String, Expr)>> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Assign) => {
            let args = expr.args_snapshot();
            if args.len() == 2 {
                let target_value = macro_assignment_target(&args[0]);
                if let Value::Symbol(name) = &target_value {
                    return Ok(Some((
                        name.as_str().to_string(),
                        value_to_expr(args[1].clone(), span, walker, lambda_ctx)?,
                    )));
                }
            }
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed assignment"),
            )
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Call) => {
            let args = expr.args_snapshot();
            if args.len() == 3 {
                let target_value = macro_assignment_target(&args[1]);
                if let (Value::Symbol(op), Value::Symbol(name)) = (&args[0], &target_value) {
                    if op.as_str() == "=" {
                        return Ok(Some((
                            name.as_str().to_string(),
                            value_to_expr(args[2].clone(), span, walker, lambda_ctx)?,
                        )));
                    }
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn for_binding_from_value<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(String, Expr)> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Block) => {
            let mut args = expr
                .args_snapshot()
                .into_iter()
                .filter(|arg| !matches!(arg, Value::LineNumberNode(_)));
            if let Some(inner) = args.next() {
                for_binding_from_value(inner, span, walker, lambda_ctx)
            } else {
                Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro expansion returned empty for binding block"),
                )
            }
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Assign) => {
            let args = expr.args_snapshot();
            if args.len() == 2 {
                if let Value::Symbol(name) = &args[0] {
                    return Ok((
                        name.as_str().to_string(),
                        value_to_expr(args[1].clone(), span, walker, lambda_ctx)?,
                    ));
                }
            }
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed for binding assignment"),
            )
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Call) => {
            let args = expr.args_snapshot();
            if args.len() == 3 {
                if let (Value::Symbol(op), Value::Symbol(name)) = (&args[0], &args[1]) {
                    if op.as_str() == "=" {
                        return Ok((
                            name.as_str().to_string(),
                            value_to_expr(args[2].clone(), span, walker, lambda_ctx)?,
                        ));
                    }
                }
            }
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed for binding call"),
            )
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported for binding {:?}",
                other.value_type()
            )),
        ),
    }
}

fn let_expr_from_args<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned malformed Expr(:let, ...)"),
        );
    }
    Ok(Expr::LetBlock {
        bindings: let_bindings_from_value(args[0].clone(), span, walker, lambda_ctx)?,
        body: value_to_value_block(args[1].clone(), span, walker, lambda_ctx)?,
        span,
    })
}

fn let_bindings_from_value<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<(String, Expr)>> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Block) => expr
            .args_snapshot()
            .into_iter()
            .filter(|arg| !matches!(arg, Value::LineNumberNode(_)))
            .map(|arg| let_binding_from_value(arg, span, walker, lambda_ctx))
            .collect(),
        other => Ok(vec![let_binding_from_value(
            other, span, walker, lambda_ctx,
        )?]),
    }
}

fn let_binding_from_value<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(String, Expr)> {
    assignment_parts_from_value(value, span, walker, lambda_ctx)?.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned unsupported let binding")
    })
}

fn value_to_expr<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    match value {
        Value::I8(n) => Ok(Expr::Literal(Literal::Int(i64::from(n)), span)),
        Value::I16(n) => Ok(Expr::Literal(Literal::Int(i64::from(n)), span)),
        Value::I32(n) => Ok(Expr::Literal(Literal::Int(i64::from(n)), span)),
        Value::I64(n) => Ok(Expr::Literal(Literal::Int(n), span)),
        Value::I128(n) => Ok(Expr::Literal(Literal::Int128(n), span)),
        Value::BigInt(n) => Ok(Expr::Literal(Literal::BigInt(n.to_string()), span)),
        Value::U8(n) => Ok(Expr::Call {
            function: "UInt8".to_string(),
            args: vec![Expr::Literal(Literal::Int(i64::from(n)), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U16(n) => Ok(Expr::Call {
            function: "UInt16".to_string(),
            args: vec![Expr::Literal(Literal::Int(i64::from(n)), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U32(n) => Ok(Expr::Call {
            function: "UInt32".to_string(),
            args: vec![Expr::Literal(Literal::Int(i64::from(n)), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U64(n) => Ok(Expr::Call {
            function: "UInt64".to_string(),
            args: vec![Expr::Literal(Literal::Int(u64_to_i64(n, span)?), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U128(n) => Ok(Expr::Call {
            function: "UInt128".to_string(),
            args: vec![Expr::Literal(Literal::Int128(u128_to_i128(n, span)?), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::F16(n) => Ok(Expr::Literal(Literal::Float16(n), span)),
        Value::F32(n) => Ok(Expr::Literal(Literal::Float32(n), span)),
        Value::F64(n) => Ok(Expr::Literal(Literal::Float(n), span)),
        Value::BigFloat(n) => Ok(Expr::Literal(Literal::BigFloat(n.to_string()), span)),
        Value::Bool(b) => Ok(Expr::Literal(Literal::Bool(b), span)),
        Value::Str(s) => Ok(Expr::Literal(Literal::Str(s), span)),
        Value::Char(c) => Ok(Expr::Literal(Literal::Char(c), span)),
        Value::Nothing => Ok(Expr::Literal(Literal::Nothing, span)),
        Value::Missing => Ok(Expr::Literal(Literal::Missing, span)),
        Value::Module(module) => Ok(Expr::Literal(Literal::Module(module.name.clone()), span)),
        Value::Symbol(sym) if sym.as_str() == "nothing" => {
            Ok(Expr::Literal(Literal::Nothing, span))
        }
        Value::Symbol(sym) if sym.as_str() == "missing" => {
            Ok(Expr::Literal(Literal::Missing, span))
        }
        Value::Symbol(sym) if sym.as_str() == "true" => {
            Ok(Expr::Literal(Literal::Bool(true), span))
        }
        Value::Symbol(sym) if sym.as_str() == "false" => {
            Ok(Expr::Literal(Literal::Bool(false), span))
        }
        Value::Symbol(sym) if matches!(sym.as_str(), "Base" | "Core" | "Main" | "Sys" | "Meta") => {
            Ok(Expr::Literal(
                Literal::Module(sym.as_str().to_string()),
                span,
            ))
        }
        Value::Symbol(sym) => Ok(Expr::Var(sym.as_str().to_string(), span)),
        Value::QuoteNode(inner) => Ok(Expr::Literal(value_to_literal(*inner, span)?, span)),
        Value::LineNumberNode(ln) => Ok(Expr::Literal(
            Literal::LineNumberNode {
                line: ln.line,
                file: ln.file,
            },
            span,
        )),
        Value::Tuple(tuple) => Ok(Expr::TupleLiteral {
            elements: tuple
                .elements
                .into_iter()
                .map(|element| value_to_expr(element, span, walker, lambda_ctx))
                .collect::<LowerResult<Vec<_>>>()?,
            span,
        }),
        Value::DataType(ty) => Ok(Expr::Var(ty.name().to_string(), span)),
        Value::Expr(expr) => expr_value_to_expr(expr, span, walker, lambda_ctx),
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported value type {:?}",
                other.value_type()
            )),
        ),
    }
}

fn expr_value_to_expr<'a>(
    expr: ExprValue,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let head_name = expr.head.as_str().to_string();
    let args = expr.args_snapshot();
    // `>:` (supertype) has no dedicated `ExprHead` variant, so a macro returning a
    // standalone `Expr(:>:, a, b)` — the quote path now emits the operator head
    // (Issue #7863) instead of `Expr(:call, :>:, ...)` — would otherwise fall into
    // the unsupported-head error below. Lower it like source `A >: B`, i.e. the
    // swapped subtype check `B <: A` (mirrors lower_binary), so it evaluates as a
    // `BinaryOp::Subtype` rather than an unknown `>:` function (regression guard for
    // #7870; symmetric to the `<:` arm).
    if head_name == ">:" && args.len() == 2 {
        let left = value_to_expr(args[0].clone(), span, walker, lambda_ctx)?;
        let right = value_to_expr(args[1].clone(), span, walker, lambda_ctx)?;
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Subtype,
            left: Box::new(right),
            right: Box::new(left),
            span,
        });
    }
    let Some(head) = ExprHead::from_name(&head_name) else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported Expr head :{}",
                head_name
            )),
        );
    };
    debug_assert_eq!(
        head.spec().macro_return_to_expr,
        macro_return_expr_support(head)
    );
    match head {
        ExprHead::Escape | ExprHead::HygienicScope if !args.is_empty() => {
            // Identifiers inside `esc(...)` resolve in the caller scope, so module
            // member qualification is suppressed for this subtree (Issue #7355).
            lambda_ctx.enter_macro_esc();
            let r = value_to_expr(args[0].clone(), span, walker, lambda_ctx);
            lambda_ctx.exit_macro_esc();
            r
        }
        ExprHead::Call => call_expr_from_values(args, span, walker, lambda_ctx),
        // String interpolation round-tripped through the quote path (Issue #7029):
        // `Expr(:string, parts...)` becomes a `StringConcat`, matching the IR
        // `lower_string_literal` produces for `"…$x…"`.
        ExprHead::String => {
            let parts = values_to_exprs(args, span, walker, lambda_ctx)?;
            if parts.is_empty() {
                return Ok(Expr::Literal(Literal::Str(String::new()), span));
            }
            Ok(Expr::StringConcat { parts, span })
        }
        ExprHead::Assign if args.len() == 2 => {
            assignment_expr_from_values(args, span, walker, lambda_ctx)
        }
        ExprHead::TypeAssert if args.len() == 2 => Ok(Expr::Call {
            function: "typeassert".to_string(),
            args: values_to_exprs(args, span, walker, lambda_ctx)?,
            kwargs: Vec::new(),
            splat_mask: vec![false, false],
            kwargs_splat_mask: Vec::new(),
            span,
        }),
        // A macro returning a standalone subtype constraint `Expr(:<:, a, b)`. The
        // quote path now emits the operator head (Issue #7863) instead of
        // `Expr(:call, :<:, ...)`, so the converter must lower it like source
        // `a <: b` — a `BinaryOp::Subtype` (not an unknown `<:` function call), so it
        // evaluates to the subtype test matching upstream. Regression guard for #7870
        // — before this arm the converter rejected `:<:` with "macro expansion
        // returned unsupported Expr head :<:". (`:>:` is handled above since it has
        // no `ExprHead` variant.)
        ExprHead::Subtype if args.len() == 2 => Ok(Expr::BinaryOp {
            op: BinaryOp::Subtype,
            left: Box::new(value_to_expr(args[0].clone(), span, walker, lambda_ctx)?),
            right: Box::new(value_to_expr(args[1].clone(), span, walker, lambda_ctx)?),
            span,
        }),
        ExprHead::Where if args.len() >= 2 => {
            where_expr_from_values(args, span, walker, lambda_ctx)
        }
        ExprHead::Tuple => tuple_expr_from_values(args, span, walker, lambda_ctx),
        ExprHead::Vect => {
            let elements = values_to_exprs(args, span, walker, lambda_ctx)?;
            Ok(Expr::ArrayLiteral {
                shape: vec![elements.len()],
                elements,
                span,
            })
        }
        ExprHead::Row => concat_expr_from_values("hcat", args, span, walker, lambda_ctx),
        ExprHead::Hcat => concat_expr_from_values("hcat", args, span, walker, lambda_ctx),
        ExprHead::Vcat => concat_expr_from_values("vcat", args, span, walker, lambda_ctx),
        ExprHead::Ref if !args.is_empty() => {
            let mut lowered = values_to_exprs(args, span, walker, lambda_ctx)?;
            let array = lowered.remove(0);
            Ok(Expr::Index {
                array: Box::new(array),
                indices: lowered,
                span,
            })
        }
        // A `block` Expr in expression position yields the value of its last
        // statement; a single-statement block lowers that statement as an
        // expression so a trailing `if`/ternary still produces a value
        // (Issue #7350 A1), mirroring `lower_block_as_expr` for source blocks.
        ExprHead::Block => value_to_branch_expr(Value::Expr(expr), span, walker, lambda_ctx),
        ExprHead::Local => {
            let stmts = args
                .into_iter()
                .map(|arg| value_to_stmt(arg, span, walker, lambda_ctx))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok(Expr::LetBlock {
                bindings: vec![],
                body: Block { stmts, span },
                span,
            })
        }
        // An `if` Expr used in *expression* position must yield the value of the
        // taken branch (Julia semantics), so lower it to a value-producing
        // `Expr::Ternary` rather than a `Stmt::If` whose value would be discarded
        // (which silently returned `nothing` — Issue #7350 A1). The statement-position
        // path (`value_to_stmt`) still produces a `Stmt::If`.
        ExprHead::If | ExprHead::ElseIf if args.len() >= 2 && args.len() <= 3 => {
            Ok(Expr::Ternary {
                condition: Box::new(if head == ExprHead::ElseIf {
                    value_to_branch_expr(args[0].clone(), span, walker, lambda_ctx)?
                } else {
                    value_to_expr(args[0].clone(), span, walker, lambda_ctx)?
                }),
                then_expr: Box::new(value_to_branch_expr(
                    args[1].clone(),
                    span,
                    walker,
                    lambda_ctx,
                )?),
                else_expr: Box::new(if args.len() == 3 {
                    value_to_branch_expr(args[2].clone(), span, walker, lambda_ctx)?
                } else {
                    Expr::Literal(Literal::Nothing, span)
                }),
                span,
            })
        }
        ExprHead::Const => Ok(Expr::LetBlock {
            bindings: vec![],
            body: Block {
                stmts: vec![const_stmt_from_args(args, span, walker, lambda_ctx)?],
                span,
            },
            span,
        }),
        ExprHead::Export => {
            validate_symbol_declaration_args(&args, "export", span)?;
            Ok(Expr::Literal(Literal::Nothing, span))
        }
        ExprHead::Public => {
            validate_symbol_declaration_args(&args, "public", span)?;
            Ok(Expr::Literal(Literal::Nothing, span))
        }
        ExprHead::For if args.len() == 2 => {
            let (var, iterable) =
                for_binding_from_value(args[0].clone(), span, walker, lambda_ctx)?;
            Ok(Expr::LetBlock {
                bindings: vec![],
                body: Block {
                    stmts: vec![Stmt::ForEach {
                        var,
                        iterable,
                        body: value_to_block(args[1].clone(), span, walker, lambda_ctx)?,
                        span,
                    }],
                    span,
                },
                span,
            })
        }
        ExprHead::Let => let_expr_from_args(args, span, walker, lambda_ctx),
        ExprHead::Try => try_expr_from_values(args, span, walker, lambda_ctx),
        ExprHead::MacroCall => expand_macrocall_value(args, span, walker, lambda_ctx),
        ExprHead::Arrow if args.len() == 2 => {
            arrow_expr_from_values(args, span, walker, lambda_ctx)
        }
        ExprHead::Curly if !args.is_empty() => {
            if let Some(type_name) = static_curly_type_name(&expr, span, lambda_ctx)? {
                Ok(Expr::Builtin {
                    name: BuiltinOp::TypeOf,
                    args: vec![Expr::Literal(Literal::Str(type_name), span)],
                    span,
                })
            } else {
                curly_expr_from_values(args, span, walker, lambda_ctx)
            }
        }
        // A macro returning a `where` type (`Expr(:where, body, var...)`) must bind
        // each introduced inner type variable as a runtime `TypeVar(:var)` fed to
        // `UnionAll`, while still resolving caller-bound type params in the body
        // dynamically (Issue #7844). Mirrors the source value-position `where`
        // lowering (`lower_where_expression_value`): leftmost variable becomes the
        // outermost `UnionAll`. Unlike the source path — which freezes the body as a
        // static `TypeOf(...)` string so free vars parse to `TypeVar`s — the
        // macro-return body may reference a caller binding (`T`) that only exists at
        // runtime, so we bind the introduced vars in a `let` and lower the body
        // through the same curly/`DynamicTypeConstruct` machinery, letting `T`
        // resolve as a caller `Var` and the introduced `S` resolve to its `TypeVar`.
        ExprHead::Where if args.len() >= 2 => {
            where_expr_from_values(args, span, walker, lambda_ctx)
        }
        ExprHead::Interpolation if args.len() == 1 => {
            // MacroTools @q can return interpolation nodes directly after nested
            // template expansion. Preserve caller-scope runtime/local bindings
            // instead of treating Expr(:$, x) as an unsupported Expr head
            // (Issues #7541/#7542).
            value_to_expr(args[0].clone(), span, walker, lambda_ctx)
        }
        ExprHead::Splat if args.len() == 1 => {
            // A splatted interpolation can also reach expression conversion as
            // Expr(:..., x) after MacroTools nested template expansion. When no
            // containing call/tuple is available to carry a splat mask, preserve
            // the caller expression rather than rejecting the expansion
            // (Issue #7541).
            value_to_expr(args[0].clone(), span, walker, lambda_ctx)
        }
        ExprHead::Adjoint if args.len() == 1 => Ok(Expr::Call {
            function: "adjoint".to_string(),
            args: vec![value_to_expr(args[0].clone(), span, walker, lambda_ctx)?],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        ExprHead::Return => Ok(Expr::ReturnExpr {
            value: if let Some(value) = args.first() {
                Some(Box::new(value_to_expr(
                    value.clone(),
                    span,
                    walker,
                    lambda_ctx,
                )?))
            } else {
                None
            },
            span,
        }),
        ExprHead::Quote if args.len() == 1 => {
            quote_expr_value_to_expr(args[0].clone(), span, walker, lambda_ctx)
        }
        // Field access `obj.field` round-trips as `Expr(:., obj, QuoteNode(:field))`
        // (or a bare `Symbol(:field)`). Needed so a macro-spliced loop body can read
        // struct fields, e.g. `@animate for … push!(plt, l.x, l.y, l.z) end` in the
        // Lorenz-attractor sample (Issues #7270/#7271/#7272).
        ExprHead::Dot if args.len() == 2 => {
            let object = value_to_expr(args[0].clone(), span, walker, lambda_ctx)?;
            let field = match &args[1] {
                Value::QuoteNode(inner) => symbol_arg(inner, span)?,
                other => symbol_arg(other, span)?,
            };
            Ok(Expr::FieldAccess {
                object: Box::new(object),
                field,
                span,
            })
        }
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported Expr head :{}",
                head_name
            )),
        ),
    }
}

fn macro_return_expr_support(head: ExprHead) -> bool {
    matches!(
        head,
        ExprHead::Escape
            | ExprHead::HygienicScope
            | ExprHead::Call
            | ExprHead::String
            | ExprHead::Assign
            | ExprHead::TypeAssert
            | ExprHead::Subtype
            | ExprHead::Where
            | ExprHead::Tuple
            | ExprHead::Vect
            | ExprHead::Row
            | ExprHead::Hcat
            | ExprHead::Vcat
            | ExprHead::Ref
            | ExprHead::Block
            | ExprHead::Local
            | ExprHead::If
            | ExprHead::ElseIf
            | ExprHead::Const
            | ExprHead::Public
            | ExprHead::For
            | ExprHead::Let
            | ExprHead::Try
            | ExprHead::MacroCall
            | ExprHead::Arrow
            | ExprHead::Curly
            | ExprHead::Interpolation
            | ExprHead::Splat
            | ExprHead::Adjoint
            | ExprHead::Return
            | ExprHead::Quote
            | ExprHead::Dot
    )
}

fn concat_expr_from_values<'a>(
    function: &str,
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let lowered_args = values_to_exprs(args, span, walker, lambda_ctx)?;
    let splat_mask = vec![false; lowered_args.len()];
    Ok(Expr::Call {
        function: function.to_string(),
        args: lowered_args,
        kwargs: Vec::new(),
        splat_mask,
        kwargs_splat_mask: Vec::new(),
        span,
    })
}

fn tuple_expr_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    // Named tuple syntax round-trips through Julia AST as
    // `Expr(:tuple, Expr(:(=), :name, value), ...)` (Issue #7765).
    if !args.is_empty() {
        let mut fields = Vec::with_capacity(args.len());
        let mut all_named_fields = true;
        for arg in &args {
            if matches!(arg, Value::LineNumberNode(_)) {
                continue;
            }
            let Value::Expr(assign) = arg else {
                all_named_fields = false;
                break;
            };
            if !ExprHead::is_expr(assign, ExprHead::Assign) {
                all_named_fields = false;
                break;
            }
            let assign_args = assign.args_snapshot();
            if assign_args.len() != 2 {
                all_named_fields = false;
                break;
            }
            let target = macro_assignment_target(&assign_args[0]);
            let Value::Symbol(name) = target else {
                all_named_fields = false;
                break;
            };
            fields.push((
                name.as_str().to_string(),
                value_to_expr(assign_args[1].clone(), span, walker, lambda_ctx)?,
            ));
        }
        if all_named_fields && !fields.is_empty() {
            return Ok(Expr::NamedTupleLiteral { fields, span });
        }
    }

    Ok(Expr::TupleLiteral {
        elements: values_to_exprs(args, span, walker, lambda_ctx)?,
        span,
    })
}

/// Lower an assignment used in *expression* position (`Expr(:call, :(=), target,
/// rhs)`) to a value-producing `Expr` that performs the assignment and yields the
/// RHS value, matching Julia's assignment-expression semantics (Issue #7900).
/// Symbol targets become a plain assignment expression; tuple targets destructure
/// via the shared CST machinery, binding the RHS to a temporary first so the
/// expression still evaluates to the whole RHS tuple.
fn assignment_value_expr_from_values<'a>(
    target: &Value,
    rhs: &Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let target_value = macro_assignment_target(target);
    if let Value::Symbol(name) = &target_value {
        return Ok(Expr::AssignExpr {
            var: name.as_str().to_string(),
            value: Box::new(value_to_expr(rhs.clone(), span, walker, lambda_ctx)?),
            span,
        });
    }
    if let Value::Expr(tuple) = &target_value {
        if ExprHead::is_expr(tuple, ExprHead::Tuple) {
            let patterns = tuple
                .args_snapshot()
                .iter()
                .filter(|arg| !matches!(arg, Value::LineNumberNode(_)))
                .map(|arg| macro_destructure_target(arg, span))
                .collect::<LowerResult<Vec<_>>>()?;
            let rhs_expr = value_to_expr(rhs.clone(), span, walker, lambda_ctx)?;
            let tmp = format!("__macro_destructure_value_{}_{}", span.start, span.end);
            let bind = Stmt::Assign {
                var: tmp.clone(),
                value: rhs_expr,
                span,
            };
            let destructure =
                lower_destructuring_from_targets(patterns, Expr::Var(tmp.clone(), span), span)?;
            return Ok(Expr::LetBlock {
                bindings: vec![],
                body: Block {
                    stmts: vec![
                        bind,
                        destructure,
                        Stmt::Expr {
                            expr: Expr::Var(tmp, span),
                            span,
                        },
                    ],
                    span,
                },
                span,
            });
        }
    }
    // Indexed / field / other targets are handled by the statement path; in
    // expression position fall back to the existing handler.
    assignment_expr_from_values(vec![target.clone(), rhs.clone()], span, walker, lambda_ctx)
}

fn assignment_expr_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned malformed assignment expression"),
        );
    }

    let target = macro_assignment_target(&args[0]);
    if let Value::Symbol(name) = target {
        return Ok(Expr::AssignExpr {
            var: name.as_str().to_string(),
            value: Box::new(value_to_expr(args[1].clone(), span, walker, lambda_ctx)?),
            span,
        });
    }

    if let Value::Expr(target_expr) = target {
        if ExprHead::is_expr(&target_expr, ExprHead::Ref) {
            let target_args = target_expr.args_snapshot();
            let Some((array_value, index_values)) = target_args.split_first() else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("macro expansion returned empty indexed assignment target"),
                );
            };

            let tmp = format!("__macro_assign_value_{}_{}", span.start, span.end);
            let indices = values_to_exprs(index_values.to_vec(), span, walker, lambda_ctx)?;
            let write_stmt = match array_value {
                Value::Symbol(array_name) => Stmt::IndexAssign {
                    array: array_name.as_str().to_string(),
                    indices,
                    value: Expr::Var(tmp.clone(), span),
                    span,
                },
                _ => {
                    let mut call_args = vec![
                        value_to_expr(array_value.clone(), span, walker, lambda_ctx)?,
                        Expr::Var(tmp.clone(), span),
                    ];
                    call_args.extend(indices);
                    Stmt::Expr {
                        expr: Expr::Call {
                            function: "setindex!".to_string(),
                            args: call_args,
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        },
                        span,
                    }
                }
            };

            return Ok(Expr::LetBlock {
                bindings: vec![(
                    tmp.clone(),
                    value_to_expr(args[1].clone(), span, walker, lambda_ctx)?,
                )],
                body: Block {
                    stmts: vec![
                        write_stmt,
                        Stmt::Expr {
                            expr: Expr::Var(tmp, span),
                            span,
                        },
                    ],
                    span,
                },
                span,
            });
        }
    }

    Err(
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned unsupported assignment expression target"),
    )
}

fn curly_expr_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let (base_value, param_values) = args.split_first().ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned empty Expr(:curly, ...)")
    })?;

    let (base, base_expr) = match base_value {
        Value::Symbol(sym) => (sym.as_str().to_string(), None),
        Value::GlobalRef(GlobalRefValue { module, name }) => (
            name.as_str().to_string(),
            Some(Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Var(module.as_str().to_string(), span)),
                field: name.as_str().to_string(),
                span,
            })),
        ),
        other => (
            "__dynamic_type_base".to_string(),
            Some(Box::new(value_to_expr(
                other.clone(),
                span,
                walker,
                lambda_ctx,
            )?)),
        ),
    };

    let mut type_args = Vec::with_capacity(param_values.len());
    let mut splat_mask = Vec::with_capacity(param_values.len());
    for value in param_values {
        if let Value::Expr(expr) = value {
            if ExprHead::is_expr(expr, ExprHead::Splat) {
                let expr_args = expr.args_snapshot();
                if expr_args.len() == 1 {
                    type_args.push(value_to_expr(
                        expr_args[0].clone(),
                        span,
                        walker,
                        lambda_ctx,
                    )?);
                    splat_mask.push(true);
                    continue;
                }
            }
        }
        type_args.push(value_to_expr(value.clone(), span, walker, lambda_ctx)?);
        splat_mask.push(false);
    }

    let splat_mask = if splat_mask.iter().any(|is_splat| *is_splat) {
        splat_mask
    } else {
        Vec::new()
    };

    Ok(Expr::DynamicTypeConstruct {
        base,
        base_expr,
        type_args,
        splat_mask,
        span,
    })
}

/// Convert a macro-returned `Expr(:where, body, var...)` into a value-position
/// `UnionAll` chain (Issue #7844).
///
/// Each introduced type variable (`var` — a bare `Symbol(:S)` or a bound form
/// `Expr(:<:, :S, Bound)` / `Expr(:>:, :S, Bound)` / `Expr(:comparison, Lower,
/// :<:, :S, :<:, Upper)`) is bound in a `let` to a runtime
/// `TypeVar(:S[, lower, upper])` value, then the body — lowered through the
/// ordinary curly/`DynamicTypeConstruct` path — references those bindings as
/// `Var`s. Caller-bound type params in the body (e.g. a `where T` from the
/// enclosing method) keep resolving dynamically because they are plain `Var`s
/// too. The leftmost where-variable becomes the OUTERMOST `UnionAll`, matching
/// upstream (`A{T,S} where {T,S}` == `... where S where T`).
fn where_expr_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let (body_value, var_values) = args.split_first().ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned empty Expr(:where, ...)")
    })?;
    if var_values.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned Expr(:where, ...) without type variables"),
        );
    }

    // Each entry: (typevar name, TypeVar(...) constructor expr).
    let mut bindings: Vec<(String, Expr)> = Vec::with_capacity(var_values.len());
    for var in var_values {
        bindings.push(where_typevar_binding(var, span, walker, lambda_ctx)?);
    }

    // Body is lowered through the ordinary expression path so the introduced
    // TypeVars (now `let`-bound names) and caller-bound params both resolve as
    // runtime `Var`s.
    let mut body_expr = value_to_expr(body_value.clone(), span, walker, lambda_ctx)?;

    // Wrap innermost-first: reverse so the first-listed variable is outermost.
    for (name, _) in bindings.iter().rev() {
        body_expr = Expr::Call {
            function: "UnionAll".to_string(),
            args: vec![Expr::Var(name.clone(), span), body_expr],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
    }

    Ok(Expr::LetBlock {
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

/// Build a `(name, TypeVar(...))` binding for a single macro-returned `where`
/// type variable. Supports the bare (`:S`), upper-bound (`Expr(:<:, :S, U)`),
/// lower-bound (`Expr(:>:, :S, L)`), and two-sided
/// (`Expr(:comparison, L, :<:, :S, :<:, U)`) forms. Bounds are lowered through
/// the ordinary expression path so caller-scope type names resolve dynamically.
///
/// NOTE: the bounded arms are currently unreachable for a *quoted* `where`
/// because the CST→Expr quote path flattens `S<:Real` into separate bare symbol
/// args rather than a single `Expr(:<:, :S, :Real)` (Issue #7845). They are kept
/// so the bounded macro-return case works as soon as that quote bug is fixed,
/// and to mirror the upstream Julia `Expr(:where, ...)` variable shapes.
fn where_typevar_binding<'a>(
    var: &Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(String, Expr)> {
    let bottom = || Expr::Builtin {
        name: BuiltinOp::TypeOf,
        args: vec![Expr::Literal(
            Literal::Str(crate::types::JuliaType::Bottom.name().into_owned()),
            span,
        )],
        span,
    };
    let any = || Expr::Builtin {
        name: BuiltinOp::TypeOf,
        args: vec![Expr::Literal(
            Literal::Str(crate::types::JuliaType::Any.name().into_owned()),
            span,
        )],
        span,
    };
    let make_typevar = |name: &str, lower: Option<Expr>, upper: Option<Expr>| {
        let mut call_args = vec![Expr::Literal(Literal::Symbol(name.to_string()), span)];
        if lower.is_some() || upper.is_some() {
            call_args.push(lower.unwrap_or_else(bottom));
            call_args.push(upper.unwrap_or_else(any));
        }
        Expr::Call {
            function: "TypeVar".to_string(),
            args: call_args,
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        }
    };

    match var {
        Value::Symbol(sym) => {
            let name = sym.as_str().to_string();
            Ok((name.clone(), make_typevar(&name, None, None)))
        }
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Subtype) => {
            // `S <: Upper`
            let parts = expr.args_snapshot();
            if parts.len() == 2 {
                let name = symbol_arg(&parts[0], span)?;
                let upper = value_to_expr(parts[1].clone(), span, walker, lambda_ctx)?;
                return Ok((name.clone(), make_typevar(&name, None, Some(upper))));
            }
            Err(unsupported_where_var(span))
        }
        Value::Expr(expr) if expr.head.as_str() == ">:" => {
            // `S >: Lower`
            let parts = expr.args_snapshot();
            if parts.len() == 2 {
                let name = symbol_arg(&parts[0], span)?;
                let lower = value_to_expr(parts[1].clone(), span, walker, lambda_ctx)?;
                return Ok((name.clone(), make_typevar(&name, Some(lower), None)));
            }
            Err(unsupported_where_var(span))
        }
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Comparison) => {
            // `Lower <: S <: Upper`
            let parts = expr.args_snapshot();
            if parts.len() == 5 {
                if let (Value::Symbol(op1), Value::Symbol(op2)) = (&parts[1], &parts[3]) {
                    if op1.as_str() == "<:" && op2.as_str() == "<:" {
                        let name = symbol_arg(&parts[2], span)?;
                        let lower = value_to_expr(parts[0].clone(), span, walker, lambda_ctx)?;
                        let upper = value_to_expr(parts[4].clone(), span, walker, lambda_ctx)?;
                        return Ok((name.clone(), make_typevar(&name, Some(lower), Some(upper))));
                    }
                }
            }
            Err(unsupported_where_var(span))
        }
        _ => Err(unsupported_where_var(span)),
    }
}

fn unsupported_where_var(span: Span) -> UnsupportedFeature {
    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
        .with_hint("macro expansion returned unsupported Expr(:where, ...) type variable")
}

fn quote_expr_value_to_expr<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    match value {
        Value::ExprArgs(args) => quote_array_ref_constructor(&args, span, walker, lambda_ctx),
        other => quote_value_constructor(other, span, walker, lambda_ctx),
    }
}

fn quote_value_constructor<'a>(
    value: Value,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    match value {
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Interpolation) => {
            let args = expr.args_snapshot();
            if args.len() == 1 {
                // Expr(:quote, Expr(:$, x)) is the value form produced by macros
                // such as `esc(Expr(:quote, ex))` when `ex` is `$y`. Lower the
                // interpolation in the caller context instead of freezing it as a
                // literal Expr(:$, ...) (Issue #7542).
                return value_to_expr(args[0].clone(), span, walker, lambda_ctx);
            }
            Ok(Expr::Literal(
                value_to_literal(Value::Expr(expr), span)?,
                span,
            ))
        }
        Value::Expr(expr) => {
            let mut args = vec![symbol_constructor(expr.head.as_str(), span)];
            for arg in expr.args_snapshot() {
                args.push(quote_value_constructor(arg, span, walker, lambda_ctx)?);
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }
        Value::QuoteNode(inner) => Ok(Expr::Builtin {
            name: BuiltinOp::QuoteNodeNew,
            args: vec![quote_value_constructor(*inner, span, walker, lambda_ctx)?],
            span,
        }),
        other => Ok(Expr::Literal(value_to_literal(other, span)?, span)),
    }
}

fn quote_array_ref_constructor<'a>(
    array_ref: &crate::vm::ArrayRef,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let array = array_ref.borrow();
    let elements = (0..array.data.raw_len())
        .map(|idx| {
            let value = array.data.get_value(idx).ok_or_else(|| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed quoted array")
            })?;
            quote_value_constructor(value, span, walker, lambda_ctx)
        })
        .collect::<LowerResult<Vec<_>>>()?;
    Ok(Expr::Index {
        array: Box::new(Expr::Var("Any".to_string(), span)),
        indices: elements,
        span,
    })
}

fn arrow_expr_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let mut iter = args.into_iter();
    let params_value = iter.next().ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned malformed arrow function")
    })?;
    let body_value = iter.next().ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned malformed arrow function")
    })?;

    let params = arrow_params_from_value(params_value, span)?;
    let body_expr = value_to_branch_expr(body_value, span, walker, lambda_ctx)?;
    let lambda_name = lambda_ctx.next_lambda_name();
    lambda_ctx.add_lifted_function(Function {
        name: lambda_name.clone(),
        params,
        kwparams: Vec::new(),
        type_params: Vec::new(),
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body_expr),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
    });

    Ok(Expr::FunctionRef {
        name: lambda_name,
        span,
    })
}

fn arrow_params_from_value(value: Value, span: Span) -> LowerResult<Vec<TypedParam>> {
    match value {
        Value::Symbol(name) => Ok(vec![TypedParam::untyped(name.as_str().to_string(), span)]),
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Tuple) => expr
            .args_snapshot()
            .into_iter()
            .map(|arg| match arg {
                Value::Symbol(name) => Ok(TypedParam::untyped(name.as_str().to_string(), span)),
                other => Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                        format!(
                            "macro expansion returned unsupported arrow parameter {:?}",
                            other.value_type()
                        ),
                    ),
                ),
            })
            .collect(),
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported arrow parameter list {:?}",
                other.value_type()
            )),
        ),
    }
}

fn call_expr_from_values<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let (callee, rest) = args.split_first().ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint("macro expansion returned empty call Expr")
    })?;
    // An assignment used in *expression* position arrives from the macro-arg
    // constructor as `Expr(:call, :(=), target, rhs)`. Julia evaluates an
    // assignment expression to its RHS value, so lower it to a value-producing
    // form (assign, then yield the value) rather than a call to the `=` operator,
    // which errored "Unknown function: =" (Issue #7900). Tuple targets destructure;
    // a bare symbol target is a plain assignment expression.
    if let Value::Symbol(op) = callee {
        if op.as_str() == "=" && rest.len() == 2 {
            return assignment_value_expr_from_values(&rest[0], &rest[1], span, walker, lambda_ctx);
        }
    }
    // Split positional from keyword args. A keyword arg round-trips through the
    // quote path as an `Expr(:kw, :name, value)` value (Issue #7029); pull those
    // back out so e.g. `@gif`/`@animate` over `plot(...; title=...)` re-lowers with
    // its keyword intact.
    let mut lowered_args = Vec::new();
    let mut splat_mask = Vec::new();
    let mut kwargs: Vec<(String, Expr)> = Vec::new();
    let mut kwargs_splat_mask = Vec::new();
    for value in rest {
        if let Value::Expr(e) = value {
            let expr_args = e.args_snapshot();
            if ExprHead::is_expr(e, ExprHead::Parameters) {
                for param in expr_args {
                    let Value::Expr(kw) = param else {
                        return Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::MacroCall,
                            span,
                        )
                        .with_hint(
                            "macro expansion returned Expr(:parameters) with non-keyword entry",
                        ));
                    };
                    let kw_args = kw.args_snapshot();
                    if ExprHead::is_expr(&kw, ExprHead::Splat) && kw_args.len() == 1 {
                        let val = value_to_expr(kw_args[0].clone(), span, walker, lambda_ctx)?;
                        kwargs.push(("".to_string(), val));
                        kwargs_splat_mask.push(true);
                        continue;
                    }
                    if !ExprHead::is_expr(&kw, ExprHead::Kw) || kw_args.len() != 2 {
                        return Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::MacroCall,
                            span,
                        )
                        .with_hint(
                            "macro expansion returned Expr(:parameters) with malformed keyword",
                        ));
                    }
                    let name = symbol_arg(&kw_args[0], span)?;
                    let val = value_to_expr(kw_args[1].clone(), span, walker, lambda_ctx)?;
                    kwargs.push((name, val));
                    kwargs_splat_mask.push(false);
                }
                continue;
            }
            if ExprHead::is_expr(e, ExprHead::Kw) && expr_args.len() == 2 {
                let name = symbol_arg(&expr_args[0], span)?;
                let val = value_to_expr(expr_args[1].clone(), span, walker, lambda_ctx)?;
                kwargs.push((name, val));
                kwargs_splat_mask.push(false);
                continue;
            }
            if ExprHead::is_expr(e, ExprHead::Splat) && expr_args.len() == 1 {
                lowered_args.push(value_to_expr(
                    expr_args[0].clone(),
                    span,
                    walker,
                    lambda_ctx,
                )?);
                splat_mask.push(true);
                continue;
            }
        }
        lowered_args.push(value_to_expr(value.clone(), span, walker, lambda_ctx)?);
        splat_mask.push(false);
    }
    match callee {
        Value::Symbol(function) => {
            if function.as_str() == "=>"
                && lowered_args.len() == 2
                && kwargs.is_empty()
                && splat_mask.iter().all(|splat| !*splat)
            {
                let mut args = lowered_args;
                let value = args.pop().expect("len checked");
                let key = args.pop().expect("len checked");
                return Ok(Expr::Pair {
                    key: Box::new(key),
                    value: Box::new(value),
                    span,
                });
            }
            if kwargs.is_empty() && splat_mask.iter().all(|splat| !*splat) {
                if let Some(name) = crate::lowering::expr::map_builtin_name(function.as_str()) {
                    return Ok(Expr::Builtin {
                        name,
                        args: lowered_args,
                        span,
                    });
                }
            }
            // A non-`esc` call target naming a member of the macro's defining
            // module is qualified `M.f` so unexported members resolve there rather
            // than in the caller scope (Issue #7355 / #7350 A4).
            let module = lambda_ctx.qualify_module_macro_member(function.as_str());
            call_named_expr(
                module.as_deref(),
                function.as_str(),
                lowered_args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            )
        }
        Value::GlobalRef(GlobalRefValue { module, name }) => call_named_expr(
            Some(module.as_str()),
            name.as_str(),
            lowered_args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        ),
        Value::DataType(ty) => {
            let name = ty.name().to_string();
            call_named_expr(
                None,
                &name,
                lowered_args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            )
        }
        // Module-qualified call target `Mod.f(...)`, round-tripped as a
        // `Expr(:., Mod, QuoteNode(:f))` callee. Without this the macro-expanded
        // qualified call errored "unsupported call target Expr" (Issue #7350 A3).
        Value::Expr(e) if ExprHead::is_expr(e, ExprHead::Dot) => {
            let dot_args = e.args_snapshot();
            if dot_args.len() == 2 {
                let module = symbol_arg(&dot_args[0], span)?;
                let name = match &dot_args[1] {
                    Value::QuoteNode(inner) => symbol_arg(inner, span)?,
                    other => symbol_arg(other, span)?,
                };
                return call_named_expr(
                    Some(&module),
                    &name,
                    lowered_args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                );
            }
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed qualified call target"),
            )
        }
        Value::Expr(e)
            if matches!(
                ExprHead::from_expr(e),
                Some(ExprHead::Curly | ExprHead::ParametrizedTypeExpression)
            ) && kwargs.is_empty()
                && splat_mask.iter().all(|splat| !*splat) =>
        {
            if let Some(type_name) = static_curly_type_name(e, span, lambda_ctx)? {
                return Ok(Expr::Call {
                    function: type_name,
                    args: lowered_args,
                    kwargs: Vec::new(),
                    splat_mask: Vec::new(),
                    kwargs_splat_mask: Vec::new(),
                    span,
                });
            }
            let callee_expr = value_to_expr(Value::Expr(e.clone()), span, walker, lambda_ctx)?;
            Ok(indirect_call_expr(
                format!("__macro_call_target_{}", span.start),
                callee_expr,
                lowered_args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            ))
        }
        Value::Expr(e) => {
            let callee_expr = value_to_expr(Value::Expr(e.clone()), span, walker, lambda_ctx)?;
            Ok(indirect_call_expr(
                format!("__macro_call_target_{}", span.start),
                callee_expr,
                lowered_args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            ))
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported call target {:?}",
                other.value_type()
            )),
        ),
    }
}

fn static_curly_type_name(
    expr: &ExprValue,
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<String>> {
    static_curly_type_name_with_static_symbols(expr, span, lambda_ctx, &[])
}

fn static_curly_type_name_with_static_symbols(
    expr: &ExprValue,
    span: Span,
    lambda_ctx: &LambdaContext,
    static_symbols: &[&str],
) -> LowerResult<Option<String>> {
    let args = expr.args_snapshot();
    let Some(base_value) = args.first() else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned empty parametric call target"),
        );
    };
    let base = match base_value {
        Value::Symbol(sym) => {
            if static_symbols.contains(&sym.as_str()) {
                sym.as_str().to_string()
            } else if lambda_ctx.is_active_type_param(sym.as_str()) {
                return Ok(None);
            } else {
                sym.as_str().to_string()
            }
        }
        Value::GlobalRef(GlobalRefValue { module, name }) => {
            format!("{}.{}", module.as_str(), name.as_str())
        }
        _ => return Ok(None),
    };

    let mut params = Vec::with_capacity(args.len().saturating_sub(1));
    for param in &args[1..] {
        let Some(rendered) =
            static_type_param_name_with_static_symbols(param, span, lambda_ctx, static_symbols)?
        else {
            return Ok(None);
        };
        params.push(rendered);
    }

    if params.is_empty() {
        Ok(Some(base))
    } else {
        Ok(Some(format!("{}{{{}}}", base, params.join(", "))))
    }
}

fn static_type_param_name_with_static_symbols(
    value: &Value,
    span: Span,
    lambda_ctx: &LambdaContext,
    static_symbols: &[&str],
) -> LowerResult<Option<String>> {
    match value {
        Value::Symbol(sym) => {
            Ok(
                static_type_symbol_name(sym.as_str(), lambda_ctx, static_symbols)?
                    .map(str::to_string),
            )
        }
        Value::GlobalRef(GlobalRefValue { module, name }) => {
            Ok(Some(format!("{}.{}", module.as_str(), name.as_str())))
        }
        Value::DataType(ty) => Ok(Some(ty.name().to_string())),
        Value::I8(n) => Ok(Some(n.to_string())),
        Value::I16(n) => Ok(Some(n.to_string())),
        Value::I32(n) => Ok(Some(n.to_string())),
        Value::I64(n) => Ok(Some(n.to_string())),
        Value::I128(n) => Ok(Some(n.to_string())),
        Value::BigInt(n) => Ok(Some(n.to_string())),
        Value::U8(n) => Ok(Some(n.to_string())),
        Value::U16(n) => Ok(Some(n.to_string())),
        Value::U32(n) => Ok(Some(n.to_string())),
        Value::U64(n) => Ok(Some(n.to_string())),
        Value::U128(n) => Ok(Some(n.to_string())),
        Value::Bool(b) => Ok(Some(b.to_string())),
        Value::Char(c) => Ok(Some(format!("'{}'", c))),
        Value::Str(s) => Ok(Some(format!("\"{}\"", s))),
        Value::QuoteNode(inner) => match inner.as_ref() {
            Value::Symbol(sym) => Ok(Some(format!(":{}", sym.as_str()))),
            other => {
                static_type_param_name_with_static_symbols(other, span, lambda_ctx, static_symbols)
            }
        },
        Value::Expr(expr)
            if matches!(
                ExprHead::from_expr(expr),
                Some(ExprHead::Curly | ExprHead::ParametrizedTypeExpression)
            ) =>
        {
            static_curly_type_name_with_static_symbols(expr, span, lambda_ctx, static_symbols)
        }
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Tuple) => {
            let mut params = Vec::new();
            for arg in expr.args_snapshot() {
                let Some(rendered) = static_type_param_name_with_static_symbols(
                    &arg,
                    span,
                    lambda_ctx,
                    static_symbols,
                )?
                else {
                    return Ok(None);
                };
                params.push(rendered);
            }
            Ok(Some(format!("({})", params.join(", "))))
        }
        _ => Ok(None),
    }
}

fn static_type_symbol_name<'a>(
    name: &'a str,
    lambda_ctx: &LambdaContext,
    static_symbols: &[&str],
) -> LowerResult<Option<&'a str>> {
    if static_symbols.contains(&name) {
        return Ok(Some(name));
    }
    if lambda_ctx.is_active_type_param(name) {
        return Ok(None);
    }
    if macro_return_static_type_symbol(name) || crate::types::JuliaType::from_name(name).is_some() {
        Ok(Some(name))
    } else {
        Ok(None)
    }
}

fn macro_return_static_type_symbol(name: &str) -> bool {
    matches!(
        name,
        "Union"
            | "Ptr"
            | "Val"
            | "Vararg"
            | "NTuple"
            | "NamedTuple"
            | "Ref"
            | "RefValue"
            | "Memory"
            | "MemoryRef"
    )
}

fn indirect_call_expr(
    temp_name: String,
    callee_expr: Expr,
    args: Vec<Expr>,
    kwargs: Vec<(String, Expr)>,
    splat_mask: Vec<bool>,
    kwargs_splat_mask: Vec<bool>,
    span: Span,
) -> Expr {
    Expr::LetBlock {
        bindings: vec![(temp_name.clone(), callee_expr)],
        body: Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Call {
                    function: temp_name,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                },
                span,
            }],
            span,
        },
        span,
    }
}

fn call_named_expr(
    module: Option<&str>,
    function: &str,
    args: Vec<Expr>,
    kwargs: Vec<(String, Expr)>,
    splat_mask: Vec<bool>,
    kwargs_splat_mask: Vec<bool>,
    span: Span,
) -> LowerResult<Expr> {
    let has_splat = splat_mask.iter().any(|is_splat| *is_splat);
    // The range and operator fast-paths below only apply to plain positional
    // calls; a call carrying keyword args always builds a full Call/ModuleCall.
    if !has_splat && kwargs.is_empty() && function == ":" {
        return match args.as_slice() {
            // The parser nests a step range `a:b:c` as `(a:b):c`, so an `esc`-ed /
            // interpolated step range reaches here as a 2-arg colon whose first
            // operand is itself a 2-arg `Expr::Range`. Flatten it to `a:b:c`,
            // mirroring `lower_range_expr` (collection.rs); otherwise this builds
            // `Range : stop` and the VM fails at runtime with "expected numeric
            // value, got Range" (Issue #7020, surfaced by `@animate`/`@gif`).
            [Expr::Range {
                start: inner_start,
                stop: inner_stop,
                step: None,
                ..
            }, stop] => Ok(Expr::Range {
                start: inner_start.clone(),
                step: Some(inner_stop.clone()),
                stop: Box::new(stop.clone()),
                span,
            }),
            [start, stop] => Ok(Expr::Range {
                start: Box::new(start.clone()),
                step: None,
                stop: Box::new(stop.clone()),
                span,
            }),
            [start, step, stop] => Ok(Expr::Range {
                start: Box::new(start.clone()),
                step: Some(Box::new(step.clone())),
                stop: Box::new(stop.clone()),
                span,
            }),
            _ => Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("macro expansion returned malformed range call"),
            ),
        };
    }

    if !has_splat && kwargs.is_empty() && function == "=>" && args.len() == 2 {
        return Ok(Expr::Pair {
            key: Box::new(args[0].clone()),
            value: Box::new(args[1].clone()),
            span,
        });
    }

    if !has_splat && kwargs.is_empty() && args.len() == 2 && function == "<:" {
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Subtype,
            left: Box::new(args[0].clone()),
            right: Box::new(args[1].clone()),
            span,
        });
    }

    if !has_splat && kwargs.is_empty() && args.len() == 2 && function == ">:" {
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Subtype,
            left: Box::new(args[1].clone()),
            right: Box::new(args[0].clone()),
            span,
        });
    }

    if !has_splat && kwargs.is_empty() && args.len() == 2 {
        match function {
            "⊆" => {
                return Ok(Expr::Call {
                    function: "issubset".to_string(),
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                });
            }
            "⊈" => {
                return Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(Expr::Call {
                        function: "issubset".to_string(),
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                        span,
                    }),
                    span,
                });
            }
            "⊊" => {
                return Ok(Expr::Call {
                    function: "issubset_proper".to_string(),
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                });
            }
            "⊇" => {
                return Ok(Expr::Call {
                    function: "issubset".to_string(),
                    args: vec![args[1].clone(), args[0].clone()],
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                });
            }
            "⊉" => {
                return Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(Expr::Call {
                        function: "issubset".to_string(),
                        args: vec![args[1].clone(), args[0].clone()],
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                        span,
                    }),
                    span,
                });
            }
            "⊋" => {
                return Ok(Expr::Call {
                    function: "issuperset_proper".to_string(),
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                });
            }
            _ => {}
        }
    }

    if !has_splat && kwargs.is_empty() && args.len() == 2 {
        if let Some(op) = match function {
            "+" => Some(BinaryOp::Add),
            "-" => Some(BinaryOp::Sub),
            "*" => Some(BinaryOp::Mul),
            "/" => Some(BinaryOp::Div),
            "^" => Some(BinaryOp::Pow),
            "%" => Some(BinaryOp::Mod),
            "==" => Some(BinaryOp::Eq),
            "!=" => Some(BinaryOp::Ne),
            "===" | "≡" => Some(BinaryOp::Egal),
            "!==" | "≢" => Some(BinaryOp::NotEgal),
            "<" => Some(BinaryOp::Lt),
            "<=" => Some(BinaryOp::Le),
            ">" => Some(BinaryOp::Gt),
            ">=" => Some(BinaryOp::Ge),
            "&&" => Some(BinaryOp::And),
            "||" => Some(BinaryOp::Or),
            _ => None,
        } {
            return Ok(Expr::BinaryOp {
                op,
                left: Box::new(args[0].clone()),
                right: Box::new(args[1].clone()),
                span,
            });
        }
    }

    if !has_splat && kwargs.is_empty() && args.len() == 1 {
        if let Some(op) = match function {
            "!" => Some(UnaryOp::Not),
            "-" => Some(UnaryOp::Neg),
            "+" => Some(UnaryOp::Pos),
            _ => None,
        } {
            return Ok(Expr::UnaryOp {
                op,
                operand: Box::new(args[0].clone()),
                span,
            });
        }
    }

    if let Some(module) = module {
        Ok(Expr::ModuleCall {
            module: module.to_string(),
            function: function.to_string(),
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        })
    } else {
        Ok(Expr::Call {
            function: function.to_string(),
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        })
    }
}

fn values_to_exprs<'a>(
    values: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Expr>> {
    values
        .into_iter()
        .map(|value| value_to_expr(value, span, walker, lambda_ctx))
        .collect()
}

fn expand_macrocall_value<'a>(
    args: Vec<Value>,
    span: Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    expand_macrocall_value_with(args, span, lambda_ctx, |value| {
        value_to_expr(value, span, walker, lambda_ctx)
    })
}

fn expand_macrocall_value_with<R>(
    args: Vec<Value>,
    span: Span,
    lambda_ctx: &LambdaContext,
    convert: impl FnOnce(Value) -> LowerResult<R>,
) -> LowerResult<R> {
    if args.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("macro expansion returned malformed Expr(:macrocall, ...)"),
        );
    }

    let macro_name = macro_name_from_value(&args[0], span)?;
    let user_args = &args[2..];
    let arg_types = user_args
        .iter()
        .map(macro_param_type_from_value)
        .collect::<Vec<_>>();
    let macro_def = lambda_ctx
        .get_macro_with_types(&macro_name, &arg_types)
        .or_else(|| lambda_ctx.get_macro_with_arity(&macro_name, user_args.len()))
        .or_else(|| {
            lambda_ctx.get_usings().into_iter().find_map(|module_name| {
                crate::stdlib_loader::get_stdlib_macro(&module_name, &macro_name).or_else(|| {
                    crate::stdlib_loader::get_bundled_package_macro(&module_name, &macro_name)
                })
            })
        })
        .or_else(|| crate::base_loader::get_base_macro_with_arity(&macro_name, user_args.len()))
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "nested macro @{} not found (with {} args)",
                macro_name,
                user_args.len()
            ))
        })?;

    let value =
        evaluate_macro_from_value_args(&macro_name, &macro_def, user_args, span, lambda_ctx)?;
    let pushed = begin_macro_hygiene(lambda_ctx, &macro_name, &macro_def);
    let result = convert(value);
    if pushed {
        lambda_ctx.end_macro_hygiene();
    }
    result
}

fn macro_name_from_value(value: &Value, span: Span) -> LowerResult<String> {
    match value {
        Value::Symbol(sym) => Ok(sym.as_str().trim_start_matches('@').to_string()),
        Value::GlobalRef(GlobalRefValue { name, .. }) => {
            Ok(name.as_str().trim_start_matches('@').to_string())
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macrocall expected macro name Symbol, got {:?}",
                other.value_type()
            )),
        ),
    }
}

fn macro_param_type_from_value(value: &Value) -> MacroParamType {
    match value {
        Value::Symbol(_) => MacroParamType::Symbol,
        Value::Expr(_) => MacroParamType::Expr,
        Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I64(_)
        | Value::I128(_)
        | Value::BigInt(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_) => MacroParamType::Integer,
        Value::F16(_) | Value::F32(_) | Value::F64(_) | Value::BigFloat(_) => MacroParamType::Float,
        Value::Str(_) | Value::Char(_) => MacroParamType::String,
        Value::LineNumberNode(_) => MacroParamType::LineNumberNode,
        _ => MacroParamType::Any,
    }
}

fn evaluate_macro_from_value_args(
    macro_name: &str,
    macro_def: &StoredMacroDef,
    args: &[Value],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Value> {
    let macro_func_name = format!("__sjulia_macro_{}_{}", macro_name, span.start);
    let mut visible_functions = lambda_ctx.compile_time_functions();
    visible_functions.extend(macro_def.expansion_functions.iter().cloned());
    let mut visible_structs = lambda_ctx.compile_time_structs();
    visible_structs.extend(macro_def.expansion_structs.iter().cloned());
    let referenced_modules = collect_referenced_modules_block(&macro_def.body);
    let dependency_functions = macro_dependency_functions(&visible_functions, &macro_def.body);
    let (mut functions, module_functions) =
        split_macro_dependency_functions(macro_name, macro_def, lambda_ctx, dependency_functions);
    functions.push(synthetic_macro_function(&macro_func_name, macro_def, span));
    let mut module_candidates = module_functions;
    module_candidates.extend(functions.iter().cloned());
    let compile_time_modules = compile_time_modules_for_macro(
        macro_name,
        macro_def,
        lambda_ctx,
        &module_candidates,
        &referenced_modules,
        span,
    );
    let compile_time_usings =
        compile_time_usings_for_macro(macro_name, macro_def, lambda_ctx, span);

    let mut call_args = vec![
        Expr::Literal(
            Literal::LineNumberNode {
                line: span_start_line_i64(span)?,
                file: source_file_literal(lambda_ctx),
            },
            span,
        ),
        Expr::Literal(Literal::Module(call_site_module_name(lambda_ctx)), span),
    ];
    for arg in args {
        call_args.push(value_to_runtime_constructor(arg, span)?);
    }

    let splat_mask = vec![false; call_args.len()];
    let main = Block {
        stmts: vec![Stmt::Expr {
            expr: Expr::Call {
                function: macro_func_name.clone(),
                args: call_args,
                kwargs: vec![],
                splat_mask,
                kwargs_splat_mask: vec![],
                span,
            },
            span,
        }],
        span,
    };
    let program = Program {
        // Same as the primary expansion path: include user type definitions so a
        // compile-time function touching a user struct still compiles (Issue #7272).
        abstract_types: lambda_ctx.compile_time_abstract_types(),
        primitive_types: lambda_ctx.compile_time_primitive_types(),
        type_aliases: vec![],
        structs: visible_structs.clone(),
        functions,
        base_function_count: 0,
        modules: compile_time_modules,
        usings: compile_time_usings,
        macros: vec![],
        enums: vec![],
        main,
    };

    let compiled = match compile_with_cache(&program) {
        Ok(compiled) => compiled,
        Err(err)
            if missing_splat_dependency_error(&err)
                && program.functions.len() < visible_functions.len() + 1 =>
        {
            let (mut fallback_functions, fallback_module_functions) =
                split_macro_dependency_functions(
                    macro_name,
                    macro_def,
                    lambda_ctx,
                    visible_functions.clone(),
                );
            fallback_functions.push(synthetic_macro_function(&macro_func_name, macro_def, span));
            let mut fallback_module_candidates = fallback_module_functions;
            fallback_module_candidates.extend(fallback_functions.iter().cloned());
            let fallback_modules = compile_time_modules_for_macro(
                macro_name,
                macro_def,
                lambda_ctx,
                &fallback_module_candidates,
                &referenced_modules,
                span,
            );
            let fallback_usings =
                compile_time_usings_for_macro(macro_name, macro_def, lambda_ctx, span);
            let fallback_program = Program {
                abstract_types: lambda_ctx.compile_time_abstract_types(),
                primitive_types: lambda_ctx.compile_time_primitive_types(),
                type_aliases: vec![],
                structs: visible_structs.clone(),
                functions: fallback_functions,
                base_function_count: 0,
                modules: fallback_modules,
                usings: fallback_usings,
                macros: vec![],
                enums: vec![],
                main: program.main.clone(),
            };
            compile_with_cache(&fallback_program).map_err(|fallback_err| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "nested macro @{} expansion compile error after full dependency retry (Issue #7548): {:?}",
                        macro_name, fallback_err
                    ),
                )
            })?
        }
        Err(err) => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "nested macro @{} expansion compile error: {:?}",
                        macro_name, err
                    ),
                ),
            );
        }
    };
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    match vm.run() {
        // A nested macrocall (e.g. an `Expr(:macrocall, …)` returned by an outer
        // MacroTools macro such as `@capture`/`@match`) is expanded by its own VM
        // whose struct heap is discarded once we return. Resolve `StructRef`
        // handles into owned `Struct` values here, exactly as the primary
        // `evaluate_macro` path does (line ~307); otherwise a heap index escapes
        // into the converted AST and later trips `value_to_literal`
        // ("macro expansion cannot quote value type Any") — Issue #7856.
        Ok(value) => {
            let struct_heap = vm.get_struct_heap().to_vec();
            Ok(resolve_macro_result_struct_refs(value, &struct_heap))
        }
        Err(err)
            if missing_runtime_dependency_error(&err)
                && program.functions.len() < visible_functions.len() + 1 =>
        {
            let (mut fallback_functions, fallback_module_functions) =
                split_macro_dependency_functions(
                    macro_name,
                    macro_def,
                    lambda_ctx,
                    visible_functions,
                );
            fallback_functions.push(synthetic_macro_function(&macro_func_name, macro_def, span));
            let mut fallback_module_candidates = fallback_module_functions;
            fallback_module_candidates.extend(fallback_functions.iter().cloned());
            let fallback_modules = compile_time_modules_for_macro(
                macro_name,
                macro_def,
                lambda_ctx,
                &fallback_module_candidates,
                &referenced_modules,
                span,
            );
            let fallback_usings =
                compile_time_usings_for_macro(macro_name, macro_def, lambda_ctx, span);
            let fallback_program = Program {
                abstract_types: lambda_ctx.compile_time_abstract_types(),
                primitive_types: lambda_ctx.compile_time_primitive_types(),
                type_aliases: vec![],
                structs: visible_structs.clone(),
                functions: fallback_functions,
                base_function_count: 0,
                modules: fallback_modules,
                usings: fallback_usings,
                macros: vec![],
                enums: vec![],
                main: program.main.clone(),
            };
            let fallback_compiled = compile_with_cache(&fallback_program).map_err(|fallback_err| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "nested macro @{} expansion compile error after full runtime-dependency retry (Issue #7569): {:?}",
                        macro_name, fallback_err
                    ),
                )
            })?;
            let mut fallback_vm = Vm::new_program(fallback_compiled, StableRng::new(0));
            let value = fallback_vm.run().map_err(|fallback_err| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    format!(
                        "nested macro @{} expansion runtime error after full dependency retry (Issue #7569): {}",
                        macro_name, fallback_err
                    ),
                )
            })?;
            let struct_heap = fallback_vm.get_struct_heap().to_vec();
            Ok(resolve_macro_result_struct_refs(value, &struct_heap))
        }
        Err(err) => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "nested macro @{} expansion runtime error: {}",
                macro_name, err
            )),
        ),
    }
}

fn value_to_runtime_constructor(value: &Value, span: Span) -> LowerResult<Expr> {
    match value {
        Value::I8(n) => Ok(Expr::Literal(Literal::Int(i64::from(*n)), span)),
        Value::I16(n) => Ok(Expr::Literal(Literal::Int(i64::from(*n)), span)),
        Value::I32(n) => Ok(Expr::Literal(Literal::Int(i64::from(*n)), span)),
        Value::I64(n) => Ok(Expr::Literal(Literal::Int(*n), span)),
        Value::I128(n) => Ok(Expr::Literal(Literal::Int128(*n), span)),
        Value::BigInt(n) => Ok(Expr::Literal(Literal::BigInt(n.to_string()), span)),
        Value::U8(n) => Ok(Expr::Call {
            function: "UInt8".to_string(),
            args: vec![Expr::Literal(Literal::Int(i64::from(*n)), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U16(n) => Ok(Expr::Call {
            function: "UInt16".to_string(),
            args: vec![Expr::Literal(Literal::Int(i64::from(*n)), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U32(n) => Ok(Expr::Call {
            function: "UInt32".to_string(),
            args: vec![Expr::Literal(Literal::Int(i64::from(*n)), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U64(n) => Ok(Expr::Call {
            function: "UInt64".to_string(),
            args: vec![Expr::Literal(Literal::Int(u64_to_i64(*n, span)?), span)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::U128(n) => Ok(Expr::Call {
            function: "UInt128".to_string(),
            args: vec![Expr::Literal(
                Literal::Int128(u128_to_i128(*n, span)?),
                span,
            )],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span,
        }),
        Value::F16(n) => Ok(Expr::Literal(Literal::Float16(*n), span)),
        Value::F32(n) => Ok(Expr::Literal(Literal::Float32(*n), span)),
        Value::F64(n) => Ok(Expr::Literal(Literal::Float(*n), span)),
        Value::BigFloat(n) => Ok(Expr::Literal(Literal::BigFloat(n.to_string()), span)),
        Value::Bool(b) => Ok(Expr::Literal(Literal::Bool(*b), span)),
        Value::Str(s) => Ok(Expr::Literal(Literal::Str(s.clone()), span)),
        Value::Char(c) => Ok(Expr::Literal(Literal::Char(*c), span)),
        Value::Nothing => Ok(Expr::Literal(Literal::Nothing, span)),
        Value::Missing => Ok(Expr::Literal(Literal::Missing, span)),
        Value::Symbol(sym) => Ok(symbol_constructor(sym.as_str(), span)),
        Value::QuoteNode(inner) => Ok(Expr::Builtin {
            name: BuiltinOp::QuoteNodeNew,
            args: vec![value_to_runtime_constructor(inner, span)?],
            span,
        }),
        Value::LineNumberNode(ln) => {
            let mut args = vec![Expr::Literal(Literal::Int(ln.line), span)];
            if let Some(file) = &ln.file {
                args.push(symbol_constructor(file, span));
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::LineNumberNodeNew,
                args,
                span,
            })
        }
        Value::GlobalRef(GlobalRefValue { module, name }) => Ok(Expr::Builtin {
            name: BuiltinOp::GlobalRefNew,
            args: vec![
                Expr::Literal(Literal::Module(module.clone()), span),
                symbol_constructor(name.as_str(), span),
            ],
            span,
        }),
        Value::DataType(ty) => Ok(Expr::Var(ty.name().to_string(), span)),
        Value::Module(module) => Ok(Expr::Literal(Literal::Module(module.name.clone()), span)),
        Value::Expr(expr) => {
            let mut args = Vec::with_capacity(expr.nargs() + 1);
            args.push(symbol_constructor(expr.head.as_str(), span));
            for arg in expr.args_snapshot() {
                args.push(value_to_runtime_constructor(&arg, span)?);
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }
        Value::Tuple(tuple) => Ok(Expr::TupleLiteral {
            elements: tuple
                .elements
                .iter()
                .map(|element| value_to_runtime_constructor(element, span))
                .collect::<LowerResult<Vec<_>>>()?,
            span,
        }),
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macrocall argument value type {:?} cannot be re-materialized",
                other.value_type()
            )),
        ),
    }
}

fn symbol_constructor(name: &str, span: Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
        span,
    }
}

fn nothing_stmt(span: Span) -> Stmt {
    Stmt::Expr {
        expr: Expr::Literal(Literal::Nothing, span),
        span,
    }
}

fn validate_symbol_declaration_args(args: &[Value], head: &str, span: Span) -> LowerResult<()> {
    for arg in args {
        match arg {
            Value::Symbol(_) => {}
            Value::QuoteNode(inner) if matches!(inner.as_ref(), Value::Symbol(_)) => {}
            other => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                        format!(
                            "macro expansion returned Expr(:{}, ...) with non-Symbol argument {:?}",
                            head,
                            other.value_type()
                        ),
                    ),
                )
            }
        }
    }
    Ok(())
}

fn symbol_declaration_names(args: &[Value], head: &str, span: Span) -> LowerResult<Vec<String>> {
    let mut names = Vec::with_capacity(args.len());
    for arg in args {
        match arg {
            Value::Symbol(sym) => names.push(sym.as_str().to_string()),
            Value::QuoteNode(inner) => match inner.as_ref() {
                Value::Symbol(sym) => names.push(sym.as_str().to_string()),
                other => {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint(format!(
                                "macro expansion returned Expr(:{}, ...) with non-Symbol QuoteNode argument {:?}",
                                head,
                                other.value_type()
                            )),
                    )
                }
            },
            other => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                        format!(
                            "macro expansion returned Expr(:{}, ...) with non-Symbol argument {:?}",
                            head,
                            other.value_type()
                        ),
                    ),
                )
            }
        }
    }
    Ok(names)
}

fn macro_assignment_target(value: &Value) -> Value {
    match value {
        Value::Expr(expr)
            if matches!(
                ExprHead::from_expr(expr),
                Some(ExprHead::Escape | ExprHead::HygienicScope)
            ) && !expr.args_snapshot().is_empty() =>
        {
            let args = expr.args_snapshot();
            macro_assignment_target(&args[0])
        }
        other => other.clone(),
    }
}

/// Build a `DestructureTarget` from a macro-produced tuple-assignment LHS `Value`
/// (`Expr(:tuple, ...)` and friends). Mirrors the CST `parse_destructure_target`
/// so that a tuple-destructuring assignment spliced into a macro body lowers like
/// source `(a, b) = rhs` instead of a call to the `=` operator (Issue #7900).
fn macro_destructure_target(value: &Value, span: Span) -> LowerResult<DestructureTarget> {
    match macro_assignment_target(value) {
        Value::Symbol(name) => Ok(DestructureTarget::Identifier(name.as_str().to_string())),
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Tuple) => {
            let children = expr
                .args_snapshot()
                .iter()
                .filter(|arg| !matches!(arg, Value::LineNumberNode(_)))
                .map(|arg| macro_destructure_target(arg, span))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok(DestructureTarget::Tuple(children))
        }
        Value::Expr(expr) if ExprHead::is_expr(&expr, ExprHead::Splat) => {
            let args = expr.args_snapshot();
            match args.first().map(macro_assignment_target) {
                Some(Value::Symbol(name)) => {
                    Ok(DestructureTarget::Rest(name.as_str().to_string()))
                }
                _ => Err(UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint(
                        "macro expansion tuple destructuring rest/splat target must be an identifier",
                    )),
            }
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion returned unsupported tuple destructuring target {:?}",
                other.value_type()
            )),
        ),
    }
}

fn symbol_arg(value: &Value, span: Span) -> LowerResult<String> {
    match value {
        Value::Symbol(sym) => Ok(sym.as_str().to_string()),
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion expected Symbol assignment target, got {:?}",
                other.value_type()
            )),
        ),
    }
}

fn value_to_literal(value: Value, span: Span) -> LowerResult<Literal> {
    match value {
        Value::I8(n) => Ok(Literal::Int(i64::from(n))),
        Value::I16(n) => Ok(Literal::Int(i64::from(n))),
        Value::I32(n) => Ok(Literal::Int(i64::from(n))),
        Value::I64(n) => Ok(Literal::Int(n)),
        Value::I128(n) => Ok(Literal::Int128(n)),
        Value::BigInt(n) => Ok(Literal::BigInt(n.to_string())),
        Value::F16(n) => Ok(Literal::Float16(n)),
        Value::F32(n) => Ok(Literal::Float32(n)),
        Value::F64(n) => Ok(Literal::Float(n)),
        Value::BigFloat(n) => Ok(Literal::BigFloat(n.to_string())),
        Value::Bool(b) => Ok(Literal::Bool(b)),
        Value::Str(s) => Ok(Literal::Str(s)),
        Value::Char(c) => Ok(Literal::Char(c)),
        Value::Nothing => Ok(Literal::Nothing),
        Value::Missing => Ok(Literal::Missing),
        Value::Module(module) => Ok(Literal::Module(module.name.clone())),
        Value::Symbol(sym) => Ok(Literal::Symbol(sym.as_str().to_string())),
        Value::DataType(ty) => Ok(Literal::DataType(ty.name().to_string())),
        Value::QuoteNode(inner) => Ok(Literal::QuoteNode(Box::new(value_to_literal(
            *inner, span,
        )?))),
        Value::LineNumberNode(ln) => Ok(Literal::LineNumberNode {
            line: ln.line,
            file: ln.file,
        }),
        Value::Struct(instance) => {
            let fields = instance
                .values
                .into_iter()
                .map(|field| value_to_literal(field, span))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok(Literal::Struct(instance.struct_name.to_string(), fields))
        }
        Value::Expr(expr) => {
            let args = expr
                .args_snapshot()
                .into_iter()
                .map(|arg| value_to_literal(arg, span))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok(Literal::Expr {
                head: expr.head.as_str().to_string(),
                args,
            })
        }
        other => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro expansion cannot quote value type {:?}",
                other.value_type()
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::{LineNumberNodeValue, SymbolValue};

    fn test_span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    fn assign_field(name: &str, value: Value) -> Value {
        Value::Expr(ExprValue::new(
            SymbolValue::new("="),
            vec![Value::Symbol(SymbolValue::new(name)), value],
        ))
    }

    #[test]
    fn tuple_expr_from_values_skips_line_nodes_for_named_tuple_detection_7802() {
        let span = test_span();
        let walker = CstWalker::new("");
        let lambda_ctx = LambdaContext::new();
        let args = vec![
            Value::LineNumberNode(LineNumberNodeValue::new(10, Some("macro.jl".to_string()))),
            assign_field("a", Value::I64(1)),
            Value::LineNumberNode(LineNumberNodeValue::new(11, Some("macro.jl".to_string()))),
            assign_field("b", Value::I64(2)),
        ];

        let expr = tuple_expr_from_values(args, span, &walker, &lambda_ctx)
            .expect("line-number metadata should not block named tuple lowering");

        let Expr::NamedTupleLiteral { fields, .. } = expr else {
            panic!("expected macro tuple with line nodes to lower as NamedTupleLiteral");
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0, "a");
        assert_eq!(fields[1].0, "b");
    }
}
