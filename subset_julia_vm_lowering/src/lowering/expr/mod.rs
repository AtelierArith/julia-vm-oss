//! Expression lowering.
//!
//! This module handles lowering of CST expressions to Core IR.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod binary;
mod call;
mod collection;
mod helpers;
mod literal;
mod macros;
mod misc;
pub mod quote;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, Expr, Literal, LocalDeclKind};
use crate::lowering::function::lower_anonymous_function_value;
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

// Re-export helpers for use in submodules.
// `map_builtin_name` is pub: the VM-backed macro expander
// (`crate::macro_runtime`, Issue #8656) maps builtin call names while
// converting macro-returned ASTs back to IR.
pub use helpers::map_builtin_name;
pub(super) use helpers::{
    is_broadcast_op, is_chainable_comparison_operator, is_flattenable_operator, is_operator_token,
    map_binary_op, map_unary_op, process_raw_string_escapes, strip_broadcast_dot,
};
pub use helpers::{make_broadcasted_call, make_broadcasted_call_with_callee};
pub use literal::{parse_float, ParsedFloat};

// Re-export macro functions for submodules
pub(super) use macros::{lower_macro_expr, lower_macro_expr_with_ctx};

// Re-export quote functions
pub(super) use quote::lower_quote_expr;
pub use quote::{
    quote_constructor_to_code, quote_constructor_to_code_with_locals,
    quote_constructor_to_code_with_varargs,
};

// Re-export public functions
pub use collection::extract_index_target;
pub use collection::extract_index_target_nodes;
pub use macros::lower_namedtuple_macro_expr;
// Statement-position `@sync` lowering (Issue #7768): keeps the sync body in the
// enclosing scope so assignments to surrounding locals are preserved.
pub use macros::lower_sync_macro_stmt_entry;
pub use misc::extract_field_target;
pub use misc::extract_nested_field_target;
pub use misc::extract_nested_field_target_with_ctx;

fn version_identifier_expr(ident: &str, span: Span) -> Expr {
    if ident.bytes().all(|b| b.is_ascii_digit()) {
        ident
            .parse::<i64>()
            .map(|n| Expr::Literal(Literal::Int(n), span))
            .unwrap_or_else(|_| Expr::Literal(Literal::Str(ident.to_string()), span))
    } else {
        Expr::Literal(Literal::Str(ident.to_string()), span)
    }
}

fn version_ident_tuple_expr(text: &str, span: Span) -> Expr {
    if text.is_empty() {
        return Expr::TupleLiteral {
            elements: Vec::new(),
            span,
        };
    }
    Expr::TupleLiteral {
        elements: text
            .split('.')
            .map(|ident| version_identifier_expr(ident, span))
            .collect(),
        span,
    }
}

fn version_literal_args(content: &str, span: Span) -> Vec<Expr> {
    let (without_build, build) = content
        .split_once('+')
        .map_or((content, ""), |(left, right)| (left, right));
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, ""), |(left, right)| (left, right));
    let mut parts = core.split('.');
    let major = parts
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    vec![
        Expr::Literal(Literal::Int(major), span),
        Expr::Literal(Literal::Int(minor), span),
        Expr::Literal(Literal::Int(patch), span),
        version_ident_tuple_expr(prerelease, span),
        version_ident_tuple_expr(build, span),
    ]
}

// Re-export for submodules
pub(super) use binary::{
    lower_binary_expr, lower_binary_expr_with_ctx, lower_juxtaposition_expr, lower_unary_expr,
    lower_unary_expr_with_ctx,
};
pub(super) use call::{
    is_operator_function_call_target, lower_argument_list, lower_call_expr,
    lower_call_expr_with_ctx,
};
pub(super) use collection::{
    lower_comprehension_expr, lower_comprehension_expr_with_ctx, lower_generator_expr,
    lower_generator_expr_with_ctx, lower_index_expr, lower_index_expr_with_ctx, lower_matrix_expr,
    lower_matrix_expr_raw, lower_matrix_expr_raw_with_ctx, lower_matrix_expr_with_ctx,
    lower_range_expr, lower_range_expr_with_ctx, lower_vector_expr, lower_vector_expr_with_ctx,
};
pub(super) use literal::{lower_char_literal, lower_string_literal};
pub(super) use misc::{
    lower_adjoint_expr, lower_broadcast_call_expr, lower_field_expr, lower_if_expr,
    lower_if_expr_with_ctx, lower_let_expr, lower_let_expr_with_ctx, lower_pair_expr,
    lower_parameter_list_named_tuple, lower_parenthesized_expr, lower_parenthesized_expr_with_ctx,
    lower_ternary_expr, lower_ternary_expr_with_ctx, lower_tuple_expr, lower_tuple_expr_with_ctx,
};

fn wrap_comprehension_body_with_call(
    comprehension: Expr,
    function: String,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    // `Any[expr for x in iter]` (and the multi-iter form): `Any(x)`
    // is not a defined Julia constructor, so wrapping the body in
    // an `Any(x)` call errors at runtime with "Unknown function:
    // Any". Instead wrap the entire (untyped) comprehension in a
    // `Vector{Any}(...)` call — the `Vector{Any}` compile intercept
    // (Issue #4818) routes through `_vector_any_collect` which
    // produces a `Vector{Any}` with each element boxed. Issue #4819.
    if function == "Any" {
        return Ok(Expr::Call {
            function: "Vector{Any}".to_string().into(),
            args: vec![comprehension],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        });
    }
    // Typed comprehension `T[expr for x in iter]` for element types whose
    // constructor is not equivalent to `convert(T, expr)` (`Bool`, `Char`,
    // `Symbol`, `String`, `Complex{T}` / `ComplexFNN`, ...). Upstream stores
    // each element through `setindex!`, which calls `convert(T, expr)` — not
    // the `T(expr)` *constructor*. For these types the constructor either is
    // not reachable in the VM (`Bool` / `Symbol` -> "Unknown function") or
    // produces the wrong element slot (`Char` was forced into an I64 slot,
    // `String` left the eltype as `Any`, `Complex{Float64}(::Complex)` tried
    // to convert the whole Complex value to the field type `Float64`). Wrap the
    // body in `convert(T, expr)` and the whole comprehension in `Vector{T}(...)`,
    // mirroring the `Any` case above; the `Vector{T}` compile intercept forces
    // the result element type to `T` so `typeof` matches upstream exactly.
    // Issues #5040 and #9505.
    //
    // Numeric/abstract numeric types keep the existing `T(expr)` body wrapping
    // (a real, reachable constructor with the right inferred element type) so
    // the resolved #4811/#4816/#4818/#4819/#4822 cluster behavior is preserved.
    let convert_body_target = matches!(
        function.as_str(),
        "Bool" | "Char" | "Symbol" | "String" | "ComplexF64" | "ComplexF32"
    ) || function.starts_with("Complex{");
    if convert_body_target {
        let convert_body = |body: Box<Expr>| -> Box<Expr> {
            let type_expr = if function.contains('{') {
                Expr::Literal(Literal::DataType(function.clone()), span)
            } else {
                Expr::Var(function.clone().into(), span)
            };
            Box::new(Expr::Call {
                function: "convert".to_string().into(),
                args: vec![type_expr, *body],
                kwargs: Vec::new(),
                splat_mask: vec![false, false],
                kwargs_splat_mask: Vec::new(),
                span,
            })
        };
        let inner = match comprehension {
            Expr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            } => Expr::Comprehension {
                body: convert_body(body),
                var,
                iter,
                filter,
                span,
            },
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                flatten,
                ..
            } => Expr::MultiComprehension {
                body: convert_body(body),
                iterations,
                filter,
                flatten,
                span,
            },
            _ => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                        .with_hint("typed comprehension did not lower to a comprehension"),
                );
            }
        };
        return Ok(Expr::Call {
            function: format!("Vector{{{function}}}").into(),
            args: vec![inner],
            kwargs: Vec::new(),
            splat_mask: vec![false],
            kwargs_splat_mask: Vec::new(),
            span,
        });
    }
    match comprehension {
        Expr::Comprehension {
            body,
            var,
            iter,
            filter,
            ..
        } => Ok(Expr::Comprehension {
            body: Box::new(Expr::Call {
                function: function.into(),
                args: vec![*body],
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            }),
            var,
            iter,
            filter,
            span,
        }),
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            flatten,
            ..
        } => Ok(Expr::MultiComprehension {
            body: Box::new(Expr::Call {
                function: function.into(),
                args: vec![*body],
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            }),
            iterations,
            filter,
            flatten,
            span,
        }),
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("typed comprehension did not lower to a comprehension"),
        ),
    }
}

/// Lower a type assertion `expr::T` (typeassert) on an arbitrary expression.
///
/// Mirrors upstream Julia lowering (`julia/src/julia-syntax.scm`): when the
/// left side of `::` is not a bare symbol (i.e. it is an arbitrary expression
/// such as a call), `a::T` expands to `(call (core typeassert) a T)`. This
/// returns `a` unchanged when `a isa T` and otherwise throws a `TypeError`.
/// The type `T` may be a literal type (`::Int`) or a computed type expression
/// (`::typeof(x)`), so both the value and the type node are lowered as ordinary
/// expressions.
fn lower_type_assertion<'a>(
    walker: &CstWalker<'a>,
    value_node: Node<'a>,
    type_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let value = match lambda_ctx {
        Some(ctx) => lower_expr_with_ctx(walker, value_node, ctx)?,
        None => lower_expr(walker, value_node)?,
    };
    let ty = match lambda_ctx {
        Some(ctx) => lower_expr_with_ctx(walker, type_node, ctx)?,
        None => lower_expr(walker, type_node)?,
    };
    Ok(Expr::Call {
        function: "typeassert".to_string().into(),
        args: vec![value, ty],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span,
    })
}

/// Lower a `try/catch/end` CST node when it appears in expression
/// position (assignment RHS, inside arithmetic, etc.). Issue #4784.
///
/// Strategy: lower the try-statement to a `Stmt::Try` as usual, then
/// rewrite the **last** statement of each branch (try body, catch
/// body, else body) — when it is an `Stmt::Expr` — into an
/// `Stmt::Assign` that stores the value into a fresh result
/// variable. Wrap the modified try-statement plus a final read of
/// the result variable in an `Expr::LetBlock` so the LetBlock's
/// "last statement is the block's value" semantics yield the
/// result variable.
///
/// If a branch ends in a non-expression statement (e.g., a bare
/// `return`), the assignment is skipped for that branch — the
/// variable stays at the result variable's default (`nothing`).
fn lower_try_as_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let try_stmt = match lambda_ctx {
        Some(ctx) => crate::lowering::stmt::lower_try_stmt_with_ctx(walker, node, ctx)?,
        None => crate::lowering::stmt::lower_try_stmt(walker, node)?,
    };
    try_stmt_into_value_expr(try_stmt, span).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("try_statement".to_string()),
            span,
        )
    })
}

/// Convert a lowered `Stmt::Try` into the equivalent value-producing
/// `Expr::LetBlock`, so a `try/catch[/else/finally]` used in value position
/// yields the value of whichever branch executed.
///
/// Strategy: rewrite the value-producing tail of each branch (try body, catch
/// body, else body) into assignments to a fresh result variable, then wrap the
/// modified try-statement plus a final read of the result variable in an
/// `Expr::LetBlock` whose "last statement is the block's value" semantics
/// yield the result variable. The `finally` block is left untouched so it never
/// contributes the produced value (matching Julia).
///
/// Returns `None` if `stmt` is not a `Stmt::Try`.
///
/// Shared by `lower_try_as_expr` (expression position, Issue #4784) and the
/// compile-layer implicit-return path (tail position, Issue #6223).
pub fn try_stmt_into_value_expr(
    stmt: crate::ir::core::Stmt,
    span: crate::parser::span::Span,
) -> Option<Expr> {
    use crate::ir::core::{Block, Stmt};

    let Stmt::Try {
        try_block,
        catch_var,
        catch_block,
        else_block,
        finally_block,
        span: try_span,
    } = stmt
    else {
        return None;
    };

    let result_var = format!("__sjvm_try_result_{}", span.start);

    let try_block = assign_block_tail_value(try_block, &result_var);
    let catch_block = catch_block.map(|b| assign_block_tail_value(b, &result_var));
    let else_block = else_block.map(|b| assign_block_tail_value(b, &result_var));

    let rewritten_try = Stmt::Try {
        try_block,
        catch_var,
        catch_block,
        else_block,
        finally_block,
        span: try_span,
    };

    // The result variable defaults to `nothing` if no branch ran
    // anything that produced a value (e.g. a bare `return`).
    let declaration = compiler_enclosing_declaration(result_var.clone(), span);
    let init = Stmt::Assign {
        var: result_var.clone(),
        value: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    };
    let read = Stmt::Expr {
        expr: Expr::Var(result_var.into(), span),
        span,
    };

    Some(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![declaration, init, rewritten_try, read],
            span,
        },
        span,
    })
}

/// Declare a lowering-generated binding as owned by its containing transparent
/// block. Nested soft scopes update this binding; they must not clean it up as
/// a fresh clause local.
pub(crate) fn compiler_enclosing_declaration(var: String, span: Span) -> crate::ir::core::Stmt {
    crate::ir::core::Stmt::LocalDecl {
        var,
        kind: LocalDeclKind::CompilerEnclosing,
        span,
    }
}

/// Recursively rewrite the last value-producing statement of a branch/block so
/// it assigns its value to `result_var` instead of being discarded. Used by
/// [`try_stmt_into_value_expr`] and [`if_stmt_into_value_expr`] to hoist the
/// value of a control-flow statement (`try` / `if`) in value/tail position.
///
/// The recursion mirrors upstream Julia's "the last statement of a block is its
/// value": a trailing plain expression becomes an assignment, and a trailing
/// nested control-flow statement (`try` — Issue #4833; `if` — elseif chains and
/// nested `if` tails; a nested `begin` `Stmt::Block`) is descended so *its* own
/// branch tails feed `result_var`. Any other trailing statement (`Stmt::Return`,
/// a loop, a bare `break`, …) is left untouched — the outer `result_var` stays
/// at its `nothing` default for those branches, matching Julia (`x = for … end`
/// / `x = while … end` are `nothing`).
pub fn assign_block_tail_value(
    block: crate::ir::core::Block,
    result_var: &str,
) -> crate::ir::core::Block {
    use crate::ir::core::{Block, Stmt};
    let mut stmts = block.stmts;
    if let Some(last) = stmts.pop() {
        match last {
            Stmt::Expr { expr, span } => {
                stmts.push(Stmt::Assign {
                    var: result_var.to_string(),
                    value: expr,
                    span,
                });
            }
            Stmt::Try {
                try_block: inner_try,
                catch_var: inner_catch_var,
                catch_block: inner_catch,
                else_block: inner_else,
                finally_block: inner_finally,
                span: inner_span,
            } => {
                let inner_try = assign_block_tail_value(inner_try, result_var);
                let inner_catch = inner_catch.map(|b| assign_block_tail_value(b, result_var));
                let inner_else = inner_else.map(|b| assign_block_tail_value(b, result_var));
                stmts.push(Stmt::Try {
                    try_block: inner_try,
                    catch_var: inner_catch_var,
                    catch_block: inner_catch,
                    else_block: inner_else,
                    finally_block: inner_finally,
                    span: inner_span,
                });
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span: if_span,
            } => {
                let then_branch = assign_block_tail_value(then_branch, result_var);
                let else_branch = else_branch.map(|b| assign_block_tail_value(b, result_var));
                stmts.push(Stmt::If {
                    condition,
                    then_branch,
                    else_branch,
                    span: if_span,
                });
            }
            Stmt::Block(block) => {
                // A tuple-destructuring statement (`(a, b) = rhs`) never lowers
                // to the dedicated `Stmt::DestructuringAssign` IR variant in the
                // The residual dependent-literal/nested/rest pipeline in
                // `lower_tuple_destructuring_impl` decomposes to a flat
                // `Stmt::Block`, routing the RHS through one or more
                // reserved `__tuple_tmp_`-prefixed compiler-internal
                // temporaries (see `generate_temp_var`). Detect that shape
                // BEFORE the generic "last statement is the value" recursion
                // below — which would otherwise return the LAST target's value
                // (e.g. `b`) instead of the whole destructured tuple — and
                // recover the tuple value from the temporaries instead
                // (Issue #10431). Only fires for that specific reserved-name
                // shape; an ordinary nested `begin ... end` recurses as before.
                if let Some(value) = destructuring_tail_value(&block.stmts) {
                    let span = block.span;
                    stmts.push(Stmt::Block(block));
                    stmts.push(Stmt::Assign {
                        var: result_var.to_string(),
                        value,
                        span,
                    });
                } else {
                    stmts.push(Stmt::Block(assign_block_tail_value(block, result_var)));
                }
            }
            other @ Stmt::DestructuringAssign { .. } => {
                if let Some((tmp, init, store)) = split_destructuring_stmt_via_temp(other) {
                    let span = init.span();
                    stmts.push(init);
                    stmts.push(store);
                    stmts.push(Stmt::Assign {
                        var: result_var.to_string(),
                        value: Expr::Var(tmp.into(), span),
                        span,
                    });
                }
            }
            other @ (Stmt::IndexAssign { .. }
            | Stmt::FieldAssign { .. }
            | Stmt::DictAssign { .. }) => {
                // Julia: `v[i] = x` / `obj.field = x` / `d[k] = x` are each an
                // expression whose value is the assigned (RHS) value — same
                // rule as `Stmt::Assign` below, extended to indexed/field/dict
                // targets (Issue #10431). `split_assign_stmt_via_temp` binds
                // the RHS to a fresh compiler-internal temporary and rewrites
                // the store to use the temporary, so the RHS and any index
                // expressions are each evaluated exactly once (matching
                // Julia's evaluation order) instead of re-reading `v[i]`
                // afterward, which would both re-evaluate `i` (a possible
                // double side effect) and re-run `getindex` needlessly.
                // `v[i] += x` / `obj.field += x` already desugar to this same
                // shape at statement-lowering time (`value = v[i] + x`), so
                // they need no separate arm. Shared with the compile-layer
                // implicit-return paths (`compile_function_body`,
                // `compile_block_with_implicit_return`, `compile_block_value`).
                if let Some((tmp, init, store)) = split_assign_stmt_via_temp(other) {
                    let span = init.span();
                    stmts.push(init);
                    stmts.push(store);
                    stmts.push(Stmt::Assign {
                        var: result_var.to_string(),
                        value: Expr::Var(tmp.into(), span),
                        span,
                    });
                }
            }
            Stmt::Assign { var, value, span } => {
                // Julia: an assignment expression evaluates to the assigned
                // value. Mirrors the `Stmt::Assign`/`Stmt::AddAssign`
                // tail-return handling already applied to
                // `compile_function_body`/`compile_block_with_implicit_return`
                // for Issues #8976/#10023 — without this arm, a `try`/`catch`
                // branch ending in a plain assignment (including the
                // `Stmt::Block([Stmt::Global, Stmt::Assign])` shape that
                // `global x = value` lowers to, handled via the `Stmt::Block`
                // arm above recursing here) left `result_var` at its `nothing`
                // default, causing either a silent wrong return value or a
                // codegen/inference mismatch crash when the surrounding
                // function's return type was independently inferred as
                // non-`Nothing` (Issue #10074). Keep the original assignment
                // (preserving its side effect and global-routing) and append a
                // read of the just-assigned variable into `result_var`.
                let var_name = var.clone();
                stmts.push(Stmt::Assign { var, value, span });
                stmts.push(Stmt::Assign {
                    var: result_var.to_string(),
                    value: Expr::Var(var_name.into(), span),
                    span,
                });
            }
            Stmt::AddAssign { var, value, span } => {
                // Same rule as `Stmt::Assign` above, for the `Stmt::AddAssign`
                // shape (Issue #10074).
                let var_name = var.clone();
                stmts.push(Stmt::AddAssign { var, value, span });
                stmts.push(Stmt::Assign {
                    var: result_var.to_string(),
                    value: Expr::Var(var_name.into(), span),
                    span,
                });
            }
            other => stmts.push(other),
        }
    }
    Block {
        stmts,
        span: block.span,
    }
}

/// Split a destructuring assignment into a single RHS evaluation followed by
/// a destructure from the saved value. The temporary is also the assignment's
/// expression value in begin/let/try tail contexts (Issue #10464).
pub fn split_destructuring_stmt_via_temp(
    stmt: crate::ir::core::Stmt,
) -> Option<(String, crate::ir::core::Stmt, crate::ir::core::Stmt)> {
    use crate::ir::core::Stmt;
    let Stmt::DestructuringAssign {
        targets,
        value,
        span,
    } = stmt
    else {
        return None;
    };
    // Macro-hygiene gensyms contain `#`, which cannot be written as a Julia
    // source identifier, and use the process-wide atomic gensym allocator.
    // Unlike a span-derived suffix this remains unique for same-span macro
    // expansions and repeated lowering in one session.
    let tmp = quote::HygieneContext::gensym("tail_destructure");
    let init = Stmt::Assign {
        var: tmp.clone(),
        value,
        span,
    };
    let store = Stmt::DestructuringAssign {
        targets,
        value: Expr::Var(tmp.clone().into(), span),
        span,
    };
    Some((tmp, init, store))
}

/// Split an indexed/field/dict assignment statement into
/// `(temp_name, init, store)`: `init` binds the RHS `value` to a fresh
/// compiler-internal temporary, and `store` performs the same assignment
/// using that temporary in place of the original `value` expression. The
/// temporary is then safe to re-read as the statement's value (e.g. in tail
/// position, Issue #10431) without re-evaluating the RHS or any index
/// expressions a second time. Returns `None` for any other `Stmt` shape.
///
/// Shared by [`assign_block_tail_value`] (lowering-layer try/if-as-value and
/// tail-position rewriting) and the compile-layer implicit-return paths in
/// `compile/stmt.rs` (`compile_function_body`, `compile_block_with_implicit_return`)
/// and `compile/expr/mod.rs` (`compile_block_value`), so a plain function-body
/// tail, an `if`/`else` branch tail, and a `begin ... end` block tail all get
/// the same "an assignment is an expression" treatment as try/catch.
pub fn split_assign_stmt_via_temp(
    stmt: crate::ir::core::Stmt,
) -> Option<(String, crate::ir::core::Stmt, crate::ir::core::Stmt)> {
    use crate::ir::core::Stmt;
    match stmt {
        Stmt::IndexAssign {
            array,
            indices,
            value,
            span,
        } => {
            let tmp = format!("__sjvm_tail_assign_tmp_{}", span.start);
            let init = Stmt::Assign {
                var: tmp.clone(),
                value,
                span,
            };
            let store = Stmt::IndexAssign {
                array,
                indices,
                value: Expr::Var(tmp.clone().into(), span),
                span,
            };
            Some((tmp, init, store))
        }
        Stmt::FieldAssign {
            object,
            field,
            value,
            span,
        } => {
            let tmp = format!("__sjvm_tail_assign_tmp_{}", span.start);
            let init = Stmt::Assign {
                var: tmp.clone(),
                value,
                span,
            };
            let store = Stmt::FieldAssign {
                object,
                field,
                value: Expr::Var(tmp.clone().into(), span),
                span,
            };
            Some((tmp, init, store))
        }
        Stmt::DictAssign {
            dict,
            key,
            value,
            span,
        } => {
            let tmp = format!("__sjvm_tail_assign_tmp_{}", span.start);
            let init = Stmt::Assign {
                var: tmp.clone(),
                value,
                span,
            };
            let store = Stmt::DictAssign {
                dict,
                key,
                value: Expr::Var(tmp.clone().into(), span),
                span,
            };
            Some((tmp, init, store))
        }
        _ => None,
    }
}

/// Recognize residual `Stmt::Block` shapes emitted by tuple destructuring
/// (lowering/stmt/assignment.rs) for a tuple-destructuring assignment
/// (`(a, b) = rhs`) whose RHS is not an independent literal tuple: a leading
/// run of one or more `Stmt::Assign`s to `__tuple_tmp_`-prefixed
/// compiler-internal temporaries (the reserved naming convention from
/// `generate_temp_var`, the same style already used for
/// `__sjvm_try_result_` elsewhere in this file), followed by the per-target
/// assignments that read from them. Recovers the value of the whole
/// destructuring expression — a single temp's value when the RHS was bound
/// whole (the common case, e.g. `(a, b) = f()`), or a reconstructed tuple of
/// every temp's value when there are several (the dependent-swap
/// per-element-temp shape, e.g. `(a, b) = (b, a)`).
///
/// Returns `None` for any other block shape: an ordinary nested
/// `begin ... end` (no leading temp-prefixed run at all), or the temp-FREE
/// independent-literal-tuple fast path (`(a, b) = (1, 2)` exactly, Issue
/// #6569's no-allocation optimization) which has no temporary to recover the
/// value from and remains a tracked residual gap (Issue #10431 follow-up).
///
/// Shared by [`assign_block_tail_value`] and the compile-layer implicit-return
/// paths in `compile/stmt.rs` (`compile_function_body`,
/// `compile_block_with_implicit_return`) and `compile/expr/mod.rs`
/// (`compile_block_value`).
pub fn destructuring_tail_value(stmts: &[crate::ir::core::Stmt]) -> Option<Expr> {
    use crate::ir::core::Stmt;
    const TUPLE_TMP_PREFIX: &str = "__tuple_tmp_";

    let mut temp_names = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, .. } if var.starts_with(TUPLE_TMP_PREFIX) => {
                temp_names.push(var.clone());
            }
            _ => break,
        }
    }
    // Require at least one leading temp AND at least one statement after the
    // leading run (the per-target assignments) — a block that is nothing but
    // temp assigns isn't this shape at all.
    if temp_names.is_empty() || temp_names.len() >= stmts.len() {
        return None;
    }
    let span = stmts[0].span();
    if temp_names.len() == 1 {
        // `temp_names.len() == 1` was just checked, so `next()` cannot yield
        // `None`; `?` still routes the (unreachable) `None` case to this
        // function's own `None` return instead of a raw unwrap (Issue
        // #10905, Phase 1b of #10869).
        let name = temp_names.into_iter().next()?;
        Some(Expr::Var(name.into(), span))
    } else {
        Some(Expr::TupleLiteral {
            elements: temp_names
                .into_iter()
                .map(|name| Expr::Var(name.into(), span))
                .collect(),
            span,
        })
    }
}

/// Convert a lowered `Stmt::If` into the equivalent value-producing
/// `Expr::LetBlock`, so an `if/elseif/else` used in value position yields the
/// value of whichever branch executed (an `if` with no matching branch yields
/// `nothing`, matching Julia). Companion to [`try_stmt_into_value_expr`].
///
/// Returns `None` if `stmt` is not a `Stmt::If`.
pub fn if_stmt_into_value_expr(
    stmt: crate::ir::core::Stmt,
    span: crate::parser::span::Span,
) -> Option<Expr> {
    use crate::ir::core::{Block, Stmt};

    let Stmt::If {
        condition,
        then_branch,
        else_branch,
        span: if_span,
    } = stmt
    else {
        return None;
    };

    let result_var = format!("__sjvm_if_result_{}", span.start);

    let then_branch = assign_block_tail_value(then_branch, &result_var);
    let else_branch = else_branch.map(|b| assign_block_tail_value(b, &result_var));

    let rewritten_if = Stmt::If {
        condition,
        then_branch,
        else_branch,
        span: if_span,
    };

    // The result variable defaults to `nothing` when no branch produced a value
    // (e.g. an `if` with no `else` whose condition is false).
    let declaration = compiler_enclosing_declaration(result_var.clone(), span);
    let init = Stmt::Assign {
        var: result_var.clone(),
        value: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    };
    let read = Stmt::Expr {
        expr: Expr::Var(result_var.into(), span),
        span,
    };

    Some(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![declaration, init, rewritten_if, read],
            span,
        },
        span,
    })
}

/// Main expression lowering function.
/// Dispatches to specialized handlers based on node kind.
///
/// # Design Rule: Do NOT hardcode prelude constants here (Issue #2866)
///
/// Only the following identifiers are special-cased at the lowering stage:
/// - **Julia keywords / literals**: `true`, `false`, `nothing`, `missing`
/// - **Built-in module references**: `Base`, `Core`, `Main`, `Meta`
///
/// All other constants — including `im`, `pi`, `ℯ`, `Inf`, `NaN`, etc. —
/// are defined in `subset_julia_vm/src/julia/base/` and must fall through to
/// `Expr::Var` so the compiler resolves them from `global_types` at compile time.
///
/// **Do NOT add** `"im" => ...`, `"pi" => ...`, or similar here. If you need
/// compile-time type information for a constant, add it to the type inference
/// layer (`compile/expr/infer/`) with a `// Workaround:` comment that names
/// the concrete tracking issue
/// explaining when it can be removed.
///
/// Lower a Julia integer literal to an [`Expr`].
///
/// Decimal literals are returned as plain numeric `Expr::Literal` nodes.
/// Typed-integer literals — hex (`0x…`), binary (`0b…`), and octal (`0o…`) —
/// carry a width tag derived from the literal's textual form and are wrapped
/// in a `UIntN(...)` constructor call so the resulting runtime `Value` has
/// the correct `UInt8` / `UInt16` / `UInt32` / `UInt64` / `UInt128` width
/// (Issue #3559). This propagation is required for downstream consumers
/// such as `MakeRangeLazy` / `derive_range_element_type` which determine the
/// runtime range element type from operand `Value` widths.
pub(super) fn lower_integer_literal<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::parser::span::Span,
) -> LowerResult<Expr> {
    let text = walker.text(&node);
    let parsed = literal::parse_int_typed(text).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
            span,
        )
    })?;
    let inner = match parsed.value {
        literal::ParsedInt::I64(v) => Expr::Literal(Literal::Int(v), span),
        literal::ParsedInt::I128(v) => Expr::Literal(Literal::Int128(v), span),
        literal::ParsedInt::BigInt(v) => Expr::Literal(Literal::BigInt(v), span),
    };
    match parsed.kind {
        Some(kind) => Ok(Expr::Call {
            function: kind.constructor_name().to_string().into(),
            args: vec![inner],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        }),
        None => Ok(inner),
    }
}

fn lower_typed_matrix_expr<'a>(
    walker: &CstWalker<'a>,
    type_node: Node<'a>,
    matrix_node: Node<'a>,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    lower_typed_matrix_expr_impl(walker, type_node, matrix_node, span, None)
}

fn lower_typed_matrix_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    type_node: Node<'a>,
    matrix_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_typed_matrix_expr_impl(walker, type_node, matrix_node, span, Some(lambda_ctx))
}

fn lower_typed_matrix_expr_impl<'a>(
    walker: &CstWalker<'a>,
    type_node: Node<'a>,
    matrix_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    // `[[1 2] [3 4]]` (space-separated bracketed matrices) is parsed by
    // tree-sitter as a `typed_expression` whose "type" position holds another
    // matrix/vector literal rather than a type name. That is not a real type
    // assertion — it is horizontal concatenation, so route it through `hcat`
    // (which flattens the array elements column-wise) instead of treating the
    // first matrix as a type to index (Issue #7203).
    //
    // Three or more matrices (`[[1 2] [3 4] [5 6]]`) nest as
    // `typed_expression(typed_expression(...), [5 6])`; a `TypedExpression`
    // type-side is therefore also part of the same misparse and lowers (via
    // `lower_expr` -> this function) to the left-hand `hcat`.
    if matches!(
        walker.kind(&type_node),
        NodeKind::MatrixExpression | NodeKind::VectorExpression | NodeKind::TypedExpression
    ) {
        let left = match lambda_ctx {
            Some(ctx) => lower_expr_with_ctx(walker, type_node, ctx)?,
            None => lower_expr(walker, type_node)?,
        };
        let right = match lambda_ctx {
            Some(ctx) => lower_expr_with_ctx(walker, matrix_node, ctx)?,
            None => lower_expr(walker, matrix_node)?,
        };
        return Ok(Expr::Call {
            function: "hcat".to_string().into(),
            args: vec![left, right],
            kwargs: vec![],
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    let type_expr = match lambda_ctx {
        Some(ctx) => lower_expr_with_ctx(walker, type_node, ctx)?,
        None => lower_expr(walker, type_node)?,
    };
    let matrix = match lambda_ctx {
        Some(ctx) => lower_matrix_expr_raw_with_ctx(walker, matrix_node, ctx)?,
        None => lower_matrix_expr_raw(walker, matrix_node)?,
    };
    let Expr::ArrayLiteral {
        elements, shape, ..
    } = matrix
    else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "typed_matrix: {}",
                walker.text(&matrix_node)
            )),
            span,
        ));
    };
    // `lower_matrix_expr_raw` may return rank 1 for pure vertical scalar
    // concatenation (`T[1; 2; 3]`) or all-semicolon empty literals (`T[;]`);
    // otherwise `;`/`;;`/`;;;`/... dimension separators reshape into any N,
    // matching upstream's array-literal lowering (Issues #10190, #10379,
    // #10380).
    if shape.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::MalformedMatrix,
            span,
        ));
    }

    let dims: Vec<i64> = shape
        .iter()
        .map(|&d| {
            i64::try_from(d).map_err(|_| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MalformedMatrix, span)
                    .with_hint("typed matrix dimension does not fit Int64")
            })
        })
        .collect::<LowerResult<_>>()?;

    let typed_vector = Expr::Index {
        array: Box::new(type_expr),
        indices: elements,
        span,
    };
    let mut args = Vec::with_capacity(1 + dims.len());
    args.push(typed_vector);
    args.extend(
        dims.into_iter()
            .map(|d| Expr::Literal(Literal::Int(d), span)),
    );
    Ok(Expr::Call {
        function: "reshape".to_string().into(),
        args,
        kwargs: Vec::new(),
        splat_mask: Vec::new(),
        kwargs_splat_mask: Vec::new(),
        span,
    })
}

fn typed_empty_ncat_rank<'a>(walker: &CstWalker<'a>, value_node: Node<'a>) -> Option<usize> {
    if walker.kind(&value_node) != NodeKind::VectorExpression {
        return None;
    }
    let children = walker.named_children_vec(&value_node);
    if children.is_empty()
        || children
            .iter()
            .any(|child| walker.kind(child) != NodeKind::Semicolon)
    {
        return None;
    }
    Some(children.len())
}

fn lower_typed_empty_ncat(element_type: String, rank: usize, span: crate::span::Span) -> Expr {
    let typed_empty = Expr::TypedEmptyArray {
        element_type: element_type.into(),
        span,
    };
    let mut args = Vec::with_capacity(rank + 1);
    args.push(typed_empty);
    args.extend((0..rank).map(|_| Expr::Literal(Literal::Int(0), span)));
    Expr::Call {
        function: "reshape".to_string().into(),
        args,
        kwargs: Vec::new(),
        splat_mask: Vec::new(),
        kwargs_splat_mask: Vec::new(),
        span,
    }
}

fn lower_assignment_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let children = walker.named_children_vec(&node);
    if children.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "assignment expression with insufficient children".to_string(),
            ),
            span,
        ));
    }

    let target = &children[0];
    let value = &children[children.len() - 1];

    // An index/field/tuple assignment used in expression position —
    // `(a[i] = v)`, `(obj.field = v)`, `(a, b) = rhs` — is a valid Julia
    // expression whose value is the assigned RHS value. This shape arises as the
    // RHS of another assignment (`y = (a[1] = 5)`) and, crucially, as the
    // single-expression body of an arrow lambda (`x -> (x[2] = v)`). Route it
    // through the value-producing assignment helper instead of rejecting the
    // non-identifier target (Issues #8007, #9792).
    match walker.kind(target) {
        NodeKind::Identifier => {}
        NodeKind::IndexExpression | NodeKind::FieldExpression | NodeKind::TupleExpression => {
            return match lambda_ctx {
                Some(ctx) => {
                    crate::lowering::stmt::lower_assignment_value_expr_with_ctx(walker, node, ctx)
                }
                None => crate::lowering::stmt::lower_assignment_value_expr(walker, node),
            };
        }
        other => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(format!(
                    "assignment target must be identifier, got {:?}",
                    other
                )),
                span,
            ));
        }
    }

    let var_name = walker.text(target).to_string();
    let value_expr = match lambda_ctx {
        Some(ctx) => lower_expr_with_ctx(walker, *value, ctx)?,
        None => lower_expr(walker, *value)?,
    };

    Ok(Expr::AssignExpr {
        var: var_name.into(),
        value: Box::new(value_expr),
        span,
    })
}

fn is_global_declaration_node<'a>(walker: &CstWalker<'a>, node: &Node<'a>) -> bool {
    matches!(walker.kind(node), NodeKind::GlobalStatement)
        || matches!(node.kind(), "global_statement" | "global_declaration")
}

pub fn lower_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    if is_global_declaration_node(walker, &node) {
        return crate::lowering::stmt::lower_global_value_expr(walker, node, None);
    }
    match walker.kind(&node) {
        NodeKind::ArrowFunctionExpression => call::lower_arrow_value_as_nested(walker, node),
        NodeKind::FunctionDefinition => lower_anonymous_function_value(walker, node, None),
        NodeKind::Identifier => {
            let name = walker.text(&node);
            match name {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                "nothing" => Ok(Expr::Literal(Literal::Nothing, span)),
                "missing" => Ok(Expr::Literal(Literal::Missing, span)),
                // Built-in module values
                "Base" => Ok(Expr::Literal(Literal::Module("Base".to_string()), span)),
                "Core" => Ok(Expr::Literal(Literal::Module("Core".to_string()), span)),
                "Main" => Ok(Expr::Literal(Literal::Module("Main".to_string()), span)),
                "Sys" => Ok(Expr::Literal(Literal::Module("Sys".to_string()), span)),
                // Meta is a submodule of Base, accessible as just "Meta"
                "Meta" => Ok(Expr::Literal(Literal::Module("Meta".to_string()), span)),
                // Note: Don't convert "pi" here - let compiler decide based on whether
                // it's a local variable. This allows `for pi in 1:10` to work.
                // Note: Don't convert "im" here either — it is defined in the prelude
                // as `const im = Complex{Bool}(false, true)`. The compiler resolves
                // its type via type inference (see compile/expr/infer/).
                _ => Ok(Expr::Var(name.to_string().into(), span)),
            }
        }
        NodeKind::IntegerLiteral => lower_integer_literal(walker, node, span),
        NodeKind::FloatLiteral => {
            let text = walker.text(&node);
            let parsed = parse_float(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match parsed {
                literal::ParsedFloat::F64(v) => Ok(Expr::Literal(Literal::Float(v), span)),
                literal::ParsedFloat::F32(v) => Ok(Expr::Literal(Literal::Float32(v), span)),
            }
        }
        NodeKind::StringLiteral => lower_string_literal(walker, node, None),
        NodeKind::CharacterLiteral => lower_char_literal(walker, node),
        NodeKind::BooleanLiteral => {
            let text = walker.text(&node);
            match text {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                _ => Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "boolean literal: {}",
                        text
                    )),
                    span,
                )),
            }
        }
        NodeKind::BinaryExpression => lower_binary_expr(walker, node),
        NodeKind::UnaryExpression => lower_unary_expr(walker, node),
        NodeKind::JuxtapositionExpression => lower_juxtaposition_expr(walker, node),
        NodeKind::CallExpression => lower_call_expr(walker, node),
        NodeKind::ParenthesizedExpression => lower_parenthesized_expr(walker, node),
        NodeKind::RangeExpression => lower_range_expr(walker, node),
        NodeKind::VectorExpression => lower_vector_expr(walker, node),
        NodeKind::MatrixExpression => lower_matrix_expr(walker, node),
        NodeKind::IndexExpression => lower_index_expr(walker, node),
        NodeKind::ComprehensionExpression => lower_comprehension_expr(walker, node),
        NodeKind::GeneratorExpression => lower_generator_expr(walker, node),
        NodeKind::FieldExpression => lower_field_expr(walker, node),
        NodeKind::AdjointExpression => lower_adjoint_expr(walker, node),
        NodeKind::TupleExpression => lower_tuple_expr(walker, node),
        // `(; ...)` semicolon-form named tuple in expression position (incl. `(;)`).
        NodeKind::ParameterList => lower_parameter_list_named_tuple(walker, node),
        NodeKind::BroadcastCallExpression => lower_broadcast_call_expr(walker, node),
        NodeKind::LetStatement | NodeKind::LetExpression => lower_let_expr(walker, node),
        NodeKind::TernaryExpression => lower_ternary_expr(walker, node),
        NodeKind::IfStatement => lower_if_expr(walker, node),
        // try/catch/end as expression: `result = try ... catch ... end`
        // (Issue #4784). Lower the try-statement normally, then rewrite the
        // last `Stmt::Expr` of each branch to assign to a fresh result
        // variable, and wrap the whole thing in a `Expr::LetBlock` that
        // yields the result variable. Mirrors the manual-assignment
        // workaround users had to write.
        NodeKind::TryStatement => lower_try_as_expr(walker, node, None),
        NodeKind::PairExpression => lower_pair_expr(walker, node),
        NodeKind::ParametrizedTypeExpression => {
            // Parametric type as expression: Complex{Float64}, Vector{Int64}, etc.
            // In Julia, this evaluates to the Type itself (a DataType value).
            //
            // For static types (all type args are concrete type names), we emit TypeOf.
            // For dynamic types (type args contain function calls or type variables),
            // we emit DynamicTypeConstruct which evaluates type args at runtime.
            lower_parametrized_type_expr(walker, node, None)
        }
        // `where`-expression in VALUE position: `Tuple{T,T} where T`,
        // `Array{T,N} where {T,N}`, `Vector{T} where T<:Number`. Desugars to a
        // nested `UnionAll(TypeVar(:V), body)` construction (Issue #5047).
        // Declaration-position `where` is handled separately in the function
        // lowering path and never reaches here.
        NodeKind::WhereExpression => lower_where_expression_value(walker, node),
        // Assignment as expression: `cond && (x = value)` evaluates to `value`
        // when it executes and mutates `x`. MacroTools uses this in ordinary
        // function bodies without a macro lowering context (Issue #7566).
        NodeKind::Assignment => lower_assignment_expr(walker, node, None, span),
        NodeKind::MacroCall => lower_macro_expr(walker, node),
        NodeKind::QuoteExpression => lower_quote_expr(walker, node),
        // Handle begin...end blocks as expressions (Issue #1794)
        NodeKind::Block => {
            // The pure parser represents `begin ... end` as an outer
            // `begin_block` containing the real statement `block`. Use the real
            // block here so assignments in a begin condition are compiled in
            // the surrounding local scope instead of an extra nested LetBlock
            // (Issue #7617).
            let actual_block = if node.kind() == "begin_block" {
                walker
                    .named_children(&node)
                    .find(|child| walker.kind(child) == NodeKind::Block)
                    .unwrap_or(node)
            } else {
                node
            };
            // Lower the block contents as statements. `begin...end` is a
            // transparent wrapper (executes unconditionally exactly once),
            // so a nested `struct`/`mutable struct` definition is legal here
            // whenever this block is itself reachable from top level,
            // matching upstream Julia (Issue #10194).
            let children = walker.named_children_vec(&actual_block);
            let stmts =
                crate::lowering::stmt::lower_transparent_block_stmts(walker, children, None)?;
            // Wrap in a LetBlock expression
            let body = crate::ir::core::Block { stmts, span };
            Ok(Expr::LetBlock {
                bindings: vec![],
                body,
                span,
            })
        }
        // Compound assignment as expression: `x += y`, `p.z *= y`, `a[i] -= y`,
        // `Z .+= y`, … evaluate to the newly assigned value (Issue #7269). Used
        // as a return value (`return p.z += 1.0`), the RHS of another assignment
        // (`y = (x += 1)`), or an argument (`println(x += 3)`).
        NodeKind::CompoundAssignment => {
            crate::lowering::stmt::lower_compound_assignment_expr(walker, node)
        }
        // Handle prefixed string literals (r"...", raw"...", big"...", etc.)
        // These are mapped to NodeKind::Other but have a specific raw kind
        _ if node.kind() == "prefixed_string_literal" => {
            // PrefixedStringLiteral has two children: [prefix, string]
            let children = walker.named_children_vec(&node);
            if children.len() >= 2 {
                let prefix_text = walker.text(&children[0]);
                let string_text = walker.text(&children[1]);
                // Remove quotes from the string content
                let content = string_text.trim_matches('"').to_string();

                match prefix_text {
                    "big" => {
                        // big"..." creates BigInt or BigFloat depending on content
                        // If content contains '.' or 'e'/'E' (scientific notation), it's BigFloat
                        if content.contains('.') || content.contains('e') || content.contains('E') {
                            Ok(Expr::Literal(Literal::BigFloat(content), span))
                        } else {
                            Ok(Expr::Literal(Literal::BigInt(content), span))
                        }
                    }
                    "raw" => {
                        // raw"..." creates a raw string literal
                        // In Julia, raw strings still process \\ (to \) and \" (to ")
                        // but all other escape sequences are kept as-is
                        let processed = process_raw_string_escapes(&content);
                        Ok(Expr::Literal(Literal::Str(processed), span))
                    }
                    "r" => {
                        // r"..." is a regex literal in Julia; an optional third child
                        // carries trailing flag characters (`r"abc"i`, Issue #5709).
                        let flags = children
                            .get(2)
                            .map(|c| walker.text(c).to_string())
                            .unwrap_or_default();
                        Ok(Expr::Literal(
                            Literal::Regex {
                                pattern: content,
                                flags,
                            },
                            span,
                        ))
                    }
                    "MIME" => {
                        // MIME"text/plain" -> _mime_construct("text/plain")
                        // This creates a MIME{Symbol("text/plain")} type instance
                        Ok(Expr::Call {
                            function: "_mime_construct".to_string().into(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "v" => Ok(Expr::Call {
                        function: "VersionNumber".to_string().into(),
                        args: version_literal_args(&content, span),
                        kwargs: Vec::new(),
                        splat_mask: Vec::new(),
                        kwargs_splat_mask: Vec::new(),
                        span,
                    }),
                    "b" => {
                        // b"data" creates a byte array (Vector{UInt8})
                        // Convert string to array of UInt8 values
                        let bytes: Vec<Expr> = content
                            .bytes()
                            .map(|b| Expr::Literal(Literal::Int(b as i64), span))
                            .collect();
                        let len = bytes.len();
                        Ok(Expr::ArrayLiteral {
                            elements: bytes,
                            shape: vec![len],
                            span,
                        })
                    }
                    // Upstream Base defines the lowercase @int128_str / @uint128_str
                    // string macros (int128"123" / uint128"123"), parsing to Int128 /
                    // UInt128 respectively (julia/base/int.jl). The capitalized
                    // spellings Int128"..." / UInt128"..." are NOT upstream — they fall
                    // through to the generic `@Prefix_str` path below and raise like any
                    // other undefined string macro (Issues #10320, #10324).
                    "int128" => {
                        // int128"123" parses to an Int128 literal (@int128_str).
                        if let Ok(val) = content.parse::<i128>() {
                            Ok(Expr::Literal(Literal::Int128(val), span))
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid Int128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "uint128" => {
                        // uint128"123" parses to a UInt128 literal (@uint128_str).
                        // Wrap the parsed value in a `UInt128(…)` constructor call so the
                        // runtime Value is a genuine UInt128. The inner literal is a
                        // BigInt (not Int128): values above typemax(Int128) have a
                        // negative i128 bit pattern that the *checked* Int128→UInt128
                        // conversion would reject, whereas BigInt→UInt128 range-checks
                        // against 0..typemax(UInt128) — the same path `0x…` UInt128
                        // literals take.
                        if let Ok(val) = content.parse::<u128>() {
                            Ok(Expr::Call {
                                function: "UInt128".to_string().into(),
                                args: vec![Expr::Literal(Literal::BigInt(val.to_string()), span)],
                                kwargs: Vec::new(),
                                splat_mask: Vec::new(),
                                kwargs_splat_mask: Vec::new(),
                                span,
                            })
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid UInt128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "s" => {
                        // s"..." creates a SubstitutionString (Issue #10174), mirroring
                        // upstream `macro s_str(string) SubstitutionString(string) end`.
                        // The raw content is preserved (capture references like \1 /
                        // \g<name> / \0 are expanded later by `replace`); a plain
                        // String literal would be indistinguishable from a normal
                        // replacement and would be copied verbatim.
                        Ok(Expr::Call {
                            function: "SubstitutionString".to_string().into(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    _ => {
                        // Unknown prefixes are string macros in Julia:
                        // prefix"text" → @prefix_str("text")
                        // e.g., html"<b>bold</b>" → @html_str("<b>bold</b>")
                        //       L"x^2" → @L_str("x^2")
                        let macro_name = format!("{}_str", prefix_text);
                        Ok(Expr::Call {
                            function: macro_name.into(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                }
            } else {
                // Fallback: treat as regular string literal
                let text = walker.text(&node);
                Ok(Expr::Literal(Literal::Str(text.to_string()), span))
            }
        }
        // Jump expressions: used in short-circuit context like `cond && return x`
        NodeKind::ReturnStatement => {
            let children = walker.named_children_vec(&node);
            let value = if children.is_empty() {
                None
            } else {
                Some(Box::new(lower_expr(walker, children[0])?))
            };
            Ok(Expr::ReturnExpr { value, span })
        }
        NodeKind::BreakStatement => Ok(Expr::BreakExpr { span }),
        NodeKind::ContinueStatement => Ok(Expr::ContinueExpr { span }),
        // Handle typed expressions like Int64[] (typed empty array)
        NodeKind::TypedExpression => {
            let children = walker.named_children_vec(&node);
            if children.len() == 2 {
                let type_node = children[0];
                let value_node = children[1];

                // Check if this is a typed empty array: Type[]
                if walker.kind(&type_node) == NodeKind::Identifier
                    && walker.kind(&value_node) == NodeKind::VectorExpression
                {
                    let type_children = walker.named_children_vec(&value_node);
                    if type_children.is_empty() {
                        // Int64[] or similar: typed empty array
                        let element_type = walker.text(&type_node).to_string();
                        return Ok(Expr::TypedEmptyArray {
                            element_type: element_type.into(),
                            span,
                        });
                    }
                }
                if let Some(rank) = typed_empty_ncat_rank(walker, value_node) {
                    return Ok(lower_typed_empty_ncat(
                        walker.text(&type_node).to_string(),
                        rank,
                        span,
                    ));
                }

                // Typed array comprehension: T[expr for x in iter].
                // Julia parses this as `typed_comprehension`; lowering models it
                // as a normal comprehension whose body is converted through T.
                if walker.kind(&value_node) == NodeKind::ComprehensionExpression {
                    let type_name = walker.text(&type_node).to_string();
                    let comprehension = lower_comprehension_expr(walker, value_node)?;
                    return wrap_comprehension_body_with_call(comprehension, type_name, span);
                }

                if walker.kind(&value_node) == NodeKind::MatrixExpression {
                    return lower_typed_matrix_expr(walker, type_node, value_node, span);
                }

                // Type assertion on an arbitrary expression: `expr::T`.
                // For this form the CST orders the children as [value, type],
                // the opposite of the `Type[...]` array-literal forms handled
                // above. `type_node`/`value_node` were named for those array
                // forms; for an assertion `children[0]` is the asserted value
                // and `children[1]` is the type (Issue #5193).
                return lower_type_assertion(walker, type_node, value_node, span, None);
            }
            // Fall through to error for unsupported typed expressions
            Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(format!(
                    "typed_expression: {}",
                    walker.text(&node)
                )),
                span,
            ))
        }
        // Bare operator as expression: treat as function reference (Issue #1985)
        // e.g., f = +; map(f, ...) or passing operators to higher-order functions
        NodeKind::Operator => {
            let op_text = walker.text(&node).to_string();
            Ok(Expr::FunctionRef {
                name: op_text.into(),
                span,
            })
        }
        NodeKind::SplatExpression => {
            // Macro quote-template substitution can leave `$(args...)` as a
            // direct SplatExpression in expression lowering; unwrap to the
            // value being splatted so Expr construction can carry it (Issue #7541).
            let mut children = walker.named_children(&node);
            let Some(inner) = children.next() else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty splat expression".to_string(),
                    ),
                    span,
                ));
            };
            lower_expr(walker, inner)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(node.kind().to_string()),
            span,
        )),
    }
}

/// Lower expression with lambda context (for use within function bodies).
pub fn lower_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    if is_global_declaration_node(walker, &node) {
        return crate::lowering::stmt::lower_global_value_expr(walker, node, Some(lambda_ctx));
    }
    match walker.kind(&node) {
        // An arrow that reaches generic expression lowering is a value in the
        // enclosing lexical scope (assignment RHS, implicit return, collection
        // element, ...). Embed it as a nested FunctionDef so the normal nested
        // function collector can discover its free variables. Lifting it into
        // LambdaContext skips closure-capture analysis and loses enclosing
        // locals whenever this context-aware path is active (Issue #11030).
        // Consumers that need to compose this container (notably `|>`) must
        // keep its FunctionDef statements visible in a LetBlock body.
        NodeKind::ArrowFunctionExpression => {
            lower_arrow_expr_as_nested_with_ctx(walker, node, lambda_ctx)
        }
        NodeKind::FunctionDefinition => {
            lower_anonymous_function_value(walker, node, Some(lambda_ctx))
        }
        NodeKind::CallExpression => lower_call_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::Identifier => {
            let name = walker.text(&node);
            match name {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                "nothing" => Ok(Expr::Literal(Literal::Nothing, span)),
                "missing" => Ok(Expr::Literal(Literal::Missing, span)),
                // Built-in module values
                "Base" => Ok(Expr::Literal(Literal::Module("Base".to_string()), span)),
                "Core" => Ok(Expr::Literal(Literal::Module("Core".to_string()), span)),
                "Main" => Ok(Expr::Literal(Literal::Module("Main".to_string()), span)),
                "Sys" => Ok(Expr::Literal(Literal::Module("Sys".to_string()), span)),
                // Meta is a submodule of Base, accessible as just "Meta"
                "Meta" => Ok(Expr::Literal(Literal::Module("Meta".to_string()), span)),
                _ => Ok(Expr::Var(name.to_string().into(), span)),
            }
        }
        NodeKind::IntegerLiteral => lower_integer_literal(walker, node, span),
        NodeKind::FloatLiteral => {
            let text = walker.text(&node);
            let parsed = parse_float(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match parsed {
                literal::ParsedFloat::F64(v) => Ok(Expr::Literal(Literal::Float(v), span)),
                literal::ParsedFloat::F32(v) => Ok(Expr::Literal(Literal::Float32(v), span)),
            }
        }
        NodeKind::StringLiteral => lower_string_literal(walker, node, Some(lambda_ctx)),
        NodeKind::CharacterLiteral => lower_char_literal(walker, node),
        NodeKind::BooleanLiteral => {
            let text = walker.text(&node);
            match text {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                _ => Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "boolean literal: {}",
                        text
                    )),
                    span,
                )),
            }
        }
        NodeKind::BinaryExpression => lower_binary_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::UnaryExpression => lower_unary_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::JuxtapositionExpression => lower_juxtaposition_expr(walker, node),
        NodeKind::ParenthesizedExpression => {
            lower_parenthesized_expr_with_ctx(walker, node, lambda_ctx)
        }
        NodeKind::RangeExpression => lower_range_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::VectorExpression => lower_vector_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::MatrixExpression => lower_matrix_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::IndexExpression => lower_index_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::ComprehensionExpression => {
            lower_comprehension_expr_with_ctx(walker, node, lambda_ctx)
        }
        NodeKind::GeneratorExpression => lower_generator_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::FieldExpression => lower_field_expr(walker, node),
        NodeKind::AdjointExpression => lower_adjoint_expr(walker, node),
        NodeKind::TupleExpression => lower_tuple_expr_with_ctx(walker, node, lambda_ctx),
        // `(; ...)` semicolon-form named tuple in expression position (incl. `(;)`).
        NodeKind::ParameterList => lower_parameter_list_named_tuple(walker, node),
        NodeKind::BroadcastCallExpression => lower_broadcast_call_expr(walker, node),
        // Propagate the lambda context into the `let` body so context-dependent
        // macros (e.g. `@test`, which checks `using Test` via the context) expand
        // correctly inside `let` blocks (Issue #7189). Using the ctx-agnostic
        // `lower_let_expr` here dropped the `using` set and made `@test` fail with
        // a misleading "requires `using Test`" error.
        NodeKind::LetStatement | NodeKind::LetExpression => {
            lower_let_expr_with_ctx(walker, node, lambda_ctx)
        }
        NodeKind::TernaryExpression => lower_ternary_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::IfStatement => lower_if_expr_with_ctx(walker, node, lambda_ctx),
        // Keep the live lowering context through every try/catch/finally block:
        // top-level runtime declarations and captured lambdas both depend on it.
        NodeKind::TryStatement => lower_try_as_expr(walker, node, Some(lambda_ctx)),
        NodeKind::PairExpression => lower_pair_expr(walker, node),
        NodeKind::ParametrizedTypeExpression => {
            // Parametric type as expression: Complex{Float64}, Vector{Int64}, etc.
            // In Julia, this evaluates to the Type itself (a DataType value).
            //
            // Keep this in sync with `lower_expr`: dynamic type arguments such
            // as `Tuple{typeof(g)}` must be evaluated instead of preserved as a
            // literal type string.
            lower_parametrized_type_expr(walker, node, Some(lambda_ctx))
        }
        // `where`-expression in VALUE position (Issue #5047). The body is a type
        // expression with free type variables, so the ctx-agnostic value-position
        // helper (which forces the static type-name path) is correct here too.
        NodeKind::WhereExpression => lower_where_expression_value(walker, node),
        NodeKind::MacroCall => lower_macro_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::QuoteExpression => lower_quote_expr(walker, node),
        // Handle begin...end blocks as expressions
        NodeKind::Block => {
            // The pure parser represents `begin ... end` as an outer
            // `begin_block` containing the real statement `block`. Use the real
            // block here so assignments in a begin condition are compiled in
            // the surrounding local scope instead of an extra nested LetBlock
            // (Issue #7617).
            let actual_block = if node.kind() == "begin_block" {
                walker
                    .named_children(&node)
                    .find(|child| walker.kind(child) == NodeKind::Block)
                    .unwrap_or(node)
            } else {
                node
            };
            // Lower the block contents as statements. `begin...end` is a
            // transparent wrapper (executes unconditionally exactly once),
            // so a nested `struct`/`mutable struct` definition is legal here
            // whenever this block is itself reachable from top level,
            // matching upstream Julia — this is the path a stdlib/Base
            // macro's static template substitution takes when converting a
            // raw `begin...end` CST argument (e.g. `Test.@testset`'s body)
            // into IR, so it is also what makes a `struct` nested inside
            // `@testset "..." begin ... end` lower correctly (Issue #10194).
            let children = walker.named_children_vec(&actual_block);
            let stmts = crate::lowering::stmt::lower_transparent_block_stmts(
                walker,
                children,
                Some(lambda_ctx),
            )?;
            // Wrap in a LetBlock expression
            let body = crate::ir::core::Block { stmts, span };
            Ok(Expr::LetBlock {
                bindings: vec![],
                body,
                span,
            })
        }
        // Assignment as expression: x = value evaluates to value and assigns it to x
        // This is used for chained assignments like `local result = x = 42`
        NodeKind::Assignment => lower_assignment_expr(walker, node, Some(lambda_ctx), span),
        // Compound assignment as expression: `x += y`, `p.z *= y`, `a[i] -= y`,
        // `Z .+= y`, … evaluate to the newly assigned value (Issue #7269). Used
        // as a return value (`return p.z += 1.0`), the RHS of another assignment
        // (`y = (x += 1)`), or an argument (`println(x += 3)`).
        NodeKind::CompoundAssignment => {
            crate::lowering::stmt::lower_compound_assignment_expr_with_ctx(walker, node, lambda_ctx)
        }
        // Handle prefixed string literals (r"...", raw"...", big"...", etc.)
        _ if node.kind() == "prefixed_string_literal" => {
            // PrefixedStringLiteral has two children: [prefix, string]
            let children = walker.named_children_vec(&node);
            if children.len() >= 2 {
                let prefix_text = walker.text(&children[0]);
                let string_text = walker.text(&children[1]);
                let content = string_text.trim_matches('"').to_string();

                match prefix_text {
                    "big" => {
                        // big"..." creates BigInt or BigFloat depending on content
                        if content.contains('.') || content.contains('e') || content.contains('E') {
                            Ok(Expr::Literal(Literal::BigFloat(content), span))
                        } else {
                            Ok(Expr::Literal(Literal::BigInt(content), span))
                        }
                    }
                    "raw" => {
                        // raw"..." creates a raw string literal
                        // In Julia, raw strings still process \\ (to \) and \" (to ")
                        // but all other escape sequences are kept as-is
                        let processed = process_raw_string_escapes(&content);
                        Ok(Expr::Literal(Literal::Str(processed), span))
                    }
                    "r" => {
                        // r"..." is a regex literal in Julia; an optional third child
                        // carries trailing flag characters (`r"abc"i`, Issue #5709).
                        let flags = children
                            .get(2)
                            .map(|c| walker.text(c).to_string())
                            .unwrap_or_default();
                        Ok(Expr::Literal(
                            Literal::Regex {
                                pattern: content,
                                flags,
                            },
                            span,
                        ))
                    }
                    "MIME" => {
                        // MIME"text/plain" -> _mime_construct("text/plain")
                        // This creates a MIME{Symbol("text/plain")} type instance
                        Ok(Expr::Call {
                            function: "_mime_construct".to_string().into(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "v" => Ok(Expr::Call {
                        function: "VersionNumber".to_string().into(),
                        args: version_literal_args(&content, span),
                        kwargs: Vec::new(),
                        splat_mask: Vec::new(),
                        kwargs_splat_mask: Vec::new(),
                        span,
                    }),
                    "b" => {
                        // b"data" creates a byte array (Vector{UInt8})
                        // Convert string to array of UInt8 values
                        let bytes: Vec<Expr> = content
                            .bytes()
                            .map(|b| Expr::Literal(Literal::Int(b as i64), span))
                            .collect();
                        let len = bytes.len();
                        Ok(Expr::ArrayLiteral {
                            elements: bytes,
                            shape: vec![len],
                            span,
                        })
                    }
                    // Upstream Base defines the lowercase @int128_str / @uint128_str
                    // string macros (int128"123" / uint128"123"), parsing to Int128 /
                    // UInt128 respectively (julia/base/int.jl). The capitalized
                    // spellings Int128"..." / UInt128"..." are NOT upstream — they fall
                    // through to the generic `@Prefix_str` path below and raise like any
                    // other undefined string macro (Issues #10320, #10324).
                    "int128" => {
                        // int128"123" parses to an Int128 literal (@int128_str).
                        if let Ok(val) = content.parse::<i128>() {
                            Ok(Expr::Literal(Literal::Int128(val), span))
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid Int128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "uint128" => {
                        // uint128"123" parses to a UInt128 literal (@uint128_str).
                        // Wrap the parsed value in a `UInt128(…)` constructor call so the
                        // runtime Value is a genuine UInt128. The inner literal is a
                        // BigInt (not Int128): values above typemax(Int128) have a
                        // negative i128 bit pattern that the *checked* Int128→UInt128
                        // conversion would reject, whereas BigInt→UInt128 range-checks
                        // against 0..typemax(UInt128) — the same path `0x…` UInt128
                        // literals take.
                        if let Ok(val) = content.parse::<u128>() {
                            Ok(Expr::Call {
                                function: "UInt128".to_string().into(),
                                args: vec![Expr::Literal(Literal::BigInt(val.to_string()), span)],
                                kwargs: Vec::new(),
                                splat_mask: Vec::new(),
                                kwargs_splat_mask: Vec::new(),
                                span,
                            })
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid UInt128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "s" => {
                        // s"..." creates a SubstitutionString (Issue #10174), mirroring
                        // upstream `macro s_str(string) SubstitutionString(string) end`.
                        // The raw content is preserved (capture references like \1 /
                        // \g<name> / \0 are expanded later by `replace`); a plain
                        // String literal would be indistinguishable from a normal
                        // replacement and would be copied verbatim.
                        Ok(Expr::Call {
                            function: "SubstitutionString".to_string().into(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    _ => {
                        // Unknown prefixes are string macros in Julia:
                        // prefix"text" → @prefix_str("text")
                        // e.g., html"<b>bold</b>" → @html_str("<b>bold</b>")
                        //       L"x^2" → @L_str("x^2")
                        let macro_name = format!("{}_str", prefix_text);
                        Ok(Expr::Call {
                            function: macro_name.into(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                }
            } else {
                let text = walker.text(&node);
                Ok(Expr::Literal(Literal::Str(text.to_string()), span))
            }
        }
        // Jump expressions: used in short-circuit context like `cond && return x`
        NodeKind::ReturnStatement => {
            let children = walker.named_children_vec(&node);
            let value = if children.is_empty() {
                None
            } else {
                Some(Box::new(lower_expr_with_ctx(
                    walker,
                    children[0],
                    lambda_ctx,
                )?))
            };
            Ok(Expr::ReturnExpr { value, span })
        }
        NodeKind::BreakStatement => Ok(Expr::BreakExpr { span }),
        NodeKind::ContinueStatement => Ok(Expr::ContinueExpr { span }),
        // Handle typed expressions like Int64[] (typed empty array)
        NodeKind::TypedExpression => {
            let children = walker.named_children_vec(&node);
            if children.len() == 2 {
                let type_node = children[0];
                let value_node = children[1];

                // Check if this is a typed empty array: Type[]
                if walker.kind(&type_node) == NodeKind::Identifier
                    && walker.kind(&value_node) == NodeKind::VectorExpression
                {
                    let type_children = walker.named_children_vec(&value_node);
                    if type_children.is_empty() {
                        // Int64[] or similar: typed empty array
                        let element_type = walker.text(&type_node).to_string();
                        return Ok(Expr::TypedEmptyArray {
                            element_type: element_type.into(),
                            span,
                        });
                    }
                }
                if let Some(rank) = typed_empty_ncat_rank(walker, value_node) {
                    return Ok(lower_typed_empty_ncat(
                        walker.text(&type_node).to_string(),
                        rank,
                        span,
                    ));
                }

                // Typed array comprehension: T[expr for x in iter].
                // Julia parses this as `typed_comprehension`; lowering models it
                // as a normal comprehension whose body is converted through T.
                if walker.kind(&value_node) == NodeKind::ComprehensionExpression {
                    let type_name = walker.text(&type_node).to_string();
                    let comprehension =
                        lower_comprehension_expr_with_ctx(walker, value_node, lambda_ctx)?;
                    return wrap_comprehension_body_with_call(comprehension, type_name, span);
                }

                if walker.kind(&value_node) == NodeKind::MatrixExpression {
                    return lower_typed_matrix_expr_with_ctx(
                        walker, type_node, value_node, span, lambda_ctx,
                    );
                }

                // Type assertion on an arbitrary expression: `expr::T`.
                // CST orders the children as [value, type] for this form,
                // opposite to the `Type[...]` array-literal forms above; here
                // `type_node` holds the value and `value_node` the type
                // (Issue #5193).
                return lower_type_assertion(walker, type_node, value_node, span, Some(lambda_ctx));
            }
            // Fall through to error for unsupported typed expressions
            Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(format!(
                    "typed_expression: {}",
                    walker.text(&node)
                )),
                span,
            ))
        }
        // Bare operator as expression: treat as function reference (Issue #1985)
        NodeKind::Operator => {
            let op_text = walker.text(&node).to_string();
            Ok(Expr::FunctionRef {
                name: op_text.into(),
                span,
            })
        }
        NodeKind::SplatExpression => {
            // See the non-context lowering arm above (Issue #7541).
            let mut children = walker.named_children(&node);
            let Some(inner) = children.next() else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty splat expression".to_string(),
                    ),
                    span,
                ));
            };
            lower_expr_with_ctx(walker, inner, lambda_ctx)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(node.kind().to_string()),
            span,
        )),
    }
}

/// Lower a parametric type expression like `Complex{Float64}` or `Complex{promote_type(T, S)}`.
///
/// For static types (all type args are concrete type names), emits a DataType literal.
/// For dynamic types (type args contain function calls or type variables that aren't
/// known concrete types), emits DynamicTypeConstruct which evaluates type args at runtime.
pub fn lower_arrow_expr_as_nested_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    call::lower_arrow_value_as_nested_with_ctx(walker, node, lambda_ctx)
}

fn lower_parametrized_type_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    // Extract the base and type arguments. For ordinary `Vector{Int}` the base
    // is the first identifier. Julia also accepts expression bases such as
    // `(Vector{T} where T){Int}`; those must be evaluated and applied at
    // runtime rather than misclassifying `Int` as the base.
    let mut base_name: Option<String> = None;
    let mut explicit_base_expr: Option<Expr> = None;
    let mut type_arg_nodes: Vec<Node<'a>> = Vec::new();
    let mut saw_base = false;

    for child in children {
        match walker.kind(&child) {
            NodeKind::Identifier if !saw_base => {
                base_name = Some(walker.text(&child).to_string());
                saw_base = true;
            }
            NodeKind::FieldExpression if !saw_base => {
                base_name = Some(walker.text(&child).to_string());
                saw_base = true;
            }
            NodeKind::CurlyExpression => {
                // Type arguments inside curly braces
                for type_arg in walker.children(&child) {
                    let text = walker.text(&type_arg);
                    if matches!(text, "{" | "}" | ",") {
                        continue;
                    }
                    type_arg_nodes.push(type_arg);
                }
            }
            _ => {
                if !saw_base {
                    base_name = Some(strip_single_outer_parens(walker.text(&child)).to_string());
                    explicit_base_expr = Some(match lambda_ctx {
                        Some(ctx) => lower_expr_with_ctx(walker, child, ctx)?,
                        None => lower_expr(walker, child)?,
                    });
                    saw_base = true;
                } else {
                    // Type argument is an identifier
                    type_arg_nodes.push(child);
                }
            }
        }
    }

    let has_runtime_base_expr = explicit_base_expr.is_some();
    let base_is_active_type_param = base_name
        .as_deref()
        .is_some_and(|base| lambda_ctx.is_some_and(|ctx| ctx.is_active_type_param(base)));
    let base_is_active_value_param = base_name
        .as_deref()
        .is_some_and(|base| lambda_ctx.is_some_and(|ctx| ctx.is_active_value_param(base)));
    let has_dynamic_base = has_runtime_base_expr
        || base_is_active_type_param
        || base_is_active_value_param
        || base_name
            .as_deref()
            .is_some_and(parametrized_base_needs_runtime_lookup);

    // Check if any type argument is dynamic (call expression, or identifier that's not
    // a known concrete type)
    let has_dynamic_arg = type_arg_nodes
        .iter()
        .any(|n| is_dynamic_type_arg(walker, *n, lambda_ctx));

    if !has_dynamic_base && !has_dynamic_arg {
        // All static - emit a type-object literal. Do not encode this as
        // `typeof("<type name>")`: ordinary strings with type-name contents
        // must keep `typeof(::String) === String` (Issue #9741).
        // Issue #5055: expand user-defined type aliases (`MyVec{Int}` ->
        // `Vector{Int}`) so the embedded type-name literal names the target.
        let name = crate::lowering::type_alias::expand(walker.text(&node));
        Ok(Expr::Literal(
            crate::ir::core::Literal::DataType(name),
            span,
        ))
    } else {
        // Has dynamic args - emit DynamicTypeConstruct
        let base = base_name.ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(
                    "parametric type without base name".to_string(),
                ),
                span,
            )
        })?;
        // Issue #5055: expand a parametric-alias base to its target base name
        // (`MyVec` -> `Vector`) for the runtime parametric-type construction.
        // Only the base name is needed here; the type arguments are evaluated
        // dynamically below.
        let expanded_base = if base_is_active_type_param || base_is_active_value_param {
            base.clone()
        } else {
            crate::lowering::type_alias::expand(&base)
        };
        // An alias whose target is itself an APPLIED type (`w = Plain{Int64}`
        // registers `w -> "Plain{Int64}"`) denotes a partial UnionAll: the
        // outer parameter list must be applied to that partial value at
        // runtime, NOT to the stripped bare base — stripping re-bound the
        // first binder and dropped/duplicated parameters (Issue #10643). Keep
        // the full expanded target as an explicit DataType-literal base so
        // `ApplyTypeDynamic` appends the remaining parameters with the same
        // arity/bound validation as `Core.apply_type(w, ...)`.
        let expanded_base_is_applied = expanded_base.contains('{');
        let base = match expanded_base.split_once('{') {
            Some((b, _)) => b.to_string(),
            None => expanded_base.clone(),
        };
        let base_expr = if let Some(expr) = explicit_base_expr {
            Some(Box::new(expr))
        } else if has_dynamic_base {
            if expanded_base_is_applied {
                Some(Box::new(Expr::Literal(
                    crate::ir::core::Literal::DataType(expanded_base),
                    span,
                )))
            } else {
                Some(Box::new(Expr::Var(base.clone().into(), span)))
            }
        } else {
            None
        };

        // Lower each type argument as an expression. A splatted argument
        // (`Tuple{xs...}`) lowers its INNER expression (the collection of type
        // values) and records `true` in a parallel `splat_mask` so the VM can
        // flatten it at runtime (Issue #5112).
        let mut type_args: Vec<Expr> = Vec::with_capacity(type_arg_nodes.len());
        let mut splat_mask: Vec<bool> = Vec::with_capacity(type_arg_nodes.len());
        // Recovering the type-argument list from the raw source text only works
        // when the base is a bare name: `split_parametric_source_args` grabs the
        // FIRST top-level `{...}` group, which for a chained application whose
        // base is itself a parametrized type (`Array{Float64}{2}`) or a
        // parenthesized `where` (`(Vector{T} where T){Int}`) is the BASE's
        // parameter list, not this level's. When the base was lowered to an
        // explicit expression, the outer arguments are already captured in
        // `type_arg_nodes`, so use those directly (Issue #10586).
        let source_args = if has_dynamic_base && !has_runtime_base_expr {
            split_parametric_source_args(walker.text(&node))
        } else {
            Vec::new()
        };
        if !has_dynamic_arg
            && !source_args.is_empty()
            && source_args
                .iter()
                .all(|arg| !arg.contains("...") && source_type_arg_is_simple(arg))
        {
            for arg in source_args {
                type_args.push(lower_type_arg_text_as_value_expr(&arg, span));
                splat_mask.push(false);
            }
        } else {
            for n in &type_arg_nodes {
                if walker.kind(n) == NodeKind::SplatExpression {
                    let inner = walker.named_children(n).next().ok_or_else(|| {
                        UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(
                                "splat in parametric type without inner expression".to_string(),
                            ),
                            span,
                        )
                    })?;
                    type_args.push(lower_parametric_type_arg_expr(
                        walker, inner, span, lambda_ctx,
                    )?);
                    splat_mask.push(true);
                } else {
                    type_args.push(lower_parametric_type_arg_expr(
                        walker, *n, span, lambda_ctx,
                    )?);
                    splat_mask.push(false);
                }
            }
        }

        // Drop the mask entirely when no argument splats, keeping the common
        // fixed-arity path byte-for-byte identical to the pre-#5112 cache.
        let splat_mask = if splat_mask.iter().any(|&b| b) {
            splat_mask
        } else {
            Vec::new()
        };

        Ok(Expr::DynamicTypeConstruct {
            base: base.into(),
            base_expr,
            type_args,
            splat_mask,
            span,
        })
    }
}

fn lower_parametric_type_arg_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    if let Some(expr) = lower_anonymous_bound_type_arg(walker, node, span, lambda_ctx)? {
        return Ok(expr);
    }
    match lambda_ctx {
        Some(ctx) => lower_expr_with_ctx(walker, node, ctx),
        None => lower_expr(walker, node),
    }
}

fn lower_anonymous_bound_type_arg<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Option<Expr>> {
    if walker.kind(&node) != NodeKind::UnaryExpression {
        return Ok(None);
    }

    let children = walker.named_children_vec(&node);
    let op = children
        .iter()
        .find(|child| walker.kind(child) == NodeKind::Operator)
        .map(|child| walker.text(child));
    let Some(op) = op else {
        return Ok(None);
    };
    if !matches!(op, "<:" | ">:") {
        return Ok(None);
    }

    let bound_node = children
        .iter()
        .rev()
        .find(|child| walker.kind(child) != NodeKind::Operator)
        .ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(
                    "anonymous bounded parametric type argument".to_string(),
                ),
                walker.span(&node),
            )
        })?;
    if let Some(ctx) =
        lambda_ctx.filter(|ctx| node_contains_active_type_param(walker, *bound_node, ctx))
    {
        let bound_expr = lower_expr_with_ctx(walker, *bound_node, ctx)?;
        let (lower_bound, upper_bound) = if op == "<:" {
            (None, Some(bound_expr))
        } else {
            (Some(bound_expr), None)
        };
        return Ok(Some(make_typevar_expr_from_value_bounds(
            crate::types::SOURCE_ANONYMOUS_TYPEVAR_NAME,
            lower_bound,
            upper_bound,
            span,
        )));
    }

    let bound = crate::lowering::type_alias::expand(walker.text(bound_node));
    let (lower_bound, upper_bound) = if op == "<:" {
        (None, Some(bound))
    } else {
        (Some(bound), None)
    };

    Ok(Some(make_typevar_expr(
        crate::types::SOURCE_ANONYMOUS_TYPEVAR_NAME,
        lower_bound.as_ref(),
        upper_bound.as_ref(),
        span,
    )))
}

/// Lower a `where`-expression in VALUE/expression position to a `UnionAll` type
/// value (Issue #5047, advances #5049/#5053).
///
/// `Body where {V1, V2<:Bound, ...}` desugars to a nested `UnionAll`
/// construction `UnionAll(TypeVar(:V1), UnionAll(TypeVar(:V2, Union{}, Bound),
/// Body))`, where the LEFTMOST variable is the OUTERMOST binder (matching
/// upstream Julia: `Array{T,N} where {T,N}` is `Array{T,N} where N where T`).
///
/// The body is emitted as a type-name literal, so its free type variables (`T`, `N`) parse into
/// `TypeVar`s rather than being evaluated as runtime variable references — the
/// latter would raise `UndefVarError`, which is exactly how the bare dynamic
/// path (`Tuple{T,T}` as a value) currently fails.
///
/// This handles only value position. Declaration-position `where` (function
/// signatures / struct definitions) is lowered elsewhere and never reaches here.
fn lower_where_expression_value<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    lower_where_expression_value_with_scope(walker, node, &[])
}

fn lower_where_expression_value_with_scope<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    outer_type_params: &[crate::types::TypeParam],
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);
    if children.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("where_expression".to_string()),
            span,
        ));
    }

    let body_node = children[0];
    let constraints_node = children[children.len() - 1];

    // Parse the `where {...}` constraints into ordered type parameters, reusing
    // the same parser as the declaration-position path so bounds are identical.
    let type_params = parse_value_where_type_constraints(walker, constraints_node)?;
    if type_params.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("where_expression".to_string()),
            span,
        ));
    }
    let mut bound_scope = type_params.clone();
    bound_scope.extend_from_slice(outer_type_params);

    if let Some(expr) =
        lower_subtype_where_rhs(walker, body_node, &type_params, &bound_scope, span)?
    {
        return Ok(expr);
    }

    let body_expr = lower_where_body_expr(walker, body_node, &type_params, &bound_scope, span)?;
    Ok(wrap_where_expr(&type_params, &bound_scope, body_expr, span))
}

fn lower_where_body_expr<'a>(
    walker: &CstWalker<'a>,
    body_node: Node<'a>,
    type_params: &[crate::types::TypeParam],
    bound_scope: &[crate::types::TypeParam],
    span: Span,
) -> LowerResult<Expr> {
    // Chained value-position `where` (`Body where A where B`) nests another
    // WhereExpression on the left; recurse so the inner binder stays innermost.
    Ok(match walker.kind(&body_node) {
        NodeKind::WhereExpression => {
            lower_where_expression_value_with_scope(walker, body_node, bound_scope)?
        }
        NodeKind::ParenthesizedExpression => {
            let inner = walker.named_children(&body_node).next().ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty parenthesized where body".to_string(),
                    ),
                    span,
                )
            })?;
            lower_where_body_expr(walker, inner, type_params, bound_scope, span)?
        }
        NodeKind::Identifier => {
            let body_name = walker.text(&body_node);
            if type_params.iter().any(|tp| tp.name == body_name) {
                Expr::Var(where_typevar_local_name(body_name).into(), span)
            } else {
                type_name_value(&crate::lowering::type_alias::expand(body_name), span)
            }
        }
        _ => {
            // Force the static type-name path, which parses free type variables
            // as `TypeVar`s. Expand user-defined type
            // aliases (Issue #5055) so an aliased base names its target.
            let body_name = crate::lowering::type_alias::expand(walker.text(&body_node));
            type_name_value(&body_name, span)
        }
    })
}

fn wrap_where_expr(
    type_params: &[crate::types::TypeParam],
    bound_scope: &[crate::types::TypeParam],
    mut body_expr: Expr,
    span: Span,
) -> Expr {
    // Wrap innermost-first: iterate parameters in reverse so the first-listed
    // variable becomes the outermost `UnionAll`.
    for tp in type_params.iter().rev() {
        let local_name = where_typevar_local_name(&tp.name);
        let typevar_expr = make_typevar_expr_with_scope(
            &tp.name,
            tp.lower_bound.as_ref(),
            tp.get_upper_bound(),
            span,
            bound_scope,
        );
        let unionall = Expr::Call {
            function: "UnionAll".to_string().into(),
            args: vec![Expr::Var(local_name.clone().into(), span), body_expr],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
        body_expr = Expr::LetBlock {
            bindings: vec![(local_name.into(), typevar_expr)],
            body: crate::ir::core::Block {
                stmts: vec![crate::ir::core::Stmt::Expr {
                    expr: unionall,
                    span,
                }],
                span,
            },
            span,
        };
    }
    body_expr
}

fn where_typevar_local_name(name: &str) -> String {
    format!("__sjulia_where_typevar_{name}")
}

fn lower_subtype_where_rhs<'a>(
    walker: &CstWalker<'a>,
    body_node: Node<'a>,
    type_params: &[crate::types::TypeParam],
    bound_scope: &[crate::types::TypeParam],
    span: Span,
) -> LowerResult<Option<Expr>> {
    if walker.kind(&body_node) != NodeKind::BinaryExpression {
        return Ok(None);
    }
    let has_subtype_operator = walker
        .children(&body_node)
        .into_iter()
        .any(|child| walker.kind(&child) == NodeKind::Operator && walker.text(&child) == "<:");
    if !has_subtype_operator {
        return Ok(None);
    }
    let operands: Vec<_> = walker
        .named_children(&body_node)
        .filter(|child| walker.kind(child) != NodeKind::Operator)
        .collect();
    if operands.len() != 2 {
        return Ok(None);
    }
    let left = lower_expr(walker, operands[0])?;
    let rhs_body = lower_where_body_expr(walker, operands[1], type_params, bound_scope, span)?;
    let right = wrap_where_expr(type_params, bound_scope, rhs_body, span);
    Ok(Some(Expr::BinaryOp {
        op: BinaryOp::Subtype,
        left: Box::new(left),
        right: Box::new(right),
        span,
    }))
}

/// Build a runtime TypeVar constructor without an enclosing binder scope.
fn make_typevar_expr(
    name: &str,
    lower_bound: Option<&String>,
    upper_bound: Option<&String>,
    span: crate::span::Span,
) -> Expr {
    make_typevar_expr_with_scope(name, lower_bound, upper_bound, span, &[])
}

/// Build the runtime TypeVar constructor for a value-position `where` binder.
fn make_typevar_expr_with_scope(
    name: &str,
    lower_bound: Option<&String>,
    upper_bound: Option<&String>,
    span: crate::span::Span,
    bound_scope: &[crate::types::TypeParam],
) -> Expr {
    let (lower_bound, upper_bound) = if lower_bound.is_some() || upper_bound.is_some() {
        // `TypeVar(:T, lower, upper)`: a declared lower bound (`where Int<:T<:..`
        // or `where T>:Int`) is passed through; otherwise it defaults to `Union{}`
        // (Bottom). The upper defaults to `Any` when only a lower is declared
        // (Issue #5650). Both are passed as type values.
        let lower_bound = match lower_bound {
            Some(l) => typevar_bound_value_expr(l, span, bound_scope, Some(name)),
            None => type_name_value(&crate::types::JuliaType::Bottom.name(), span),
        };
        let upper_bound = match upper_bound {
            Some(u) => typevar_bound_value_expr(u, span, bound_scope, Some(name)),
            None => type_name_value(&crate::types::JuliaType::Any.name(), span),
        };
        (Some(lower_bound), Some(upper_bound))
    } else {
        (None, None)
    };
    make_typevar_expr_from_value_bounds(name, lower_bound, upper_bound, span)
}

fn make_typevar_expr_from_value_bounds(
    name: &str,
    lower_bound: Option<Expr>,
    upper_bound: Option<Expr>,
    span: crate::span::Span,
) -> Expr {
    let name_arg = Expr::Literal(Literal::Symbol(name.to_string()), span);
    let mut args = vec![name_arg];
    if lower_bound.is_some() || upper_bound.is_some() {
        args.push(
            lower_bound
                .unwrap_or_else(|| type_name_value(&crate::types::JuliaType::Bottom.name(), span)),
        );
        args.push(
            upper_bound
                .unwrap_or_else(|| type_name_value(&crate::types::JuliaType::Any.name(), span)),
        );
    }
    Expr::Call {
        function: "TypeVar".to_string().into(),
        args,
        kwargs: Vec::new(),
        splat_mask: Vec::new(),
        kwargs_splat_mask: Vec::new(),
        span,
    }
}

fn typevar_bound_value_expr(
    bound: &str,
    span: crate::span::Span,
    bound_scope: &[crate::types::TypeParam],
    current_binder: Option<&str>,
) -> Expr {
    let mut skipped_current_binder = false;
    for tp in bound_scope {
        if tp.name != bound {
            continue;
        }
        if current_binder == Some(bound) && !skipped_current_binder {
            skipped_current_binder = true;
            continue;
        }
        // Resolve every binder reference through its already-constructed
        // lexical value. Reconstructing `TypeVar(:Y, ...)` for `Z<:Y` creates
        // a distinct rigid object, so dependent bounds lose the identity of
        // the enclosing `UnionAll` binder. The wrapping order makes earlier
        // binders visible here; a forward reference loads an unbound local and
        // raises `UndefVarError`, matching Julia. Skipping the current binder
        // above preserves outer same-name shadowing semantics.
        return Expr::Var(where_typevar_local_name(&tp.name).into(), span);
    }
    // Issue #10226: `bound` is neither a where-binder in scope (checked
    // above) nor, as checked next, a recognized builtin/static type name.
    // `type_name_value` unconditionally treats ANY unresolved name as a
    // nominal `Struct(name)` placeholder at runtime
    // (`JuliaType::from_name_or_struct`), which silently accepts a typo'd
    // or genuinely undefined bound (`Vector{Int64} where
    // Int64<:SomeUndefinedName123`, `Vector{T} where T<:T` with no outer
    // `T`) instead of raising `UndefVarError` like upstream Julia.
    //
    // Upstream resolves a where-bound expression as an ordinary expression
    // evaluated in the scope enclosing the `where` (see `julia/src/`
    // lowering of `where`: the bound is just another expression, not a
    // special type-only grammar) — it happens to succeed for builtin type
    // names, user struct/abstract type names, and type aliases because
    // those are all ordinary global bindings, and fails with `UndefVarError`
    // only when the name truly has no binding.
    //
    // Mirror that: for a *bare identifier* bound that isn't a known static
    // type, lower it the same way any other bare identifier reference is
    // lowered elsewhere in this module (see the `NodeKind::Identifier` arm
    // of `lower_expr`) — as `Expr::Var`. The compiler resolves that through
    // the ordinary variable-lookup path (typically `Instr::LoadAny`, which
    // also checks frame-local `type_bindings` for enclosing `where`
    // type-parameters before falling back to globals — see
    // `vm/exec/locals.rs`). That correctly resolves legitimately-declared
    // globals (user structs/abstract types register a real global binding
    // under their name, as does a plain `Alias = Int64` reassignment) and
    // raises `UndefVarError` for anything else, exactly matching upstream.
    //
    // Compound bound expressions (`Vector{Int}`, `Union{Int,Float64}`,
    // `Base.Number`, ...) are deliberately excluded from this fallback: they
    // are not single identifiers, so `Expr::Var` cannot represent them, and
    // qualified/parametric bound resolution is a separate, pre-existing gap
    // outside this issue's scope — leave them on the permissive
    // `type_name_value` path unchanged.
    if crate::types::JuliaType::from_name(bound).is_none() && is_bare_identifier_name(bound) {
        return Expr::Var(bound.to_string().into(), span);
    }
    type_name_value(bound, span)
}

/// Whether `name` is a single bare identifier token (`T`, `Foo`, `_x1`, …)
/// as opposed to a compound type expression (`Vector{Int}`, `Base.Number`,
/// `Union{Int,Float64}`, a covariant/contravariant bound shorthand, …).
/// Used by [`typevar_bound_value_expr`] (Issue #10226) to decide whether an
/// unresolved where-bound name is safe to re-lower as an ordinary variable
/// reference — compound expressions already have dedicated static-type
/// parsing elsewhere and must keep going through that path unchanged.
/// Also used by the declaration-position sibling check (Issue #10396):
/// `CoreCompiler`'s `Stmt::FunctionDef` compilation probes function-signature
/// `where`-bound names for resolvability at method-definition time.
pub fn is_bare_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '!')
}

fn parse_value_where_type_constraints<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<crate::types::TypeParam>> {
    if let Some(params) = parse_chained_bound_where_constraint(walker, node)? {
        return Ok(params);
    }
    crate::lowering::function::parse_type_constraints(walker, node)
}

fn parse_chained_bound_where_constraint<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<Vec<crate::types::TypeParam>>> {
    if !matches!(
        walker.kind(&node),
        NodeKind::SubtypeExpression
            | NodeKind::BinaryExpression
            | NodeKind::SubtypeConstraint
            | NodeKind::SupertypeConstraint
    ) {
        return Ok(None);
    }

    let children = walker.named_children_vec(&node);
    if children.len() != 2 || walker.kind(&children[1]) != NodeKind::WhereExpression {
        return Ok(None);
    }

    let name = walker.text(&children[0]).to_string();
    let where_children = walker.named_children_vec(&children[1]);
    if where_children.len() < 2 {
        return Ok(None);
    }

    let where_body = where_children[0];
    let where_constraints = where_children[where_children.len() - 1];
    let mut params = parse_value_where_type_constraints(walker, where_constraints)?;
    let mut excluded = Vec::with_capacity(params.len() + 1);
    excluded.push(name.clone());
    excluded.extend(params.iter().map(|param| param.name.clone()));
    let bound = crate::lowering::type_alias::expand_excluding(walker.text(&where_body), &excluded);

    let current = if walker.kind(&node) == NodeKind::SupertypeConstraint {
        crate::types::TypeParam::with_lower_bound(name, bound)
    } else {
        crate::types::TypeParam::with_upper_bound(name, bound)
    };
    params.push(current);
    Ok(Some(params))
}

/// Emit a type-name string as a type value.
fn type_name_value(name: &str, span: crate::span::Span) -> Expr {
    Expr::Literal(Literal::DataType(name.to_string()), span)
}

/// Check if a type argument node is dynamic (requires runtime evaluation).
/// Dynamic arguments include:
/// - Call expressions (e.g., promote_type(T, S))
/// - Identifiers that are not known type names (e.g., local DataType aliases like
///   `T = typeof(g); Tuple{T}`)
fn is_dynamic_type_arg<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> bool {
    match walker.kind(&node) {
        NodeKind::CallExpression => true, // Function calls are always dynamic
        // A splatted type argument (`Tuple{xs...}`) must be expanded at
        // runtime from the splatted collection's elements (Issue #5112).
        NodeKind::SplatExpression => true,
        NodeKind::Identifier => {
            let name = walker.text(&node);
            lambda_ctx.is_some_and(|ctx| ctx.is_active_type_param(name))
                || (!matches!(name, "true" | "false")
                    && !is_static_type_name_in_curly(name)
                    && crate::types::JuliaType::from_name(name).is_none())
        }
        NodeKind::ParametrizedTypeExpression => {
            // A nested parametric type is dynamic when either its base or an
            // argument is runtime-bound. Checking only arguments would turn
            // `A{Int64}` into a DataType literal when `A` is a lexical
            // `where` binder (Issue #10934).
            let children = walker.named_children_vec(&node);
            children
                .first()
                .is_some_and(|base| match walker.kind(base) {
                    NodeKind::Identifier => {
                        let name = walker.text(base);
                        lambda_ctx.is_some_and(|ctx| ctx.is_active_type_param(name))
                            || parametrized_base_needs_runtime_lookup(name)
                    }
                    NodeKind::FieldExpression => false,
                    _ => true,
                })
                || children
                    .iter()
                    .skip(1)
                    .any(|c| is_dynamic_type_arg(walker, *c, lambda_ctx))
        }
        _ => {
            // Value-parameter expressions such as `Val{N-1}` are not ordinary
            // type names. They must be evaluated at runtime against the method
            // frame's value type parameters instead of being preserved as the
            // literal type name `Val{N-1}` (Issue #8330).
            let text = walker.text(&node).trim();
            !(text.parse::<i64>().is_ok()
                || matches!(text, "true" | "false")
                || text.starts_with(':')
                // A covariant/contravariant bound shorthand (`Foo{<:Real}`,
                // `Foo{>:Int}`) is a *static* bounded type expression, not a
                // runtime value parameter. The #8330 change above defaulted such
                // nodes to dynamic, which routed `<:Real` through expression
                // lowering where `<:` is rejected as an operator (Issue #8352).
                //
                // Issue #10373: the shorthand stays static only when its bound
                // resolves against the static tables. A bare-identifier bound
                // that nothing static knows (`Vector{<:SomeUndefinedName}`, or
                // a user struct/abstract type only defined at runtime) must go
                // through the dynamic path, whose
                // `lower_anonymous_bound_type_arg` ->
                // `typevar_bound_value_expr` lowering (Issue #10226) resolves
                // the bound via ordinary runtime global lookup -- raising
                // `UndefVarError` for genuinely undefined names exactly like
                // upstream Julia.
                || ((text.starts_with("<:") || text.starts_with(">:"))
                    && !lambda_ctx.is_some_and(|ctx| {
                        node_contains_active_type_param(walker, node, ctx)
                    })
                    && anonymous_bound_text_is_static(&text[2..]))
                || is_static_type_name_in_curly(text)
                || crate::types::JuliaType::from_name(text).is_some())
        }
    }
}

fn node_contains_active_type_param<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> bool {
    (walker.kind(&node) == NodeKind::Identifier
        && lambda_ctx.is_active_type_param(walker.text(&node)))
        || walker
            .named_children(&node)
            .any(|child| node_contains_active_type_param(walker, child, lambda_ctx))
}

/// Whether the bound text of an anonymous covariant/contravariant type-arg
/// shorthand (`<:Bound` / `>:Bound`, bound text passed WITHOUT the operator)
/// resolves statically, keeping the whole shorthand on the static
/// compound-type-name-literal path (Issue #10373).
///
/// Static bounds are: registered type aliases (expanded first, Issue #5055),
/// recognized builtin/static type names, and compound bound expressions
/// (`<:Vector{Int}`, `<:Base.Number`, `<:Union{...}`) -- the latter keep the
/// permissive pre-existing static path, the same compound-bound exclusion as
/// Issue #10226's `typevar_bound_value_expr`. A bare-identifier bound that no
/// static table knows is NOT static: it must be resolved (and
/// `UndefVarError`-validated) at runtime through the dynamic type-construct
/// path.
fn anonymous_bound_text_is_static(bound: &str) -> bool {
    let expanded = crate::lowering::type_alias::expand(bound.trim());
    !is_bare_identifier_name(&expanded)
        || is_static_type_name_in_curly(&expanded)
        || crate::types::JuliaType::from_name(&expanded).is_some()
}

/// Recursively scan a value-position type expression for an anonymous
/// covariant/contravariant bound shorthand (`<:Name` / `>:Name`) whose bound
/// is a bare identifier that no static table resolves (Issue #10373).
///
/// Used by the type-alias extraction in `lowering::stmt`
/// (`extract_type_alias_from_binding`): an assignment RHS such as
/// `Vector{<:SomeUndefinedName}` or `Dict{String, <:SomeUndefinedName}` must
/// NOT be registered as a static string alias (which would bypass runtime
/// name resolution entirely); rejecting it routes the assignment through
/// ordinary value lowering, where `is_dynamic_type_arg` sends the shorthand
/// down the validating dynamic path.
pub fn parametric_type_has_unresolved_anonymous_bound<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> bool {
    if walker.kind(&node) == NodeKind::UnaryExpression {
        let children = walker.named_children_vec(&node);
        let op = children
            .iter()
            .find(|child| walker.kind(child) == NodeKind::Operator)
            .map(|child| walker.text(child));
        if matches!(op, Some("<:") | Some(">:")) {
            if let Some(bound) = children
                .iter()
                .rev()
                .find(|child| walker.kind(child) != NodeKind::Operator)
            {
                if !anonymous_bound_text_is_static(walker.text(bound)) {
                    return true;
                }
            }
        }
    }
    walker
        .named_children(&node)
        .any(|child| parametric_type_has_unresolved_anonymous_bound(walker, child))
}

fn parametrized_base_needs_runtime_lookup(base: &str) -> bool {
    if base.contains('.') {
        return false;
    }
    if is_static_type_name_in_curly(base) || crate::types::JuliaType::from_name(base).is_some() {
        return false;
    }
    // A non-parametric alias whose target is already an applied type
    // (`w = Plain{Int64}`) used with a further parameter list (`w{Float64}`)
    // is a chained UnionAll application: the base value must be looked up at
    // runtime and applied through `ApplyTypeDynamic`, which appends the new
    // arguments to the partial UnionAll and validates arity/bounds. Static
    // alias expansion cannot represent the chained application and used to
    // silently drop the outer arguments (Issue #10643).
    crate::lowering::type_alias::is_applied_type_alias(base)
        || crate::lowering::type_alias::expand(base) == base
}

fn strip_single_outer_parens(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        trimmed[1..trimmed.len() - 1].trim()
    } else {
        trimmed
    }
}

fn is_static_type_name_in_curly(name: &str) -> bool {
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

fn split_parametric_source_args(source: &str) -> Vec<String> {
    let Some((open, close)) = top_level_parametric_braces(source) else {
        return Vec::new();
    };

    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = open + 1;
    for (idx, ch) in source[open + 1..close].char_indices() {
        let absolute = open + 1 + idx;
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let arg = source[start..absolute].trim();
                if !arg.is_empty() {
                    args.push(arg.to_string());
                }
                start = absolute + ch.len_utf8();
            }
            _ => {}
        }
    }
    let arg = source[start..close].trim();
    if !arg.is_empty() {
        args.push(arg.to_string());
    }
    args
}

fn top_level_parametric_braces(source: &str) -> Option<(usize, usize)> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut open = None;
    for (idx, ch) in source.char_indices() {
        match ch {
            '{' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                open = Some(idx);
                break;
            }
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            _ => {}
        }
    }
    let open = open?;
    let mut depth = 0usize;
    for (idx, ch) in source[open + 1..].char_indices() {
        let absolute = open + 1 + idx;
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' if depth == 0 => return Some((open, absolute)),
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn lower_type_arg_text_as_value_expr(arg: &str, span: crate::span::Span) -> Expr {
    if let Ok(value) = arg.parse::<i64>() {
        return Expr::Literal(crate::ir::core::Literal::Int(value), span);
    }
    if arg == "true" {
        return Expr::Literal(crate::ir::core::Literal::Bool(true), span);
    }
    if arg == "false" {
        return Expr::Literal(crate::ir::core::Literal::Bool(false), span);
    }
    if let Some(symbol) = arg.strip_prefix(':') {
        return Expr::Literal(crate::ir::core::Literal::Symbol(symbol.to_string()), span);
    }
    if is_static_type_name_in_curly(arg)
        || crate::types::JuliaType::from_name(arg).is_some()
        || arg.contains('{')
    {
        return Expr::Literal(
            crate::ir::core::Literal::DataType(crate::lowering::type_alias::expand(arg)),
            span,
        );
    }
    Expr::Var(arg.to_string().into(), span)
}

fn source_type_arg_is_simple(arg: &str) -> bool {
    arg.parse::<i64>().is_ok()
        || matches!(arg, "true" | "false")
        || arg.starts_with(':')
        || is_static_type_name_in_curly(arg)
        || crate::types::JuliaType::from_name(arg).is_some()
        || arg.contains('{')
}
