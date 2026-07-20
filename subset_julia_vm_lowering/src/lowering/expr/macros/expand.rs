//! User-defined and Base macro expansion in expression context.

use super::super::lower_expr_with_ctx;
#[cfg(debug_assertions)]
use super::macro_debug_log;
use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::Expr;
use crate::lowering::{get_node_macro_type, LowerResult, MacroParamType};
use crate::parser::cst::{CstWalker, Node, NodeKind};

pub fn lower_macroexpand_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@macroexpand requires a macro call as argument"),
        );
    }

    let inner = args[0];
    if walker.kind(&inner) != NodeKind::MacroCall {
        return lower_expr_with_ctx(walker, inner, lambda_ctx);
    }

    let macro_ident = walker.find_child(&inner, NodeKind::MacroIdentifier);
    let inner_macro_name = match macro_ident {
        Some(ident) => walker.text(&ident).trim_start_matches('@').to_string(),
        None => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("@macroexpand: could not find macro identifier"),
            );
        }
    };

    let _args_node = walker.find_child(&inner, NodeKind::MacroArgumentList);
    let direct_args: Vec<_> = walker
        .named_children(&inner)
        .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
        .collect();

    #[cfg(debug_assertions)]
    macro_debug_log(format_args!("@macroexpand @{}", inner_macro_name));

    let inner_span = walker.span(&inner);

    if lambda_ctx.has_macro(&inner_macro_name) {
        let expanded = expand_user_defined_macro_value_expr(
            walker,
            &inner_macro_name,
            &direct_args,
            inner_span,
            lambda_ctx,
        )?;
        #[cfg(debug_assertions)]
        macro_debug_log(format_args!("  Expanded to: {:?}", expanded));
        return Ok(expanded);
    }

    if crate::lowering::macros_registry::has_base_macro(&inner_macro_name) {
        let expanded = expand_base_macro_value_expr(
            walker,
            &inner_macro_name,
            &direct_args,
            inner_span,
            lambda_ctx,
        )?;
        #[cfg(debug_assertions)]
        macro_debug_log(format_args!("  Expanded to: {:?}", expanded));
        return Ok(expanded);
    }

    let result = if crate::lowering::macros_registry::has_base_macro(&inner_macro_name) {
        expand_base_macro_value_expr(
            walker,
            &inner_macro_name,
            &direct_args,
            inner_span,
            lambda_ctx,
        )
    } else {
        Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, inner_span)
                .with_hint(format!("unknown macro @{}", inner_macro_name)),
        )
    };

    #[cfg(debug_assertions)]
    if let Ok(expanded) = &result {
        macro_debug_log(format_args!("  Expanded to: {:?}", expanded));
    }

    result
}

/// Expand a user-defined macro call in expression context.
pub fn expand_user_defined_macro_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let arg_types: Vec<MacroParamType> = args
        .iter()
        .map(|arg| get_node_macro_type(walker, arg))
        .collect();

    let macro_def = lambda_ctx
        .get_macro_with_types(macro_name, &arg_types)
        .or_else(|| lambda_ctx.get_macro_with_arity(macro_name, args.len()))
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} not found (with {} args)",
                macro_name,
                args.len()
            ))
        })?;

    let min_args = if macro_def.has_varargs {
        macro_def.params.len() - 1
    } else {
        macro_def.params.len()
    };
    if args.len() < min_args {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} expects at least {} arguments, got {}",
                macro_name,
                min_args,
                args.len()
            )),
        );
    }

    crate::lowering::macro_expander::expand_macro_to_expr(
        walker, macro_name, &macro_def, args, span, lambda_ctx,
    )
}

pub fn expand_user_defined_macro_value_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let arg_types: Vec<MacroParamType> = args
        .iter()
        .map(|arg| get_node_macro_type(walker, arg))
        .collect();

    let macro_def = lambda_ctx
        .get_macro_with_types(macro_name, &arg_types)
        .or_else(|| lambda_ctx.get_macro_with_arity(macro_name, args.len()))
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} not found (with {} args)",
                macro_name,
                args.len()
            ))
        })?;

    let min_args = if macro_def.has_varargs {
        macro_def.params.len() - 1
    } else {
        macro_def.params.len()
    };
    if args.len() < min_args {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} expects at least {} arguments, got {}",
                macro_name,
                min_args,
                args.len()
            )),
        );
    }

    crate::lowering::macro_expander::expand_macro_to_value_expr(
        walker, macro_name, &macro_def, args, span, lambda_ctx,
    )
}

/// Expand a bundled-package macro call (e.g. Plots' `@animate`/`@gif`) in
/// expression context.
///
/// Bundled-package macros are expanded through the same full `macro_runtime` path
/// as user-defined macros — not the limited template path used for Base/stdlib —
/// so they may construct AST nodes (`Expr(:for, …)`, `.args` access) at expansion
/// time. Issue #6355: `anim = @animate for … end` is a macro call in value
/// position, which previously never reached the bundled-macro registry.
pub fn expand_bundled_package_macro_expr<'a>(
    walker: &CstWalker<'a>,
    module_name: &str,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let macro_def =
        crate::lowering::macros_registry::get_bundled_package_macro(module_name, macro_name)
            .ok_or_else(|| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                    "bundled-package macro @{} not found in {}",
                    macro_name, module_name
                ))
            })?;

    let min_args = if macro_def.has_varargs {
        macro_def.params.len() - 1
    } else {
        macro_def.params.len()
    };
    if args.len() < min_args {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} expects at least {} arguments, got {}",
                macro_name,
                min_args,
                args.len()
            )),
        );
    }

    crate::lowering::macros_registry::add_bundled_package_macro_context(module_name, lambda_ctx);
    crate::lowering::macro_expander::expand_macro_to_expr(
        walker, macro_name, &macro_def, args, span, lambda_ctx,
    )
}

/// Expand a Base macro call in expression context.
pub fn expand_base_macro_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    // Multi-argument @show (Issue #4868) in expression context, e.g.
    // `(@show a b c) == c`. Build the same `_do_show` block as the statement
    // path and wrap it in a LetBlock so the expression evaluates to the value
    // of the last argument. See build_multiarg_show_block for details.
    if macro_name == "show" && args.len() != 1 {
        let block = crate::lowering::stmt::macros::expand::build_multiarg_show_block(
            walker, args, span, lambda_ctx,
        )?;
        return Ok(Expr::LetBlock {
            bindings: vec![],
            body: block,
            span,
        });
    }

    // Select the overload matching the call arity. A base macro like `@sprintf`
    // has several fixed-arity definitions (`sprintf(fmt)`, `sprintf(fmt, a1)`, ...);
    // using the arity-agnostic `get_base_macro` here picked the first (1-arg)
    // overload, so `@sprintf("%d", 42)` in value position dropped all value
    // arguments and produced "" (Issue #5683). The statement-position expander
    // already uses the arity-aware lookup.
    let macro_def =
        crate::lowering::macros_registry::get_base_macro_with_arity(macro_name, args.len())
            .ok_or_else(|| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint(format!("Base macro @{} not found", macro_name))
            })?;

    let min_args = if macro_def.has_varargs {
        macro_def.params.len() - 1
    } else {
        macro_def.params.len()
    };
    if args.len() < min_args {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} expects at least {} arguments, got {}",
                macro_name,
                min_args,
                args.len()
            )),
        );
    }

    crate::lowering::macro_expander::expand_macro_to_expr(
        walker, macro_name, &macro_def, args, span, lambda_ctx,
    )
}

pub fn expand_base_macro_value_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let macro_def =
        crate::lowering::macros_registry::get_base_macro_with_arity(macro_name, args.len())
            .ok_or_else(|| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint(format!("Base macro @{} not found", macro_name))
            })?;

    let min_args = if macro_def.has_varargs {
        macro_def.params.len() - 1
    } else {
        macro_def.params.len()
    };
    if args.len() < min_args {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} expects at least {} arguments, got {}",
                macro_name,
                min_args,
                args.len()
            )),
        );
    }

    crate::lowering::macro_expander::expand_macro_to_value_expr(
        walker, macro_name, &macro_def, args, span, lambda_ctx,
    )
}

/// Expand a stdlib macro call (e.g. `Test.@test`) in expression context.
///
/// stdlib macros (`STDLIB_MACROS`, registered when `using <Module>` loads a
/// module with `macro ... end` definitions) are expanded by a static
/// quote-substitution engine that is *separate* from the VM-backed
/// `macro_expander` used for user-defined/Base/bundled-package macros (see
/// `crate::lowering::stmt::macros::expand::expand_stdlib_macro` and Issue
/// #10266 for the two-engine background). Before this function existed, that
/// engine was reachable only from statement position, so a stdlib macro like
/// `Test.@test` used as an operand/argument/assignment RHS fell through to an
/// unconditional "requires `using Test`" error even when the module *was*
/// imported (Issue #10293 / #10307) -- the hint was structurally wrong, not
/// import-related.
///
/// Expression position uses the value-preserving common macro-runtime entry;
/// statement position continues to use its statement adapter, which naturally
/// discards the quote tail after retaining its effects (Issue #10307/#10496).
pub fn expand_stdlib_macro_expr<'a>(
    walker: &CstWalker<'a>,
    _node: Node<'a>,
    module_name: &str,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let macro_def = crate::lowering::macros_registry::get_stdlib_macro(module_name, macro_name)
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "stdlib macro @{} not found in module {}",
                macro_name, module_name
            ))
        })?;
    crate::lowering::macro_expander::expand_macro_to_expr(
        walker, macro_name, &macro_def, args, span, lambda_ctx,
    )
}
