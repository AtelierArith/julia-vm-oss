//! CST to Expr constructor conversion for quote expressions.
//!
//! Contains `lower_quote_expr` (entry point) and `cst_to_expr_constructor`
//! which convert CST nodes to IR Expr constructors for quoted values.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::cell::Cell;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::expr_heads::ExprHead;
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::lowering::{internal_lowering_error, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

use super::super::helpers::{is_broadcast_op, process_raw_string_escapes, strip_broadcast_dot};
use super::super::literal::{parse_float, process_escape_sequences};

thread_local! {
    static PRESERVE_MACRO_DOLLAR: Cell<bool> = const { Cell::new(false) };
}

fn preserve_macro_dollar() -> bool {
    PRESERVE_MACRO_DOLLAR.with(Cell::get)
}

pub fn cst_to_macro_arg_constructor<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Expr> {
    let old = PRESERVE_MACRO_DOLLAR.with(|flag| {
        let old = flag.get();
        flag.set(true);
        old
    });
    let result = cst_to_expr_constructor(walker, node);
    PRESERVE_MACRO_DOLLAR.with(|flag| flag.set(old));
    result
}

/// Build the Expr-constructor for a broadcast, producing (at macro runtime) an
/// `Expr` value equal to `materialize(Broadcasted(fn, (args...)))` — the same
/// shape `make_broadcasted_call` emits for non-quoted broadcasts, so the quoted
/// value re-lowers to a real broadcast (Issue #7029). `fn_name` is the base
/// operator/function name (e.g. "sin" or "-", never the dotted ".-").
fn make_broadcast_constructor(fn_name: &str, arg_constructors: Vec<Expr>, span: Span) -> Expr {
    // `&&` / `||` are syntax, not callable functions, so dotted short-circuit
    // broadcasts (`xs .&& ys`) use the `andand`/`oror` wrappers — mirror the binary
    // lowering path (binary.rs, Issue #2545) so the quoted form matches (Issue #7029).
    let fn_name = match fn_name {
        "&&" => "andand",
        "||" => "oror",
        other => other,
    };
    let symbol = |s: &str| Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args: vec![Expr::Literal(Literal::Str(s.to_string()), span)],
        span,
    };
    // (args...)  =>  Expr(:tuple, args...)
    let mut tuple_args = vec![symbol("tuple")];
    tuple_args.extend(arg_constructors);
    let args_tuple = Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: tuple_args,
        span,
    };
    // Broadcasted(fn, (args...))  =>  Expr(:call, :Broadcasted, :fn, args_tuple)
    let broadcasted = Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: vec![
            symbol("call"),
            symbol("Broadcasted"),
            symbol(fn_name),
            args_tuple,
        ],
        span,
    };
    // materialize(Broadcasted(...))  =>  Expr(:call, :materialize, broadcasted)
    Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: vec![symbol("call"), symbol("materialize"), broadcasted],
        span,
    }
}

fn for_binding_constructor<'a>(
    walker: &CstWalker<'a>,
    binding: Node<'a>,
    span: Span,
) -> LowerResult<Expr> {
    let binding_children = walker.named_children_vec(&binding);
    let (var_node, iter_node) = if binding_children.len() >= 2 {
        let first_text = walker.text(&binding_children[0]);
        if first_text == "outer" && binding_children.len() >= 3 {
            (binding_children[1], binding_children[2])
        } else {
            (binding_children[0], binding_children[1])
        }
    } else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "quote of for binding: malformed binding".to_string(),
            ),
            span,
        ));
    };

    Ok(Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: vec![
            Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("=".to_string()), span)],
                span,
            },
            cst_to_expr_constructor(walker, var_node)?,
            cst_to_expr_constructor(walker, iter_node)?,
        ],
        span,
    })
}

/// Build the `Expr(:generator, body, binding...)` constructor shared by a
/// comprehension's inner generator (`ComprehensionExpression`) and a bare
/// generator expression (`GeneratorExpression`) (Issue #10626). `children` is
/// the node's named children: the body expression followed by one or more
/// `ForClause`s. Only a single `ForClause` is supported (a comma-grouped
/// multi-binding clause or a whitespace-flattened multi-clause form, and an
/// `IfClause` filter, are rejected with a clear hint — `kind_label` names the
/// construct in the error message; tracked by Issue #10923, filed separately
/// from #10626 so the reference stays live after #10626 closes).
fn generator_constructor<'a>(
    walker: &CstWalker<'a>,
    children: &[Node<'a>],
    span: Span,
    kind_label: &str,
) -> LowerResult<Expr> {
    // Group the clauses: each `ForClause` contributes its (possibly
    // comma-grouped) `=` bindings; an `IfClause` filter attaches to the
    // clause group it follows, wrapping those bindings in upstream's
    // `Expr(:filter, cond, binding...)` (Issue #10923). Multiple `ForClause`s
    // (the whitespace `for ... for ...` form) nest: the innermost generator
    // wraps the body, each outer clause wraps the previous generator, and
    // the whole chain is wrapped in `Expr(:flatten, ...)` — the exact shapes
    // upstream `Meta.quot` produces.
    struct ClauseGroup {
        bindings: Vec<Expr>,
        filter: Option<Expr>,
    }
    let mut groups: Vec<ClauseGroup> = Vec::new();
    for child in children.iter().skip(1) {
        match walker.kind(child) {
            NodeKind::ForClause => {
                let mut bindings = Vec::new();
                for binding in walker.named_children(child) {
                    if walker.kind(&binding) == NodeKind::ForBinding {
                        bindings.push(for_binding_constructor(walker, binding, span)?);
                    }
                }
                groups.push(ClauseGroup {
                    bindings,
                    filter: None,
                });
            }
            NodeKind::IfClause => {
                let cond = walker
                    .named_children_vec(child)
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(format!(
                                "quote of {}: empty if clause",
                                kind_label
                            )),
                            span,
                        )
                    })?;
                let Some(group) = groups.last_mut() else {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(format!(
                            "quote of {}: if clause without a for clause",
                            kind_label
                        )),
                        span,
                    ));
                };
                group.filter = Some(cst_to_expr_constructor(walker, cond)?);
            }
            _ => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "quote of {}: unsupported clause {}",
                        kind_label,
                        child.kind()
                    )),
                    span,
                ));
            }
        }
    }
    if groups.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "quote of {}: missing for clause",
                kind_label
            )),
            span,
        ));
    }

    let symbol = |name: &str| Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
        span,
    };
    let make_generator = |body: Expr, group: ClauseGroup| -> Expr {
        let mut args = vec![symbol("generator"), body];
        match group.filter {
            Some(cond) => {
                let mut filter_args = vec![symbol("filter"), cond];
                filter_args.extend(group.bindings);
                args.push(Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args: filter_args,
                    span,
                });
            }
            None => args.extend(group.bindings),
        }
        Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args,
            span,
        }
    };

    let nested = groups.len() > 1;
    let mut expr = cst_to_expr_constructor(walker, children[0])?;
    for group in groups.into_iter().rev() {
        expr = make_generator(expr, group);
    }
    if nested {
        expr = Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args: vec![symbol("flatten"), expr],
            span,
        };
    }
    Ok(expr)
}

fn symbol_constructor(name: &str, span: Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
        span,
    }
}

fn line_number_constructor(line: usize, span: Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::LineNumberNodeNew,
        args: vec![Expr::Literal(Literal::Int(line as i64), span)],
        span,
    }
}

fn globalref_constructor(module: &str, name: &str, span: Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::GlobalRefNew,
        args: vec![
            symbol_constructor(module, span),
            symbol_constructor(name, span),
        ],
        span,
    }
}

fn expr_constructor(head: &str, mut args: Vec<Expr>, span: Span) -> Expr {
    if let Some(expr_head) = ExprHead::from_name(head) {
        debug_assert!(expr_head.spec().cst_to_expr_value);
    }
    let mut expr_args = vec![symbol_constructor(head, span)];
    expr_args.append(&mut args);
    Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: expr_args,
        span,
    }
}

fn matrix_row_constructor<'a>(
    walker: &CstWalker<'a>,
    row: Node<'a>,
    span: Span,
) -> LowerResult<Expr> {
    let children = walker.named_children_vec(&row);
    if children.len() == 1 {
        cst_to_expr_constructor(walker, children[0])
    } else {
        let mut args = Vec::with_capacity(children.len());
        for child in children {
            args.push(cst_to_expr_constructor(walker, child)?);
        }
        Ok(expr_constructor("row", args, span))
    }
}

fn matrix_constructor<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);
    let rows: Vec<Node<'a>> = children
        .iter()
        .copied()
        .filter(|child| walker.kind(child) == NodeKind::MatrixRow)
        .collect();

    if rows.is_empty() {
        let mut args = Vec::with_capacity(children.len());
        for child in children {
            args.push(cst_to_expr_constructor(walker, child)?);
        }
        return Ok(expr_constructor("hcat", args, span));
    }

    if rows.len() == 1 {
        let row_children = walker.named_children_vec(&rows[0]);
        let mut args = Vec::with_capacity(row_children.len());
        for child in row_children {
            args.push(cst_to_expr_constructor(walker, child)?);
        }
        return Ok(expr_constructor("hcat", args, span));
    }

    let mut args = Vec::with_capacity(rows.len());
    for row in rows {
        args.push(matrix_row_constructor(walker, row, span)?);
    }
    Ok(expr_constructor("vcat", args, span))
}

fn call_constructor_with_callee<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    callee: Expr,
    span: Span,
) -> LowerResult<Expr> {
    let children = walker.named_children_vec(&node);
    let mut args = vec![symbol_constructor("call", span), callee];

    // Remaining children are arguments. A `;` separating positional and
    // keyword args (`f(a; b=v)`) is a structural marker, not an argument; the
    // `KeywordArgument` children carry the kwargs (Issue #7029).
    for child in children.iter().skip(1) {
        if walker.kind(child) == NodeKind::ArgumentList {
            for arg in walker.named_children(child) {
                if walker.kind(&arg) == NodeKind::Semicolon {
                    continue;
                }
                args.push(cst_to_expr_constructor(walker, arg)?);
            }
        } else if walker.kind(child) != NodeKind::Semicolon {
            args.push(cst_to_expr_constructor(walker, *child)?);
        }
    }

    Ok(Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        span,
    })
}

fn parameter_constructor<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    match walker.kind(&node) {
        NodeKind::Parameter | NodeKind::KwParameter => {
            let children = walker.named_children_vec(&node);
            match children.as_slice() {
                [] => cst_to_expr_constructor(walker, node),
                [single] => cst_to_expr_constructor(walker, *single),
                [name, ty, ..] => Ok(expr_constructor(
                    "::",
                    vec![
                        cst_to_expr_constructor(walker, *name)?,
                        cst_to_expr_constructor(walker, *ty)?,
                    ],
                    walker.span(&node),
                )),
            }
        }
        NodeKind::SplatParameter => {
            let children = walker.named_children_vec(&node);
            let arg = children.first().copied().unwrap_or(node);
            Ok(expr_constructor(
                "...",
                vec![cst_to_expr_constructor(walker, arg)?],
                walker.span(&node),
            ))
        }
        _ => cst_to_expr_constructor(walker, node),
    }
}

fn subtype_constraint_constructor<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);
    // The parser consumes the `<:` / `>:` token without keeping an Operator
    // child node (`parse_type_constraint`), so recover the constraint direction
    // from the node kind: a SupertypeConstraint is `>:`, everything else `<:`.
    // Only fall back to the default when no explicit Operator child is present
    // (Issue #7845 — value-position `where S>:Int` previously defaulted to `<:`).
    let default_op = if walker.kind(&node) == NodeKind::SupertypeConstraint {
        ">:"
    } else {
        "<:"
    };
    let op = children
        .iter()
        .find(|child| walker.kind(child) == NodeKind::Operator)
        .map(|child| walker.text(child))
        .unwrap_or(default_op);
    let operands: Vec<_> = children
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::Operator)
        .collect();
    if operands.len() < 2 {
        return cst_to_expr_constructor(walker, node);
    }
    // Double-bounded `Lower<:T<:Upper` is represented by the parser as
    // [name, upper, lower]. Preserve both bounds using Julia's quoted
    // `Expr(:comparison, lower, :<:, name, :<:, upper)` shape.
    if walker.kind(&node) == NodeKind::SubtypeConstraint && operands.len() >= 3 {
        return Ok(expr_constructor(
            "comparison",
            vec![
                cst_to_expr_constructor(walker, operands[2])?,
                symbol_constructor("<:", span),
                cst_to_expr_constructor(walker, operands[0])?,
                symbol_constructor("<:", span),
                cst_to_expr_constructor(walker, operands[1])?,
            ],
            span,
        ));
    }
    Ok(expr_constructor(
        op,
        vec![
            cst_to_expr_constructor(walker, operands[0])?,
            cst_to_expr_constructor(walker, operands[1])?,
        ],
        span,
    ))
}

fn where_clause_args<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Vec<Expr>> {
    let mut args = Vec::new();

    // A value-position bounded `where` (`expr where S<:Real`) reaches here as a
    // single SubtypeConstraint/SupertypeConstraint (or SubtypeExpression) node
    // whose named children are the *operands* (`S` and `Real`, or
    // `[T, Real, Int]` for `Int<:T<:Real`). It is one `where` arg, not a
    // parameter container, so route the whole node through
    // `where_param_constructor` → `subtype_constraint_constructor`. Without this
    // guard the loop below descends into the operands and appends them as bare
    // symbol args, dropping the constraint structure (Issue #7845). The
    // unbounded `where S` form is unaffected because a bare identifier is not a
    // constraint kind.
    if matches!(
        walker.kind(&node),
        NodeKind::SubtypeConstraint | NodeKind::SupertypeConstraint | NodeKind::SubtypeExpression
    ) {
        args.push(where_param_constructor(walker, node)?);
        return Ok(args);
    }

    let children = walker.named_children_vec(&node);
    if children.is_empty() {
        if matches!(
            walker.kind(&node),
            NodeKind::WhereClause
                | NodeKind::TypeParameters
                | NodeKind::TypeParameterList
                | NodeKind::CurlyExpression
        ) {
            return Ok(args);
        }
        // Bare `where T` reaches here as the leaf identifier `T` rather than as
        // a parameter container.
        args.push(where_param_constructor(walker, node)?);
        return Ok(args);
    }
    for child in children {
        match walker.kind(&child) {
            NodeKind::TypeParameters | NodeKind::TypeParameterList | NodeKind::CurlyExpression => {
                for param in walker.named_children(&child) {
                    args.push(where_param_constructor(walker, param)?);
                }
            }
            _ => args.push(where_param_constructor(walker, child)?),
        }
    }
    Ok(args)
}

fn doc_macrocall_constructor<'a>(
    walker: &CstWalker<'a>,
    doc_node: Node<'a>,
    target_node: Node<'a>,
) -> LowerResult<Expr> {
    let span = walker.span(&doc_node);
    Ok(expr_constructor(
        "macrocall",
        vec![
            globalref_constructor("Core", "@doc", span),
            line_number_constructor(span.start_line, span),
            cst_to_expr_constructor(walker, doc_node)?,
            cst_to_expr_constructor(walker, target_node)?,
        ],
        span,
    ))
}

fn where_param_constructor<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    match walker.kind(&node) {
        NodeKind::SubtypeExpression
        | NodeKind::BinaryExpression
        | NodeKind::SubtypeConstraint
        | NodeKind::SupertypeConstraint => subtype_constraint_constructor(walker, node),
        NodeKind::TypeParameter => {
            let children = walker.named_children_vec(&node);
            match children.as_slice() {
                [] => cst_to_expr_constructor(walker, node),
                [single] => cst_to_expr_constructor(walker, *single),
                [name, bound, ..] => Ok(expr_constructor(
                    "<:",
                    vec![
                        cst_to_expr_constructor(walker, *name)?,
                        cst_to_expr_constructor(walker, *bound)?,
                    ],
                    walker.span(&node),
                )),
            }
        }
        _ => cst_to_expr_constructor(walker, node),
    }
}

fn struct_header_constructor<'a>(
    walker: &CstWalker<'a>,
    parts: &[Node<'a>],
    span: Span,
) -> LowerResult<Expr> {
    let Some(name_node) = parts.first().copied() else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "quote of struct: missing name".to_string(),
            ),
            span,
        ));
    };
    let span = walker.span(&name_node);
    let mut header = cst_to_expr_constructor(walker, name_node)?;
    let mut supertype = None;

    for part in parts.iter().skip(1) {
        match walker.kind(part) {
            NodeKind::TypeParameters | NodeKind::TypeParameterList | NodeKind::CurlyExpression => {
                let mut curly_args = vec![header];
                for param in walker.named_children(part) {
                    curly_args.push(where_param_constructor(walker, param)?);
                }
                header = expr_constructor("curly", curly_args, walker.span(part));
            }
            _ => {
                supertype = Some(cst_to_expr_constructor(walker, *part)?);
            }
        }
    }

    if let Some(parent) = supertype {
        Ok(expr_constructor("<:", vec![header, parent], span))
    } else {
        Ok(header)
    }
}

fn struct_definition_constructor<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    mutable: bool,
    span: Span,
) -> LowerResult<Expr> {
    let children = walker.named_children_vec(&node);
    let body_index = children
        .iter()
        .position(|child| walker.kind(child) == NodeKind::Block)
        .ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(
                    "quote of struct: missing body".to_string(),
                ),
                span,
            )
        })?;
    let header = struct_header_constructor(walker, &children[..body_index], span)?;
    let body = cst_to_expr_constructor(walker, children[body_index])?;

    Ok(expr_constructor(
        "struct",
        vec![Expr::Literal(Literal::Bool(mutable), span), header, body],
        span,
    ))
}

fn parameter_list_signature_args<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: Span,
) -> LowerResult<Vec<Expr>> {
    let mut positional = Vec::new();
    let mut keyword = Vec::new();
    let mut in_kwargs = false;

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Semicolon => in_kwargs = true,
            _ if in_kwargs => keyword.push(parameter_constructor(walker, child)?),
            _ => positional.push(parameter_constructor(walker, child)?),
        }
    }

    let mut args = Vec::new();
    if !keyword.is_empty() {
        args.push(expr_constructor("parameters", keyword, span));
    }
    args.extend(positional);
    Ok(args)
}

fn argument_list_call_args<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Vec<Expr>> {
    let mut positional = Vec::new();
    let mut keyword = Vec::new();
    let mut in_kwargs = false;

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Semicolon => in_kwargs = true,
            _ if in_kwargs => keyword.push(cst_to_expr_constructor(walker, child)?),
            _ => positional.push(cst_to_expr_constructor(walker, child)?),
        }
    }

    let mut args = Vec::new();
    if !keyword.is_empty() {
        args.push(expr_constructor("parameters", keyword, walker.span(&node)));
    }
    args.extend(positional);
    Ok(args)
}

fn function_signature_constructor<'a>(
    walker: &CstWalker<'a>,
    mut parts: Vec<Node<'a>>,
    span: Span,
) -> LowerResult<Expr> {
    if parts.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "quote of function: missing signature".to_string(),
            ),
            span,
        ));
    }

    let first = parts.remove(0);
    let mut signature = if walker.kind(&first) == NodeKind::ParameterList {
        expr_constructor(
            "tuple",
            parameter_list_signature_args(walker, first, walker.span(&first))?,
            span,
        )
    } else if walker.kind(&first) == NodeKind::ParenthesizedExpression
        && matches!(
            walker
                .named_children_vec(&first)
                .first()
                .map(|inner| walker.kind(inner)),
            Some(NodeKind::TypedExpression | NodeKind::TypedParameter | NodeKind::SplatParameter)
        )
        && walker.named_children_vec(&first).len() == 1
    {
        // Anonymous function with a single typed/splat parameter,
        // `function (n::Int) ... end`: the parser hands the signature over as
        // a `ParenthesizedExpression` (not a `ParameterList`), but upstream
        // quotes it as the one-element parameter tuple
        // `Expr(:tuple, Expr(:(::), :n, :Int))`. Wrap it accordingly so a
        // macro-returned anonymous function keeps the upstream Expr shape
        // (Issue #10926). Operator-head signatures such as
        // `function (a | b) ... end` stay on the fallthrough path below (their
        // parenthesized child is a `BinaryExpression`, matching upstream's
        // `Expr(:call, :|, :a, :b)` signature).
        let children = walker.named_children_vec(&first);
        expr_constructor(
            "tuple",
            vec![cst_to_expr_constructor(walker, children[0])?],
            span,
        )
    } else if matches!(parts.first(), Some(next) if walker.kind(next) == NodeKind::ParameterList) {
        let params = parts.remove(0);
        let callee = cst_to_expr_constructor(walker, first)?;
        let mut args = vec![callee];
        args.extend(parameter_list_signature_args(
            walker,
            params,
            walker.span(&params),
        )?);
        expr_constructor("call", args, span)
    } else {
        cst_to_expr_constructor(walker, first)?
    };

    let mut where_clause = None;
    let mut return_type = None;
    for part in parts {
        if walker.kind(&part) == NodeKind::Block {
            continue;
        }
        if walker.kind(&part) == NodeKind::WhereClause {
            where_clause = Some(part);
            continue;
        }
        return_type = Some(part);
    }

    if let Some(part) = return_type {
        signature = expr_constructor(
            "::",
            vec![signature, cst_to_expr_constructor(walker, part)?],
            span,
        );
    }
    if let Some(part) = where_clause {
        let mut args = vec![signature];
        args.extend(where_clause_args(walker, part)?);
        signature = expr_constructor("where", args, span);
    }

    Ok(signature)
}

fn interpolated_field_constructor<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: Span,
) -> LowerResult<Expr> {
    let children = walker.named_children_vec(&node);
    if children.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "interpolated field expression: missing operands".to_string(),
            ),
            span,
        ));
    }

    let object = match walker.kind(&children[0]) {
        NodeKind::Identifier => Expr::Var(
            walker.text(&children[0]).to_string().into(),
            walker.span(&children[0]),
        ),
        _ => super::super::lower_expr(walker, children[0])?,
    };
    let field = match walker.kind(&children[1]) {
        NodeKind::UnaryExpression => Expr::Builtin {
            name: BuiltinOp::QuoteNodeNew,
            args: vec![cst_to_expr_constructor(walker, children[1])?],
            span,
        },
        NodeKind::Identifier => Expr::Builtin {
            name: BuiltinOp::QuoteNodeNew,
            args: vec![symbol_constructor(walker.text(&children[1]), span)],
            span,
        },
        _ => cst_to_expr_constructor(walker, children[1])?,
    };

    Ok(expr_constructor(".", vec![object, field], span))
}

/// Build the interpolation constructor for the operand of a `$…` in quote
/// construction — the value-splicing form for `$a`, `$a.b`, `$f(args)`,
/// `$(expr)`, `$(esc(x))`. Factored out of the `$` `UnaryExpression` arm so the
/// `$a::T` re-association (Issue #9176) can interpolate just the left operand
/// while reusing the exact same per-shape handling.
fn dollar_inner_constructor<'a>(
    walker: &CstWalker<'a>,
    inner: Node<'a>,
    span: Span,
) -> LowerResult<Expr> {
    let inner_kind = walker.kind(&inner);

    if inner_kind == NodeKind::FieldExpression {
        return interpolated_field_constructor(walker, inner, span);
    }

    if inner_kind == NodeKind::CallExpression {
        let call_children = walker.named_children_vec(&inner);
        if let Some(callee) = call_children.first() {
            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                span,
            }];
            if walker.kind(callee) == NodeKind::Identifier {
                args.push(Expr::Var(
                    walker.text(callee).to_string().into(),
                    walker.span(callee),
                ));
            } else if walker.kind(callee) == NodeKind::ParenthesizedExpression {
                // `$(esc(:f))(args)` / `$(expr)(args)`: the parser binds `$`
                // greedily, so the interpolated callee landed here as a
                // parenthesized group. It is the `$(...)` payload — interpolate it
                // (handling `esc`), do not quote it literally (Issue #8066).
                match paren_dollar_payload(walker, *callee, span)? {
                    Some(payload) => args.push(payload),
                    None => args.push(cst_to_expr_constructor(walker, *callee)?),
                }
            } else {
                args.push(cst_to_expr_constructor(walker, *callee)?);
            }
            if let Some(arglist) = call_children
                .iter()
                .find(|child| walker.kind(child) == NodeKind::ArgumentList)
            {
                args.extend(argument_list_call_args(walker, *arglist)?);
            }
            return Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            });
        }
    }

    // Handle `$(esc(expr))` (escape marker preserved), `$(p...)` (splat) and the
    // normal `$(expr)` interpolation via the shared helper (also reused for
    // interpolated short-form function names, Issue #8066).
    if inner_kind == NodeKind::ParenthesizedExpression {
        if let Some(payload) = paren_dollar_payload(walker, inner, span)? {
            return Ok(payload);
        }
    }

    if inner_kind == NodeKind::Identifier {
        Ok(Expr::Var(walker.text(&inner).to_string().into(), span))
    } else {
        super::super::lower_expr(walker, inner)
    }
}

/// Build the Expr-constructor for the inner expression of a `$…` interpolation
/// chunk (text like `$t` or `$(a + b)`), so a quoted interpolated string round-trips
/// (Issue #7029). Re-parses the inner expression to CST and converts it the same
/// way every other quoted sub-expression is converted.
fn interpolation_constructor(text: &str, span: Span) -> LowerResult<Option<Expr>> {
    let inner = text.strip_prefix('$').unwrap_or(text);
    let expr_text = inner
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .map(str::trim)
        .unwrap_or_else(|| inner.trim());
    parse_text_expr_constructor(expr_text, span, "interpolation")
}

/// Build the constructor for the payload of a `$(...)` interpolation whose
/// parenthesized operand is `inner` (a `ParenthesizedExpression` node). Handles
/// `$(esc(x))` (the escape marker is preserved via `BuiltinOp::Esc` for
/// expansion-time hygiene — Issue #7631), `$(p...)` (splat interpolation), and
/// the normal `$(expr)` case (evaluate `expr` in the macro body). Returns
/// `Ok(None)` only when the parenthesized group has no inner expression.
///
/// Shared by the standalone `$(...)` interpolation arm and the
/// interpolated-callee fix-up for `$(...)(args)` short-form function names: the
/// parser binds `$` greedily so `$(esc(:f))(x)` parses as a call whose callee is
/// the parenthesized `(esc(:f))`, which must be interpolated, not quoted
/// literally (Issue #8066).
fn paren_dollar_payload<'a>(
    walker: &CstWalker<'a>,
    inner: Node<'a>,
    span: Span,
) -> LowerResult<Option<Expr>> {
    let paren_children = walker.named_children_vec(&inner);
    let Some(&paren_inner) = paren_children.first() else {
        return Ok(None);
    };
    if walker.kind(&paren_inner) == NodeKind::CallExpression {
        let call_children = walker.named_children_vec(&paren_inner);
        if !call_children.is_empty() {
            let func_name = walker.text(&call_children[0]);
            if func_name == "esc" && call_children.len() >= 2 {
                let esc_arg = &call_children[1];
                // Handle ArgumentList if present.
                let actual_arg = if walker.kind(esc_arg) == NodeKind::ArgumentList {
                    let arg_children = walker.named_children_vec(esc_arg);
                    if arg_children.is_empty() {
                        return Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(
                                "empty esc() call".to_string(),
                            ),
                            span,
                        ));
                    }
                    arg_children[0]
                } else {
                    *esc_arg
                };
                // A bare identifier remains a macro parameter reference, while
                // complex expressions such as `esc(Expr(:quote, pat))` must be
                // evaluated in the macro body (Issue #7536). In both cases keep
                // `BuiltinOp::Esc` so the VM returns `Expr(:escape, value)` for
                // the later macro-runtime lowering pass (Issue #7631).
                let escaped = if walker.kind(&actual_arg) == NodeKind::Identifier {
                    Expr::Var(walker.text(&actual_arg).to_string().into(), span)
                } else {
                    super::super::lower_expr(walker, actual_arg)?
                };
                return Ok(Some(Expr::Builtin {
                    name: BuiltinOp::Esc,
                    args: vec![escaped],
                    span,
                }));
            }
        }
    }
    // Splat interpolation: `$(p...)`. The splatted value can be a full
    // expression, not only a bare parameter name (Issue #7536).
    if walker.kind(&paren_inner) == NodeKind::SplatExpression {
        let splat_children = walker.named_children_vec(&paren_inner);
        if let Some(&splat_inner) = splat_children.first() {
            return Ok(Some(Expr::Builtin {
                name: BuiltinOp::SplatInterpolation,
                args: vec![super::super::lower_expr(walker, splat_inner)?],
                span,
            }));
        }
    }
    Ok(Some(super::super::lower_expr(walker, paren_inner)?))
}

fn parse_text_expr_constructor(
    expr_text: &str,
    span: Span,
    context: &str,
) -> LowerResult<Option<Expr>> {
    if expr_text.is_empty() {
        return Ok(None);
    }
    let parser = subset_julia_vm_parser::Parser::new(expr_text);
    let (cst, errors) = parser.parse();
    if !errors.is_empty() {
        let msg = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "{} parse error in quote: {}",
                context, msg
            )),
            span,
        ));
    }
    let walker = CstWalker::new(expr_text);
    match cst.children.first() {
        Some(first) => {
            let node = Node::new(first, expr_text);
            Ok(Some(cst_to_expr_constructor(&walker, node)?))
        }
        None => Ok(None),
    }
}

/// Lower a quote expression: :symbol or :(expr)
/// Converts to a QuoteLiteral that constructs the quoted value at runtime.
pub fn lower_quote_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    // Issue #4923: when the QuoteExpression has children (e.g. `:42` or
    // `:(expr)`), it's a quote of a non-bare-Symbol form — recurse into
    // the child to produce the right constructor. Only the LEAF
    // QuoteExpression (no children) should follow the
    // `text -> Symbol(name)` shortcut.
    if children.is_empty() {
        let text = walker.text(&node);
        if text.starts_with(':') {
            // Leaf form `:foo` / `:.` / etc. — text minus the leading
            // colon is the Symbol name.
            let symbol_name = text.trim_start_matches(':');
            let constructor = Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str(symbol_name.to_string()), span)],
                span,
            };
            return Ok(Expr::QuoteLiteral {
                constructor: Box::new(constructor),
                span,
            });
        }
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("empty quote expression".to_string()),
            span,
        ));
    }

    // With children: `:(expr)` or the new `:literal` form (Issue #4923).
    // Recurse into the inner child to build the constructor.
    //
    // For the literal-child cases (`:42`, `:3.14`, `:"hi"`, `:'A'`,
    // `:true`), upstream Julia evaluates the colon-prefix to the
    // *literal value itself* — `typeof(:42) === Int64`, not
    // `QuoteNode`. The colon is essentially a no-op on a self-quoting
    // literal at top level. We emit the literal directly as the
    // QuoteLiteral constructor; the runtime then evaluates to the
    // literal value.
    //
    // (The nested-in-outer-quote case — `:(:42)` — is handled in
    // the `cst_to_expr_constructor`'s `QuoteExpression` arm
    // (Issue #4911/#4920), which wraps in `QuoteNode` because the
    // outer quote treats the inner as a value to embed.)
    let inner_node = children[0];
    let inner = cst_to_expr_constructor(walker, inner_node)?;

    Ok(Expr::QuoteLiteral {
        constructor: Box::new(inner),
        span,
    })
}

/// Convert a CST node to an IR Expr that constructs the corresponding Expr/Symbol value at runtime.
/// This is used for quote expressions like :(1 + 2) which becomes Expr(:call, :+, 1, 2)
pub fn cst_to_expr_constructor<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);

    match walker.kind(&node) {
        // Literals become themselves (not quoted).
        //
        // Issue #4927: hex / binary / octal literals (`0xFF`, `0b1010`,
        // `0o17`) must preserve their natural unsigned-int type
        // (UInt8 / UInt16 / …) — previously the quote-arm called
        // `parse_int` (untyped), losing the width tag and emitting an
        // `Int64`. Route through `super::super::lower_integer_literal`,
        // the same helper the bare-literal lowering uses; it returns
        // an `Expr::Call("UInt8", [Expr::Literal(Int(0xFF))])` for
        // typed cases and a bare `Expr::Literal(Int(_))` for decimals.
        NodeKind::IntegerLiteral => super::super::lower_integer_literal(walker, node, span),
        NodeKind::FloatLiteral => {
            let text = walker.text(&node);
            let value = parse_float(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match value {
                super::super::literal::ParsedFloat::F64(v) => {
                    Ok(Expr::Literal(Literal::Float(v), span))
                }
                super::super::literal::ParsedFloat::F32(v) => {
                    Ok(Expr::Literal(Literal::Float32(v), span))
                }
            }
        }
        NodeKind::StringLiteral => {
            // Detect interpolation (`"t=$t"`): build an `Expr(:string, parts...)`
            // value so the quoted string round-trips to a real interpolation when
            // re-lowered (Issue #7029), matching `lower_string_literal`. Without
            // interpolation, keep the simple literal shape.
            let child_count = node.child_count();
            let mut interp_children = Vec::new();
            for i in 0..child_count {
                if let Some(child) = node.child(i) {
                    let kind = child.kind();
                    if kind == "content"
                        || kind == "string_interpolation"
                        || kind == "interpolation_expression"
                    {
                        interp_children.push((kind.to_string(), child));
                    }
                }
            }
            let has_interp = interp_children.iter().any(|(k, _)| k != "content");
            if !has_interp {
                let text = walker.text(&node);
                let content = process_escape_sequences(text.trim_matches('"'));
                return Ok(Expr::Literal(Literal::Str(content), span));
            }

            // head: :string
            let mut parts = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("string".to_string()), span)],
                span,
            }];
            for (kind, child) in interp_children {
                if kind == "content" {
                    let processed = process_escape_sequences(walker.text(&child));
                    if !processed.is_empty() {
                        parts.push(Expr::Literal(Literal::Str(processed), span));
                    }
                } else {
                    // `string_interpolation` / `interpolation_expression`: tree-sitter
                    // nests the expression as named children; the pure-Rust parser
                    // stores it as leaf text (`$t` / `$(expr)`).
                    let named = walker.named_children_vec(&child);
                    if let Some(expr_node) = named.first() {
                        parts.push(cst_to_expr_constructor(walker, *expr_node)?);
                    } else if let Some(c) = interpolation_constructor(walker.text(&child), span)? {
                        parts.push(c);
                    }
                }
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: parts,
                span,
            })
        }
        NodeKind::BooleanLiteral => {
            let text = walker.text(&node);
            let value = text == "true";
            Ok(Expr::Literal(Literal::Bool(value), span))
        }

        // Issue #4923: `:'A'` (a quoted Char literal) needs a
        // `NodeKind::CharacterLiteral` arm that produces the
        // corresponding `Literal::Char` so the `:literal` colon-prefix
        // sugar (handled at parser side) lowers end-to-end.
        NodeKind::CharacterLiteral => {
            let text = walker.text(&node);
            // Strip the surrounding `'` characters and parse the inner.
            let inner = text.trim_matches('\'');
            let value = if let Some(rest) = inner.strip_prefix('\\') {
                match rest {
                    "n" => '\n',
                    "t" => '\t',
                    "r" => '\r',
                    "0" => '\0',
                    "\\" => '\\',
                    "'" => '\'',
                    "\"" => '"',
                    _ => inner.chars().next().unwrap_or('\0'),
                }
            } else {
                inner.chars().next().ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "empty character literal in quote".to_string(),
                        ),
                        span,
                    )
                })?
            };
            Ok(Expr::Literal(Literal::Char(value), span))
        }

        // Identifiers become Symbols: x -> :x
        NodeKind::Identifier => {
            let name = walker.text(&node);
            // Special cases for literals.
            //
            // Issue #4895: upstream Julia treats `true` / `false` as
            // literal Bool nodes in the AST, so `:(true)` quotes back
            // to the `true` Bool value — keep those two arms. In
            // contrast `nothing` / `missing` are ordinary identifiers:
            // `:(nothing)` is the Symbol `:nothing` and `:(missing)`
            // the Symbol `:missing`, only becoming the actual values
            // when the quoted Expr is later evaluated. They therefore
            // fall through to the regular `Symbol(name)` path below.
            match name {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                _ => Ok(Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
                    span,
                }),
            }
        }

        // Issue #4872: a bare operator inside a quoted expression — e.g.
        // `:(%)`, `:(+)`, `:(*)`, or an operator used as a value like
        // `:(foo(%, x))` — is rejected at the catch-all below with
        // `UnsupportedExpression("quote for operator not yet supported")`.
        // Upstream Julia treats a quoted operator as a `Symbol`,
        // identical to a quoted identifier. Mirror the `NodeKind::Identifier`
        // arm above so the operator's text becomes `Symbol(text)`.
        NodeKind::Operator => {
            let text = walker.text(&node);
            Ok(Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str(text.to_string()), span)],
                span,
            })
        }

        // Binary expressions become Expr(:call, :op, left, right)
        NodeKind::BinaryExpression => {
            let children = walker.named_children_vec(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "malformed binary expression".to_string(),
                    ),
                    span,
                ));
            }

            // Find operator - it's usually the middle element or has Operator kind
            let mut left_node = None;
            let mut op_text = None;
            let mut right_node = None;

            for child in &children {
                if walker.kind(child) == NodeKind::Operator {
                    op_text = Some(walker.text(child).to_string());
                } else if left_node.is_none() {
                    left_node = Some(*child);
                } else {
                    right_node = Some(*child);
                }
            }

            // If no Operator node found, try getting operator from text between children
            let op = op_text.unwrap_or_else(|| {
                // Fallback: extract operator from the middle
                if children.len() >= 2 {
                    let left_end = walker.span(&children[0]).end;
                    let right_start = walker.span(&children[children.len() - 1]).start;
                    let full_text = walker.text(&node);
                    let left_len = left_end - walker.span(&node).start;
                    let right_offset = right_start - walker.span(&node).start;
                    if right_offset > left_len
                        && right_offset <= full_text.len()
                        && left_len <= full_text.len()
                    {
                        full_text[left_len..right_offset].trim().to_string()
                    } else {
                        "+".to_string() // fallback
                    }
                } else {
                    "+".to_string()
                }
            });

            let left = left_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "missing left operand".to_string(),
                    ),
                    span,
                )
            })?;
            let right = right_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "missing right operand".to_string(),
                    ),
                    span,
                )
            })?;

            let left_constructor = cst_to_expr_constructor(walker, left)?;
            let right_constructor = cst_to_expr_constructor(walker, right)?;

            // Subtype/supertype constraint (`S<:Real`, `S>:Int`): upstream Julia
            // represents these with the operator as the *head* — `Expr(:<:, :S,
            // :Real)` / `Expr(:>:, :S, :Int)` — not as an `Expr(:call, :<:, …)`.
            // The bounded `where`/parameter paths already produce this shape via
            // `subtype_constraint_constructor`, but a *standalone* quoted
            // constraint reaches this generic BinaryExpression arm, so mirror that
            // head-as-operator shape here too (Issue #7863).
            if op == "<:" || op == ">:" {
                return Ok(expr_constructor(
                    &op,
                    vec![left_constructor, right_constructor],
                    span,
                ));
            }

            // Broadcast binary (`.+`, `.-`, …): emit the materialize/Broadcasted
            // form so the quoted value re-lowers to a real broadcast (Issue #7029),
            // matching `make_broadcasted_call`. Plain operators keep the simple
            // `Expr(:call, :op, left, right)` shape below.
            if is_broadcast_op(&op) {
                return Ok(make_broadcast_constructor(
                    strip_broadcast_dot(&op),
                    vec![left_constructor, right_constructor],
                    span,
                ));
            }

            // Expr(:call, :op, left, right)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :call
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                        span,
                    },
                    // operator as symbol
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(op), span)],
                        span,
                    },
                    left_constructor,
                    right_constructor,
                ],
                span,
            })
        }

        // Call expressions become Expr(:call, callee, args...)
        //
        // Issue #4901: the callee must be lowered by recursing through
        // `cst_to_expr_constructor` rather than extracting its raw text
        // and wrapping in `Symbol(text)`. Recursion lets a dotted callee
        // like `Base.foo` route through the `NodeKind::FieldExpression`
        // arm (added in PR #4903 for #4899), which produces the proper
        // `Expr(:., :Base, QuoteNode(:foo))` shape — matching upstream
        // Julia. Plain-identifier and operator callees continue to emit
        // `Symbol(text)` because their own arms already do that, so the
        // simple `:(f(x))` case is unchanged.
        NodeKind::CallExpression => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty call expression".to_string(),
                    ),
                    span,
                ));
            }

            let callee = cst_to_expr_constructor(walker, children[0])?;

            let mut args = vec![
                // head: :call
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                    span,
                },
                callee,
            ];

            // Remaining children are arguments. A `;` separating positional and
            // keyword args (`f(a; b=v)`) is a structural marker, not an argument —
            // skip it; the `KeywordArgument` children carry the kwargs (Issue #7029).
            for child in children.iter().skip(1) {
                // Check for ArgumentList
                if walker.kind(child) == NodeKind::ArgumentList {
                    args.extend(argument_list_call_args(walker, *child)?);
                } else if walker.kind(child) != NodeKind::Semicolon {
                    args.push(cst_to_expr_constructor(walker, *child)?);
                }
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Broadcast calls `f.(args...)` (Issue #7029): emit the materialize/
        // Broadcasted form so the quoted value re-lowers to a real broadcast,
        // matching `lower_broadcast_call_expr`. Children are `[callee, args...]`
        // (the pure-Rust parser stores args as direct children, no ArgumentList).
        NodeKind::BroadcastCallExpression => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty broadcast call expression".to_string(),
                    ),
                    span,
                ));
            }

            // The callee is an identifier (`sin`) or a dotted operator (`.+`);
            // use its base name (strip a leading dot from operators).
            let callee = children[0];
            let fn_name = match walker.kind(&callee) {
                NodeKind::Operator => strip_broadcast_dot(walker.text(&callee)).to_string(),
                _ => walker.text(&callee).to_string(),
            };

            let mut arg_constructors = Vec::new();
            for child in children.iter().skip(1) {
                if walker.kind(child) == NodeKind::ArgumentList
                    || walker.kind(child) == NodeKind::TupleExpression
                {
                    for arg in walker.named_children(child) {
                        arg_constructors.push(cst_to_expr_constructor(walker, arg)?);
                    }
                } else {
                    arg_constructors.push(cst_to_expr_constructor(walker, *child)?);
                }
            }

            Ok(make_broadcast_constructor(&fn_name, arg_constructors, span))
        }

        // Keyword argument `name = value` inside a call (Issue #7029): becomes
        // `Expr(:kw, :name, value)`, matching upstream Julia. `call_expr_from_values`
        // pulls these `:kw` args back out as keyword arguments when the quoted call
        // is re-lowered (e.g. `@gif`/`@animate` over `plot(...; title=...)`).
        NodeKind::KeywordArgument => {
            let children: Vec<_> = walker
                .named_children(&node)
                .filter(|n| walker.kind(n) != NodeKind::Operator)
                .collect();
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "malformed keyword argument".to_string(),
                    ),
                    span,
                ));
            }
            let name = walker.text(&children[0]).to_string();
            let value_constructor = cst_to_expr_constructor(walker, children[1])?;
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("kw".to_string()), span)],
                        span,
                    },
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(name), span)],
                        span,
                    },
                    value_constructor,
                ],
                span,
            })
        }

        NodeKind::WhereExpression => {
            let children = walker.named_children_vec(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of where expression: missing parameters".to_string(),
                    ),
                    span,
                ));
            }

            let mut args = vec![cst_to_expr_constructor(walker, children[0])?];
            for child in children.iter().skip(1) {
                args.extend(where_clause_args(walker, *child)?);
            }
            Ok(expr_constructor("where", args, span))
        }

        NodeKind::AdjointExpression => {
            let children = walker.named_children_vec(&node);
            let Some(inner) = children.first() else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of adjoint expression: missing operand".to_string(),
                    ),
                    span,
                ));
            };
            Ok(expr_constructor(
                "'",
                vec![cst_to_expr_constructor(walker, *inner)?],
                span,
            ))
        }

        NodeKind::TypedExpression | NodeKind::UnaryTypedExpression => {
            let args = walker
                .named_children(&node)
                .map(|child| cst_to_expr_constructor(walker, child))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(expr_constructor("::", args, span))
        }

        // Parenthesized expressions: unwrap the inner expression
        NodeKind::ParenthesizedExpression => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty parenthesized expression".to_string(),
                    ),
                    span,
                ));
            }
            cst_to_expr_constructor(walker, children[0])
        }

        // `begin ... end` is represented as a begin_block containing the actual
        // Block. Quote construction should use that inner block directly; the
        // outer begin_block is syntactic delimiters, not another Expr(:block).
        _ if node.kind() == "begin_block" => {
            let children = walker.named_children_vec(&node);
            let block = children
                .into_iter()
                .find(|child| walker.kind(child) == NodeKind::Block)
                .ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "quote of begin block: missing body".to_string(),
                        ),
                        span,
                    )
                })?;
            cst_to_expr_constructor(walker, block)
        }

        // Block: quote ... end -> Expr(:block, LineNumberNode, stmt1, LineNumberNode, stmt2, ...)
        // Julia inserts LineNumberNode before each statement in quote blocks
        NodeKind::Block | NodeKind::CompoundStatement => {
            let children = walker.named_children_vec(&node);
            // Skip comments and get actual statements
            let stmts: Vec<_> = children
                .into_iter()
                .filter(|c| {
                    let k = walker.kind(c);
                    k != NodeKind::LineComment
                        && k != NodeKind::BlockComment
                        && k != NodeKind::Semicolon
                })
                .collect();

            if stmts.is_empty() {
                // Empty block returns nothing
                Ok(Expr::Literal(Literal::Nothing, span))
            } else {
                // Create Expr(:block, LineNumberNode, stmt1, LineNumberNode, stmt2, ...)
                let mut args = vec![Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                    span,
                }];
                let mut i = 0;
                while i < stmts.len() {
                    let stmt = stmts[i];
                    // Insert LineNumberNode before each statement
                    let stmt_span = walker.span(&stmt);
                    args.push(line_number_constructor(stmt_span.start_line, stmt_span));
                    if walker.kind(&stmt) == NodeKind::StringLiteral && i + 1 < stmts.len() {
                        let next_stmt = stmts[i + 1];
                        let next_span = walker.span(&next_stmt);
                        if next_span.start_line > stmt_span.start_line {
                            args.push(doc_macrocall_constructor(walker, stmt, next_stmt)?);
                            i += 2;
                            continue;
                        }
                    }
                    args.push(cst_to_expr_constructor(walker, stmt)?);
                    i += 1;
                }
                Ok(Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args,
                    span,
                })
            }
        }

        // Function definition: function f(args...) body end
        // Upstream quoted functions are Expr(:function, signature, body), where
        // signature is a call expression (`:(f(x))`), a tuple for anonymous
        // functions (`:((x,))`), or the parsed head expression for operator
        // heads such as `function (a | b) ... end`.
        NodeKind::FunctionDefinition => {
            let children = walker.named_children_vec(&node);
            let body_node = children
                .iter()
                .find(|child| walker.kind(child) == NodeKind::Block)
                .copied()
                .ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "quote of function: missing body".to_string(),
                        ),
                        span,
                    )
                })?;
            let signature_parts: Vec<_> = children
                .into_iter()
                .take_while(|child| walker.kind(child) != NodeKind::Block)
                .collect();
            let signature = function_signature_constructor(walker, signature_parts, span)?;
            let body = cst_to_expr_constructor(walker, body_node)?;

            Ok(expr_constructor("function", vec![signature, body], span))
        }

        // Macro definition inside quote: macro m(args...) body end
        // Upstream quoted macros are Expr(:macro, signature, body), matching
        // the function quote shape but with a :macro head (Issue #9134).
        NodeKind::MacroDefinition => {
            let children = walker.named_children_vec(&node);
            let body_node = children
                .iter()
                .find(|child| walker.kind(child) == NodeKind::Block)
                .copied()
                .ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "quote of macro: missing body".to_string(),
                        ),
                        span,
                    )
                })?;
            let signature_parts: Vec<_> = children
                .into_iter()
                .take_while(|child| walker.kind(child) != NodeKind::Block)
                .collect();
            let signature = function_signature_constructor(walker, signature_parts, span)?;
            let body = cst_to_expr_constructor(walker, body_node)?;

            Ok(expr_constructor("macro", vec![signature, body], span))
        }

        NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
            struct_definition_constructor(
                walker,
                node,
                walker.kind(&node) == NodeKind::MutableStructDefinition,
                span,
            )
        }

        // Assignment: x = expr -> Expr(:(=), target_constructor, expr_constructor)
        //
        // Issue #4993: when the LHS is a dotted name (`x.y = z`), recurse so a
        // `FieldExpression` produces the upstream-shaped `Expr(:., ...)` target.
        // Issue #6616: interpolated assignment targets (`$tmp = expr`) must also
        // recurse so normal quote interpolation evaluates the local binding,
        // matching upstream macro execution. Plain identifiers stay text-based:
        // `:(x = 1)` stores the symbol `:x`.
        NodeKind::Assignment => {
            let children = walker.named_children_vec(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "assignment with insufficient children".to_string(),
                    ),
                    span,
                ));
            }

            let target = children[0];
            let value = children[children.len() - 1];

            // Only recurse when the LHS is a complex expression (e.g.
            // a dotted field-access — Issue #4993; an indexed target like
            // `a[i] = v` — Issue #7350 A2, which otherwise quoted the LHS as a
            // single malformed `Symbol("a[i]")` instead of `Expr(:ref, :a, :i)`);
            // or a typed binding like `x::T = v`, whose LHS is `Expr(:(::), :x, :T)`
            // upstream (Issue #7622).
            // Issue #7641: vector destructuring targets also need recursion so
            // `:([a, b] = rhs)` preserves `Expr(:vect, :a, :b)` instead of
            // collapsing the LHS into `Symbol("[a, b]")`.
            // Issue #7647: where-qualified short-form function targets such as
            // `:(f(x::T) where T = x)` must preserve `Expr(:where, ...)`
            // instead of becoming a flat Symbol.
            // Issue #7535: parser recovery can hand quote lowering a flat-text
            // Identifier for `f_(args__) = body_`; reparse call-shaped targets so
            // MacroTools patterns see `Expr(:call, :f_, :args__)`, matching Julia.
            // For Identifier / UnaryExpression / etc., keep the original
            // flat-text Symbol-wrap path.
            let target_constructor = if walker.kind(&target) == NodeKind::FieldExpression
                || walker.kind(&target) == NodeKind::IndexExpression
                || walker.kind(&target) == NodeKind::TypedExpression
                || walker.kind(&target) == NodeKind::WhereExpression
                || walker.kind(&target) == NodeKind::VectorExpression
                || walker.kind(&target) == NodeKind::CallExpression
                || (walker.kind(&target) == NodeKind::UnaryExpression
                    && walker.text(&target).starts_with('$'))
            {
                cst_to_expr_constructor(walker, target)?
            } else {
                let target_name = walker.text(&target);
                if target_name.contains('(') && target_name.ends_with(')') {
                    parse_text_expr_constructor(
                        target_name,
                        walker.span(&target),
                        "assignment target",
                    )?
                    .unwrap_or_else(|| Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(target_name.to_string()), span)],
                        span,
                    })
                } else {
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(target_name.to_string()), span)],
                        span,
                    }
                }
            };
            let value_constructor = cst_to_expr_constructor(walker, value)?;

            // Create Expr(:(=), target, value)
            let args = vec![
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("=".to_string()), span)],
                    span,
                },
                target_constructor,
                value_constructor,
            ];
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Const declaration: const x = expr -> Expr(:const, Expr(:(=), :x, expr))
        NodeKind::ConstStatement => {
            let children = walker.named_children_vec(&node);
            for child in &children {
                match walker.kind(child) {
                    NodeKind::Assignment | NodeKind::BinaryExpression => {
                        let assignment = if walker.kind(child) == NodeKind::Assignment {
                            cst_to_expr_constructor(walker, *child)?
                        } else {
                            let expr_children = walker.named_children_vec(child);
                            if expr_children.len() < 2 {
                                return Err(UnsupportedFeature::new(
                                    UnsupportedFeatureKind::UnsupportedExpression(
                                        "quote of const: assignment missing operands".to_string(),
                                    ),
                                    span,
                                ));
                            }
                            let target = expr_children[0];
                            let value = expr_children[expr_children.len() - 1];
                            let target_constructor = match walker.kind(&target) {
                                NodeKind::FieldExpression | NodeKind::UnaryExpression => {
                                    cst_to_expr_constructor(walker, target)?
                                }
                                _ => Expr::Builtin {
                                    name: BuiltinOp::SymbolNew,
                                    args: vec![Expr::Literal(
                                        Literal::Str(walker.text(&target).to_string()),
                                        span,
                                    )],
                                    span,
                                },
                            };
                            let value_constructor = cst_to_expr_constructor(walker, value)?;
                            Expr::Builtin {
                                name: BuiltinOp::ExprNew,
                                args: vec![
                                    Expr::Builtin {
                                        name: BuiltinOp::SymbolNew,
                                        args: vec![Expr::Literal(
                                            Literal::Str("=".to_string()),
                                            span,
                                        )],
                                        span,
                                    },
                                    target_constructor,
                                    value_constructor,
                                ],
                                span,
                            }
                        };
                        return Ok(Expr::Builtin {
                            name: BuiltinOp::ExprNew,
                            args: vec![
                                Expr::Builtin {
                                    name: BuiltinOp::SymbolNew,
                                    args: vec![Expr::Literal(
                                        Literal::Str("const".to_string()),
                                        span,
                                    )],
                                    span,
                                },
                                assignment,
                            ],
                            span,
                        });
                    }
                    _ => {}
                }
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("const".to_string()), span)],
                    span,
                }],
                span,
            })
        }

        // Export statement: `export a, $b` -> Expr(:export, :a, b).
        //
        // Upstream macros such as AbstractAlgebra's @alias quote export
        // declarations and interpolate the exported alias name. Preserve the
        // statement shape as an Expr so macro-return lowering can rebuild
        // Stmt::Export (Issue #7908).
        NodeKind::ExportStatement => {
            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("export".to_string()), span)],
                span,
            }];
            for child in walker.named_children(&node) {
                match walker.kind(&child) {
                    NodeKind::Identifier => {
                        args.push(Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(
                                Literal::Str(walker.text(&child).to_string()),
                                span,
                            )],
                            span,
                        });
                    }
                    NodeKind::UnaryExpression if walker.text(&child).starts_with('$') => {
                        args.push(cst_to_expr_constructor(walker, child)?);
                    }
                    _ => {}
                }
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Global declaration: global x -> Expr(:global, :x)
        // Global assignment: global x = expr -> Expr(:global, Expr(:(=), :x, expr))
        NodeKind::GlobalStatement => {
            let children = walker.named_children_vec(&node);
            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("global".to_string()), span)],
                span,
            }];
            for child in &children {
                match walker.kind(child) {
                    NodeKind::Assignment | NodeKind::BinaryExpression => {
                        args.push(cst_to_expr_constructor(walker, *child)?);
                    }
                    NodeKind::Identifier => {
                        args.push(Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(
                                Literal::Str(walker.text(child).to_string()),
                                span,
                            )],
                            span,
                        });
                    }
                    _ => {}
                }
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Local declaration: local x = expr -> Expr(:local, Expr(:(=), :x, expr))
        NodeKind::LocalStatement | NodeKind::LocalDeclaration => {
            let children = walker.named_children_vec(&node);
            let mut inner_args = Vec::new();
            for child in &children {
                let child_kind = walker.kind(child);
                match child_kind {
                    NodeKind::Assignment | NodeKind::BinaryExpression => {
                        inner_args.push(cst_to_expr_constructor(walker, *child)?);
                    }
                    NodeKind::Identifier => {
                        let name = walker.text(child);
                        inner_args.push(Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
                            span,
                        });
                    }
                    _ => {}
                }
            }
            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("local".to_string()), span)],
                span,
            }];
            args.extend(inner_args);
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        NodeKind::LetBindings => {
            let children = walker.named_children_vec(&node);
            if children.len() == 1 {
                cst_to_expr_constructor(walker, children[0])
            } else {
                let mut args = vec![Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                    span,
                }];
                for child in children {
                    args.push(cst_to_expr_constructor(walker, child)?);
                }
                Ok(Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args,
                    span,
                })
            }
        }

        NodeKind::LetExpression | NodeKind::LetStatement => {
            let children = walker.named_children_vec(&node);
            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("let".to_string()), span)],
                span,
            }];
            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Return statement: `return x` -> Expr(:return, x)
        //
        // MacroTools' matcher helpers quote `return ...` inside macro bodies.
        // Upstream Julia represents that syntax as `Expr(:return, value)`.
        // Preserve that AST shape instead of rejecting the quoted statement
        // during package lowering (Issues #7450/#7437).
        NodeKind::ReturnStatement => {
            let children = walker.named_children_vec(&node);
            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("return".to_string()), span)],
                span,
            }];
            if let Some(value) = children.first() {
                args.push(cst_to_expr_constructor(walker, *value)?);
            }
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Unary expression with $ operator is interpolation - evaluate instead of quoting
        NodeKind::UnaryExpression => {
            let children = walker.named_children_vec(&node);
            let text = walker.text(&node);

            // Check if this is $ interpolation
            if text.starts_with('$') && !children.is_empty() {
                // $ interpolation: $x or $(expr) -> evaluate and use as value
                let inner = &children[children.len() - 1];
                let inner_kind = walker.kind(inner);

                if preserve_macro_dollar() {
                    if inner_kind == NodeKind::CallExpression {
                        let call_children = walker.named_children_vec(inner);
                        if let Some(callee_node) = call_children.first().copied() {
                            let callee = expr_constructor(
                                "$",
                                vec![cst_to_expr_constructor(walker, callee_node)?],
                                walker.span(&callee_node),
                            );
                            return call_constructor_with_callee(walker, *inner, callee, span);
                        }
                    }
                    let payload = if inner_kind == NodeKind::ParenthesizedExpression {
                        let paren_children = walker.named_children_vec(inner);
                        if let Some(paren_inner) = paren_children.first().copied() {
                            cst_to_expr_constructor(walker, paren_inner)?
                        } else {
                            cst_to_expr_constructor(walker, *inner)?
                        }
                    } else {
                        cst_to_expr_constructor(walker, *inner)?
                    };
                    return Ok(expr_constructor("$", vec![payload], span));
                }

                // `$a::T` re-association (Issue #9176). The parser binds `$`
                // looser than `::`, so `$a::T` parses as a `$` over a bare
                // (unparenthesized) `TypedExpression` `a::T`. Julia binds `$`
                // tighter — `$a::T` is `($a)::T` — so interpolate only the left
                // operand and quote the annotation (which still interpolates a
                // `$typ` on the right). Without this the whole `a::T` was lowered
                // as plain code (a real `typeassert`), breaking MacroTools'
                // `combinestructdef` (`:($fieldname::$typ)`) and thus
                // `using MacroTools`. A genuine `$(a::T)` arrives as a
                // `ParenthesizedExpression` inner and is handled below.
                if inner_kind == NodeKind::TypedExpression {
                    let typed_children = walker.named_children_vec(inner);
                    if let Some((first, rest)) = typed_children.split_first() {
                        if !rest.is_empty() {
                            let mut args = Vec::with_capacity(typed_children.len());
                            args.push(dollar_inner_constructor(
                                walker,
                                *first,
                                walker.span(first),
                            )?);
                            for child in rest {
                                args.push(cst_to_expr_constructor(walker, *child)?);
                            }
                            return Ok(expr_constructor("::", args, span));
                        }
                    }
                }

                dollar_inner_constructor(walker, *inner, span)
            } else {
                // Other unary operators: -, !, ~, etc.
                if !children.is_empty() {
                    // Find operator
                    let op_text = text.chars().next().unwrap_or('+').to_string();
                    let operand = cst_to_expr_constructor(walker, children[children.len() - 1])?;

                    // Create Expr(:call, :op, operand)
                    let args = vec![
                        Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                            span,
                        },
                        Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(op_text), span)],
                            span,
                        },
                        operand,
                    ];
                    Ok(Expr::Builtin {
                        name: BuiltinOp::ExprNew,
                        args,
                        span,
                    })
                } else {
                    Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "empty unary expression".to_string(),
                        ),
                        span,
                    ))
                }
            }
        }

        // Macro calls inside quote blocks: @inner(x) -> Expr(:macrocall, Symbol("@inner"), nothing, x)
        NodeKind::MacroCall => {
            // Find the macro identifier
            let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier);
            let macro_name = match macro_ident {
                Some(ident) => walker.text(&ident).to_string(), // Keep the @ prefix
                None => {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "macro call without identifier".to_string(),
                        ),
                        span,
                    ))
                }
            };

            // Get arguments (all children except MacroIdentifier)
            let args: Vec<Node<'a>> = walker
                .named_children(&node)
                .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
                .collect();

            // Build Expr(:macrocall, Symbol("@inner"), nothing, args...)
            let mut expr_args = vec![
                // head: :macrocall
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("macrocall".to_string()), span)],
                    span,
                },
                // macro name as symbol (with @ prefix)
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(macro_name), span)],
                    span,
                },
                // LineNumberNode with line number from span
                Expr::Builtin {
                    name: BuiltinOp::LineNumberNodeNew,
                    args: vec![Expr::Literal(Literal::Int(span.start_line as i64), span)],
                    span,
                },
            ];

            // Add arguments
            for arg in args {
                expr_args.push(cst_to_expr_constructor(walker, arg)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: expr_args,
                span,
            })
        }

        // Tuple expression: (a, b, c) -> Expr(:tuple, a, b, c)
        // Named tuple: (a=1, b=2) -> Expr(:tuple, Expr(:(=), :a, 1), Expr(:(=), :b, 2))
        NodeKind::TupleExpression => {
            let children = walker.named_children_vec(&node);

            let mut args = vec![
                // head: :tuple
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("tuple".to_string()), span)],
                    span,
                },
            ];

            // Add each element
            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Issue #4890: `:([1, 2, 3])` previously fell through to the
        // catch-all with `quote for vector_expression not yet supported`.
        // Mirrors the `TupleExpression` shape above; the head Symbol is
        // `:vect` per upstream Julia (`base/expr.jl`: a `[…]` literal
        // lowers to `Expr(:vect, elements...)`).
        NodeKind::VectorExpression => {
            let children = walker.named_children_vec(&node);

            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("vect".to_string()), span)],
                span,
            }];

            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Matrix literals in macro arguments round-trip through runtime macro
        // expansion as Julia-compatible `:hcat`/`:vcat`/`:row` Expr heads
        // (Issue #7763).
        NodeKind::MatrixExpression => matrix_constructor(walker, node),

        // Comprehension: [body for x in xs] ->
        // Expr(:comprehension, Expr(:generator, body, :(x = xs)))
        NodeKind::ComprehensionExpression => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of comprehension: missing body".to_string(),
                    ),
                    span,
                ));
            }

            let generator = generator_constructor(walker, &children, span, "comprehension")?;

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(
                            Literal::Str("comprehension".to_string()),
                            span,
                        )],
                        span,
                    },
                    generator,
                ],
                span,
            })
        }

        // Bare generator expression (Issue #10626): `(body for x in xs)`, used
        // e.g. directly as a call argument like `sum(x^2 for x in 1:n)`. Quotes
        // to the same `Expr(:generator, body, binding...)` shape a
        // comprehension's inner generator uses (see the
        // `ComprehensionExpression` arm above), just without the outer
        // `:comprehension` wrapper — matching upstream Julia, which quotes a
        // bare generator to a standalone `Expr(:generator, ...)`.
        NodeKind::GeneratorExpression => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of generator: missing body".to_string(),
                    ),
                    span,
                ));
            }

            generator_constructor(walker, &children, span, "generator")
        }

        // Issue #7275: `:(a[i])`, `:(a[i, j])`, and the typed-array-literal form
        // `:(Any[])` / `:(Int[1, 2])` previously fell through to the catch-all with
        // `quote for index_expression not yet supported`. Both shapes are a single
        // `index_expression` CST node, and upstream Julia lowers them identically to
        // `Expr(:ref, target, indices...)` — `:(a[i])` → `Expr(:ref, :a, :i)`,
        // `:(Any[])` → `Expr(:ref, :Any)`, `:(Int[1, 2])` → `Expr(:ref, :Int, 1, 2)`.
        // The first named child is the indexed target (array, or element type for a
        // typed-array literal); the remaining children are the indices/elements. This
        // unblocks indexing in macro `quote` bodies (e.g. `Interact.@manipulate`'s and
        // `Plots.@animate`'s `esc`-ed loop bodies that index into user data). Mirrors
        // the `VectorExpression` arm above.
        NodeKind::IndexExpression => {
            let children = walker.named_children_vec(&node);

            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("ref".to_string()), span)],
                span,
            }];

            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Issue #4890: `:(Tuple{Int, Int})` / `:(Vector{Int})` previously
        // fell through to the catch-all with
        // `quote for parametrized_type_expression not yet supported`.
        // Upstream Julia lowers `T{P1, P2, ...}` to
        // `Expr(:curly, :T, :P1, :P2, ...)`. The first named child is the
        // base type name; the rest are the type parameters.
        NodeKind::ParametrizedTypeExpression => {
            let children = walker.named_children_vec(&node);

            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("curly".to_string()), span)],
                span,
            }];

            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Issue #4899: `:(a.b)` previously fell through to the catch-all
        // with `quote for field_expression not yet supported`. Upstream
        // Julia lowers `a.b` to `Expr(:., :a, QuoteNode(:b))` — note the
        // second arg is a `QuoteNode` wrapping the field-name Symbol,
        // not a bare Symbol. Field-expression CST has two named
        // children: the object expression (recursively quoted) and the
        // field-name identifier.
        NodeKind::FieldExpression => {
            let children = walker.named_children_vec(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of field expression: missing operands".to_string(),
                    ),
                    span,
                ));
            }
            let object = cst_to_expr_constructor(walker, children[0])?;
            let field_node = children[1];
            let field = match walker.kind(&field_node) {
                NodeKind::Identifier => {
                    let field_name = walker.text(&field_node).to_string();
                    Expr::Builtin {
                        name: BuiltinOp::QuoteNodeNew,
                        args: vec![Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(field_name), span)],
                            span,
                        }],
                        span,
                    }
                }
                NodeKind::UnaryExpression => cst_to_expr_constructor(walker, field_node)?,
                NodeKind::QuoteExpression if walker.named_children_vec(&field_node).is_empty() => {
                    let text = walker.text(&field_node);
                    let raw_name = text.strip_prefix(':').unwrap_or(text);
                    let field_name = raw_name
                        .strip_prefix('(')
                        .and_then(|inner| inner.strip_suffix(')'))
                        .unwrap_or(raw_name)
                        .to_string();
                    Expr::Builtin {
                        name: BuiltinOp::QuoteNodeNew,
                        args: vec![Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(field_name), span)],
                            span,
                        }],
                        span,
                    }
                }
                NodeKind::QuoteExpression => cst_to_expr_constructor(walker, field_node)?,
                _ => {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "quote of field expression: invalid field name".to_string(),
                        ),
                        walker.span(&field_node),
                    ));
                }
            };

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :.
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(".".to_string()), span)],
                        span,
                    },
                    // object (already quoted)
                    object,
                    field,
                ],
                span,
            })
        }

        // Issue #4911 / #4920: `:(:foo)` (a meta-quote nested inside an
        // outer quote) lowers to either `QuoteNode(value)` or
        // `Expr(:quote, value)` depending on the inner kind, matching
        // upstream Julia:
        //
        // - Inner is an atom (Symbol/identifier, operator, literal) →
        //   `QuoteNode(atom)` — e.g. `:(:foo)` → `QuoteNode(:foo)`,
        //   `:(:42)` → `QuoteNode(42)`.
        // - Inner is a complex Expr (Call, BinaryExpression, …) →
        //   `Expr(:quote, complex_expr)` — e.g. `:(:(x+y))` →
        //   `Expr(:quote, :(x+y))`. The pattern-match in metaprogramming
        //   code relies on this distinction.
        //
        // The leaf form (`:foo` as a CstNode::leaf with no children)
        // always produces a QuoteNode because the only thing the leaf
        // can wrap is a Symbol name extracted from the leaf text.
        NodeKind::QuoteExpression => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                // Leaf form: `:foo`. Text includes the leading colon.
                let text = walker.text(&node);
                let name = text.strip_prefix(':').unwrap_or(text).to_string();
                Ok(Expr::Builtin {
                    name: BuiltinOp::QuoteNodeNew,
                    args: vec![Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(name), span)],
                        span,
                    }],
                    span,
                })
            } else {
                // With-children form: `:(expr)` nested inside outer
                // quote. Recurse on the inner; branch on the inner
                // CST kind to match upstream's QuoteNode vs Expr(:quote)
                // distinction.
                let inner_node = children[0];
                let inner = cst_to_expr_constructor(walker, inner_node)?;
                let inner_kind = walker.kind(&inner_node);
                let is_atom = matches!(
                    inner_kind,
                    NodeKind::Identifier
                        | NodeKind::Operator
                        | NodeKind::IntegerLiteral
                        | NodeKind::FloatLiteral
                        | NodeKind::StringLiteral
                        | NodeKind::CharacterLiteral
                        | NodeKind::BooleanLiteral
                        | NodeKind::QuoteExpression // nested leaf-form Symbol like :(:(:foo))
                );
                if is_atom {
                    Ok(Expr::Builtin {
                        name: BuiltinOp::QuoteNodeNew,
                        args: vec![inner],
                        span,
                    })
                } else {
                    // Complex Expr → Expr(:quote, complex_expr)
                    Ok(Expr::Builtin {
                        name: BuiltinOp::ExprNew,
                        args: vec![
                            Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("quote".to_string()), span)],
                                span,
                            },
                            inner,
                        ],
                        span,
                    })
                }
            }
        }

        // Issue #4904: `:(f(args...))` previously fell through to the
        // catch-all with `quote for splat_expression not yet supported`.
        // Upstream Julia lowers `x...` to `Expr(:..., x)` — head is the
        // three-dot Symbol literally named "...". The splat CST has
        // one named child: the expression being splatted (recursively
        // quoted).
        NodeKind::SplatExpression => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of splat expression: missing operand".to_string(),
                    ),
                    span,
                ));
            }
            let inner = cst_to_expr_constructor(walker, children[0])?;

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :...
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("...".to_string()), span)],
                        span,
                    },
                    inner,
                ],
                span,
            })
        }

        // Ternary expression: a ? b : c -> Expr(:if, a, b, c)
        NodeKind::TernaryExpression => {
            // Get operands (filter out ? and : operators)
            let operands: Vec<_> = walker
                .named_children(&node)
                .filter(|n| walker.kind(n) != NodeKind::Operator)
                .collect();

            if operands.len() != 3 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "quote of ternary expression (expected 3 operands, got {})",
                        operands.len()
                    )),
                    span,
                ));
            }

            let condition = cst_to_expr_constructor(walker, operands[0])?;
            let then_expr = cst_to_expr_constructor(walker, operands[1])?;
            let else_expr = cst_to_expr_constructor(walker, operands[2])?;

            // Expr(:if, condition, then_expr, else_expr)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :if
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("if".to_string()), span)],
                        span,
                    },
                    condition,
                    then_expr,
                    else_expr,
                ],
                span,
            })
        }

        // If statement: if cond ... end -> Expr(:if, cond, then_block[, else_block])
        NodeKind::IfStatement => {
            let all_children: Vec<Node<'a>> = walker.children(&node);

            let mut condition: Option<Node<'a>> = None;
            let mut then_block: Option<Node<'a>> = None;
            let mut elseif_clauses: Vec<Node<'a>> = Vec::new();
            let mut else_block: Option<Node<'a>> = None;

            let mut i = 0;
            while i < all_children.len() {
                let child = all_children[i];
                let kind_str = child.kind();

                match kind_str {
                    "if" | "end" => {
                        // Skip keywords
                    }
                    "elseif_clause" => {
                        elseif_clauses.push(child);
                    }
                    "else" => {
                        // Next child should be the else block
                        i += 1;
                        if i < all_children.len() {
                            let else_node = all_children[i];
                            if walker.kind(&else_node) == NodeKind::Block {
                                else_block = Some(else_node);
                            }
                        }
                        break;
                    }
                    "else_clause" => {
                        // else_clause contains: else keyword + block
                        let else_all: Vec<Node<'a>> = walker.children(&child);
                        for else_child in else_all.iter() {
                            if else_child.kind() == "block" {
                                else_block = Some(*else_child);
                                break;
                            }
                        }
                    }
                    _ => {
                        // Must be condition or block
                        if condition.is_none() {
                            condition = Some(child);
                        } else if then_block.is_none() && walker.kind(&child) == NodeKind::Block {
                            then_block = Some(child);
                        }
                    }
                }
                i += 1;
            }

            let condition_node = condition.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of if expression: missing condition".to_string(),
                    ),
                    span,
                )
            })?;

            let then_block_node = then_block.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of if expression: missing then block".to_string(),
                    ),
                    span,
                )
            })?;

            let condition_expr = cst_to_expr_constructor(walker, condition_node)?;
            let then_expr = cst_to_expr_constructor(walker, then_block_node)?;

            let mut args = vec![
                // head: :if
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("if".to_string()), span)],
                    span,
                },
                condition_expr,
                then_expr,
            ];

            // Build the else/elseif chain
            // Process elseif clauses in reverse to build nested structure
            // if a; 1; elseif b; 2; else; 3; end
            // -> Expr(:if, :a, block1, Expr(:elseif, cond_block_b, block2, block3))
            if !elseif_clauses.is_empty() || else_block.is_some() {
                // Start from the else block (if any) and work backwards through elseifs
                let mut tail_expr: Option<Expr> = if let Some(else_node) = else_block {
                    Some(cst_to_expr_constructor(walker, else_node)?)
                } else {
                    None
                };

                // Process elseif clauses in reverse order
                for elseif_clause in elseif_clauses.into_iter().rev() {
                    let elseif_children = walker.named_children_vec(&elseif_clause);
                    if elseif_children.len() < 2 {
                        return Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(
                                "quote of elseif: missing condition or body".to_string(),
                            ),
                            span,
                        ));
                    }

                    let elseif_cond = cst_to_expr_constructor(walker, elseif_children[0])?;
                    let elseif_body = cst_to_expr_constructor(walker, elseif_children[1])?;

                    // Wrap condition in a block for Julia AST compatibility
                    // Julia's elseif has condition wrapped: Expr(:elseif, Expr(:block, cond), body, else)
                    let cond_block = Expr::Builtin {
                        name: BuiltinOp::ExprNew,
                        args: vec![
                            Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                                span,
                            },
                            elseif_cond,
                        ],
                        span,
                    };

                    let mut elseif_args = vec![
                        Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str("elseif".to_string()), span)],
                            span,
                        },
                        cond_block,
                        elseif_body,
                    ];

                    if let Some(tail) = tail_expr {
                        elseif_args.push(tail);
                    }

                    tail_expr = Some(Expr::Builtin {
                        name: BuiltinOp::ExprNew,
                        args: elseif_args,
                        span,
                    });
                }

                if let Some(tail) = tail_expr {
                    args.push(tail);
                }
            } else if let Some(else_node) = else_block {
                // Simple if-else without elseif
                let else_expr = cst_to_expr_constructor(walker, else_node)?;
                args.push(else_expr);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // While statement: while cond ... end -> Expr(:while, cond, body)
        NodeKind::WhileStatement => {
            let children = walker.named_children_vec(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of while: missing condition or body".to_string(),
                    ),
                    span,
                ));
            }

            let condition = cst_to_expr_constructor(walker, children[0])?;
            let body = cst_to_expr_constructor(walker, children[1])?;

            // Expr(:while, condition, body)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("while".to_string()), span)],
                        span,
                    },
                    condition,
                    body,
                ],
                span,
            })
        }

        // For statement: for i in iter ... end -> Expr(:for, :(i = iter), body)
        NodeKind::ForStatement => {
            let children = walker.named_children_vec(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of for: empty for statement".to_string(),
                    ),
                    span,
                ));
            }

            // Find all ForBindings and the Block. Multiple bindings (`for a = …,
            // b = …`) are supported (Issue #7338): upstream represents them as
            // `Expr(:for, Expr(:block, :(a=…), :(b=…)), body)`.
            let mut binding_nodes = Vec::new();
            let mut body_node = None;

            for child in &children {
                match walker.kind(child) {
                    NodeKind::ForBinding => binding_nodes.push(*child),
                    NodeKind::Block => body_node = Some(*child),
                    _ => {}
                }
            }

            if binding_nodes.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of for: missing binding".to_string(),
                    ),
                    span,
                ));
            }

            let body = body_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of for: missing body".to_string(),
                    ),
                    span,
                )
            })?;

            // Build one `Expr(:(=), var, iter)` per binding.
            let mut binding_exprs = Vec::with_capacity(binding_nodes.len());
            for binding in &binding_nodes {
                // Parse ForBinding: [var, iter] or [outer, var, iter]
                let binding_children = walker.named_children_vec(binding);
                let (var_node, iter_node) = if binding_children.len() >= 2 {
                    // Check if first is "outer"
                    let first_text = walker.text(&binding_children[0]);
                    if first_text == "outer" && binding_children.len() >= 3 {
                        (binding_children[1], binding_children[2])
                    } else {
                        (binding_children[0], binding_children[1])
                    }
                } else {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "quote of for: malformed binding".to_string(),
                        ),
                        span,
                    ));
                };

                let var_expr = cst_to_expr_constructor(walker, var_node)?;
                let iter_expr = cst_to_expr_constructor(walker, iter_node)?;

                // Create binding as Expr(:(=), var, iter)
                binding_exprs.push(Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args: vec![
                        Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str("=".to_string()), span)],
                            span,
                        },
                        var_expr,
                        iter_expr,
                    ],
                    span,
                });
            }

            let body_expr = cst_to_expr_constructor(walker, body)?;

            // A single binding goes in directly; multiple bindings are wrapped in
            // `Expr(:block, b1, b2, …)` (mirrors upstream's `for a = …, b = …`).
            let binding_arg = if binding_exprs.len() == 1 {
                binding_exprs.pop().ok_or_else(|| {
                    internal_lowering_error(span, "binding_exprs length checked above")
                })?
            } else {
                let mut block_args = Vec::with_capacity(binding_exprs.len() + 1);
                block_args.push(Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                    span,
                });
                block_args.extend(binding_exprs);
                Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args: block_args,
                    span,
                }
            };

            // Expr(:for, binding, body)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("for".to_string()), span)],
                        span,
                    },
                    binding_arg,
                    body_expr,
                ],
                span,
            })
        }

        // Try statement: try ... catch e ... else ... finally ... end
        // -> Expr(:try, try_block, catch_var_or_false, catch_block_or_false,
        //         finally_block_or_false[, else_block])
        NodeKind::TryStatement => {
            let mut try_block_node = None;
            let mut catch_clause_node = None;
            let mut else_clause_node = None;
            let mut finally_clause_node = None;

            for child in walker.named_children(&node) {
                match walker.kind(&child) {
                    NodeKind::Block if try_block_node.is_none() => {
                        try_block_node = Some(child);
                    }
                    NodeKind::CatchClause => catch_clause_node = Some(child),
                    NodeKind::ElseClause => else_clause_node = Some(child),
                    NodeKind::FinallyClause => finally_clause_node = Some(child),
                    _ => {}
                }
            }

            // Convert try block
            let try_block_expr = match try_block_node {
                Some(block) => cst_to_expr_constructor(walker, block)?,
                None => Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args: vec![Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                        span,
                    }],
                    span,
                },
            };

            // Parse catch clause
            let (catch_var_expr, catch_block_expr) = match catch_clause_node {
                Some(catch_node) => {
                    let mut var_name: Option<String> = None;
                    let mut block_node: Option<Node<'a>> = None;

                    for child in walker.named_children(&catch_node) {
                        match walker.kind(&child) {
                            NodeKind::Identifier if var_name.is_none() => {
                                var_name = Some(walker.text(&child).to_string());
                            }
                            NodeKind::Block if block_node.is_none() => {
                                block_node = Some(child);
                            }
                            _ => {}
                        }
                    }

                    let var_expr = match var_name {
                        Some(name) => Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(name), span)],
                            span,
                        },
                        None => Expr::Literal(Literal::Bool(false), span),
                    };

                    let block_expr = match block_node {
                        Some(block) => cst_to_expr_constructor(walker, block)?,
                        None => Expr::Builtin {
                            name: BuiltinOp::ExprNew,
                            args: vec![Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                                span,
                            }],
                            span,
                        },
                    };

                    (var_expr, block_expr)
                }
                None => (
                    Expr::Literal(Literal::Bool(false), span),
                    Expr::Literal(Literal::Bool(false), span),
                ),
            };

            // Parse finally clause
            let finally_block_expr = match finally_clause_node {
                Some(finally_node) => {
                    let block_node = walker
                        .named_children(&finally_node)
                        .find(|child| walker.kind(child) == NodeKind::Block);

                    match block_node {
                        Some(block) => Some(cst_to_expr_constructor(walker, block)?),
                        None => Some(Expr::Builtin {
                            name: BuiltinOp::ExprNew,
                            args: vec![Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                                span,
                            }],
                            span,
                        }),
                    }
                }
                None => None,
            };

            // Parse else clause. Julia stores it as the optional fifth arg; if
            // there is no finally block, the fourth slot is `false`.
            let else_block_expr = match else_clause_node {
                Some(else_node) => {
                    let block_node = walker
                        .named_children(&else_node)
                        .find(|child| walker.kind(child) == NodeKind::Block);

                    match block_node {
                        Some(block) => Some(cst_to_expr_constructor(walker, block)?),
                        None => Some(Expr::Builtin {
                            name: BuiltinOp::ExprNew,
                            args: vec![Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                                span,
                            }],
                            span,
                        }),
                    }
                }
                None => None,
            };

            // Build Expr(:try, try_block, catch_var, catch_block[, finally_or_false, else_block])
            let mut args = vec![
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("try".to_string()), span)],
                    span,
                },
                try_block_expr,
                catch_var_expr,
                catch_block_expr,
            ];

            match (finally_block_expr, else_block_expr) {
                (Some(finally_expr), Some(else_expr)) => {
                    args.push(finally_expr);
                    args.push(else_expr);
                }
                (Some(finally_expr), None) => args.push(finally_expr),
                (None, Some(else_expr)) => {
                    args.push(Expr::Literal(Literal::Bool(false), span));
                    args.push(else_expr);
                }
                (None, None) => {}
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Arrow function: x -> expr -> Expr(:(->), params, body)
        NodeKind::ArrowFunctionExpression => {
            let children = walker.named_children_vec(&node);

            // Filter out operator nodes
            let non_ops: Vec<_> = children
                .into_iter()
                .filter(|c| walker.kind(c) != NodeKind::Operator)
                .collect();

            if non_ops.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of arrow function: missing parameters or body".to_string(),
                    ),
                    span,
                ));
            }

            let params = cst_to_expr_constructor(walker, non_ops[0])?;
            let body = cst_to_expr_constructor(walker, non_ops[1])?;

            // Expr(:(->), params, body)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("->".to_string()), span)],
                        span,
                    },
                    params,
                    body,
                ],
                span,
            })
        }

        // Range expression: 1:10 -> Expr(:call, :(:), 1, 10)
        NodeKind::RangeExpression => {
            let children = walker.named_children_vec(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of range: missing operands".to_string(),
                    ),
                    span,
                ));
            }

            // Range can have 2 or 3 operands (start:end or start:step:end)
            let mut args = vec![
                // head: :call
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                    span,
                },
                // function: :(:)
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(":".to_string()), span)],
                    span,
                },
            ];

            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Compound assignment: x -= 1 -> Expr(:-=, :x, 1)
        NodeKind::CompoundAssignment => {
            let children = walker.named_children_vec(&node);

            // Find the operator and operands
            let mut left_node = None;
            let mut op_str = None;
            let mut right_node = None;

            for child in &children {
                match walker.kind(child) {
                    NodeKind::Operator => {
                        op_str = Some(walker.text(child).to_string());
                    }
                    _ => {
                        if left_node.is_none() {
                            left_node = Some(*child);
                        } else {
                            right_node = Some(*child);
                        }
                    }
                }
            }

            let left = left_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of compound assignment: missing left operand".to_string(),
                    ),
                    span,
                )
            })?;

            let op = op_str.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of compound assignment: missing operator".to_string(),
                    ),
                    span,
                )
            })?;

            let right = right_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of compound assignment: missing right operand".to_string(),
                    ),
                    span,
                )
            })?;

            let left_expr = cst_to_expr_constructor(walker, left)?;
            let right_expr = cst_to_expr_constructor(walker, right)?;

            // Expr(:-=, left, right) or Expr(:+=, left, right), etc.
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(op), span)],
                        span,
                    },
                    left_expr,
                    right_expr,
                ],
                span,
            })
        }

        // Juxtaposition expression: 2x -> Expr(:call, :*, 2, x)
        NodeKind::JuxtapositionExpression => {
            let children = walker.named_children_vec(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "juxtaposition expression missing operands".to_string(),
                    ),
                    span,
                ));
            }

            let left_constructor = cst_to_expr_constructor(walker, children[0])?;
            let right_constructor = cst_to_expr_constructor(walker, children[1])?;

            // Expr(:call, :*, left, right)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :call
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                        span,
                    },
                    // operator: :*
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("*".to_string()), span)],
                        span,
                    },
                    left_constructor,
                    right_constructor,
                ],
                span,
            })
        }

        // Handle prefixed string literals in quote context (r"...", raw"...", big"...", etc.)
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
                        // r"..." is a regex literal in Julia
                        // For now, return an error as Regex is not yet implemented
                        Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(
                                "Regex literals (r\"...\") are not yet supported".to_string(),
                            ),
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
                    // Upstream Base defines the lowercase @int128_str / @uint128_str
                    // string macros (int128"123" / uint128"123"), parsing to Int128 /
                    // UInt128 respectively (julia/base/int.jl). The capitalized
                    // spellings Int128"..." / UInt128"..." are NOT upstream — they fall
                    // through to the generic string-literal path below like any other
                    // undefined string macro (Issues #10320, #10324).
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
                        // Wrap the parsed value in a `UInt128(…)` constructor call over a
                        // BigInt inner literal so the full 0..typemax(UInt128) range is
                        // range-checked (an Int128 bit pattern would make the checked
                        // Int128→UInt128 conversion reject values above typemax(Int128)).
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
                    "var" => {
                        // Issue #7676: `var"name"` is a non-standard identifier,
                        // not a string literal. Upstream Julia represents it as a
                        // `Symbol` inside a quoted / macro-argument AST (e.g.
                        // `var"@q"` becomes `Symbol("@q")`, printed back as
                        // `var"@q"`). Mirror the `NodeKind::Identifier` arm above
                        // and emit a Symbol rather than falling through to the
                        // `Str` literal default below — which previously made
                        // `@showarg var"@q", var"@qq", postwalk` quote the
                        // identifiers as the strings "@q"/"@qq".
                        Ok(Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            span,
                        })
                    }
                    _ => Ok(Expr::Literal(Literal::Str(content), span)),
                }
            } else {
                let text = walker.text(&node);
                Ok(Expr::Literal(Literal::Str(text.to_string()), span))
            }
        }

        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "quote for {} not yet supported",
                node.kind()
            )),
            span,
        )),
    }
}
