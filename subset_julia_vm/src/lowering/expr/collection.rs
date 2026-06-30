//! Collection expression lowering.
//!
//! This module handles lowering of vectors, matrices, ranges,
//! index expressions, and comprehensions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{encode_tuple_comprehension_binding, Expr};
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::{lower_expr, lower_expr_with_ctx};

/// Lower vector expression: [1, 2, 3] or []
pub fn lower_vector_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    lower_vector_expr_impl(walker, node, None)
}

pub fn lower_vector_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_vector_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_vector_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Empty array [] is now supported - creates empty Any array
    if named.is_empty() {
        return Ok(Expr::ArrayLiteral {
            elements: vec![],
            shape: vec![0],
            span,
        });
    }

    // `[[1 2] [3 4]]` (space-separated bracketed matrices) is parsed as a
    // `vector_expression` whose single child is a `typed_expression` grouping
    // the inner matrices. That child is really horizontal concatenation, not a
    // one-element vector, so lower it directly (unwrapping the outer brackets)
    // to the `hcat(...)` produced by `lower_typed_matrix_expr` instead of
    // boxing it as a single `Any` element (Issue #7203).
    if named.len() == 1 && walker.kind(&named[0]) == NodeKind::TypedExpression {
        let typed_children = walker.named_children(&named[0]);
        if typed_children.len() == 2
            && is_concat_misparse_node(walker, typed_children[0])
            && is_concat_misparse_node(walker, typed_children[1])
        {
            return lower_expr_maybe_ctx(walker, named[0], lambda_ctx);
        }
    }

    // Splat inside an untyped array literal (e.g. `[a, v..., b]`) is lowered to
    // a splat-call to the `Base._array_splat_literal(vals...)` helper, mirroring
    // upstream Julia's `Base.vect(X...)` lowering: the splatted operands are
    // spread into the call (one VM argument per element) and the helper builds a
    // `Vector{T}` whose element type is the `promote_typeof` of the values, with
    // every value placed as a single element (Issue #7255).
    if named
        .iter()
        .any(|n| walker.kind(n) == NodeKind::SplatExpression)
    {
        let (args, splat_mask) = lower_array_literal_splat_args(walker, &named, lambda_ctx)?;
        return Ok(Expr::Call {
            function: "_array_splat_literal".to_string(),
            args,
            kwargs: vec![],
            splat_mask,
            kwargs_splat_mask: vec![],
            span,
        });
    }

    let mut elements = Vec::new();
    for child in named {
        elements.push(lower_expr_maybe_ctx(walker, child, lambda_ctx)?);
    }

    let shape = vec![elements.len()];
    Ok(Expr::ArrayLiteral {
        elements,
        shape,
        span,
    })
}

/// Lower the elements of an array literal that contains a positional splat into
/// the argument list + `splat_mask` for a splat-call to an array-builder helper
/// (`_array_splat_literal` / `_array_splat_literal_typed`). A `xs...` element is
/// lowered to its inner expression with `splat_mask = true` (spread into the
/// call); every other element is lowered as-is with `splat_mask = false` (a
/// single argument / single element). Mirrors upstream Julia's array-literal
/// lowering, which spreads splats through `Core._apply_iterate` (Issue #7255).
fn lower_array_literal_splat_args<'a>(
    walker: &CstWalker<'a>,
    elements: &[Node<'a>],
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<(Vec<Expr>, Vec<bool>)> {
    let mut args = Vec::with_capacity(elements.len());
    let mut splat_mask = Vec::with_capacity(elements.len());
    for child in elements {
        if walker.kind(child) == NodeKind::SplatExpression {
            let inner_children = walker.named_children(child);
            let inner = inner_children.first().ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression("splat_expression".to_string()),
                    walker.span(child),
                )
            })?;
            args.push(lower_expr_maybe_ctx(walker, *inner, lambda_ctx)?);
            splat_mask.push(true);
        } else {
            args.push(lower_expr_maybe_ctx(walker, *child, lambda_ctx)?);
            splat_mask.push(false);
        }
    }
    Ok((args, splat_mask))
}

/// Whether an element of a matrix/vector literal is *guaranteed* to be a
/// scalar (so the fast `ArrayLiteral` path can place it directly). Anything
/// that could evaluate to an array or range at runtime — a variable, a call,
/// an index, a range, a nested array/matrix literal — is treated as
/// non-scalar so the literal is routed through `hcat`/`vcat`/`hvcat`, which
/// flatten array/range elements column-/row-wise like upstream Julia
/// (Issue #7203).
fn is_definitely_scalar_element(expr: &Expr) -> bool {
    match expr {
        // Numeric / character / string / nothing / missing literals are scalar.
        // Array/struct/etc. literal payloads (REPL persistence) are not.
        Expr::Literal(lit, _) => matches!(
            lit,
            crate::ir::core::Literal::Int(_)
                | crate::ir::core::Literal::Int128(_)
                | crate::ir::core::Literal::BigInt(_)
                | crate::ir::core::Literal::BigFloat(_)
                | crate::ir::core::Literal::Float(_)
                | crate::ir::core::Literal::Float32(_)
                | crate::ir::core::Literal::Float16(_)
                | crate::ir::core::Literal::Bool(_)
                | crate::ir::core::Literal::Str(_)
                | crate::ir::core::Literal::Char(_)
                | crate::ir::core::Literal::Nothing
                | crate::ir::core::Literal::Missing
        ),
        // Arithmetic over guaranteed-scalar operands stays scalar (`-1`, `2*3`).
        Expr::UnaryOp { operand, .. } => is_definitely_scalar_element(operand),
        Expr::BinaryOp { left, right, .. } => {
            is_definitely_scalar_element(left) && is_definitely_scalar_element(right)
        }
        // Everything else (Var, Call, Index, Range, ArrayLiteral, FieldAccess,
        // comprehensions, ...) could be a collection at runtime.
        _ => false,
    }
}

/// Whether a CST node is part of the `[[..] [..]]` horizontal-concatenation
/// misparse: an array/matrix literal, or a nested `typed_expression` grouping
/// such literals (the 3+-matrix form). Used to distinguish that misparse from
/// a genuine `value::Type` assertion (Issue #7203).
fn is_concat_misparse_node(walker: &CstWalker<'_>, node: Node<'_>) -> bool {
    matches!(
        walker.kind(&node),
        NodeKind::MatrixExpression | NodeKind::VectorExpression | NodeKind::TypedExpression
    )
}

/// Build a `function(args...)` call expression with no kwargs/splats.
fn concat_call(function: &str, args: Vec<Expr>, span: crate::span::Span) -> Expr {
    let splat_mask = vec![false; args.len()];
    Expr::Call {
        function: function.to_string(),
        args,
        kwargs: vec![],
        splat_mask,
        kwargs_splat_mask: vec![],
        span,
    }
}

/// Lower matrix expression: [1 2; 3 4]
/// Julia uses column-major order, so elements are stored column by column.
/// For [1 2 3; 4 5 6], the storage order is [1, 4, 2, 5, 3, 6].
pub fn lower_matrix_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    lower_matrix_expr_impl(walker, node, None, true)
}

pub fn lower_matrix_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_matrix_expr_impl(walker, node, Some(lambda_ctx), true)
}

/// Lower a matrix expression to a raw `ArrayLiteral` without routing
/// non-scalar elements through `hcat`/`vcat`/`hvcat`. Used by the typed-matrix
/// path (`T[...]`), which needs the flat element list + shape to build a typed
/// array; concatenation routing only applies to plain untyped literals
/// (Issue #7203).
pub fn lower_matrix_expr_raw<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    lower_matrix_expr_impl(walker, node, None, false)
}

fn lower_matrix_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
    concat_route: bool,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::EmptyArray,
            span,
        ));
    }

    // Check if we have matrix rows
    let rows: Vec<_> = named
        .iter()
        .filter(|n| walker.kind(n) == NodeKind::MatrixRow)
        .collect();

    if rows.is_empty() {
        // Single row matrix: [1 2 3] - treat as row vector (1×n matrix)
        // For 1×n matrix, column-major order is the same as row order
        let mut elements = Vec::new();
        for child in named {
            elements.push(lower_expr_maybe_ctx(walker, child, lambda_ctx)?);
        }

        // If any element could be an array/range at runtime, concatenate the
        // elements horizontally (`hcat`) so they are flattened column-wise like
        // upstream Julia. `[g 4]`, `[1:2 3:4]`, `[[1 2] [3 4]]` reach this path
        // (Issue #7203). All-scalar literals keep the direct `ArrayLiteral`
        // fast path below.
        if concat_route && !elements.iter().all(is_definitely_scalar_element) {
            return Ok(concat_call("hcat", elements, span));
        }

        let cols = elements.len();
        return Ok(Expr::ArrayLiteral {
            elements,
            shape: vec![1, cols],
            span,
        });
    }

    // Multi-row matrix: collect elements row by row first.
    //
    // Unlike the single-row case, rows may legitimately contain differing
    // element counts when the elements are themselves arrays/matrices
    // (`[A B; C D]` blocks, or `[g; row]` with one block per row). Validation
    // of a uniform column count only applies to the all-scalar fast path; the
    // block-concatenation path (`hvcat`) tolerates per-row block counts and
    // checks shapes at runtime like upstream Julia (Issue #7203).
    let mut row_elements: Vec<Vec<Expr>> = Vec::new();
    let mut all_scalar = true;

    for row_node in &rows {
        let this_row_elements = walker.named_children(row_node);
        let mut row_vec = Vec::new();
        for elem in this_row_elements {
            let lowered = lower_expr_maybe_ctx(walker, elem, lambda_ctx)?;
            if !is_definitely_scalar_element(&lowered) {
                all_scalar = false;
            }
            row_vec.push(lowered);
        }
        row_elements.push(row_vec);
    }

    let row_count = rows.len();

    if all_scalar || !concat_route {
        // All-scalar fast path (or typed-matrix context that needs the flat
        // element list): build the matrix directly. Require a uniform column
        // count, mirroring the previous behavior.
        let cols = row_elements[0].len();
        for row in &row_elements {
            if row.len() != cols {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MalformedMatrix, span)
                        .with_hint(format!(
                            "inconsistent column count: expected {}, got {}",
                            cols,
                            row.len()
                        )),
                );
            }
        }

        // Convert from row-major to column-major order.
        // For [1 2 3; 4 5 6], row_elements = [[1,2,3], [4,5,6]]
        // column-major order: [1, 4, 2, 5, 3, 6] (column 0, column 1, column 2)
        let mut all_elements = Vec::with_capacity(row_count * cols);
        for col in 0..cols {
            for row in &row_elements {
                all_elements.push(row[col].clone());
            }
        }

        return Ok(Expr::ArrayLiteral {
            elements: all_elements,
            shape: vec![row_count, cols],
            span,
        });
    }

    // When every row holds exactly one element, the literal is pure vertical
    // concatenation (`[a; b; c]`). Lower to `vcat(a, b, c)` so that columnar
    // arguments (ranges/vectors/scalars) flatten into a 1-D `Vector` rather
    // than an N×1 matrix, matching upstream Julia (Issue #7203).
    if row_elements.iter().all(|row| row.len() == 1) {
        let args: Vec<Expr> = row_elements
            .into_iter()
            .map(|mut row| row.remove(0))
            .collect();
        return Ok(concat_call("vcat", args, span));
    }

    // Otherwise lower to `hvcat((c1, c2, ...), e11, e12, ...)` with arguments
    // in row-major order. `hvcat` concatenates each row block horizontally and
    // then stacks the rows vertically, flattening array/range elements like
    // upstream Julia.
    let row_lengths: Vec<Expr> = row_elements
        .iter()
        .map(|row| Expr::Literal(crate::ir::core::Literal::Int(row.len() as i64), span))
        .collect();
    let rows_tuple = Expr::TupleLiteral {
        elements: row_lengths,
        span,
    };

    let mut args = Vec::with_capacity(1 + row_count);
    args.push(rows_tuple);
    for row in row_elements {
        for elem in row {
            args.push(elem);
        }
    }

    Ok(concat_call("hvcat", args, span))
}

/// Lower index expression: arr[i] or arr[i, j]
/// Supports `end` keyword: arr[end] -> arr[lastindex(arr)] for 1D, arr[lastindex(arr, dim)] for nD
/// Supports `begin` keyword: arr[begin] -> arr[firstindex(arr)] for 1D, arr[firstindex(arr, dim)] for nD
/// (Issue #2310, Issue #2349)
pub fn lower_index_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    lower_index_expr_impl(walker, node, None)
}

pub fn lower_index_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_index_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_index_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("empty index".to_string()),
            span,
        ));
    }

    // First child is the array being indexed
    let array = lower_expr_maybe_ctx(walker, named[0], lambda_ctx)?;

    let preserve_tuple_elements = is_type_like_index_target(&array);

    // A typed array literal `T[a, xs..., b]` is parsed as an index expression
    // whose target is the element type and whose "indices" are the literal's
    // elements. When one of those elements is a positional splat (`xs...`), it
    // must be spread into a `Vector{T}` like upstream Julia's
    // `getindex(::Type{T}, vals...)` lowering, not treated as array indexing.
    // Route it to the `Base._array_splat_literal_typed(T, vals...)` helper with
    // a splat-call so each spread element becomes one converted element
    // (Issue #7255).
    if is_typed_array_constructor_target(&array)
        && named
            .iter()
            .skip(1)
            .any(|n| walker.kind(n) == NodeKind::SplatExpression)
    {
        let element_nodes: Vec<Node<'a>> = named.iter().skip(1).copied().collect();
        let (mut args, mut splat_mask) =
            lower_array_literal_splat_args(walker, &element_nodes, lambda_ctx)?;
        // Prepend the element type as the first (never-splatted) argument.
        args.insert(0, array);
        splat_mask.insert(0, false);
        return Ok(Expr::Call {
            function: "_array_splat_literal_typed".to_string(),
            args,
            kwargs: vec![],
            splat_mask,
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Collect all index nodes first to determine total number of dimensions.
    // For typed array constructors, `T[(a, b)]` means a one-element
    // `Vector{T}` whose element is the tuple. Do not reinterpret the tuple as
    // multi-dimensional indexing in that constructor form (Issue #4627).
    let mut all_index_nodes = Vec::new();
    for child in named.iter().skip(1) {
        if preserve_tuple_elements && walker.kind(child) == NodeKind::TupleExpression {
            all_index_nodes.push(*child);
        } else {
            for idx_node in collect_index_nodes(walker, *child) {
                all_index_nodes.push(idx_node);
            }
        }
    }

    let total_indices = all_index_nodes.len();

    // Remaining children are indices (could be wrapped in vector/tuple/argument list)
    let mut indices = Vec::new();

    for (dim_index, idx_node) in all_index_nodes.into_iter().enumerate() {
        let idx_expr = lower_index_component(walker, idx_node, lambda_ctx)?;
        // Replace `end` with `lastindex(array)` or `lastindex(array, dim)` (Issue #2349)
        // Replace `begin` with `firstindex(array)` or `firstindex(array, dim)` (Issue #2349)
        // Use dimension-aware version when there are multiple indices
        let dim = if total_indices > 1 {
            Some(dim_index + 1) // Julia uses 1-based dimension indexing
        } else {
            None
        };
        let idx_expr = replace_end_with_lastindex(idx_expr, &array, dim);
        let idx_expr = replace_begin_with_firstindex(idx_expr, &array, dim);
        indices.push(idx_expr);
    }

    // Check for T[] syntax: type name with empty indices creates empty typed array
    if indices.is_empty() {
        // Check if the "array" is actually a type name
        let type_name = match &array {
            Expr::Var(name, _) => Some(name.clone()),
            // Handle parametric types like Complex{Float64}
            Expr::Call { function, args, .. } => {
                // Reconstruct the full type name from function(arg)
                if args.len() == 1 {
                    if let Expr::Var(param, _) = &args[0] {
                        Some(format!("{}{{{}}}", function, param))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            // All-static parametric types (`UnitRange{Int64}`, `Vector{Int}`,
            // `Dict{String,Int}`, ...) lower to `TypeOf("<name>")`; recover the
            // declared element-type name so `T[]` keeps its eltype (Issue #6768).
            Expr::Builtin {
                name: crate::ir::core::BuiltinOp::TypeOf,
                args,
                ..
            } => match args.as_slice() {
                [Expr::Literal(crate::ir::core::Literal::Str(name), _)] => Some(name.clone()),
                _ => None,
            },
            _ => None,
        };

        // Check if this is a known type (basic types or structs)
        if let Some(name) = type_name {
            let is_type = matches!(
                name.as_str(),
                "Int"
                    | "Int64"
                    | "Int32"
                    | "Float64"
                    | "Float32"
                    | "Bool"
                    | "String"
                    | "Char"
                    | "Any"
            ) || name.starts_with("Complex")
                || name.starts_with("Point")
                || name
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false); // Heuristic: capitalized names are types

            if is_type {
                return Ok(Expr::TypedEmptyArray {
                    element_type: name,
                    span,
                });
            }
        }
    }

    Ok(Expr::Index {
        array: Box::new(array),
        indices,
        span,
    })
}

fn lower_expr_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    match lambda_ctx {
        Some(ctx) => lower_expr_with_ctx(walker, node, ctx),
        None => lower_expr(walker, node),
    }
}

fn is_type_like_index_target(expr: &Expr) -> bool {
    match expr {
        Expr::Var(name, _) => {
            matches!(
                name.as_str(),
                "Any"
                    | "Bool"
                    | "Char"
                    | "String"
                    | "Int"
                    | "Int8"
                    | "Int16"
                    | "Int32"
                    | "Int64"
                    | "Int128"
                    | "UInt8"
                    | "UInt16"
                    | "UInt32"
                    | "UInt64"
                    | "UInt128"
                    | "Float16"
                    | "Float32"
                    | "Float64"
            ) || name
                .chars()
                .next()
                .map(|ch| ch.is_uppercase())
                .unwrap_or(false)
        }
        Expr::Call { function, .. } => function
            .chars()
            .next()
            .map(|ch| ch.is_uppercase())
            .unwrap_or(false),
        _ => false,
    }
}

/// Whether the lowered index target is an element type used as a typed-array
/// constructor (`T[...]`). This covers the bare/parametric forms a type can
/// lower to: an uppercase identifier or `Type{...}` call (`Any`, `MyStruct`),
/// a static parametric type that lowers to `typeof("...")` (`Complex{Float64}`,
/// `Vector{Int}`), and a dynamic parametric type. Used to route a typed array
/// literal that contains a positional splat (`T[a, xs..., b]`) to the typed
/// array-builder helper (Issue #7255).
fn is_typed_array_constructor_target(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Builtin {
            name: crate::ir::core::BuiltinOp::TypeOf,
            ..
        } | Expr::DynamicTypeConstruct { .. }
    ) || is_type_like_index_target(expr)
}

/// Lower range expression: 1:10 or 1:2:10
/// Tree-sitter parses `1:2:10` as nested: (1:2):10
/// We need to flatten this to start=1, step=2, stop=10
pub fn lower_range_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Filter out operator nodes (the `:` operators)
    let operands: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if operands.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedRange,
            span,
        ));
    }

    if operands.len() == 2 {
        // Check if first operand is also a RangeExpression (nested case: (1:2):10)
        if walker.kind(&operands[0]) == NodeKind::RangeExpression {
            // Flatten: (start:step):stop
            let inner = walker.named_children(&operands[0]);
            let inner_operands: Vec<_> = inner
                .into_iter()
                .filter(|n| walker.kind(n) != NodeKind::Operator)
                .collect();

            if inner_operands.len() == 2 {
                let start = lower_expr(walker, inner_operands[0])?;
                let step = lower_expr(walker, inner_operands[1])?;
                let stop = lower_expr(walker, operands[1])?;
                return Ok(Expr::Range {
                    start: Box::new(start),
                    step: Some(Box::new(step)),
                    stop: Box::new(stop),
                    span,
                });
            }
        }

        // Simple range: start:stop
        let start = lower_expr(walker, operands[0])?;
        let stop = lower_expr(walker, operands[1])?;
        Ok(Expr::Range {
            start: Box::new(start),
            step: None,
            stop: Box::new(stop),
            span,
        })
    } else if operands.len() == 3 {
        // start:step:stop (direct case, if tree-sitter ever produces this)
        let start = lower_expr(walker, operands[0])?;
        let step = lower_expr(walker, operands[1])?;
        let stop = lower_expr(walker, operands[2])?;
        Ok(Expr::Range {
            start: Box::new(start),
            step: Some(Box::new(step)),
            stop: Box::new(stop),
            span,
        })
    } else {
        Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedRange,
            span,
        ))
    }
}

/// Extract the array *node* and index nodes from an index expression, for any
/// array target (not just a bare identifier). Used to desugar
/// `obj.field[i] = v` / `f(x)[i] = v` to `setindex!(<array>, v, i)` when the
/// array is not a simple variable (Issue #6640).
pub fn extract_index_target_nodes<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(Node<'a>, Vec<Node<'a>>)> {
    let named = walker.named_children(&node);
    if named.is_empty() {
        return None;
    }
    let array_node = named[0];
    let mut index_nodes = Vec::new();
    for child in named.into_iter().skip(1) {
        index_nodes.extend(collect_index_nodes(walker, child));
    }
    Some((array_node, index_nodes))
}

/// Extract array name from an index expression (for IndexAssign)
pub fn extract_index_target<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(String, Vec<Node<'a>>)> {
    let named = walker.named_children(&node);
    if named.is_empty() {
        return None;
    }

    // First child should be the array (identifier)
    let array_node = named[0];
    if walker.kind(&array_node) != NodeKind::Identifier {
        return None;
    }

    let array_name = walker.text(&array_node).to_string();

    // Get index nodes
    let mut index_nodes = Vec::new();
    for child in named.into_iter().skip(1) {
        index_nodes.extend(collect_index_nodes(walker, child));
    }

    Some((array_name, index_nodes))
}

fn collect_index_nodes<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Vec<Node<'a>> {
    match walker.kind(&node) {
        // An `ArgumentList` index spreads into multiple dimensions: `arr[1, 2]`
        // is `getindex(arr, 1, 2)`.
        //
        // A parenthesized `TupleExpression` must NOT spread: `d[(1, 2)]` is a
        // SINGLE tuple index `getindex(d, (1, 2))` — e.g. a tuple-keyed `Dict`
        // lookup — not multi-dimensional `d[1, 2]` (Issue #6693). Multi-index
        // `arr[1, 2]` is already parsed as direct index children, never as a
        // `TupleExpression`, so excluding it here does not affect array indexing
        // while it matches upstream, where `arr[(1, 2)]` is an invalid tuple
        // index rather than 2-D indexing.
        //
        // A `VectorExpression` (`[1,3,5]`) must NOT spread either: `arr[[1,3,5]]`
        // is fancy (vector) indexing — a single index — not multi-dimensional
        // `arr[1,3,5]` (Issue #5756). Flattening it produced a spurious
        // dimension mismatch.
        NodeKind::ArgumentList => walker.named_children(&node),
        _ => vec![node],
    }
}

fn lower_index_component<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    match walker.kind(&node) {
        NodeKind::Operator => {
            let text = walker.text(&node);
            if text == ":" {
                Ok(Expr::SliceAll {
                    span: walker.span(&node),
                })
            } else {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!("operator {}", text)),
                    walker.span(&node),
                ))
            }
        }
        _ => lower_expr_maybe_ctx(walker, node, lambda_ctx),
    }
}

/// Replace all occurrences of `end` identifier with `lastindex(array)` or `lastindex(array, dim)`.
/// This enables Julia's `arr[end]`, `arr[end-1]`, `arr[1:end]` syntax.
/// When `dim` is Some(d), uses dimension-aware `lastindex(array, d)` for multi-dimensional indexing (Issue #2349).
fn replace_end_with_lastindex(expr: Expr, array: &Expr, dim: Option<usize>) -> Expr {
    match expr {
        Expr::Var(ref name, span) if name == "end" => {
            // Replace `end` with `lastindex(array)` or `lastindex(array, dim)`
            let args = if let Some(d) = dim {
                vec![
                    array.clone(),
                    Expr::Literal(crate::ir::core::Literal::Int(d as i64), span),
                ]
            } else {
                vec![array.clone()]
            };
            Expr::Call {
                function: "lastindex".to_string(),
                args,
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }
        }
        // Recursively process binary operations (e.g., end-1, 1:end)
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Expr::BinaryOp {
            op,
            left: Box::new(replace_end_with_lastindex(*left, array, dim)),
            right: Box::new(replace_end_with_lastindex(*right, array, dim)),
            span,
        },
        // Recursively process unary operations (e.g., -end, though rare)
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op,
            operand: Box::new(replace_end_with_lastindex(*operand, array, dim)),
            span,
        },
        // Recursively process range expressions (e.g., 1:end, 1:2:end)
        Expr::Range {
            start,
            step,
            stop,
            span,
        } => Expr::Range {
            start: Box::new(replace_end_with_lastindex(*start, array, dim)),
            step: step.map(|e| Box::new(replace_end_with_lastindex(*e, array, dim))),
            stop: Box::new(replace_end_with_lastindex(*stop, array, dim)),
            span,
        },
        // Recursively process function calls (e.g., min(end, 5))
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Expr::Call {
            function,
            args: args
                .into_iter()
                .map(|a| replace_end_with_lastindex(a, array, dim))
                .collect(),
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        },
        // All other expressions pass through unchanged
        _ => expr,
    }
}

/// Replace all occurrences of `begin` identifier with `firstindex(array)` or `firstindex(array, dim)`.
/// This enables Julia's `arr[begin]`, `arr[begin+1]`, `arr[begin:end]` syntax (Issue #2310).
/// When `dim` is Some(d), uses dimension-aware `firstindex(array, d)` for multi-dimensional indexing (Issue #2349).
fn replace_begin_with_firstindex(expr: Expr, array: &Expr, dim: Option<usize>) -> Expr {
    match expr {
        Expr::Var(ref name, span) if name == "begin" => {
            // Replace `begin` with `firstindex(array)` or `firstindex(array, dim)`
            let args = if let Some(d) = dim {
                vec![
                    array.clone(),
                    Expr::Literal(crate::ir::core::Literal::Int(d as i64), span),
                ]
            } else {
                vec![array.clone()]
            };
            Expr::Call {
                function: "firstindex".to_string(),
                args,
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }
        }
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Expr::BinaryOp {
            op,
            left: Box::new(replace_begin_with_firstindex(*left, array, dim)),
            right: Box::new(replace_begin_with_firstindex(*right, array, dim)),
            span,
        },
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op,
            operand: Box::new(replace_begin_with_firstindex(*operand, array, dim)),
            span,
        },
        Expr::Range {
            start,
            step,
            stop,
            span,
        } => Expr::Range {
            start: Box::new(replace_begin_with_firstindex(*start, array, dim)),
            step: step.map(|e| Box::new(replace_begin_with_firstindex(*e, array, dim))),
            stop: Box::new(replace_begin_with_firstindex(*stop, array, dim)),
            span,
        },
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Expr::Call {
            function,
            args: args
                .into_iter()
                .map(|a| replace_begin_with_firstindex(a, array, dim))
                .collect(),
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        },
        _ => expr,
    }
}

/// Lower comprehension expression: [x^2 for x in 1:10] or [x for x in arr if x > 0]
pub fn lower_comprehension_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty comprehension"),
        );
    }

    // Find the body expression, all for-clauses, and optional if-clause
    let mut body_expr = None;
    let mut for_clauses = Vec::new();
    let mut if_clause = None;

    for child in &named {
        match walker.kind(child) {
            NodeKind::ForClause => {
                for_clauses.push(*child);
            }
            NodeKind::IfClause => {
                if_clause = Some(*child);
            }
            _ => {
                if body_expr.is_none() {
                    body_expr = Some(*child);
                }
            }
        }
    }

    let body_node = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
            .with_hint("missing body expression")
    })?;

    if for_clauses.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("missing for clause"),
        );
    }

    // Parse body expression
    let body = lower_expr(walker, body_node)?;

    // Parse optional filter
    let filter = if let Some(if_node) = if_clause {
        Some(Box::new(parse_if_clause(walker, if_node)?))
    } else {
        None
    };

    // Collect bindings grouped by ForClause. The parser packs multiple
    // comma-separated bindings (e.g. `for i in R, j in R`) into a SINGLE
    // ForClause with multiple ForBinding children, while the whitespace
    // `for i in R for j in R` flatten form produces MULTIPLE ForClauses each
    // with one binding. This grouping is exactly the comma-vs-whitespace
    // distinction the eval path needs (Issue #8014).
    let mut clause_bindings: Vec<Vec<(String, Expr)>> = Vec::with_capacity(for_clauses.len());
    for fc in &for_clauses {
        clause_bindings.push(parse_for_clause_bindings(walker, *fc)?);
    }

    // More than one `for` clause ⇒ whitespace flatten form: 1-D Vector with
    // `Iterators.flatten` semantics (Issue #8014). A single clause (possibly
    // with comma-separated bindings) is the cartesian / multidimensional form.
    let is_flatten = for_clauses.len() > 1;

    if !is_flatten {
        let mut all_bindings: Vec<(String, Expr)> = clause_bindings.into_iter().flatten().collect();

        if all_bindings.len() == 1 {
            // Single-variable comprehension: use existing Comprehension IR
            let Some((var_name, iter_expr)) = all_bindings.pop() else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                        .with_hint("missing for clause binding"),
                );
            };
            return Ok(Expr::Comprehension {
                body: Box::new(body),
                var: var_name,
                iter: Box::new(iter_expr),
                filter,
                span,
            });
        }

        // Comma cartesian form `[expr for i in R1, j in R2]`: N-D Array by rank
        // (Issue #2143).
        return Ok(Expr::MultiComprehension {
            body: Box::new(body),
            iterations: all_bindings,
            filter,
            flatten: false,
            span,
        });
    }

    // Whitespace flatten form: store the bindings in outermost→innermost loop
    // order. Clauses run in source order (clause 1 = outermost group); within a
    // comma-grouped clause the bindings are reversed so the first variable is
    // innermost (column-major iteration of the group), matching upstream
    // `[expr for i in A, j in B for k in C]` ordering (Issue #8014).
    let total: usize = clause_bindings.iter().map(Vec::len).sum();
    let mut iterations: Vec<(String, Expr)> = Vec::with_capacity(total);
    for clause in clause_bindings {
        for binding in clause.into_iter().rev() {
            iterations.push(binding);
        }
    }

    Ok(Expr::MultiComprehension {
        body: Box::new(body),
        iterations,
        filter,
        flatten: true,
        span,
    })
}

/// Lower generator expression: (x^2 for x in 1:10) or (x for x in arr if x > 0)
/// Produces a lazy Generator that doesn't evaluate until iterated.
pub fn lower_generator_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty generator"),
        );
    }

    let mut body_expr = None;
    let mut for_clause = None;
    let mut if_clause = None;

    for child in &named {
        match walker.kind(child) {
            NodeKind::ForClause => {
                if for_clause.is_some() {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::Comprehension,
                        span,
                    )
                    .with_hint("nested generators not supported"));
                }
                for_clause = Some(*child);
            }
            NodeKind::IfClause => {
                if_clause = Some(*child);
            }
            _ => {
                if body_expr.is_none() {
                    body_expr = Some(*child);
                }
            }
        }
    }

    let body_node = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
            .with_hint("missing body expression")
    })?;

    let for_node = for_clause.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
            .with_hint("missing for clause")
    })?;

    let (var_name, iter_expr) = parse_for_clause(walker, for_node)?;
    let body = lower_expr(walker, body_node)?;
    let filter = if let Some(if_node) = if_clause {
        Some(Box::new(parse_if_clause(walker, if_node)?))
    } else {
        None
    };

    // Return Generator (lazy evaluation)
    Ok(Expr::Generator {
        body: Box::new(body),
        var: var_name,
        iter: Box::new(iter_expr),
        filter,
        span,
    })
}

/// Parse ALL bindings from a for clause.
/// A single ForClause may contain multiple ForBindings when comma-separated:
///   `for i in 1:3, j in 1:3` produces one ForClause with two ForBinding children.
fn parse_for_clause_bindings<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<(String, Expr)>> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Collect all ForBinding children
    let mut bindings = Vec::new();
    for child in &named {
        if walker.kind(child) == NodeKind::ForBinding {
            bindings.push(parse_for_binding(walker, *child)?);
        }
    }

    if !bindings.is_empty() {
        return Ok(bindings);
    }

    // Fallback: try to parse as a single binding from direct children
    let non_op: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if non_op.len() >= 2 {
        let var_node = non_op[0];
        let iter_node = non_op[1];

        let var_name = parse_for_binding_name(walker, var_node)?;
        let iter_expr = lower_expr(walker, iter_node)?;
        return Ok(vec![(var_name, iter_expr)]);
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedForBinding,
        span,
    ))
}

/// Parse a for clause: "for x in range" or "for x = range"
fn parse_for_clause<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<(String, Expr)> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Look for for_binding or direct children
    for child in &named {
        if walker.kind(child) == NodeKind::ForBinding {
            return parse_for_binding(walker, *child);
        }
    }

    // Filter out operator nodes (like "in" or "=")
    let non_op: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    // Try to parse directly: should have identifier and range
    if non_op.len() >= 2 {
        let var_node = non_op[0];
        let iter_node = non_op[1];

        let var_name = parse_for_binding_name(walker, var_node)?;
        let iter_expr = lower_expr(walker, iter_node)?;

        return Ok((var_name, iter_expr));
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedForBinding,
        span,
    ))
}

/// Parse a for binding: "x in range" or "x = range"
fn parse_for_binding<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<(String, Expr)> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Filter out operator nodes (like "in" or "=")
    let non_op: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if non_op.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            span,
        ));
    }

    let var_node = non_op[0];
    let iter_node = non_op[1];

    let var_name = parse_for_binding_name(walker, var_node)?;
    let iter_expr = lower_expr(walker, iter_node)?;

    Ok((var_name, iter_expr))
}

fn parse_for_binding_name<'a>(walker: &CstWalker<'a>, var_node: Node<'a>) -> LowerResult<String> {
    match walker.kind(&var_node) {
        NodeKind::Identifier => Ok(walker.text(&var_node).to_string()),
        NodeKind::TupleExpression => {
            let vars: Vec<String> = walker
                .named_children(&var_node)
                .into_iter()
                .filter(|child| walker.kind(child) == NodeKind::Identifier)
                .map(|child| walker.text(&child).to_string())
                .collect();
            if vars.is_empty() {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedForBinding,
                    walker.span(&var_node),
                )
                .with_hint("tuple comprehension binding must contain identifiers"))
            } else {
                Ok(encode_tuple_comprehension_binding(&vars))
            }
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            walker.span(&var_node),
        )),
    }
}

/// Parse an if clause: "if condition"
fn parse_if_clause<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty if clause"),
        );
    }

    // The if clause contains just the condition expression
    lower_expr(walker, named[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Literal};
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string(), s())
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), s())
    }

    fn array_ref() -> Expr {
        Expr::Var("arr".to_string(), s())
    }

    // ── replace_end_with_lastindex ────────────────────────────────────────────

    #[test]
    fn test_replace_end_becomes_lastindex_no_dim() {
        let result = replace_end_with_lastindex(var("end"), &array_ref(), None);
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "lastindex" && args.len() == 1),
            "Expected lastindex(arr), got {:?}",
            result
        );
    }

    #[test]
    fn test_replace_end_with_dim_becomes_lastindex_with_dim() {
        let result = replace_end_with_lastindex(var("end"), &array_ref(), Some(2));
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "lastindex" && args.len() == 2),
            "Expected lastindex(arr, 2), got {:?}",
            result
        );
    }

    #[test]
    fn test_replace_end_in_binary_op() {
        // end - 1 → lastindex(arr) - 1
        let expr = Expr::BinaryOp {
            op: BinaryOp::Sub,
            left: Box::new(var("end")),
            right: Box::new(lit_int(1)),
            span: s(),
        };
        let result = replace_end_with_lastindex(expr, &array_ref(), None);
        assert!(
            matches!(result, Expr::BinaryOp { .. }),
            "Expected BinaryOp, got {:?}",
            result
        );
        if let Expr::BinaryOp { left, right, .. } = result {
            assert!(matches!(*left, Expr::Call { ref function, .. } if function == "lastindex"));
            assert!(matches!(*right, Expr::Literal(Literal::Int(1), _)));
        }
    }

    #[test]
    fn test_replace_end_non_end_var_passes_through() {
        // Var("x") is not "end" → unchanged
        let x = var("x");
        let result = replace_end_with_lastindex(x, &array_ref(), None);
        assert!(
            matches!(&result, Expr::Var(name, _) if name == "x"),
            "Expected Var(x), got {:?}",
            result
        );
    }

    #[test]
    fn test_replace_end_literal_passes_through() {
        let lit = lit_int(42);
        let result = replace_end_with_lastindex(lit, &array_ref(), None);
        assert!(matches!(result, Expr::Literal(Literal::Int(42), _)));
    }

    // ── replace_begin_with_firstindex ─────────────────────────────────────────

    #[test]
    fn test_replace_begin_becomes_firstindex_no_dim() {
        let result = replace_begin_with_firstindex(var("begin"), &array_ref(), None);
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "firstindex" && args.len() == 1),
            "Expected firstindex(arr), got {:?}",
            result
        );
    }

    #[test]
    fn test_replace_begin_with_dim_becomes_firstindex_with_dim() {
        let result = replace_begin_with_firstindex(var("begin"), &array_ref(), Some(1));
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "firstindex" && args.len() == 2),
            "Expected firstindex(arr, 1), got {:?}",
            result
        );
    }

    #[test]
    fn test_replace_begin_in_binary_op() {
        // begin + 1 → firstindex(arr) + 1
        let expr = Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(var("begin")),
            right: Box::new(lit_int(1)),
            span: s(),
        };
        let result = replace_begin_with_firstindex(expr, &array_ref(), None);
        assert!(
            matches!(result, Expr::BinaryOp { .. }),
            "Expected BinaryOp, got {:?}",
            result
        );
        if let Expr::BinaryOp { left, right, .. } = result {
            assert!(matches!(*left, Expr::Call { ref function, .. } if function == "firstindex"));
            assert!(matches!(*right, Expr::Literal(Literal::Int(1), _)));
        }
    }

    #[test]
    fn test_replace_begin_non_begin_var_passes_through() {
        let x = var("y");
        let result = replace_begin_with_firstindex(x, &array_ref(), None);
        assert!(
            matches!(&result, Expr::Var(name, _) if name == "y"),
            "Expected Var(y), got {:?}",
            result
        );
    }
}
