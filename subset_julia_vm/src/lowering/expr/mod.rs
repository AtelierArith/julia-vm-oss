//! Expression lowering.
//!
//! This module handles lowering of CST expressions to Core IR.

mod binary;
mod call;
mod collection;
mod helpers;
mod literal;
mod macros;
mod misc;
pub(in crate::lowering) mod quote;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::function::lower_anonymous_function_value;
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};

// Re-export helpers for use in submodules
pub(super) use helpers::{
    is_broadcast_op, is_comparison_operator, is_flattenable_operator, is_operator_token,
    map_binary_op, map_builtin_name, map_unary_op, process_raw_string_escapes, strip_broadcast_dot,
};
pub(crate) use helpers::{make_broadcasted_call, make_broadcasted_call_with_callee};

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
pub(crate) use macros::lower_namedtuple_macro_expr;
// Statement-position `@sync` lowering (Issue #7768): keeps the sync body in the
// enclosing scope so assignments to surrounding locals are preserved.
pub(crate) use macros::lower_sync_macro_stmt_entry;
pub use misc::extract_field_target;
pub use misc::extract_nested_field_target;
pub use misc::extract_nested_field_target_with_ctx;

// Re-export for submodules
pub(super) use binary::{
    lower_binary_expr, lower_binary_expr_with_ctx, lower_juxtaposition_expr, lower_unary_expr,
    lower_unary_expr_with_ctx,
};
pub(super) use call::{
    is_operator_function_call_target, lower_argument_list, lower_arrow_function, lower_call_expr,
    lower_call_expr_with_ctx,
};
pub(super) use collection::{
    lower_comprehension_expr, lower_generator_expr, lower_index_expr, lower_index_expr_with_ctx,
    lower_matrix_expr, lower_matrix_expr_raw, lower_matrix_expr_with_ctx, lower_range_expr,
    lower_vector_expr, lower_vector_expr_with_ctx,
};
pub(super) use literal::{lower_char_literal, lower_string_literal, parse_float};
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
            function: "Vector{Any}".to_string(),
            args: vec![comprehension],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        });
    }
    // Typed comprehension `T[expr for x in iter]` for non-numeric element
    // types (`Bool`, `Char`, `Symbol`, `String`, ...). Upstream Julia stores
    // each element through `setindex!`, which calls `convert(T, expr)` — not
    // the `T(expr)` *constructor*. For these types the constructor either is
    // not reachable in the VM (`Bool` / `Symbol` -> "Unknown function") or
    // produces the wrong element slot (`Char` was forced into an I64 slot,
    // `String` left the eltype as `Any`). Wrap the body in `convert(T, expr)`
    // and the whole comprehension in `Vector{T}(...)`, mirroring the `Any`
    // case above; the `Vector{T}` compile intercept forces the result element
    // type to `T` so `typeof` matches upstream exactly. Issue #5040.
    //
    // Numeric/abstract numeric types keep the existing `T(expr)` body wrapping
    // (a real, reachable constructor with the right inferred element type) so
    // the resolved #4811/#4816/#4818/#4819/#4822 cluster behavior is preserved.
    if matches!(function.as_str(), "Bool" | "Char" | "Symbol" | "String") {
        let convert_body = |body: Box<Expr>| -> Box<Expr> {
            Box::new(Expr::Call {
                function: "convert".to_string(),
                args: vec![Expr::Var(function.clone(), span), *body],
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
            function: format!("Vector{{{function}}}"),
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
                function,
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
                function,
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
        function: "typeassert".to_string(),
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
fn lower_try_as_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let try_stmt = crate::lowering::stmt::lower_try_stmt(walker, node)?;
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
pub(crate) fn try_stmt_into_value_expr(
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

    // Recursively rewrite the last value-producing statement of a branch to
    // assign its value to `result_var`. Issue #4833 covers nested
    // try-as-expression tails; macro-returned try bodies can also end in a
    // nested `Stmt::Block`, whose own tail supplies the branch value.
    // Other non-expression statements (Stmt::Return, etc.) are left as-is —
    // the outer result_var stays at its `nothing` default for those branches.
    fn assign_last_value(block: Block, result_var: &str) -> Block {
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
                    let inner_try = assign_last_value(inner_try, result_var);
                    let inner_catch = inner_catch.map(|b| assign_last_value(b, result_var));
                    let inner_else = inner_else.map(|b| assign_last_value(b, result_var));
                    stmts.push(Stmt::Try {
                        try_block: inner_try,
                        catch_var: inner_catch_var,
                        catch_block: inner_catch,
                        else_block: inner_else,
                        finally_block: inner_finally,
                        span: inner_span,
                    });
                }
                Stmt::Block(block) => {
                    stmts.push(Stmt::Block(assign_last_value(block, result_var)));
                }
                other => stmts.push(other),
            }
        }
        Block {
            stmts,
            span: block.span,
        }
    }

    let try_block = assign_last_value(try_block, &result_var);
    let catch_block = catch_block.map(|b| assign_last_value(b, &result_var));
    let else_block = else_block.map(|b| assign_last_value(b, &result_var));

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
    let init = Stmt::Assign {
        var: result_var.clone(),
        value: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    };
    let read = Stmt::Expr {
        expr: Expr::Var(result_var, span),
        span,
    };

    Some(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![init, rewritten_try, read],
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
            function: kind.constructor_name().to_string(),
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
        let left = lower_expr(walker, type_node)?;
        let right = lower_expr(walker, matrix_node)?;
        return Ok(Expr::Call {
            function: "hcat".to_string(),
            args: vec![left, right],
            kwargs: vec![],
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    let type_expr = lower_expr(walker, type_node)?;
    let matrix = lower_matrix_expr_raw(walker, matrix_node)?;
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
    if shape.len() != 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::MalformedMatrix,
            span,
        ));
    }

    let rows = i64::try_from(shape[0]).map_err(|_| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MalformedMatrix, span)
            .with_hint("typed matrix row count does not fit Int64")
    })?;
    let cols = i64::try_from(shape[1]).map_err(|_| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MalformedMatrix, span)
            .with_hint("typed matrix column count does not fit Int64")
    })?;

    let typed_vector = Expr::Index {
        array: Box::new(type_expr),
        indices: elements,
        span,
    };
    Ok(Expr::Call {
        function: "reshape".to_string(),
        args: vec![
            typed_vector,
            Expr::Literal(Literal::Int(rows), span),
            Expr::Literal(Literal::Int(cols), span),
        ],
        kwargs: Vec::new(),
        splat_mask: Vec::new(),
        kwargs_splat_mask: Vec::new(),
        span,
    })
}

fn lower_assignment_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let children = walker.named_children(&node);
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

    // An index/field assignment used in expression position — `(a[i] = v)`,
    // `(obj.field = v)`, `(obj.field[i] = v)` — is a valid Julia expression whose
    // value is the assigned RHS value. This shape arises as the RHS of another
    // assignment (`y = (a[1] = 5)`) and, crucially, as the single-expression body
    // of an arrow lambda (`x -> (x[2] = v)`). Route it through the statement-form
    // lowering (which produces the canonical `setindex!`/`setproperty!`
    // desugaring) and convert to a value-producing expression instead of
    // rejecting the non-identifier target (Issue #8007).
    match walker.kind(target) {
        NodeKind::Identifier => {}
        NodeKind::IndexExpression | NodeKind::FieldExpression => {
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
        var: var_name,
        value: Box::new(value_expr),
        span,
    })
}

pub fn lower_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
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
                _ => Ok(Expr::Var(name.to_string(), span)),
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
        NodeKind::TryStatement => lower_try_as_expr(walker, node),
        NodeKind::PairExpression => lower_pair_expr(walker, node),
        NodeKind::ParametrizedTypeExpression => {
            // Parametric type as expression: Complex{Float64}, Vector{Int64}, etc.
            // In Julia, this evaluates to the Type itself (a DataType value).
            //
            // For static types (all type args are concrete type names), we emit TypeOf.
            // For dynamic types (type args contain function calls or type variables),
            // we emit DynamicTypeConstruct which evaluates type args at runtime.
            lower_parametrized_type_expr(walker, node)
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
                    .into_iter()
                    .find(|child| walker.kind(child) == NodeKind::Block)
                    .unwrap_or(node)
            } else {
                node
            };
            // Lower the block contents as statements
            let children = walker.named_children(&actual_block);
            let mut stmts = Vec::new();
            for child in children {
                let stmt = crate::lowering::stmt::lower_stmt(walker, child)?;
                stmts.push(stmt);
            }
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
            let children = walker.named_children(&node);
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
                            function: "_mime_construct".to_string(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "v" => {
                        // v"1.2.3" creates a VersionNumber
                        // Parse version string and create constructor call
                        let parts: Vec<&str> = content.split('.').collect();
                        let major = parts
                            .first()
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let minor = parts
                            .get(1)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let patch = parts
                            .get(2)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        Ok(Expr::Call {
                            function: "VersionNumber".to_string(),
                            args: vec![
                                Expr::Literal(Literal::Int(major), span),
                                Expr::Literal(Literal::Int(minor), span),
                                Expr::Literal(Literal::Int(patch), span),
                            ],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
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
                    "Int128" => {
                        // Int128"123" creates an Int128 literal
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
                    "UInt128" => {
                        // UInt128"123" creates a UInt128 literal
                        // Note: We store as i128 but the type system will treat it as UInt128
                        if let Ok(val) = content.parse::<u128>() {
                            // Convert to i128 for storage (bit pattern preservation)
                            Ok(Expr::Literal(Literal::Int128(val as i128), span))
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
                        // s"..." creates a SubstitutionString (used in replace)
                        // For now, just return as regular string
                        Ok(Expr::Literal(Literal::Str(content), span))
                    }
                    _ => {
                        // Unknown prefixes are string macros in Julia:
                        // prefix"text" → @prefix_str("text")
                        // e.g., html"<b>bold</b>" → @html_str("<b>bold</b>")
                        //       L"x^2" → @L_str("x^2")
                        let macro_name = format!("{}_str", prefix_text);
                        Ok(Expr::Call {
                            function: macro_name,
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
            let children = walker.named_children(&node);
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
            let children = walker.named_children(&node);
            if children.len() == 2 {
                let type_node = children[0];
                let value_node = children[1];

                // Check if this is a typed empty array: Type[]
                if walker.kind(&type_node) == NodeKind::Identifier
                    && walker.kind(&value_node) == NodeKind::VectorExpression
                {
                    let type_children = walker.named_children(&value_node);
                    if type_children.is_empty() {
                        // Int64[] or similar: typed empty array
                        let element_type = walker.text(&type_node).to_string();
                        return Ok(Expr::TypedEmptyArray { element_type, span });
                    }
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
                name: op_text,
                span,
            })
        }
        NodeKind::SplatExpression => {
            // Macro quote-template substitution can leave `$(args...)` as a
            // direct SplatExpression in expression lowering; unwrap to the
            // value being splatted so Expr construction can carry it (Issue #7541).
            let children = walker.named_children(&node);
            let Some(inner) = children.first() else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty splat expression".to_string(),
                    ),
                    span,
                ));
            };
            lower_expr(walker, *inner)
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
    match walker.kind(&node) {
        NodeKind::ArrowFunctionExpression => lower_arrow_function(walker, node, lambda_ctx),
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
                _ => Ok(Expr::Var(name.to_string(), span)),
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
        NodeKind::RangeExpression => lower_range_expr(walker, node),
        NodeKind::VectorExpression => lower_vector_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::MatrixExpression => lower_matrix_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::IndexExpression => lower_index_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::ComprehensionExpression => lower_comprehension_expr(walker, node),
        NodeKind::GeneratorExpression => lower_generator_expr(walker, node),
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
        // try/catch/end as expression (Issue #4784). See note on the
        // `lower_expr` variant above; the context-aware lowering shares
        // the same rewrite — try-block lambda capture inside catch is
        // not yet supported (no in-tree fixture needs it), so the
        // shared helper is intentionally ctx-agnostic.
        NodeKind::TryStatement => lower_try_as_expr(walker, node),
        NodeKind::PairExpression => lower_pair_expr(walker, node),
        NodeKind::ParametrizedTypeExpression => {
            // Parametric type as expression: Complex{Float64}, Vector{Int64}, etc.
            // In Julia, this evaluates to the Type itself (a DataType value).
            //
            // Keep this in sync with `lower_expr`: dynamic type arguments such
            // as `Tuple{typeof(g)}` must be evaluated instead of preserved as a
            // literal type string.
            lower_parametrized_type_expr(walker, node)
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
                    .into_iter()
                    .find(|child| walker.kind(child) == NodeKind::Block)
                    .unwrap_or(node)
            } else {
                node
            };
            // Lower the block contents as statements
            let children = walker.named_children(&actual_block);
            let mut stmts = Vec::new();
            for child in children {
                let stmt = crate::lowering::stmt::lower_stmt_with_ctx(walker, child, lambda_ctx)?;
                stmts.push(stmt);
            }
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
            let children = walker.named_children(&node);
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
                            function: "_mime_construct".to_string(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "v" => {
                        // v"1.2.3" creates a VersionNumber
                        // Parse version string and create constructor call
                        let parts: Vec<&str> = content.split('.').collect();
                        let major = parts
                            .first()
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let minor = parts
                            .get(1)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let patch = parts
                            .get(2)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        Ok(Expr::Call {
                            function: "VersionNumber".to_string(),
                            args: vec![
                                Expr::Literal(Literal::Int(major), span),
                                Expr::Literal(Literal::Int(minor), span),
                                Expr::Literal(Literal::Int(patch), span),
                            ],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
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
                    "Int128" => {
                        // Int128"123" creates an Int128 literal
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
                    "UInt128" => {
                        // UInt128"123" creates a UInt128 literal
                        // Note: We store as i128 but the type system will treat it as UInt128
                        if let Ok(val) = content.parse::<u128>() {
                            // Convert to i128 for storage (bit pattern preservation)
                            Ok(Expr::Literal(Literal::Int128(val as i128), span))
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
                        // s"..." creates a SubstitutionString (used in replace)
                        // For now, just return as regular string
                        Ok(Expr::Literal(Literal::Str(content), span))
                    }
                    _ => {
                        // Unknown prefixes are string macros in Julia:
                        // prefix"text" → @prefix_str("text")
                        // e.g., html"<b>bold</b>" → @html_str("<b>bold</b>")
                        //       L"x^2" → @L_str("x^2")
                        let macro_name = format!("{}_str", prefix_text);
                        Ok(Expr::Call {
                            function: macro_name,
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
            let children = walker.named_children(&node);
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
            let children = walker.named_children(&node);
            if children.len() == 2 {
                let type_node = children[0];
                let value_node = children[1];

                // Check if this is a typed empty array: Type[]
                if walker.kind(&type_node) == NodeKind::Identifier
                    && walker.kind(&value_node) == NodeKind::VectorExpression
                {
                    let type_children = walker.named_children(&value_node);
                    if type_children.is_empty() {
                        // Int64[] or similar: typed empty array
                        let element_type = walker.text(&type_node).to_string();
                        return Ok(Expr::TypedEmptyArray { element_type, span });
                    }
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
                name: op_text,
                span,
            })
        }
        NodeKind::SplatExpression => {
            // See the non-context lowering arm above (Issue #7541).
            let children = walker.named_children(&node);
            let Some(inner) = children.first() else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty splat expression".to_string(),
                    ),
                    span,
                ));
            };
            lower_expr_with_ctx(walker, *inner, lambda_ctx)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(node.kind().to_string()),
            span,
        )),
    }
}

/// Lower a parametric type expression like `Complex{Float64}` or `Complex{promote_type(T, S)}`.
///
/// For static types (all type args are concrete type names), emits TypeOf builtin.
/// For dynamic types (type args contain function calls or type variables that aren't
/// known concrete types), emits DynamicTypeConstruct which evaluates type args at runtime.
pub(crate) fn lower_arrow_expr_as_nested_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    call::lower_arrow_value_as_nested_with_ctx(walker, node, lambda_ctx)
}

fn lower_parametrized_type_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

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
                    explicit_base_expr = Some(lower_expr(walker, child)?);
                    saw_base = true;
                } else {
                    // Type argument is an identifier
                    type_arg_nodes.push(child);
                }
            }
        }
    }

    let has_runtime_base_expr = explicit_base_expr.is_some();
    let has_dynamic_base = has_runtime_base_expr
        || base_name
            .as_deref()
            .is_some_and(parametrized_base_needs_runtime_lookup);

    // Check if any type argument is dynamic (call expression, or identifier that's not
    // a known concrete type)
    let has_dynamic_arg = type_arg_nodes
        .iter()
        .any(|n| is_dynamic_type_arg(walker, *n));

    if !has_dynamic_base && !has_dynamic_arg {
        // All static - use current behavior with TypeOf.
        // Issue #5055: expand user-defined type aliases (`MyVec{Int}` ->
        // `Vector{Int}`) so the embedded type-name literal names the target.
        let name = crate::lowering::type_alias::expand(walker.text(&node));
        Ok(Expr::Builtin {
            name: crate::ir::core::BuiltinOp::TypeOf,
            args: vec![Expr::Literal(crate::ir::core::Literal::Str(name), span)],
            span,
        })
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
        let expanded_base = crate::lowering::type_alias::expand(&base);
        let base = match expanded_base.split_once('{') {
            Some((b, _)) => b.to_string(),
            None => expanded_base,
        };
        let base_expr = if let Some(expr) = explicit_base_expr {
            Some(Box::new(expr))
        } else if has_dynamic_base {
            Some(Box::new(Expr::Var(base.clone(), span)))
        } else {
            None
        };

        // Lower each type argument as an expression. A splatted argument
        // (`Tuple{xs...}`) lowers its INNER expression (the collection of type
        // values) and records `true` in a parallel `splat_mask` so the VM can
        // flatten it at runtime (Issue #5112).
        let mut type_args: Vec<Expr> = Vec::with_capacity(type_arg_nodes.len());
        let mut splat_mask: Vec<bool> = Vec::with_capacity(type_arg_nodes.len());
        let source_args = if has_dynamic_base {
            split_parametric_source_args(walker.text(&node))
        } else {
            Vec::new()
        };
        if !source_args.is_empty()
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
                    let inner = walker.named_children(n);
                    let inner = inner.first().ok_or_else(|| {
                        UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(
                                "splat in parametric type without inner expression".to_string(),
                            ),
                            span,
                        )
                    })?;
                    type_args.push(lower_parametric_type_arg_expr(walker, *inner, span)?);
                    splat_mask.push(true);
                } else {
                    type_args.push(lower_parametric_type_arg_expr(walker, *n, span)?);
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
            base,
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
) -> LowerResult<Expr> {
    if let Some(expr) = lower_anonymous_bound_type_arg(walker, node, span)? {
        return Ok(expr);
    }
    lower_expr(walker, node)
}

fn lower_anonymous_bound_type_arg<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
) -> LowerResult<Option<Expr>> {
    if walker.kind(&node) != NodeKind::UnaryExpression {
        return Ok(None);
    }

    let children = walker.named_children(&node);
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

    let bound = children
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
    let bound = crate::lowering::type_alias::expand(walker.text(bound));
    let (lower_bound, upper_bound) = if op == "<:" {
        (None, Some(bound))
    } else {
        (Some(bound), None)
    };

    Ok(Some(make_typevar_expr(
        "_",
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
/// The body is emitted through the static type-name path (`TypeOf` on a literal
/// type-name string), so its free type variables (`T`, `N`) parse into
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
    let children = walker.named_children(&node);
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
    let type_params = crate::lowering::function::parse_type_constraints(walker, constraints_node)?;
    if type_params.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("where_expression".to_string()),
            span,
        ));
    }
    let mut bound_scope = type_params.clone();
    bound_scope.extend_from_slice(outer_type_params);

    // Chained value-position `where` (`Body where A where B`) nests another
    // WhereExpression on the left; recurse so the inner binder stays innermost.
    let mut body_expr = match walker.kind(&body_node) {
        NodeKind::WhereExpression => {
            lower_where_expression_value_with_scope(walker, body_node, &bound_scope)?
        }
        NodeKind::Identifier => {
            let body_name = walker.text(&body_node);
            if let Some(tp) = type_params.iter().find(|tp| tp.name == body_name) {
                make_typevar_expr_with_scope(
                    &tp.name,
                    tp.lower_bound.as_ref(),
                    tp.get_upper_bound(),
                    span,
                    &bound_scope,
                )
            } else {
                type_name_value(&crate::lowering::type_alias::expand(body_name), span)
            }
        }
        _ => {
            // Force the static type-name path: emit `TypeOf("<body text>")`, which
            // parses free type variables as `TypeVar`s. Expand user-defined type
            // aliases (Issue #5055) so an aliased base names its target.
            let body_name = crate::lowering::type_alias::expand(walker.text(&body_node));
            type_name_value(&body_name, span)
        }
    };

    // Wrap innermost-first: iterate parameters in reverse so the first-listed
    // variable becomes the outermost `UnionAll`.
    for tp in type_params.iter().rev() {
        let typevar_expr = make_typevar_expr_with_scope(
            &tp.name,
            tp.lower_bound.as_ref(),
            tp.get_upper_bound(),
            span,
            &bound_scope,
        );
        body_expr = Expr::Call {
            function: "UnionAll".to_string(),
            args: vec![typevar_expr, body_expr],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
    }

    Ok(body_expr)
}

/// Build a `TypeVar(:name)` (unbounded) or `TypeVar(:name, Union{}, Bound)`
/// (upper-bounded) constructor expression for value-position `where` lowering.
/// The bound is emitted as a static type value via `TypeOf("<bound>")`.
fn make_typevar_expr(
    name: &str,
    lower_bound: Option<&String>,
    upper_bound: Option<&String>,
    span: crate::span::Span,
) -> Expr {
    make_typevar_expr_with_scope(name, lower_bound, upper_bound, span, &[])
}

fn make_typevar_expr_with_scope(
    name: &str,
    lower_bound: Option<&String>,
    upper_bound: Option<&String>,
    span: crate::span::Span,
    bound_scope: &[crate::types::TypeParam],
) -> Expr {
    let name_arg = Expr::Literal(Literal::Symbol(name.to_string()), span);
    let mut args = vec![name_arg];
    if lower_bound.is_some() || upper_bound.is_some() {
        // `TypeVar(:T, lower, upper)`: a declared lower bound (`where Int<:T<:..`
        // or `where T>:Int`) is passed through; otherwise it defaults to `Union{}`
        // (Bottom). The upper defaults to `Any` when only a lower is declared
        // (Issue #5650). Both are passed as type values.
        let lower_val = match lower_bound {
            Some(l) => typevar_bound_value_expr(l, span, bound_scope),
            None => type_name_value(&crate::types::JuliaType::Bottom.name(), span),
        };
        let upper_val = match upper_bound {
            Some(u) => typevar_bound_value_expr(u, span, bound_scope),
            None => type_name_value(&crate::types::JuliaType::Any.name(), span),
        };
        args.push(lower_val);
        args.push(upper_val);
    }
    Expr::Call {
        function: "TypeVar".to_string(),
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
) -> Expr {
    if let Some(tp) = bound_scope.iter().find(|tp| tp.name == bound) {
        return make_typevar_expr_with_scope(
            &tp.name,
            tp.lower_bound.as_ref(),
            tp.get_upper_bound(),
            span,
            bound_scope,
        );
    }
    type_name_value(bound, span)
}

/// Emit a type-name string as a type value via the static `TypeOf` path.
fn type_name_value(name: &str, span: crate::span::Span) -> Expr {
    Expr::Builtin {
        name: crate::ir::core::BuiltinOp::TypeOf,
        args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
        span,
    }
}

/// Check if a type argument node is dynamic (requires runtime evaluation).
/// Dynamic arguments include:
/// - Call expressions (e.g., promote_type(T, S))
/// - Identifiers that are not known type names (e.g., local DataType aliases like
///   `T = typeof(g); Tuple{T}`)
fn is_dynamic_type_arg<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    match walker.kind(&node) {
        NodeKind::CallExpression => true, // Function calls are always dynamic
        // A splatted type argument (`Tuple{xs...}`) must be expanded at
        // runtime from the splatted collection's elements (Issue #5112).
        NodeKind::SplatExpression => true,
        NodeKind::Identifier => {
            let name = walker.text(&node);
            !matches!(name, "true" | "false")
                && !is_static_type_name_in_curly(name)
                && crate::types::JuliaType::from_name(name).is_none()
        }
        NodeKind::ParametrizedTypeExpression => {
            // Nested parametric type - recursively check
            let children = walker.named_children(&node);
            children
                .iter()
                .skip(1)
                .any(|c| is_dynamic_type_arg(walker, *c))
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
                || text.starts_with("<:")
                || text.starts_with(">:")
                || is_static_type_name_in_curly(text)
                || crate::types::JuliaType::from_name(text).is_some())
        }
    }
}

fn parametrized_base_needs_runtime_lookup(base: &str) -> bool {
    if base.contains('.') {
        return false;
    }
    !is_static_type_name_in_curly(base)
        && crate::types::JuliaType::from_name(base).is_none()
        && crate::lowering::type_alias::expand(base) == base
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
        return Expr::Builtin {
            name: crate::ir::core::BuiltinOp::TypeOf,
            args: vec![Expr::Literal(
                crate::ir::core::Literal::Str(crate::lowering::type_alias::expand(arg)),
                span,
            )],
            span,
        };
    }
    Expr::Var(arg.to_string(), span)
}

fn source_type_arg_is_simple(arg: &str) -> bool {
    arg.parse::<i64>().is_ok()
        || matches!(arg, "true" | "false")
        || arg.starts_with(':')
        || is_static_type_name_in_curly(arg)
        || crate::types::JuliaType::from_name(arg).is_some()
        || arg.contains('{')
}
