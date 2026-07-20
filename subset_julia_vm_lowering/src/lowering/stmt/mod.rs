//! Statement lowering module
//!
//! This module handles the conversion of CST statement nodes to Core IR statements.
//!
//! ## Submodules
//!
//! - `assignment`: Variable and array assignments, compound assignments
//! - `control_for`: For loop statements
//! - `control_if`: If/elseif/else statements
//! - `control_while`: While loop statements
//! - `control_try`: Try/catch/finally statements
//! - `macros`: Macro calls (@show, @assert, @time, etc.)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod assignment;
mod control_for;
mod control_if;
mod control_try;
mod control_while;
pub mod macros;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{
    Block, BuiltinOp, Expr, Function, Literal, LocalDeclKind, RuntimeNominalDef, Stmt,
};
use crate::lowering::expr;
use crate::lowering::function;
use crate::lowering::{reject_macro_expanded_structs_added_since, LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

// Re-export public functions
pub use assignment::{
    lower_assignment, lower_assignment_value_expr, lower_assignment_value_expr_with_ctx,
    lower_assignment_with_ctx, lower_compound_assignment, lower_compound_assignment_expr,
    lower_compound_assignment_expr_with_ctx, lower_compound_assignment_with_ctx,
};
// Tuple-destructuring helpers reused by the macro-expansion lowering path
// (Issue #7900), where assignments arrive as `Expr` values, not CST nodes.
pub use assignment::{
    lower_destructuring_from_targets, parse_destructure_targets, DestructureTarget,
};
pub use control_for::lower_for_stmt_with_body;
pub use control_for::{lower_for_stmt, lower_for_stmt_with_ctx};
pub use control_if::{lower_if_stmt, lower_if_stmt_with_ctx};
pub use control_try::{lower_try_stmt, lower_try_stmt_with_ctx};
pub use control_while::{lower_while_stmt, lower_while_stmt_with_ctx};
pub use macros::{lower_macro, lower_macro_with_ctx};

/// Convert a Vec<Function> into a single Stmt.
/// The names defined by a lowered function-definition statement (one
/// `Stmt::FunctionDef`, or the `Stmt::Block` of them produced for a definition
/// with default arguments). Used to emit the `global` marker for
/// `global function f(...) ... end` (Issue #11015).
fn defined_function_names(stmt: &Stmt) -> Vec<String> {
    match stmt {
        Stmt::FunctionDef { func, .. } => vec![func.name.clone()],
        Stmt::Block(block) => {
            let mut names: Vec<String> = Vec::new();
            for s in &block.stmts {
                for name in defined_function_names(s) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            names
        }
        _ => Vec::new(),
    }
}

fn lower_function_defs_to_stmt(mut funcs: Vec<Function>, span: crate::span::Span) -> Stmt {
    if funcs.len() == 1 {
        if let Some(func) = funcs.pop() {
            Stmt::FunctionDef {
                func: Box::new(func),
                span,
            }
        } else {
            Stmt::Block(Block {
                stmts: Vec::new(),
                span,
            })
        }
    } else {
        let stmts = funcs
            .into_iter()
            .map(|f| {
                let s = f.span;
                Stmt::FunctionDef {
                    func: Box::new(f),
                    span: s,
                }
            })
            .collect();
        Stmt::Block(Block { stmts, span })
    }
}

/// Lower a statement node to Core IR.
pub fn lower_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    lower_stmt_impl(walker, node, None)
}

fn lower_stmt_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    match walker.kind(&node) {
        // Assignment can also be a short function definition (f(x) = 2x) - check this first
        NodeKind::Assignment if function::is_short_function_definition(walker, node) => {
            let mut funcs = match lambda_ctx {
                Some(ctx) => {
                    crate::lowering::lower_short_function_all_with_ctx_if_needed(walker, node, ctx)?
                }
                None => function::lower_short_function_all(walker, node)?,
            };
            if let Some(ctx) = lambda_ctx {
                ctx.stamp_function_definitions(&mut funcs);
            }
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        NodeKind::Assignment => lower_assignment_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::CompoundAssignment => {
            lower_compound_assignment_maybe_ctx(walker, node, lambda_ctx)
        }
        NodeKind::ReturnStatement => lower_return(walker, node, lambda_ctx),
        NodeKind::ForStatement => lower_for_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::IfStatement => lower_if_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::WhileStatement => lower_while_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::BreakStatement => Ok(Stmt::Break { span }),
        NodeKind::ContinueStatement => Ok(Stmt::Continue { span }),
        NodeKind::TryStatement => lower_try_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::StructDefinition | NodeKind::MutableStructDefinition
            if lambda_ctx.is_some_and(|ctx| {
                ctx.inside_top_level_control_flow() && !ctx.in_function_body()
            }) =>
        {
            let ctx = lambda_ctx.ok_or_else(|| {
                crate::lowering::internal_lowering_error(
                    span,
                    "runtime struct definition lost its lowering context",
                )
            })?;
            let definition =
                crate::lowering::struct_::lower_struct_definition_with_ctx(walker, node, ctx)?;
            let mut definition = RuntimeNominalDef::Struct(Box::new(definition));
            let mut span = span;
            span.definition_order = ctx.stamp_runtime_nominal_definition(&mut definition);
            Ok(Stmt::RuntimeNominalDef {
                definition,
                published_members: None,
                span,
            })
        }
        NodeKind::AbstractDefinition
            if lambda_ctx.is_some_and(|ctx| {
                ctx.inside_top_level_control_flow() && !ctx.in_function_body()
            }) =>
        {
            let ctx = lambda_ctx.ok_or_else(|| {
                crate::lowering::internal_lowering_error(
                    span,
                    "runtime abstract definition lost its lowering context",
                )
            })?;
            let definition = crate::lowering::abstract_::lower_abstract_definition(walker, node)?;
            let mut definition = RuntimeNominalDef::AbstractType(definition);
            let mut span = span;
            span.definition_order = ctx.stamp_runtime_nominal_definition(&mut definition);
            Ok(Stmt::RuntimeNominalDef {
                definition,
                published_members: None,
                span,
            })
        }
        NodeKind::PrimitiveDefinition
            if lambda_ctx.is_some_and(|ctx| {
                ctx.inside_top_level_control_flow() && !ctx.in_function_body()
            }) =>
        {
            let ctx = lambda_ctx.ok_or_else(|| {
                crate::lowering::internal_lowering_error(
                    span,
                    "runtime primitive definition lost its lowering context",
                )
            })?;
            let definition =
                crate::lowering::primitive::lower_runtime_primitive_definition(walker, node)?;
            let mut definition = RuntimeNominalDef::PrimitiveType(definition);
            let mut span = span;
            span.definition_order = ctx.stamp_runtime_nominal_definition(&mut definition);
            Ok(Stmt::RuntimeNominalDef {
                definition,
                published_members: None,
                span,
            })
        }
        NodeKind::MacroCall => lower_macro_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::CompoundStatement => Ok(Stmt::Block(lower_block_impl(walker, node, lambda_ctx)?)),
        // const x = 1 -> treat as regular assignment (simplified implementation)
        NodeKind::ConstStatement => lower_const_statement(walker, node, lambda_ctx),
        // global x -> ignored unless an assignment is present.
        NodeKind::GlobalStatement => lower_global_statement(walker, node, lambda_ctx),
        // local x = value -> treat as regular assignment
        NodeKind::LocalStatement | NodeKind::LocalDeclaration => {
            lower_local_statement(walker, node, lambda_ctx)
        }
        NodeKind::UsingStatement => {
            // Using statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (using is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        NodeKind::ImportStatement => {
            // Import statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (import is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        NodeKind::PublicStatement => {
            // Public statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (public is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        NodeKind::ModuleDefinition | NodeKind::BaremoduleDefinition => {
            // Module definitions are handled at the top level in lowering/mod.rs
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::ModuleDefinition, span)
                    .with_hint("module definitions must be at the top level"),
            )
        }
        NodeKind::ExportStatement => {
            let names = crate::lowering::lower_export_statement(walker, node)?;
            Ok(Stmt::Export { names, span })
        }
        // Function definitions inside blocks (e.g., inside @testset)
        NodeKind::FunctionDefinition => {
            let funcs = match lambda_ctx {
                Some(ctx) => {
                    let struct_watermark = ctx.macro_expanded_struct_count();
                    let mut funcs =
                        crate::lowering::lower_function_all_with_ctx_if_needed(walker, node, ctx)?;
                    ctx.stamp_function_definitions(&mut funcs);
                    // A struct queued while lowering this function's own body
                    // (e.g. a `struct` nested inside one of the function's own
                    // `let`/`begin` blocks, now legal per #10194's
                    // `lower_transparent_block_stmts`) can never legitimately
                    // belong to *this* function — struct definitions are not
                    // allowed inside a function body. Reject, matching the
                    // Program/module-top-level `FunctionDefinition` arms'
                    // existing safety net, but only the delta added here: an
                    // earlier struct *sibling* queued by a preceding statement
                    // in the same still-undrained enclosing transparent block
                    // must survive (Issue #10402).
                    reject_macro_expanded_structs_added_since(ctx, struct_watermark, span)?;
                    funcs
                }
                None => function::lower_function_all(walker, node)?,
            };
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        // Short function definitions (e.g., f(x) = 2x inside blocks)
        NodeKind::ShortFunctionDefinition => {
            let func = match lambda_ctx {
                Some(ctx) => {
                    let struct_watermark = ctx.macro_expanded_struct_count();
                    let mut func = crate::lowering::lower_operator_method_with_ctx_if_needed(
                        walker, node, ctx,
                    )?;
                    ctx.stamp_function_definitions(std::slice::from_mut(&mut func));
                    // See the matching comment in the `FunctionDefinition` arm
                    // above (Issue #10402).
                    reject_macro_expanded_structs_added_since(ctx, struct_watermark, span)?;
                    func
                }
                None => function::lower_operator_method(walker, node)?,
            };
            Ok(Stmt::FunctionDef {
                func: Box::new(func),
                span,
            })
        }
        NodeKind::BinaryExpression => {
            if let Some(ctx) = lambda_ctx {
                if let Some(stmt) = try_lower_test_isa_macro_with_ctx(walker, node, ctx)? {
                    return Ok(stmt);
                }
            }
            // Check if this is an assignment (BinaryExpression with = operator)
            let is_assignment = walker
                .children(&node)
                .iter()
                .any(|child| child.kind() == "operator" && walker.text(child) == "=");
            if is_assignment {
                return lower_assignment_maybe_ctx(walker, node, lambda_ctx);
            }
            let expr = lower_expr_maybe_ctx(walker, node, lambda_ctx)?;
            Ok(Stmt::Expr { expr, span })
        }
        _ => {
            let expr = lower_expr_maybe_ctx(walker, node, lambda_ctx)?;
            Ok(Stmt::Expr { expr, span })
        }
    }
}

/// Lower a block node to Core IR.
pub fn lower_block<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Block> {
    lower_block_impl(walker, node, None)
}

fn lower_block_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Block> {
    // For CompoundStatement (begin...end), extract the inner block if it exists.
    let kind = walker.kind(&node);
    let actual_block = if kind == NodeKind::CompoundStatement {
        // Check if this node has an inner Block child.
        walker
            .named_children(&node)
            .find(|child| walker.kind(child) == NodeKind::Block)
            .unwrap_or(node)
    } else {
        node
    };

    let mut stmts = Vec::new();
    for child in walker.named_children(&actual_block) {
        // Skip comments
        match walker.kind(&child) {
            NodeKind::LineComment | NodeKind::BlockComment => continue,
            _ => {}
        }
        stmts.push(lower_stmt_impl(walker, child, lambda_ctx)?);
    }
    Ok(Block {
        stmts,
        span: walker.span(&node),
    })
}

/// Lower a statement with lambda context for collecting lifted functions.
/// This version handles arrow functions and do syntax in expressions.
pub fn lower_stmt_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    lower_stmt_impl(walker, node, Some(lambda_ctx))
}

/// Lower a block with lambda context.
pub fn lower_block_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Block> {
    lower_block_impl(walker, node, Some(lambda_ctx))
}

/// Lower the direct statement children of a `begin...end` or `let...end`
/// body — a "transparent" block that upstream Julia executes unconditionally
/// exactly once wherever it is reached. Unlike `if`/`for`/`while`/`try`
/// branches or function/closure bodies, upstream Julia allows a nested
/// `struct`/`mutable struct` definition directly inside such a block as long
/// as the block itself is not lexically inside a function body — struct
/// definitions always bind at module scope regardless of `let`/`begin`
/// nesting (verified against upstream `julia` 1.12; Issue #10194). This is
/// what makes `Test.@testset "..." begin ... end` (which macro-expands its
/// body into a `let ... end`) accept a nested `struct` upstream.
///
/// There is no `Stmt::StructDef` variant (structs are not runtime-executable
/// statements), so a struct found here is lowered via
/// `struct_::lower_struct_definition` and registered into `lambda_ctx`'s
/// macro-expanded-struct queue instead of becoming a `Stmt` — reusing the
/// existing macro-expansion struct queue (Issue #7915;
/// `add_macro_expanded_struct` also registers into the compile-time struct
/// table internally, so callers don't need to do that separately), which the
/// enclosing top-level/module-body lowering pass already drains (or rejects,
/// via `reject_macro_expanded_structs_in_non_toplevel`, when the enclosing
/// position turns out not to be top level after all — e.g. a direct
/// top-level function whose body macro-expands to a struct). A no-op
/// `nothing` statement takes its place in the statement stream.
///
/// Callers MUST only use this for genuinely transparent block bodies
/// (`begin`/`let`). `if`/`for`/`while`/`try` branches and function/closure
/// bodies keep calling `lower_stmt`/`lower_stmt_with_ctx` directly and so
/// keep rejecting a nested struct today. Function/closure bodies are
/// correctly rejected — upstream Julia genuinely disallows `struct` there.
/// `if`/`for`/`while`/`try`, however, *are* legal upstream (struct binds
/// when/if that branch or iteration actually executes — Issue #10401
/// tracks this as a distinct unsupported-feature gap, since faithfully
/// reproducing it needs a real runtime struct-definition statement, which
/// this "unconditional hoist to compile time" trick cannot provide:
/// hoisting a struct out of a conditionally/repeatedly executed body would
/// silently run it even when upstream wouldn't reach it, i.e. wrong
/// semantics, not just an unsupported case).
///
/// Without a `lambda_ctx` there is nowhere to register the struct, so a
/// `struct`/`mutable struct` child falls back to the ordinary
/// `lower_stmt`/`lower_stmt_with_ctx` path, preserving today's error.
pub fn lower_transparent_block_stmts<'a>(
    walker: &CstWalker<'a>,
    children: impl IntoIterator<Item = Node<'a>>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Vec<Stmt>> {
    let mut stmts = Vec::new();
    for child in children {
        if let Some(ctx) = lambda_ctx {
            if matches!(
                walker.kind(&child),
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition
            ) {
                let span = walker.span(&child);
                let struct_def =
                    crate::lowering::struct_::lower_struct_definition_with_ctx(walker, child, ctx)?;
                ctx.add_macro_expanded_struct(struct_def);
                stmts.push(Stmt::Expr {
                    expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                    span,
                });
                continue;
            }
        }
        stmts.push(match lambda_ctx {
            Some(ctx) => lower_stmt_with_ctx(walker, child, ctx)?,
            None => lower_stmt(walker, child)?,
        });
    }
    Ok(stmts)
}

fn try_lower_test_isa_macro_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<Stmt>> {
    if walker.kind(&node) != NodeKind::BinaryExpression {
        return Ok(None);
    }

    let span = walker.span(&node);
    let operands: Vec<_> = walker
        .named_children(&node)
        .filter(|child| walker.kind(child) != NodeKind::Operator)
        .collect();
    if operands.len() < 2 {
        return Ok(None);
    }

    let op_text = walker
        .children(&node)
        .into_iter()
        .find(|child| child.kind() == "operator")
        .map(|child| walker.text(&child));
    if op_text != Some("isa") {
        return Ok(None);
    }

    let left = operands[0];
    let right = operands[1];

    if walker.kind(&left) != NodeKind::MacroCall || walker.kind(&right) != NodeKind::TupleExpression
    {
        return Ok(None);
    }

    let macro_ident = walker.find_child(&left, NodeKind::MacroIdentifier);
    let macro_name = match macro_ident {
        Some(ident) => walker.text(&ident).trim_start_matches('@').to_string(),
        None => return Ok(None),
    };
    if macro_name != "test" {
        return Ok(None);
    }

    if !lambda_ctx.has_using("Test") {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@test macro requires `using Test`"),
        );
    }

    let args_nodes = walker.named_children_vec(&right);
    if args_nodes.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::MacroCall,
            span,
        ));
    }

    let mut args = Vec::with_capacity(args_nodes.len());
    for arg in args_nodes {
        args.push(expr::lower_expr_with_ctx(walker, arg, lambda_ctx)?);
    }

    let condition = Expr::Builtin {
        name: BuiltinOp::Isa,
        args,
        span,
    };

    // Reconstruct the user-visible test expression, e.g. `isa(f(x), Int)`.
    let expr_str = format!("isa{}", walker.text(&right));

    Ok(Some(build_test_record_try_stmt(condition, expr_str, span)))
}

/// Build the unified `@test`-recording IR for an already-lowered Boolean
/// condition:
///
/// ```julia
/// try
///     _test_record!(<condition>, "<expr_str>")
/// catch __test_isa_exc_10273
///     _test_record_error!("<expr_str>",
///         string("Test threw exception: ", sprint(showerror, __test_isa_exc_10273)))
/// end
/// ```
///
/// This mirrors the `macro test` expansion in
/// `subset_julia_vm/src/julia/stdlib/Test/src/Test.jl`, so every `@test`-family
/// entry point routes through the same recording builtins
/// (`_test_record!`/`_test_record_error!`): an exception thrown while
/// evaluating the condition is recorded as an errored outcome instead of
/// propagating out of the enclosing `@testset` (Issues #10093 / #10273).
/// The non-Boolean branch of the macro is not needed here because the only
/// caller passes an `isa` builtin condition, which always yields `Bool`.
/// The catch variable is `__test_*`-prefixed like the Test.jl quote locals so
/// it can never collide with (or shadow) a user binding (see Issue #10242).
fn build_test_record_try_stmt(condition: Expr, expr_str: String, span: Span) -> Stmt {
    const EXC_VAR: &str = "__test_isa_exc_10273";

    let record_pass_fail = Stmt::Expr {
        expr: Expr::Builtin {
            name: BuiltinOp::TestRecord,
            args: vec![
                condition,
                Expr::Literal(Literal::Str(expr_str.clone()), span),
            ],
            span,
        },
        span,
    };

    let detail = Expr::Call {
        function: "string".to_string().into(),
        args: vec![
            Expr::Literal(Literal::Str("Test threw exception: ".to_string()), span),
            Expr::Call {
                function: "sprint".to_string().into(),
                args: vec![
                    Expr::Var("showerror".into(), span),
                    Expr::Var(EXC_VAR.into(), span),
                ],
                kwargs: Vec::new(),
                splat_mask: vec![false, false],
                kwargs_splat_mask: Vec::new(),
                span,
            },
        ],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span,
    };
    let record_error = Stmt::Expr {
        expr: Expr::Builtin {
            name: BuiltinOp::TestRecordError,
            args: vec![Expr::Literal(Literal::Str(expr_str), span), detail],
            span,
        },
        span,
    };

    Stmt::Try {
        try_block: Block {
            stmts: vec![record_pass_fail],
            span,
        },
        catch_var: Some(EXC_VAR.to_string()),
        catch_block: Some(Block {
            stmts: vec![record_error],
            span,
        }),
        else_block: None,
        finally_block: None,
        span,
    }
}

// ==================== Return Statement ====================

fn lower_return<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let mut named = walker.named_children_vec(&node);
    let value = match named.pop() {
        Some(value_node) => Some(lower_expr_maybe_ctx(walker, value_node, lambda_ctx)?),
        None => None,
    };
    Ok(Stmt::Return { value, span })
}

// ==================== Const Statement ====================

/// Lower a const statement to Core IR.
/// Type alias consts like `const ComplexF64 = Complex{Float64}` are skipped.
fn lower_const_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    for child in children {
        match walker.kind(&child) {
            NodeKind::Assignment => {
                let stmt = match lambda_ctx {
                    Some(ctx) => lower_assignment_with_ctx(walker, child, ctx),
                    None => lower_assignment(walker, child),
                }?;
                return Ok(wrap_const_assignment(stmt, span));
            }
            // `const global c = 1` / `global const c = 1` nest the global
            // declaration under the const wrapper (Issue #10943). Reuse the
            // global lowering (Global marker + module-scope assignment) and
            // add constness to the assignment; previously the child fell
            // through `_ => {}` and the binding was silently dropped.
            NodeKind::GlobalStatement => {
                // Parametric/structured type aliases (`const global A = Vector{Int}`)
                // are registered by the program-level type-alias pre-pass and
                // stay a no-op here, mirroring the plain-const arm below.
                if find_scoped_const_type_alias(walker, child).is_some() {
                    return Ok(Stmt::Expr {
                        expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                        span,
                    });
                }
                let stmt = lower_global_statement(walker, child, lambda_ctx)?;
                return Ok(wrap_const_assignment_deep(stmt, span));
            }
            // `const local c = 1` / `local const c = 1` are rejected by
            // upstream lowering with `expected assignment after "const"`
            // (verified against julia 1.12.6; Issue #10943). Previously the
            // child was silently dropped, leaving the binding undefined.
            NodeKind::LocalStatement | NodeKind::LocalDeclaration => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::Other(
                        "syntax: expected assignment after \"const\"".to_string(),
                    ),
                    span,
                ));
            }
            NodeKind::BinaryExpression => {
                // Pure Rust parser creates BinaryExpression for const assignments
                // Structure: BinaryExpression -> Identifier, Operator("="), value
                let expr_children = walker.named_children_vec(&child);
                if expr_children.len() >= 2 {
                    let rhs = &expr_children[expr_children.len() - 1];
                    // Skip type alias assignments (const X = Type{Param})
                    if walker.kind(rhs) == NodeKind::ParametrizedTypeExpression {
                        return Ok(Stmt::Expr {
                            expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                            span,
                        });
                    }
                    let stmt = match lambda_ctx {
                        Some(ctx) => lower_assignment_with_ctx(walker, child, ctx),
                        None => lower_assignment(walker, child),
                    }?;
                    return Ok(wrap_const_assignment(stmt, span));
                }
            }
            _ => {}
        }
    }
    // If no assignment found, return a no-op
    Ok(Stmt::Expr {
        expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    })
}

fn wrap_const_assignment(stmt: Stmt, span: Span) -> Stmt {
    if let Stmt::Assign { var, .. } = &stmt {
        Stmt::Block(Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "#__sjulia_declare_const__".to_string().into(),
                        args: vec![Expr::Literal(
                            crate::ir::core::Literal::Str(var.clone()),
                            span,
                        )],
                        kwargs: Vec::new(),
                        splat_mask: vec![false],
                        kwargs_splat_mask: Vec::new(),
                        span,
                    },
                    span,
                },
                stmt,
            ],
            span,
        })
    } else {
        stmt
    }
}

/// Apply `wrap_const_assignment` through the `Stmt::Block` produced by
/// `lower_global_statement` (`[Stmt::Global marker, Stmt::Assign]`) so a
/// `const global c = 1` binding is both global-routed and const-declared
/// (Issue #10943).
fn wrap_const_assignment_deep(stmt: Stmt, span: Span) -> Stmt {
    match stmt {
        Stmt::Block(block) => {
            let block_span = block.span;
            Stmt::Block(Block {
                stmts: block
                    .stmts
                    .into_iter()
                    .map(|s| wrap_const_assignment_deep(s, span))
                    .collect(),
                span: block_span,
            })
        }
        other => wrap_const_assignment(other, span),
    }
}

/// Find the structured type alias inside a scoped `const global` declaration
/// (`const global A = Vector{Int}`, `global const MyVec{T} = Vector{T}`), so
/// the const lowering can keep the plain-const no-op behavior for aliases
/// (the program-level pre-pass registers them; Issue #10943).
fn find_scoped_const_type_alias<'a>(
    walker: &CstWalker<'a>,
    scoped: Node<'a>,
) -> Option<TypeAliasDef> {
    let span = walker.span(&scoped);
    for inner in walker.named_children(&scoped) {
        let inner_kind = walker.kind(&inner);
        if inner_kind == NodeKind::BinaryExpression || inner_kind == NodeKind::Assignment {
            if let Some(alias) = extract_type_alias_from_binding(walker, inner, span) {
                return Some(alias);
            }
        }
    }
    None
}

// ==================== Global Statement ====================

/// Upstream lowering rejects `global`/`local` declarations whose child is not
/// a declarable name, an assignment, or a method definition with
/// `syntax: invalid syntax in "global" declaration` (verified against julia
/// 1.12.6; Issues #10945/#10937). `keyword` is `"global"` or `"local"`.
fn invalid_scoped_declaration_error(keyword: &str, span: Span) -> UnsupportedFeature {
    UnsupportedFeature::new(
        UnsupportedFeatureKind::Other(format!(
            "syntax: invalid syntax in \"{keyword}\" declaration"
        )),
        span,
    )
}

/// Validate that every child of a bare `global`/`local` declaration is a
/// declarable name shape. Upstream parses a full expression after the
/// keyword and rejects everything that is not a name/assignment/definition at
/// lowering; previously such children were silently dropped as
/// `Stmt::Global { names: [] }` (Issues #10945/#10951).
fn ensure_scoped_declaration_name_children<'a>(
    walker: &CstWalker<'a>,
    children: &[Node<'a>],
    keyword: &str,
) -> LowerResult<()> {
    for child in children {
        match walker.kind(child) {
            NodeKind::Identifier
            | NodeKind::TypedExpression
            | NodeKind::Operator
            | NodeKind::LineComment
            | NodeKind::BlockComment
            | NodeKind::Semicolon => {}
            NodeKind::TupleExpression => {
                for inner in walker.named_children(child) {
                    match walker.kind(&inner) {
                        NodeKind::Identifier | NodeKind::TypedExpression => {}
                        _ => {
                            return Err(invalid_scoped_declaration_error(
                                keyword,
                                walker.span(&inner),
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(invalid_scoped_declaration_error(
                    keyword,
                    walker.span(child),
                ));
            }
        }
    }
    Ok(())
}

/// Lower a global statement to Core IR.
///
/// A bare `global x` (or `global x, y`) lowers to a `Stmt::Global` marker that
/// records the declared names. `global x = value` and `global x += value` lower
/// to that same marker followed by the assignment, so the compiler can route
/// the binding to the module-level frame (Issues #5548, #5549).
fn lower_global_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    // Look for assignments or compound assignments in the children
    for child in &children {
        let assign_stmt = match walker.kind(child) {
            // `global f(x) = 2x` is a short-form method definition, not a
            // variable assignment — mirror the `local` arm's #8065 handling
            // (Issue #11008; upstream defines a module-scope method).
            NodeKind::Assignment if function::is_short_function_definition(walker, *child) => {
                let funcs = match lambda_ctx {
                    Some(ctx) => crate::lowering::lower_short_function_all_with_ctx_if_needed(
                        walker, *child, ctx,
                    )?,
                    None => function::lower_short_function_all(walker, *child)?,
                };
                let def = lower_function_defs_to_stmt(funcs, span);
                // Same `global` marker as the long-form arm below: inside a `let`
                // the method closes over the let-locals, and its closure value must
                // bind to the module-level name (Issue #11015).
                let names = defined_function_names(&def);
                return Ok(if names.is_empty() {
                    def
                } else {
                    Stmt::Block(Block {
                        stmts: vec![Stmt::Global { names, span }, def],
                        span,
                    })
                });
            }
            NodeKind::Assignment => Some(lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?),
            NodeKind::BinaryExpression => {
                // Check if this is an assignment (BinaryExpression with = operator)
                let is_assignment = walker
                    .children(child)
                    .iter()
                    .any(|c| c.kind() == "operator" && walker.text(c) == "=");
                if is_assignment {
                    Some(lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?)
                } else {
                    None
                }
            }
            NodeKind::CompoundAssignment => Some(lower_compound_assignment_maybe_ctx(
                walker, *child, lambda_ctx,
            )?),
            // Long-form definition item: `global function f(x) ... end` (the
            // Base bootstrap pattern inside `let` bodies; Issue #10937).
            // Named definitions lower to module-scope bindings on the plain
            // definition path, which is exactly the `global` intent —
            // delegate to it. The in-function case is rejected with
            // upstream's "Global method definition needs to be placed at the
            // top level" error by the function-body pre-scan
            // (`reject_global_method_definitions_in_body`). Previously the
            // child fell through `_ => None` and the definition was silently
            // dropped as `Stmt::Global { names: [] }`.
            NodeKind::FunctionDefinition => {
                let def = lower_stmt_impl(walker, *child, lambda_ctx)?;
                // Prefix a `global` marker naming the defined method(s), exactly
                // like the assignment arm below. Inside a `let` (the Base
                // bootstrap pattern) the definition closes over the let-locals,
                // and the resulting closure value must bind to the MODULE-level
                // name so callers outside the `let` see it — without the marker it
                // would bind to a let-local that the block discards on exit
                // (Issue #11015).
                let names = defined_function_names(&def);
                return Ok(if names.is_empty() {
                    def
                } else {
                    Stmt::Block(Block {
                        stmts: vec![Stmt::Global { names, span }, def],
                        span,
                    })
                });
            }
            // `global macro m() ... end` parses upstream but is rejected at
            // lowering with `invalid syntax in "global" declaration`
            // (verified against julia 1.12.6; Issues #10937/#10945).
            NodeKind::MacroDefinition => {
                return Err(invalid_scoped_declaration_error(
                    "global",
                    walker.span(child),
                ));
            }
            _ => None,
        };

        if let Some(stmt) = assign_stmt {
            // Prefix the assignment with a `global` marker so the compiler routes
            // both the read and the write to the module-level binding.
            return Ok(match assigned_var_names(&stmt) {
                Some(names) if !names.is_empty() => Stmt::Block(Block {
                    stmts: vec![Stmt::Global { names, span }, stmt],
                    span,
                }),
                _ => stmt,
            });
        }
    }

    // No assignment found - a bare declaration like `global x` or `global x, y`.
    // Anything that is not a declarable name (module/control-flow/jump/import
    // statements, non-assignment expressions, nested scope modifiers) is a
    // lowering error upstream — never drop it silently (Issue #10945).
    ensure_scoped_declaration_name_children(walker, &children, "global")?;
    let names = collect_scoped_decl_names(walker, &children);
    Ok(Stmt::Global { names, span })
}

/// Lower a `global` declaration used in value position.
///
/// Short-form function bodies such as `f() = global x = 1` are expression-shaped
/// syntactically, but Julia still treats the `global` declaration as a statement
/// whose assignment value becomes the body value. Emit an empty-binding
/// `LetBlock` so the compiler executes the declaration marker before the
/// value-producing assignment expression.
pub fn lower_global_value_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    for child in &children {
        let lowered = match walker.kind(child) {
            NodeKind::Assignment => {
                let stmt = lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?;
                let names = assigned_var_names(&stmt).unwrap_or_default();
                let expr = lower_assignment_value_expr_maybe_ctx(walker, *child, lambda_ctx)?;
                Some((names, expr))
            }
            NodeKind::BinaryExpression => {
                let is_assignment = walker
                    .children(child)
                    .iter()
                    .any(|c| c.kind() == "operator" && walker.text(c) == "=");
                if is_assignment {
                    let stmt = lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?;
                    let names = assigned_var_names(&stmt).unwrap_or_default();
                    let expr = lower_assignment_value_expr_maybe_ctx(walker, *child, lambda_ctx)?;
                    Some((names, expr))
                } else {
                    None
                }
            }
            NodeKind::CompoundAssignment => {
                let stmt = lower_compound_assignment_maybe_ctx(walker, *child, lambda_ctx)?;
                let names = assigned_var_names(&stmt).unwrap_or_default();
                let expr =
                    lower_compound_assignment_value_expr_maybe_ctx(walker, *child, lambda_ctx)?;
                Some((names, expr))
            }
            // `global function ... end` in value position (Issue #10937):
            // no meaningful value lowering exists — reject with a typed
            // error rather than silently dropping the definition.
            NodeKind::FunctionDefinition | NodeKind::MacroDefinition => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::Other(
                        "global declaration of a function/macro definition in value position"
                            .to_string(),
                    ),
                    span,
                ));
            }
            _ => None,
        };

        if let Some((names, expr)) = lowered {
            let mut stmts = Vec::new();
            if !names.is_empty() {
                stmts.push(Stmt::Global { names, span });
            }
            stmts.push(Stmt::Expr { expr, span });
            return Ok(Expr::LetBlock {
                bindings: vec![],
                body: Block { stmts, span },
                span,
            });
        }
    }

    // Value-position `global` declarations reject non-name children exactly
    // like the statement form — never drop them silently (Issue #10945).
    ensure_scoped_declaration_name_children(walker, &children, "global")?;
    let names = collect_scoped_decl_names(walker, &children);
    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![Stmt::Global { names, span }],
            span,
        },
        span,
    })
}

/// Extract the variable name(s) bound by a lowered assignment statement so a
/// `global` declaration can be attached to them. Returns `None` for assignment
/// forms that mutate an existing object (index/field/dict assignment) rather
/// than rebinding a name.
fn assigned_var_names(stmt: &Stmt) -> Option<Vec<String>> {
    match stmt {
        Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => Some(vec![var.clone()]),
        Stmt::DestructuringAssign { targets, .. } => Some(targets.clone()),
        _ => None,
    }
}

/// Collect the identifier names from a bare `global` declaration such as
/// `global x` or `global x, y`.
fn collect_scoped_decl_names<'a>(walker: &CstWalker<'a>, children: &[Node<'a>]) -> Vec<String> {
    let mut names = Vec::new();
    for child in children {
        match walker.kind(child) {
            NodeKind::Identifier => names.push(walker.text(child).to_string()),
            NodeKind::TypedExpression => {
                // `global x::T` — record the leading identifier.
                if let Some(id) = walker
                    .named_children(child)
                    .find(|c| walker.kind(c) == NodeKind::Identifier)
                {
                    names.push(walker.text(&id).to_string());
                }
            }
            NodeKind::TupleExpression => {
                // `global (x, y)` — record each identifier element.
                for inner in walker.named_children(child) {
                    if walker.kind(&inner) == NodeKind::Identifier {
                        names.push(walker.text(&inner).to_string());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

// ==================== Local Statement ====================

/// Lower a local statement to Core IR.
/// Explicit declarations retain Core.NewvarNode-like provenance via
/// [`Stmt::LocalDecl`], followed by the executable assignment/definition when
/// present.
fn lower_local_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);
    for child in &children {
        match walker.kind(child) {
            // `local f(args) = body` / `local f(args) where {T} = body`: the item
            // is a short-form function definition, not a variable assignment
            // (Issue #8065). Lower it as a function definition like a top-level
            // short form, otherwise the `Call`/`Where` LHS is rejected as an
            // unsupported assignment target.
            NodeKind::Assignment if function::is_short_function_definition(walker, *child) => {
                let funcs = match lambda_ctx {
                    Some(ctx) => crate::lowering::lower_short_function_all_with_ctx_if_needed(
                        walker, *child, ctx,
                    )?,
                    None => function::lower_short_function_all(walker, *child)?,
                };
                return Ok(with_local_declarations(
                    lower_function_defs_to_stmt(funcs, span),
                    span,
                ));
            }
            NodeKind::Assignment => {
                return Ok(with_local_declarations(
                    lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?,
                    span,
                ));
            }
            // Only `=` assignments lower on the assignment path; a
            // non-assignment operator tail (`local x + 1`) is upstream's
            // `invalid syntax in "local" declaration` case below.
            NodeKind::BinaryExpression
                if walker.named_children_vec(child).len() >= 2
                    && walker
                        .children(child)
                        .iter()
                        .any(|c| c.kind() == "operator" && walker.text(c) == "=") =>
            {
                return Ok(with_local_declarations(
                    lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?,
                    span,
                ));
            }
            // `local x += 1` parses upstream (the compound read then fails at
            // runtime when `x` is undefined); previously the child fell
            // through `_ => {}` and was silently dropped (Issue #10945).
            NodeKind::CompoundAssignment => {
                return Ok(with_local_declarations(
                    lower_compound_assignment_maybe_ctx(walker, *child, lambda_ctx)?,
                    span,
                ));
            }
            // Long-form definition item: `local function g() ... end`
            // (Issue #10937). Delegate to the plain definition lowering —
            // inside a function body that is the nested-closure path (local
            // semantics); previously the child fell through and the
            // definition was silently dropped.
            NodeKind::FunctionDefinition => {
                return Ok(with_local_declarations(
                    lower_stmt_impl(walker, *child, lambda_ctx)?,
                    span,
                ));
            }
            // `local macro m() ... end` parses upstream but is rejected at
            // lowering with `invalid syntax in "local" declaration`
            // (verified against julia 1.12.6; Issues #10937/#10945).
            NodeKind::MacroDefinition => {
                return Err(invalid_scoped_declaration_error(
                    "local",
                    walker.span(child),
                ));
            }
            NodeKind::Identifier => continue,
            _ => {}
        }
    }
    // Everything left must be a declarable name shape; upstream rejects the
    // rest at lowering — never drop it silently (Issue #10945).
    ensure_scoped_declaration_name_children(walker, &children, "local")?;
    let names = collect_scoped_decl_names(walker, &children);
    Ok(local_declarations_stmt(names, span))
}

#[doc(hidden)]
pub fn with_local_declarations(stmt: Stmt, span: Span) -> Stmt {
    let mut names = Vec::new();
    collect_stmt_binding_names(&stmt, &mut names);
    let mut stmts: Vec<_> = names
        .into_iter()
        .map(|var| Stmt::LocalDecl {
            var,
            kind: LocalDeclKind::Explicit,
            span,
        })
        .collect();
    stmts.push(stmt);
    Stmt::Block(Block { stmts, span })
}

fn collect_stmt_binding_names(stmt: &Stmt, names: &mut Vec<String>) {
    match stmt {
        Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => names.push(var.clone()),
        Stmt::DestructuringAssign { targets, .. } => names.extend(targets.iter().cloned()),
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. }
            if !names.contains(&func.name) =>
        {
            names.push(func.name.clone());
        }
        Stmt::FunctionDef { .. } | Stmt::EvalFunctionDef { .. } => {}
        Stmt::Block(block) => {
            for stmt in &block.stmts {
                collect_stmt_binding_names(stmt, names);
            }
        }
        _ => {}
    }
}

fn local_declarations_stmt(names: Vec<String>, span: Span) -> Stmt {
    let mut stmts: Vec<_> = names
        .into_iter()
        .map(|var| Stmt::LocalDecl {
            var,
            kind: LocalDeclKind::Explicit,
            span,
        })
        .collect();
    if stmts.len() == 1 {
        stmts.remove(0)
    } else {
        Stmt::Block(Block { stmts, span })
    }
}

fn lower_expr_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    match lambda_ctx {
        Some(ctx) => expr::lower_expr_with_ctx(walker, node, ctx),
        None => expr::lower_expr(walker, node),
    }
}

fn lower_assignment_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_assignment_with_ctx(walker, node, ctx),
        None => lower_assignment(walker, node),
    }
}

fn lower_assignment_value_expr_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    match lambda_ctx {
        Some(ctx) => lower_assignment_value_expr_with_ctx(walker, node, ctx),
        None => lower_assignment_value_expr(walker, node),
    }
}

fn lower_compound_assignment_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_compound_assignment_with_ctx(walker, node, ctx),
        None => lower_compound_assignment(walker, node),
    }
}

fn lower_compound_assignment_value_expr_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    match lambda_ctx {
        Some(ctx) => lower_compound_assignment_expr_with_ctx(walker, node, ctx),
        None => lower_compound_assignment_expr(walker, node),
    }
}

fn lower_for_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_for_stmt_with_ctx(walker, node, ctx),
        None => lower_for_stmt(walker, node),
    }
}

fn lower_if_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_if_stmt_with_ctx(walker, node, ctx),
        None => lower_if_stmt(walker, node),
    }
}

fn lower_while_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_while_stmt_with_ctx(walker, node, ctx),
        None => lower_while_stmt(walker, node),
    }
}

fn lower_try_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_try_stmt_with_ctx(walker, node, ctx),
        None => lower_try_stmt(walker, node),
    }
}

fn lower_macro_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_macro_with_ctx(walker, node, ctx),
        None => lower_macro(walker, node),
    }
}

// ==================== Type Alias Detection ====================

use crate::ir::core::TypeAliasDef;

/// Check if a const statement is a type alias definition.
/// Type aliases have the form: `const Name = TypeExpr` or the parametric form
/// `const Name{T...} = TypeExpr{T...}` where TypeExpr is a type expression.
/// Returns Some(TypeAliasDef) if this is a type alias, None otherwise.
pub fn try_extract_type_alias<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<TypeAliasDef> {
    if walker.kind(&node) != NodeKind::ConstStatement {
        return None;
    }

    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    for child in children {
        let child_kind = walker.kind(&child);
        if child_kind == NodeKind::BinaryExpression || child_kind == NodeKind::Assignment {
            if let Some(alias) = extract_type_alias_from_binding(walker, child, span) {
                return Some(alias);
            }
        }
        // `const global A = Vector{Int}` nests the binding one level deeper
        // (Issue #10943) — descend into the scoped declaration wrapper.
        if child_kind == NodeKind::GlobalStatement {
            if let Some(alias) = find_scoped_const_type_alias(walker, child) {
                return Some(alias);
            }
        }
    }

    None
}

/// Extract a type alias from a plain (non-`const`) assignment node such as
/// `MyVec{T} = Vector{T}` or `IntVec = Vector{Int}` (Issue #5055). Returns
/// `None` when the node is not a structured type alias (e.g. an ordinary value
/// assignment or a short function definition).
pub fn try_extract_type_alias_from_assignment<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<TypeAliasDef> {
    if walker.kind(&node) != NodeKind::Assignment {
        return None;
    }
    let span = walker.span(&node);
    extract_type_alias_from_binding(walker, node, span)
}

/// Shared `LHS = RHS` type-alias extraction for both `const` and plain
/// assignment forms. Handles a bare identifier LHS (`Name = T`) and a
/// parametric LHS (`Name{P...} = T{P...}`).
fn extract_type_alias_from_binding<'a>(
    walker: &CstWalker<'a>,
    binding: Node<'a>,
    span: Span,
) -> Option<TypeAliasDef> {
    let expr_children = walker.named_children_vec(&binding);
    if expr_children.len() < 2 {
        return None;
    }
    let lhs = &expr_children[0];
    let rhs = &expr_children[expr_children.len() - 1];
    let rhs_kind = walker.kind(rhs);

    // `Name = Body where {T...}` is a runtime UnionAll-valued binding, not a
    // static string alias. Registering it as a non-parametric alias makes
    // `Name{Args...}` expand to the unsubstituted target and ignore `Args`
    // (Issue #5053). Parametric alias syntax (`Name{T} = Target{T}`) remains
    // handled below.
    if rhs_kind == NodeKind::WhereExpression {
        // Issue #10501: record the declined name as a RUNTIME type binding
        // (`MyVec{T} = Vector{T} where T<:Real` binds `MyVec` to a runtime
        // `UnionAll`, Issue #10372; likewise `W = Vector{T} where T<:Real`
        // binds `W`). A later bare-identifier assignment applying such a name
        // (`z = MyVec{Float64}`) must then also decline below and lower as an
        // ordinary runtime assignment.
        if let Some(name) = where_binding_runtime_name(walker, lhs) {
            crate::lowering::type_alias::register_runtime_type_binding(&name);
        }
        return None;
    }

    // RHS must be a type expression for this to be a type alias.
    if !is_type_expression(walker, rhs, rhs_kind) {
        return None;
    }

    // Issue #10373: an anonymous covariant/contravariant bound shorthand
    // whose bound is a bare identifier that no static table resolves
    // (`x = Vector{<:SomeUndefinedName}`, `x = Dict{String, >:Undef}`) is
    // NOT a static string alias. Registering it would freeze the unresolved
    // bound name into a compound type-name literal that the runtime string
    // parser accepts silently. Rejecting it here routes the assignment
    // through ordinary value lowering, where `is_dynamic_type_arg` sends the
    // shorthand down the dynamic path that resolves the bound via runtime
    // global lookup -- raising `UndefVarError` for genuinely undefined names
    // (and still resolving user structs/abstract types correctly), exactly
    // like upstream Julia.
    if expr::parametric_type_has_unresolved_anonymous_bound(walker, *rhs) {
        return None;
    }

    let target_type = walker.text(rhs).to_string();

    match walker.kind(lhs) {
        // Non-parametric alias: `Name = TypeExpr`.
        NodeKind::Identifier => {
            // Issue #10501: a bare-identifier binding whose RHS applies (or
            // names) a runtime type binding — e.g. `z = MyVec{Float64}` after
            // the `where`-clause alias `MyVec{T} = Vector{T} where T<:Real`
            // — assigns a type VALUE in upstream Julia; it is not a new
            // static alias, and its target cannot be expanded statically.
            // Decline so ordinary assignment lowering emits a genuine runtime
            // binding (the value-position parametric-type path applies the
            // `UnionAll` at runtime), and record the LHS itself as a runtime
            // type binding so chained applications stay runtime as well.
            let name = walker.text(lhs).to_string();
            let rhs_base = target_type
                .split('{')
                .next()
                .unwrap_or(target_type.as_str())
                .trim();
            if crate::lowering::type_alias::is_runtime_type_binding(rhs_base) {
                crate::lowering::type_alias::register_runtime_type_binding(&name);
                return None;
            }
            // Issue #10501: freeze the RESOLVED target, not the verbatim RHS
            // text. `z = AliasG{Float64}` after `const AliasG{T} = Vector{T}`
            // must register `z -> "Vector{Float64}"`: the compile-time
            // consumer (`resolve_visible_type_alias`) resolves exactly one
            // level, so an unexpanded alias-application target would surface
            // as the frozen text `AliasG{Float64}`.
            Some(TypeAliasDef {
                name,
                target_type: crate::lowering::type_alias::expand(&target_type),
                params: Vec::new(),
                span,
            })
        }
        // Parametric alias: `Name{P...} = TypeExpr` (Issue #5055).
        NodeKind::ParametrizedTypeExpression => {
            let (name, params) = extract_parametric_alias_lhs(walker, *lhs)?;
            Some(TypeAliasDef {
                name,
                target_type,
                params,
                span,
            })
        }
        _ => None,
    }
}

/// Extract `(base_name, params)` from a parametric type-alias LHS such as
/// `MyVec{T}` -> `("MyVec", ["T"])` or `MyDict{K, V}` -> `("MyDict", ["K","V"])`.
/// Returns `None` if no parameters are present (a bare parametric head is not a
/// valid parametric alias LHS).
///
/// `pub` so `lowering::stmt::assignment` can reuse it for the
/// runtime-value fallback (`Name{P...} = RHS` where RHS is not a flat type
/// template, e.g. carries a `where` clause — Issue #10372): both call sites
/// need the identical `(base_name, param_names)` extraction.
pub fn extract_parametric_alias_lhs<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
) -> Option<(String, Vec<String>)> {
    let children = walker.named_children_vec(&lhs);
    let mut base: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    for child in children {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if base.is_none() {
                    base = Some(walker.text(&child).to_string());
                } else {
                    params.push(walker.text(&child).to_string());
                }
            }
            NodeKind::CurlyExpression => {
                for p in walker.named_children(&child) {
                    // A parameter may carry a bound (`T<:Number`); record just
                    // the leading identifier name for positional substitution.
                    if let Some(pname) = first_identifier_text(walker, p) {
                        params.push(pname);
                    }
                }
            }
            NodeKind::TypeParameterList => {
                for p in walker.named_children(&child) {
                    if let Some(pname) = first_identifier_text(walker, p) {
                        params.push(pname);
                    }
                }
            }
            _ => {
                if base.is_some() {
                    if let Some(pname) = first_identifier_text(walker, child) {
                        params.push(pname);
                    }
                }
            }
        }
    }
    let base = base?;
    if params.is_empty() {
        return None;
    }
    Some((base, params))
}

/// For a declined `LHS = RHS where ...` binding, return the name that will
/// carry the runtime `UnionAll` value at execution time: the identifier itself
/// for a bare LHS (`W = Vector{T} where T<:Real`), or the parametric base for
/// a parametric LHS (`MyVec{T} = Vector{T} where T<:Real`, Issue #10372).
/// Used to record runtime type bindings for Issue #10501.
fn where_binding_runtime_name<'a>(walker: &CstWalker<'a>, lhs: &Node<'a>) -> Option<String> {
    match walker.kind(lhs) {
        NodeKind::Identifier => Some(walker.text(lhs).to_string()),
        NodeKind::ParametrizedTypeExpression => {
            extract_parametric_alias_lhs(walker, *lhs).map(|(name, _params)| name)
        }
        _ => None,
    }
}

/// Return the text of the first `Identifier` at or under `node` (the node
/// itself if it is an identifier), used to read a type-parameter name.
fn first_identifier_text<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    if walker.kind(&node) == NodeKind::Identifier {
        return Some(walker.text(&node).to_string());
    }
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Identifier {
            return Some(walker.text(&child).to_string());
        }
    }
    None
}

/// Check if a node represents a type expression.
fn is_type_expression<'a>(walker: &CstWalker<'a>, node: &Node<'a>, kind: NodeKind) -> bool {
    match kind {
        // Parametrized types: Complex{Float64}, Array{Int64, 2}
        NodeKind::ParametrizedTypeExpression => true,
        // Union types: Union{Int64, Float64}
        NodeKind::CurlyExpression => {
            let text = walker.text(node);
            text.starts_with("Union{")
        }
        // Type identifiers that look like type names (start with uppercase)
        NodeKind::Identifier => {
            let text = walker.text(node);
            // Check if it looks like a type name (uppercase first letter)
            // and is a known type or ends with a common type suffix
            is_likely_type_name(text)
        }
        // Module-qualified type name (`const Y = OwnerA.X`, Issue #11068):
        // a field chain whose leaf looks like a type name. The const still
        // lowers as an ordinary runtime binding either way; the alias entry
        // only adds compile-time type-position resolution.
        NodeKind::FieldExpression => {
            let text = walker.text(node);
            text.rsplit('.').next().is_some_and(is_likely_type_name)
                && text.split('.').all(|segment| {
                    segment
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                })
        }
        _ => false,
    }
}

/// Check if an identifier looks like a type name.
fn is_likely_type_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let first_char = match name.chars().next() {
        Some(c) => c,
        None => return false,
    };
    if !first_char.is_ascii_uppercase() {
        return false;
    }

    // Issue #11104: a name the program itself DECLARES as a type (`struct E`,
    // `abstract type MyAbs`, `primitive type ...`, in this file or one of its
    // modules) is a type name too. Without this, `const AE = E` was not
    // recognized as a type alias, so `f(x::AE)` registered a method against the
    // nominal placeholder `AE` and never matched an `E` value (`MethodError`).
    // The set comes from the CST pre-scan, so it is independent of source order
    // and of whether Base was loaded from cache or from source.
    if crate::lowering::type_alias::is_declared_type(name)
        || crate::lowering::type_alias::is_registered_alias(name)
    {
        return true;
    }

    // Check against known builtin types
    matches!(
        name,
        "Int8"
            | "Int16"
            | "Int32"
            | "Int64"
            | "Int128"
            | "Int"
            | "UInt8"
            | "UInt16"
            | "UInt32"
            | "UInt64"
            | "UInt128"
            | "UInt"
            | "Float16"
            | "Float32"
            | "Float64"
            | "Bool"
            | "String"
            | "Char"
            | "Array"
            | "Vector"
            | "Matrix"
            | "DenseArray"
            | "DenseVector"
            | "DenseMatrix"
            | "Tuple"
            | "NamedTuple"
            | "Dict"
            | "Set"
            | "Any"
            | "Number"
            | "Real"
            | "Integer"
            | "AbstractFloat"
            | "Nothing"
            | "Missing"
            | "Complex"
            | "Rational"
    )
}
