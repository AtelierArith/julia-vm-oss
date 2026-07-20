//! Collection expression lowering.
//!
//! This module handles lowering of vectors, matrices, ranges,
//! index expressions, and comprehensions.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{encode_tuple_comprehension_binding, Expr};
use crate::lowering::{internal_lowering_error, LambdaContext, LowerResult};
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
    let named = walker.named_children_vec(&node);

    // Empty array [] is now supported - creates empty Any array
    if named.is_empty() {
        return Ok(Expr::ArrayLiteral {
            elements: vec![],
            shape: vec![0],
            span,
        });
    }

    // Degenerate all-semicolon literals (`[;]`, `[;;]`, `[;;;]`, ...)
    // contain no elements but parse as one `Semicolon` leaf per separator.
    // Upstream treats them as empty Array{Any,N}, where N is the semicolon
    // count (Issue #10379).
    if named.iter().all(|n| walker.kind(n) == NodeKind::Semicolon) {
        return Ok(Expr::ArrayLiteral {
            elements: vec![],
            shape: vec![0; named.len()],
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
        let typed_children = walker.named_children_vec(&named[0]);
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
            function: "_array_splat_literal".to_string().into(),
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
            let inner = walker.named_children(child).next().ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression("splat_expression".to_string()),
                    walker.span(child),
                )
            })?;
            args.push(lower_expr_maybe_ctx(walker, inner, lambda_ctx)?);
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
        function: function.to_string().into(),
        args,
        kwargs: vec![],
        splat_mask,
        kwargs_splat_mask: vec![],
        span,
    }
}

/// A single block of an (N-dimensional) array literal: `elements` are stored
/// in Julia's column-major order for `shape` (dimension 1 varies fastest).
/// Used by [`nd_cat`]/[`nd_fold`] to build the final literal shape from the
/// `;`/`;;`/`;;;`/... dimension-separated rows (Issue #10190).
#[derive(Debug)]
struct NDBlock {
    shape: Vec<usize>,
    elements: Vec<Expr>,
}

/// A single space-separated row (`1 2 3`) is always a rank-2 `(1, k)` block:
/// space is Julia's dimension-2 separator, so `[1 2 3]` alone is a `Matrix`
/// (size `(1, 3)`), not a `Vector` — mirrors the existing single-row fast
/// path a few lines below.
fn nd_block_from_row(elements: Vec<Expr>) -> NDBlock {
    let cols = elements.len();
    NDBlock {
        shape: vec![1, cols],
        elements,
    }
}

/// Right-pad `shape` with size-1 dimensions up to `ndims`. Column-major
/// linear indexing is unaffected by trailing size-1 dimensions, so this is
/// always a free reinterpretation (never reorders `elements`).
fn nd_pad_shape(shape: &[usize], ndims: usize) -> Vec<usize> {
    let mut padded = shape.to_vec();
    padded.resize(ndims, 1);
    padded
}

/// Column-major (Fortran-order) strides for `shape`: dimension 0 has stride
/// 1, and each subsequent dimension's stride is the previous dimension's
/// stride times its size.
fn nd_strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = vec![1usize; shape.len()];
    for i in 1..shape.len() {
        strides[i] = strides[i - 1] * shape[i - 1];
    }
    strides
}

/// Concatenate `blocks` along the 1-indexed dimension `dim`, mirroring
/// upstream Julia's array-literal semantics for `N` consecutive semicolons
/// (`cat(blocks...; dims = N)`): every dimension other than `dim` must agree
/// across all blocks (upstream raises `DimensionMismatch` for this at
/// runtime; this literal-shape fast path can only ever concatenate
/// compile-time-known scalar shapes, so it is reported as a compile error
/// instead), and the result's `dim` size is the sum of each block's `dim`
/// size.
fn nd_cat(dim: usize, blocks: Vec<NDBlock>, span: crate::span::Span) -> LowerResult<NDBlock> {
    debug_assert!(dim >= 1);
    if blocks.len() == 1 {
        return blocks
            .into_iter()
            .next()
            .ok_or_else(|| internal_lowering_error(span, "nd_cat: blocks length checked above"));
    }

    let target_ndims = blocks
        .iter()
        .map(|b| b.shape.len())
        .max()
        .unwrap_or(0)
        .max(dim);
    let d0 = dim - 1;
    let padded_shapes: Vec<Vec<usize>> = blocks
        .iter()
        .map(|b| nd_pad_shape(&b.shape, target_ndims))
        .collect();

    for j in 0..target_ndims {
        if j == d0 {
            continue;
        }
        let expected = padded_shapes[0][j];
        for shape in &padded_shapes[1..] {
            if shape[j] != expected {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MalformedMatrix, span)
                        .with_hint(format!(
                            "mismatched shape along dimension {}: expected {}, got {}",
                            j + 1,
                            expected,
                            shape[j]
                        )),
                );
            }
        }
    }

    let mut new_shape = padded_shapes[0].clone();
    new_shape[d0] = padded_shapes.iter().map(|s| s[d0]).sum();
    let new_strides = nd_strides(&new_shape);
    let block_strides: Vec<Vec<usize>> = padded_shapes.iter().map(|s| nd_strides(s)).collect();

    // Each block's extent along `d0`, laid out back to back in the merged
    // dimension (block 0 first, then block 1, ...).
    let mut block_offsets = Vec::with_capacity(blocks.len());
    let mut acc = 0usize;
    for shape in &padded_shapes {
        block_offsets.push(acc);
        acc += shape[d0];
    }

    let total: usize = new_shape.iter().product();
    let mut elements = Vec::with_capacity(total);
    let mut multi = vec![0usize; target_ndims];
    for lin in 0..total {
        // Decompose the column-major linear index into a multi-index.
        let mut rem = lin;
        for k in (0..target_ndims).rev() {
            multi[k] = rem / new_strides[k];
            rem %= new_strides[k];
        }

        // Which block owns `coord` along the merged dimension: binary search
        // over the sorted block-start offsets (`partition_point` finds the
        // first offset strictly greater than `coord`, so the owning block is
        // the one just before it) rather than a linear scan, so a literal
        // with many rows/blocks at one separator level doesn't cost
        // `O(total_elements * num_blocks)`.
        let coord = multi[d0];
        let block_idx = block_offsets.partition_point(|&off| off <= coord) - 1;
        let local_coord = coord - block_offsets[block_idx];

        let bstrides = &block_strides[block_idx];
        let mut local_lin = 0usize;
        for k in 0..target_ndims {
            let mk = if k == d0 { local_coord } else { multi[k] };
            local_lin += mk * bstrides[k];
        }
        elements.push(blocks[block_idx].elements[local_lin].clone());
    }

    Ok(NDBlock {
        shape: new_shape,
        elements,
    })
}

/// Fold a flat sequence of row-blocks and the `;`/`;;`/`;;;`/... separator
/// level between each adjacent pair into the array literal's final
/// N-dimensional shape and column-major element order.
///
/// Mirrors upstream Julia's array-literal dimension rule (`N` consecutive
/// semicolons concatenate along dimension `N`; `julia/base/abstractarray.jl`
/// `hvncat`): blocks are merged strictly by increasing separator level, so
/// each pass concatenates along what is, at that point, the outermost
/// dimension built so far — this holds even for the very first (level-1)
/// pass, since every row starts as a rank-2 `(1, k)` block (space is
/// dimension 2) and `;` is dimension 1 (Issue #10190).
fn nd_fold(
    blocks: Vec<NDBlock>,
    levels: Vec<usize>,
    span: crate::span::Span,
) -> LowerResult<NDBlock> {
    debug_assert_eq!(levels.len() + 1, blocks.len());
    let mut cur_blocks = blocks;
    let mut cur_levels = levels;
    let mut level = 1usize;
    while cur_blocks.len() > 1 {
        if !cur_levels.contains(&level) {
            level += 1;
            continue;
        }

        let mut next_blocks: Vec<NDBlock> = Vec::new();
        let mut next_levels: Vec<usize> = Vec::new();
        let mut blocks_iter = cur_blocks.into_iter();
        let mut run = vec![blocks_iter.next().ok_or_else(|| {
            internal_lowering_error(
                span,
                "nd_fold: cur_blocks length tracks cur_levels length + 1",
            )
        })?];
        for lv in cur_levels {
            let next_block = blocks_iter.next().ok_or_else(|| {
                internal_lowering_error(
                    span,
                    "nd_fold: cur_blocks length tracks cur_levels length + 1",
                )
            })?;
            if lv == level {
                run.push(next_block);
            } else {
                next_blocks.push(nd_cat(level, std::mem::take(&mut run), span)?);
                next_levels.push(lv);
                run.push(next_block);
            }
        }
        next_blocks.push(nd_cat(level, run, span)?);

        cur_blocks = next_blocks;
        cur_levels = next_levels;
        level += 1;
    }
    cur_blocks
        .into_iter()
        .next()
        .ok_or_else(|| internal_lowering_error(span, "nd_fold requires at least one block"))
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

pub fn lower_matrix_expr_raw_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_matrix_expr_impl(walker, node, Some(lambda_ctx), false)
}

fn lower_matrix_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
    concat_route: bool,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::EmptyArray,
            span,
        ));
    }

    // Typed all-semicolon literals (`T[;]`, `T[;;]`, ...) reach the matrix
    // literal path. They contain no elements but carry their rank in the
    // separator count, matching the untyped `VectorExpression` case above
    // (Issue #10379).
    if named.iter().all(|n| walker.kind(n) == NodeKind::Semicolon) {
        return Ok(Expr::ArrayLiteral {
            elements: vec![],
            shape: vec![0; named.len()],
            span,
        });
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
    // The separator level (semicolon count) between each pair of adjacent
    // rows, in document order — `N` semicolons is Julia's dimension-`N`
    // separator (`;` = dim 1, `;;` = dim 2, `;;;` = dim 3, ...); a run made
    // up only of newlines is level 1, same as a single `;`. See `nd_fold`
    // (Issue #10190).
    //
    // A *trailing* separator run with no following row (syntactically
    // accepted since Issue #8759, e.g. `[1 2;;;]`) still contributes to the
    // literal's rank in upstream Julia — it pads the shape with trailing
    // size-1 dimensions rather than being dropped (`size([1 2;;;])` is
    // `(1, 2, 1)`, not `(1, 2)`) — so its level is captured separately as
    // `trailing_level` and applied as a final padding step below
    // (Issue #10378).
    let mut sep_levels: Vec<usize> = Vec::with_capacity(rows.len().saturating_sub(1));
    let trailing_level: usize;
    {
        let mut pending_semis = 0usize;
        let mut seen_first_row = false;
        for child in &named {
            match walker.kind(child) {
                NodeKind::Semicolon => pending_semis += 1,
                NodeKind::MatrixRow => {
                    if seen_first_row {
                        sep_levels.push(pending_semis.max(1));
                    }
                    pending_semis = 0;
                    seen_first_row = true;
                }
                _ => {}
            }
        }
        trailing_level = pending_semis;
    }

    let mut row_elements: Vec<Vec<Expr>> = Vec::new();
    let mut all_scalar = true;

    for row_node in &rows {
        let this_row_elements = walker.named_children_vec(row_node);
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
        // A single-`;` chain with one scalar per row (`[1; 2; 3]`) is
        // lowered by upstream as `vcat` of scalars and produces a rank-1
        // vector. Keep `;;`/`;;;`/space-separated rows on the N-D literal
        // fold path, where they are genuine higher-rank arrays (Issue #10380).
        if !row_elements.is_empty()
            && row_elements.iter().all(|row| row.len() == 1)
            && sep_levels.iter().all(|&level| level == 1)
            && trailing_level == 0
        {
            let elements: Vec<Expr> = row_elements
                .into_iter()
                .map(|mut row| row.remove(0))
                .collect();
            return Ok(Expr::ArrayLiteral {
                shape: vec![elements.len()],
                elements,
                span,
            });
        }

        // All-scalar fast path (or typed-matrix context that needs the flat
        // element list): fold the rows into their final N-dimensional shape
        // via the `;`/`;;`/`;;;`/... separator levels recorded above. This
        // also validates shape consistency at every level (mismatched column
        // counts within one `;`-group, mismatched block shapes across a
        // `;;`/`;;;`/... boundary), superseding the old uniform-column-count
        // check (Issue #10190).
        let blocks: Vec<NDBlock> = row_elements
            .iter()
            .cloned()
            .map(nd_block_from_row)
            .collect();
        let merged = match nd_fold(blocks, sep_levels.clone(), span) {
            Ok(merged) => Some(merged),
            // Julia parses unequal row widths and lets `hvcat` raise the
            // catchable runtime ArgumentError. Preserve that behavior instead
            // of aborting lowering for an otherwise valid literal (Issue #10519).
            Err(err) if concat_route && err.kind == UnsupportedFeatureKind::MalformedMatrix => None,
            Err(err) => return Err(err),
        };
        if let Some(merged) = merged {
            // A dangling trailing separator (Issue #10378) still pads the shape
            // with trailing size-1 dimensions; padding never reorders `elements`
            // (see `nd_pad_shape`).
            let shape = nd_pad_shape(&merged.shape, trailing_level.max(merged.shape.len()));

            return Ok(Expr::ArrayLiteral {
                elements: merged.elements,
                shape,
                span,
            });
        }
        // A malformed all-scalar 2-D shape falls through to the ordinary
        // runtime `hvcat` construction below so it raises catchably.
    }

    // N-dimensional separators (`;;`/`;;;`/...) with array-valued blocks
    // route through the pure-Julia `hvncat` ragged shape form
    // (Issue #10381): `hvncat(shape, row_first, elements...)` with
    // `shape[d]` listing the cumulative element count of each dimension-`d`
    // group. Upstream lowers balanced literals to the dims form and ragged
    // ones to this shape form; the shape form is the general algorithm and
    // produces identical arrays for balanced input, so sjulia emits it
    // uniformly. `row_first` is whether the literal has space-separated
    // (dimension-2-first) rows.
    let max_level = sep_levels
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .max(trailing_level);
    if max_level >= 2 {
        let has_spaces = row_elements.iter().any(|row| row.len() > 1);
        let counts: Vec<usize> = row_elements.iter().map(|row| row.len()).collect();
        let total: usize = counts.iter().sum();
        // Cumulative element counts of the groups delimited by separators of
        // level >= `threshold`.
        let groups = |threshold: usize| -> Vec<usize> {
            let mut out = Vec::new();
            let mut acc = 0usize;
            for (i, c) in counts.iter().enumerate() {
                acc += c;
                if i < sep_levels.len() && sep_levels[i] >= threshold {
                    out.push(acc);
                    acc = 0;
                }
            }
            out.push(acc);
            out
        };
        let rank = max_level.max(2);
        let mut levels: Vec<Vec<usize>> = Vec::with_capacity(rank);
        if has_spaces {
            // Space-separated rows: level 1 is the per-row element count.
            levels.push(counts.clone());
        } else {
            // Pure-semicolon form: level 1 groups are delimited by `;;`+.
            levels.push(groups(2));
        }
        for d in 2..rank {
            levels.push(groups(d + 1));
        }
        levels.push(vec![total]);

        let shape_tuple = Expr::TupleLiteral {
            elements: levels
                .into_iter()
                .map(|level| Expr::TupleLiteral {
                    elements: level
                        .into_iter()
                        .map(|c| Expr::Literal(crate::ir::core::Literal::Int(c as i64), span))
                        .collect(),
                    span,
                })
                .collect(),
            span,
        };

        let mut args = Vec::with_capacity(2 + total);
        args.push(shape_tuple);
        args.push(Expr::Literal(
            crate::ir::core::Literal::Bool(has_spaces),
            span,
        ));
        for row in row_elements {
            for elem in row {
                args.push(elem);
            }
        }
        return Ok(concat_call("hvncat", args, span));
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
    let named = walker.named_children_vec(&node);

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
            function: "_array_splat_literal_typed".to_string().into(),
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
            Expr::Var(name, _) => Some(name.to_string()),
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
            // `Dict{String,Int}`, ...) lower to a DataType literal; recover the
            // declared element-type name so `T[]` keeps its eltype (Issue #6768).
            Expr::Literal(crate::ir::core::Literal::DataType(name), _) => Some(name.clone()),
            // Legacy cached/lowered shape, kept for compatibility.
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
                    element_type: name.into(),
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
/// a static parametric type that lowers to a DataType literal (`Complex{Float64}`,
/// `Vector{Int}`), and a dynamic parametric type. Used to route a typed array
/// literal that contains a positional splat (`T[a, xs..., b]`) to the typed
/// array-builder helper (Issue #7255).
fn is_typed_array_constructor_target(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Literal(crate::ir::core::Literal::DataType(_), _)
            | Expr::Builtin {
                name: crate::ir::core::BuiltinOp::TypeOf,
                ..
            }
            | Expr::DynamicTypeConstruct { .. }
    ) || is_type_like_index_target(expr)
}

/// Lower range expression: 1:10 or 1:2:10
/// Tree-sitter parses `1:2:10` as nested: (1:2):10
/// We need to flatten this to start=1, step=2, stop=10
pub fn lower_range_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    lower_range_expr_impl(walker, node, None)
}

pub fn lower_range_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_range_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_range_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

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
            let inner = walker.named_children_vec(&operands[0]);
            let inner_operands: Vec<_> = inner
                .into_iter()
                .filter(|n| walker.kind(n) != NodeKind::Operator)
                .collect();

            if inner_operands.len() == 2 {
                let start = lower_expr_maybe_ctx(walker, inner_operands[0], lambda_ctx)?;
                let step = lower_expr_maybe_ctx(walker, inner_operands[1], lambda_ctx)?;
                let stop = lower_expr_maybe_ctx(walker, operands[1], lambda_ctx)?;
                return Ok(Expr::Range {
                    start: Box::new(start),
                    step: Some(Box::new(step)),
                    stop: Box::new(stop),
                    span,
                });
            }
        }

        // Simple range: start:stop
        let start = lower_expr_maybe_ctx(walker, operands[0], lambda_ctx)?;
        let stop = lower_expr_maybe_ctx(walker, operands[1], lambda_ctx)?;
        Ok(Expr::Range {
            start: Box::new(start),
            step: None,
            stop: Box::new(stop),
            span,
        })
    } else if operands.len() == 3 {
        // start:step:stop (direct case, if tree-sitter ever produces this)
        let start = lower_expr_maybe_ctx(walker, operands[0], lambda_ctx)?;
        let step = lower_expr_maybe_ctx(walker, operands[1], lambda_ctx)?;
        let stop = lower_expr_maybe_ctx(walker, operands[2], lambda_ctx)?;
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
    let named = walker.named_children_vec(&node);
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
    let named = walker.named_children_vec(&node);
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
        NodeKind::ArgumentList => walker.named_children_vec(&node),
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
                function: "lastindex".to_string().into(),
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
                function: "firstindex".to_string().into(),
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
    lower_comprehension_expr_impl(walker, node, None)
}

pub fn lower_comprehension_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_comprehension_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_comprehension_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

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
    let body = lower_expr_maybe_ctx(walker, body_node, lambda_ctx)?;

    // Parse optional filter
    let filter = if let Some(if_node) = if_clause {
        Some(Box::new(parse_if_clause(walker, if_node, lambda_ctx)?))
    } else {
        None
    };

    // Collect bindings grouped by ForClause. The parser packs multiple
    // comma-separated bindings (e.g. `for i in R, j in R`) into a SINGLE
    // ForClause with multiple ForBinding children, while the whitespace
    // `for i in R for j in R` flatten form produces MULTIPLE ForClauses each
    // with one binding. This grouping is exactly the comma-vs-whitespace
    // distinction the eval path needs (Issue #8014).
    let mut clause_bindings: Vec<Vec<(crate::ir::core::InternedStr, Expr)>> =
        Vec::with_capacity(for_clauses.len());
    for fc in &for_clauses {
        clause_bindings.push(parse_for_clause_bindings(walker, *fc, lambda_ctx)?);
    }

    // More than one `for` clause ⇒ whitespace flatten form: 1-D Vector with
    // `Iterators.flatten` semantics (Issue #8014). A single clause (possibly
    // with comma-separated bindings) is the cartesian / multidimensional form.
    let is_flatten = for_clauses.len() > 1;

    if !is_flatten {
        let mut all_bindings: Vec<(crate::ir::core::InternedStr, Expr)> =
            clause_bindings.into_iter().flatten().collect();

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
    let mut iterations: Vec<(crate::ir::core::InternedStr, Expr)> = Vec::with_capacity(total);
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
    lower_generator_expr_impl(walker, node, None)
}

pub fn lower_generator_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_generator_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_generator_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty generator"),
        );
    }

    let mut body_expr = None;
    let mut for_clauses: Vec<Node<'a>> = Vec::new();
    let mut if_clause = None;

    for child in &named {
        match walker.kind(child) {
            NodeKind::ForClause => for_clauses.push(*child),
            NodeKind::IfClause => if_clause = Some(*child),
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

    let body = lower_expr_maybe_ctx(walker, body_node, lambda_ctx)?;
    let filter = if let Some(if_node) = if_clause {
        Some(Box::new(parse_if_clause(walker, if_node, lambda_ctx)?))
    } else {
        None
    };

    // Issue #9200 (S4b): a whitespace `for ... for ...` flatten generator parses
    // to MULTIPLE ForClauses. Desugar it to upstream's `Iterators.flatten` over
    // nested `Base.Generator`s (`julia-syntax.scm` `expand-generator` with
    // `flat=#t`): `flatten(Generator(x -> Generator(y -> body, b), a))`. The `if`
    // filter (parsed once, textually after the innermost `for`) wraps the
    // innermost iterator in an `Iterators.Filter`, so a flatten-with-filter
    // composes the S3 filtered-generator collapse per nesting level (Issue #9325).
    if for_clauses.len() > 1 {
        let mut clauses: Vec<Vec<(crate::ir::core::InternedStr, Expr)>> =
            Vec::with_capacity(for_clauses.len());
        for fc in &for_clauses {
            clauses.push(parse_for_clause_bindings(walker, *fc, lambda_ctx)?);
        }
        return Ok(desugar_flatten_generator(
            body, clauses, filter, span, lambda_ctx,
        ));
    }

    // A single ForClause. Its ForBinding count distinguishes the SIMPLE / FILTERED
    // scalar form (S2/S3) from the comma cartesian PRODUCT form (S4a).
    let for_node = for_clauses[0];
    let bindings = parse_for_clause_bindings(walker, for_node, lambda_ctx)?;

    // Issue #9200 (S4a): a comma multi-binding `for x in a, y in b` parses to one
    // ForClause with several ForBindings — the cartesian PRODUCT form. Desugar to
    // `Base.Generator(func, Iterators.product(a, b))` where `func` maps the
    // product's destructured tuple (`julia-syntax.scm`
    // `func-for-generator-ranges` for multiple ranges), plus an optional
    // `Iterators.Filter` wrapping the product when the generator has an `if`.
    if bindings.len() > 1 {
        return Ok(desugar_product_generator(
            body, bindings, filter, span, lambda_ctx,
        ));
    }

    // Single scalar (or tuple-destructuring) binding: the S2/S3 path.
    let Some((var_name, iter_expr)) = bindings.into_iter().next() else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("missing for clause binding"),
        );
    };

    // Issue #9103 / #9127: an arbitrary generator body (and, when a filter is
    // present, its predicate) must stay LAZY. The compiler's generator fast
    // paths only cover a plain `f(var)` body (map) and a plain
    // `f(var)`/`p(var)` map+filter pair; every other shape used to fall back to
    // an eager comprehension wrapped in a Generator, firing side effects at
    // construction time instead of at iteration time. Lift such bodies /
    // predicates into nested `__gen_body_N` / `__gen_pred_N` functions here (the
    // same nested-FunctionDef-in-LetBlock shape as arrow lambda lowering), so
    // the generator compiles onto the existing lazy runtime-callable / filtered
    // paths. Tuple-destructuring bindings (`(a, b) in pairs`, encoded as a
    // `__tuple_comprehension_binding__:` pseudo-variable) get a destructuring
    // prologue injected into the synthetic functions so they lift too.
    let tuple_vars = crate::ir::core::decode_tuple_comprehension_binding(&var_name);

    // Issue #9200 (S2/S3): desugar the single-scalar-binding generator form to
    // the upstream `expand-generator` call shape (julia-syntax.scm, single scalar
    // range, no splat). A single tuple-destructuring binding stays on the
    // existing MakeGenerator lift path below.
    if tuple_vars.is_none() {
        return Ok(match filter {
            // S2: `(f(x) for x in iter)` -> `Base.Generator(func, iter)`.
            None => {
                desugar_simple_generator(body, var_name.to_string(), iter_expr, span, lambda_ctx)
            }
            // S3: `(f(x) for x in iter if p(x))` ->
            // `Base.Generator(func, Base.Iterators.Filter(pred, iter))`.
            Some(filter_expr) => desugar_filtered_generator(
                body,
                var_name.to_string(),
                iter_expr,
                *filter_expr,
                span,
                lambda_ctx,
            ),
        });
    }

    if generator_needs_lift(&body, filter.as_deref(), &var_name, tuple_vars.is_some()) {
        return Ok(lift_generator_as_nested(
            body,
            var_name.to_string(),
            iter_expr,
            filter,
            tuple_vars,
            span,
            lambda_ctx,
        ));
    }

    // Return Generator (lazy evaluation)
    Ok(Expr::Generator {
        body: Box::new(body),
        var: var_name,
        iter: Box::new(iter_expr),
        filter,
        span,
    })
}

/// Desugar the SIMPLE generator form `(body for var in iter)` — single scalar
/// binding, unfiltered — into the upstream `Base.Generator(func, iter)` call
/// shape (Issue #9200 S2).
///
/// Mirrors `julia-syntax.scm`'s `func-for-generator-ranges` for a single scalar
/// range with no destructuring splat:
///
/// * `body === var`  =>  `Generator(identity, iter)` (upstream `(top identity)`);
/// * otherwise       =>  `let __gen_body_N(var) = body; Generator(__gen_body_N, iter) end`.
///
/// The second form is sjulia's representation of upstream's anonymous function
/// `var -> body`: sjulia lifts every value-position lambda into a nested
/// `FunctionDef` (discovered by `collect_stmt_functions` for closure analysis)
/// and refers to it by value, so `Generator(__gen_body_N, iter)` is `Generator(var
/// -> body, iter)`. The `__gen_body_N` prefix is shared with `lift_generator_as_nested`
/// so the lifted body reuses the existing generator-body handling — top-level /
/// `@testset` capture analysis (Issue #9250), cache anonymous-def detection, and
/// the AoT lift reversal.
///
/// The `Generator(...)` call is routed through the compiler's `BuiltinOp::Generator`
/// bridge (`try_compile_generator_bridge_call`), which builds the same native
/// `Value::Generator` the old `Expr::Generator` node produced — so every consumer
/// (`collect` / `sum` / `for` / `first` / `zip` / …) and the S1 iterator-trait
/// behavior are unchanged. S5/S6 will retire that interception in favour of a
/// genuine `Base.Generator` struct construction driven by the pure-Julia iterate
/// protocol. The AoT backend reverses this shape back to an inline `Expr::Generator`
/// in `crate::aot::analyze::lift_reversal` (before inference / IR conversion).
fn generator_body_name(
    lambda_ctx: Option<&LambdaContext>,
    source_start: usize,
    level: Option<usize>,
) -> String {
    lambda_ctx.map_or_else(
        || {
            let suffix = level.map_or_else(String::new, |level| format!("_{level}"));
            format!("__gen_body_{source_start}{suffix}")
        },
        |context| context.generator_body_name(source_start, level),
    )
}

fn generator_predicate_name(lambda_ctx: Option<&LambdaContext>, source_start: usize) -> String {
    lambda_ctx.map_or_else(
        || format!("__gen_pred_{source_start}"),
        |context| context.generator_predicate_name(source_start),
    )
}

fn desugar_simple_generator(
    body: Expr,
    var_name: String,
    iter_expr: Expr,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> Expr {
    use crate::ir::core::{Block, Function, Stmt, TypedParam};

    // Identity: the body is exactly the loop variable (upstream `(top identity)`).
    if matches!(&body, Expr::Var(v, _) if *v == var_name) {
        return make_generator_call(
            Expr::Var("identity".to_string().into(), span),
            iter_expr,
            span,
        );
    }

    // General: lift `body` into a nested single-parameter function whose parameter
    // is the loop variable, then pass it by value to `Generator`. The `__gen_body_`
    // prefix (shared with `lift_generator_as_nested`) reuses the existing lifted
    // generator-body handling (top-level capture analysis, cache, AoT reversal).
    let func_name = generator_body_name(lambda_ctx, span.start, None);
    let func = Function {
        name: func_name.clone(),
        params: vec![TypedParam::untyped(var_name, span)],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper();
    let gen_call = make_generator_call(Expr::Var(func_name.into(), span), iter_expr, span);

    Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::FunctionDef {
                    func: Box::new(func),
                    span,
                },
                Stmt::Expr {
                    expr: gen_call,
                    span,
                },
            ],
            span,
        },
        span,
    }
}

/// Build an unqualified `Generator(func, iter)` call. The unqualified name routes
/// to the VM-native `Base.Generator` boundary via `try_compile_generator_bridge_call`
/// exactly like Base's own internal `Generator(...)` references (Issue #9200 S2).
fn make_generator_call(func: Expr, iter: Expr, span: crate::span::Span) -> Expr {
    Expr::Call {
        function: "Generator".to_string().into(),
        args: vec![func, iter],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    }
}

/// Build an unqualified `Filter(pred, iter)` call — the upstream
/// `Base.Iterators.Filter` (`src/julia/base/iterators.jl`) wrapping `iter` with the
/// predicate `pred` (Issue #9200 S3). At the lowering level this is the upstream
/// `expand-generator` shape; the compiler's `Generator(...)` interception
/// recognizes this `Filter(pred, iter)` second argument and collapses the pair back
/// onto the native lazy filtered-generator representation
/// (`try_compile_generator_over_filter`).
fn make_filter_call(pred: Expr, iter: Expr, span: crate::span::Span) -> Expr {
    Expr::Call {
        function: "Filter".to_string().into(),
        args: vec![pred, iter],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    }
}

/// Build a single-scalar-parameter lifted function `name(param) = <expr>` whose
/// body is a single trailing `return <expr>`. Shared by the S2/S3 simple-generator
/// desugars for the `__gen_body_N` / `__gen_pred_N` lifted closures (Issue #9200).
fn make_lifted_unary_function(
    name: String,
    param: String,
    body: Expr,
    span: crate::span::Span,
) -> crate::ir::core::Function {
    use crate::ir::core::{Block, Function, Stmt, TypedParam};
    Function {
        name,
        params: vec![TypedParam::untyped(param, span)],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper()
}

/// Desugar the FILTERED single-scalar-binding generator form
/// `(body for var in iter if pred)` into the upstream
/// `Base.Generator(func, Base.Iterators.Filter(pred, iter))` call shape
/// (Issue #9200 S3), mirroring `julia-syntax.scm`'s `expand-generator`:
///
/// ```text
/// let
///     __gen_pred_N(var) = pred          # always lifted (upstream `x -> pred`)
///     __gen_body_N(var) = body          # omitted when `body === var` (identity)
///     Generator(__gen_body_N, Filter(__gen_pred_N, iter))
/// end
/// ```
///
/// The predicate and (non-identity) body are lifted into nested
/// single-parameter functions and passed **by value** to `Filter` / `Generator`
/// — sjulia's representation of upstream's anonymous `var -> pred` / `var -> body`.
/// The `__gen_pred_` / `__gen_body_` prefixes are shared with
/// `lift_generator_as_nested`, so the lifted closures reuse the existing
/// generator-lift handling: top-level / `@testset` capture analysis (Issue #9250),
/// cache anonymous-def detection, and AoT lift reversal.
///
/// The compiler's `BuiltinOp::Generator` interception recognizes the
/// `Generator(map, Filter(pred, iter))` shape and COLLAPSES it back onto the
/// native lazy filtered-generator representation
/// (`try_compile_generator_over_filter` -> `FilteredFunctionIndex` /
/// `MakeGeneratorRuntimeFiltered`, #9127 / #9271), rather than building a
/// `Generator` over a real (eagerly-materialized) `Filter` value — the VM's
/// synchronous consumers cannot drive a lazy `Filter`'s predicate. Only the
/// LOWERED shape changed to upstream's; laziness (side-effect ordering, error
/// timing) and every consumer stay identical to the pre-desugar path. Because a
/// filtered generator is reported with `typeof(g)`'s iterator parameter spelled
/// `Iterators.Filter`, `length` / `size` / `IteratorSize(typeof(g))` become a
/// MethodError / `SizeUnknown()` (`IteratorSize(::Type{<:Filter}) ==
/// SizeUnknown()`). S5/S6 will retire the collapse in favour of a genuine lazy
/// `Base.Generator` over a real `Iterators.Filter` driven purely by the
/// pure-Julia iterate protocol. The AoT backend reverses this shape back to an
/// inline filtered `Expr::Generator` in `crate::aot::analyze::lift_reversal`
/// (before inference / IR conversion).
fn desugar_filtered_generator(
    body: Expr,
    var_name: String,
    iter_expr: Expr,
    filter_expr: Expr,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> Expr {
    use crate::ir::core::{Block, Stmt};

    // Always lift the predicate into `__gen_pred_N(var) = pred` and pass it by
    // value to `Filter` (upstream's `x -> pred`). This preserves the
    // `__gen_pred_` naming that capture analysis + AoT reversal recognize.
    let pred_name = generator_predicate_name(lambda_ctx, span.start);
    let pred_func =
        make_lifted_unary_function(pred_name.clone(), var_name.clone(), filter_expr, span);
    let mut stmts = vec![Stmt::FunctionDef {
        func: Box::new(pred_func),
        span,
    }];
    let filter_call = make_filter_call(Expr::Var(pred_name.into(), span), iter_expr, span);

    // Map: `identity` when the body is exactly the loop variable (upstream
    // `(top identity)`), otherwise a lifted `__gen_body_N(var) = body` passed by
    // value (upstream's `x -> body`).
    let map_expr = if matches!(&body, Expr::Var(v, _) if *v == var_name) {
        Expr::Var("identity".to_string().into(), span)
    } else {
        let body_name = generator_body_name(lambda_ctx, span.start, None);
        let body_func = make_lifted_unary_function(body_name.clone(), var_name.clone(), body, span);
        stmts.push(Stmt::FunctionDef {
            func: Box::new(body_func),
            span,
        });
        Expr::Var(body_name.into(), span)
    };

    let gen_call = make_generator_call(map_expr, filter_call, span);
    stmts.push(Stmt::Expr {
        expr: gen_call,
        span,
    });

    Expr::LetBlock {
        bindings: vec![],
        body: Block { stmts, span },
        span,
    }
}

/// Build an unqualified `product(iters...)` call — the upstream
/// `Base.Iterators.product` (`src/julia/base/iterators.jl`) whose iteration
/// yields the cartesian-product tuples of `iters`, column-major (first iterator
/// changes fastest). Used by the S4 product desugar (Issue #9200). The unqualified
/// name routes to Base exactly like the S2/S3 desugar's `Generator` / `Filter`
/// (upstream `(top product)`). The resulting `ProductIterator` / `Product` value
/// carries a multi-dimensional shape, so a `Base.Generator` over it collects to a
/// `Matrix` (the VM recovers the shape in `collect_iterator`).
fn make_product_call(iters: Vec<Expr>, span: crate::span::Span) -> Expr {
    let n = iters.len();
    Expr::Call {
        function: "product".to_string().into(),
        args: iters,
        kwargs: vec![],
        splat_mask: vec![false; n],
        kwargs_splat_mask: vec![],
        span,
    }
}

/// Build an unqualified `flatten(iter)` call — the upstream `Base.Iterators.flatten`
/// (`src/julia/base/iterators.jl`) that concatenates the elements of a nested
/// iterable (upstream `(top Flatten)`). Used by the S4 flatten desugar (Issue #9200).
fn make_flatten_call(iter: Expr, span: crate::span::Span) -> Expr {
    Expr::Call {
        function: "flatten".to_string().into(),
        args: vec![iter],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    }
}

/// Build a lifted generator "clause" function for one nesting level's bound
/// variables (Issue #9200 S4). This is the sjulia representation of upstream's
/// anonymous `var -> fn_body` / `(vars...) -> fn_body` inside
/// `func-for-generator-ranges`:
///
/// * a single simple-identifier binding uses the loop variable directly as the
///   parameter (`__gen_body_N(x) = fn_body`), matching the S2/S3 shape;
/// * a comma product (`x, y`) or a tuple-destructuring binding (`(a, b)`) takes a
///   fresh `__gen_arg` parameter and destructures it in a prologue — for a
///   product the parameter is the yielded product tuple, for a lone tuple binding
///   the parameter is the yielded element. Nested tuple bindings inside a product
///   destructure recursively.
///
/// `level_tag` disambiguates the synthetic `__gen_arg` / `__gen_tmp` names across
/// the nesting levels of a flatten generator so they never collide.
fn make_generator_clause_function(
    name: String,
    vars: &[String],
    fn_body: Expr,
    level_tag: usize,
    span: crate::span::Span,
) -> crate::ir::core::Function {
    use crate::ir::core::{
        decode_tuple_comprehension_binding, Block, Function, Literal, Stmt, TypedParam,
    };

    let index_of = |base: &str, idx: usize| Expr::Index {
        array: Box::new(Expr::Var(base.to_string().into(), span)),
        indices: vec![Expr::Literal(Literal::Int(idx as i64), span)],
        span,
    };

    // A single simple identifier binding uses the loop variable directly; a
    // single tuple-destructuring binding and a multi-variable product both
    // need a fresh `__gen_arg` parameter with a destructuring prologue. This
    // matches on `vars` and the decoded binding directly (rather than a
    // `vars.len() == 1 && decode(..).is_none()` boolean re-checked and
    // re-decoded a few lines later) so the "single tuple binding decodes to
    // `Some`" fact is carried by the match arm itself instead of relying on a
    // raw indexing/decode call to agree with an earlier boolean a second time
    // (Issue #10905, Phase 1b of #10869).
    let (param, mut stmts): (String, Vec<Stmt>) = match vars {
        [only] => match decode_tuple_comprehension_binding(only) {
            None => (only.clone(), Vec::new()),
            Some(inner) => {
                // A lone tuple-destructuring binding: the parameter IS the
                // yielded element tuple; bind each inner name to a component.
                let arg = format!("__gen_arg_{}_{}", span.start, level_tag);
                let mut prologue = Vec::new();
                for (j, bound) in inner.iter().enumerate() {
                    prologue.push(Stmt::Assign {
                        var: bound.clone(),
                        value: index_of(&arg, j + 1),
                        span,
                    });
                }
                (arg, prologue)
            }
        },
        _ => {
            // A product: the parameter is the yielded product tuple; bind each
            // component, destructuring nested tuple bindings recursively.
            let arg = format!("__gen_arg_{}_{}", span.start, level_tag);
            let mut prologue = Vec::new();
            for (i, v) in vars.iter().enumerate() {
                if let Some(inner) = decode_tuple_comprehension_binding(v) {
                    let tmp = format!("__gen_tmp_{}_{}_{}", span.start, level_tag, i);
                    prologue.push(Stmt::Assign {
                        var: tmp.clone(),
                        value: index_of(&arg, i + 1),
                        span,
                    });
                    for (j, bound) in inner.iter().enumerate() {
                        prologue.push(Stmt::Assign {
                            var: bound.clone(),
                            value: index_of(&tmp, j + 1),
                            span,
                        });
                    }
                } else {
                    prologue.push(Stmt::Assign {
                        var: v.clone(),
                        value: index_of(&arg, i + 1),
                        span,
                    });
                }
            }
            (arg, prologue)
        }
    };

    stmts.push(Stmt::Return {
        value: Some(fn_body),
        span,
    });

    Function {
        name,
        params: vec![TypedParam::untyped(param, span)],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block { stmts, span },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper()
}

/// Desugar the comma cartesian PRODUCT generator form
/// `(body for x in a, y in b [if pred])` into the upstream
/// `Base.Generator(func, Iterators.product(a, b))` call shape (Issue #9200 S4a),
/// mirroring `julia-syntax.scm`'s `func-for-generator-ranges` for multiple
/// ranges (the mapping function takes the product's destructured tuple):
///
/// ```text
/// let
///     __gen_pred_N(t) = (x = t[1]; y = t[2]; pred)   # only when `if` present
///     __gen_body_N(t) = (x = t[1]; y = t[2]; body)
///     Generator(__gen_body_N, Filter(__gen_pred_N, product(a, b)))  # or no Filter
/// end
/// ```
///
/// Without a filter the `ProductIterator` base carries a 2-D (N-D) shape, so
/// `collect` yields a `Matrix` matching upstream `[body for x in a, y in b]`; the
/// VM recovers the shape in `collect_iterator`. With a filter the compiler's
/// `BuiltinOp::Generator` interception collapses `Generator(map, Filter(pred,
/// product))` onto the native filtered-generator representation (Issue #9200 S3 /
/// #9127 / #9271), which — like upstream `Iterators.Filter` — reports
/// `SizeUnknown()`, so the result is a `Vector`.
fn desugar_product_generator(
    body: Expr,
    bindings: Vec<(crate::ir::core::InternedStr, Expr)>,
    filter: Option<Box<Expr>>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> Expr {
    use crate::ir::core::{Block, Stmt};

    let vars: Vec<String> = bindings.iter().map(|(v, _)| v.to_string()).collect();
    let iters: Vec<Expr> = bindings.into_iter().map(|(_, it)| it).collect();
    let product_call = make_product_call(iters, span);

    let body_name = generator_body_name(lambda_ctx, span.start, None);
    let body_func = make_generator_clause_function(body_name.clone(), &vars, body, 0, span);
    let mut stmts = vec![Stmt::FunctionDef {
        func: Box::new(body_func),
        span,
    }];

    let iter_arg = if let Some(pred) = filter {
        let pred_name = generator_predicate_name(lambda_ctx, span.start);
        let pred_func = make_generator_clause_function(pred_name.clone(), &vars, *pred, 0, span);
        stmts.push(Stmt::FunctionDef {
            func: Box::new(pred_func),
            span,
        });
        make_filter_call(Expr::Var(pred_name.into(), span), product_call, span)
    } else {
        product_call
    };

    let gen_call = make_generator_call(Expr::Var(body_name.into(), span), iter_arg, span);
    stmts.push(Stmt::Expr {
        expr: gen_call,
        span,
    });

    Expr::LetBlock {
        bindings: vec![],
        body: Block { stmts, span },
        span,
    }
}

/// Desugar the whitespace FLATTEN generator form
/// `(body for x in a for y in b [if pred])` into the upstream nested
/// `Iterators.flatten` / `Base.Generator` shape (Issue #9200 S4b), mirroring
/// `julia-syntax.scm`'s `expand-generator` with `flat=#t`:
///
/// ```text
/// flatten(Generator(x -> flatten(Generator(y -> ... Generator(z -> body, c) ..., b)), a))
/// ```
///
/// Each non-innermost `for` clause becomes a `Base.Generator` mapping its loop
/// variable to the inner (already-flattened) generator, wrapped in
/// `Iterators.flatten`; the innermost clause is a simple/filtered/product
/// generator. The `if` filter (parsed once, textually after the innermost `for`)
/// wraps the innermost iterator in an `Iterators.Filter`, so a flatten-with-filter
/// composes the S3 filtered-generator collapse (Issue #9325). A comma clause
/// inside the flatten becomes a product level.
fn desugar_flatten_generator(
    body: Expr,
    clauses: Vec<Vec<(crate::ir::core::InternedStr, Expr)>>,
    filter: Option<Box<Expr>>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> Expr {
    build_flatten_level(&clauses, 0, body, filter, span, lambda_ctx)
}

/// Recursive worker for [`desugar_flatten_generator`]: builds the generator for
/// clause `level` and, when it is not the innermost, wraps the inner generator in
/// `Iterators.flatten(Generator(mapfunc, iter))`.
fn build_flatten_level(
    clauses: &[Vec<(crate::ir::core::InternedStr, Expr)>],
    level: usize,
    body: Expr,
    filter: Option<Box<Expr>>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> Expr {
    use crate::ir::core::{Block, Stmt};

    let bindings = &clauses[level];
    let vars: Vec<String> = bindings.iter().map(|(v, _)| v.to_string()).collect();

    if level + 1 == clauses.len() {
        // Innermost clause: a simple / product generator carrying the `if` filter.
        // A single simple-identifier binding reuses the S2/S3 shape verbatim (with
        // the identity optimization); everything else lifts through the
        // destructuring clause-function builder.
        if vars.len() == 1
            && crate::ir::core::decode_tuple_comprehension_binding(&vars[0]).is_none()
        {
            let (var, iter) = bindings[0].clone();
            return match filter {
                None => desugar_simple_generator(body, var.to_string(), iter, span, lambda_ctx),
                Some(pred) => {
                    desugar_filtered_generator(body, var.to_string(), iter, *pred, span, lambda_ctx)
                }
            };
        }
        return desugar_product_generator(body, bindings.clone(), filter, span, lambda_ctx);
    }

    // Non-innermost clause: map its loop variable(s) to the inner generator, then
    // flatten. `level` tags the lifted map function and its `__gen_arg` names so
    // the nesting levels never collide.
    let inner = build_flatten_level(clauses, level + 1, body, filter, span, lambda_ctx);
    let map_name = generator_body_name(lambda_ctx, span.start, Some(level));
    let map_func = make_generator_clause_function(map_name.clone(), &vars, inner, level, span);
    let level_iter = flatten_level_iter(bindings, span);
    let gen_call = make_generator_call(Expr::Var(map_name.into(), span), level_iter, span);
    let flatten_call = make_flatten_call(gen_call, span);

    Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::FunctionDef {
                    func: Box::new(map_func),
                    span,
                },
                Stmt::Expr {
                    expr: flatten_call,
                    span,
                },
            ],
            span,
        },
        span,
    }
}

/// The iterator a non-innermost flatten clause maps over: the single binding's
/// iterator, or `Iterators.product(...)` for a comma clause inside the flatten.
fn flatten_level_iter(
    bindings: &[(crate::ir::core::InternedStr, Expr)],
    span: crate::span::Span,
) -> Expr {
    if bindings.len() == 1 {
        bindings[0].1.clone()
    } else {
        make_product_call(bindings.iter().map(|(_, it)| it.clone()).collect(), span)
    }
}

/// Is `body` a plain unary call `f(var)` (no kwargs / splats, single positional
/// argument that is exactly the loop variable)? These the compiler already
/// lowers lazily via `MakeGenerator` / `MakeGeneratorRuntime`, so they do not
/// need lifting.
fn is_plain_unary_var_call(body: &Expr, var_name: &str) -> bool {
    if let Expr::Call {
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        ..
    } = body
    {
        kwargs.is_empty()
            && kwargs_splat_mask.iter().all(|&is_splat| !is_splat)
            && splat_mask.iter().all(|&is_splat| !is_splat)
            && args.len() == 1
            && matches!(&args[0], Expr::Var(arg, _) if arg == var_name)
    } else {
        false
    }
}

/// Decide whether a generator expression must be lifted into synthetic
/// `__gen_body_N` / `__gen_pred_N` functions (Issue #9103 / #9127).
///
/// The compiler compiles these lazily without lifting:
/// - a filterless scalar-binding generator whose body is a plain `f(var)` call
///   (`MakeGenerator` / `MakeGeneratorRuntime`);
/// - a filtered scalar-binding generator whose body AND filter are both plain
///   `f(var)` / `p(var)` calls (`FilteredFunctionIndex`).
///
/// Everything else — a non-trivial body, a non-trivial filter, or a
/// tuple-destructuring binding — used to hit the eager comprehension fallback,
/// so it is lifted here.
fn generator_needs_lift(
    body: &Expr,
    filter: Option<&Expr>,
    var_name: &str,
    is_tuple_binding: bool,
) -> bool {
    match filter {
        None => is_tuple_binding || !is_plain_unary_var_call(body, var_name),
        Some(filter_expr) => {
            is_tuple_binding
                || !is_plain_unary_var_call(body, var_name)
                || !is_plain_unary_var_call(filter_expr, var_name)
        }
    }
}

/// Lift an arbitrary generator body (and, when present, its filter predicate)
/// into nested single-parameter functions so the generator expression compiles
/// onto the lazy runtime-callable / filtered paths (Issue #9103 / #9127).
///
/// `(body for var in iter if pred)` becomes
///
/// ```text
/// let
///     function __gen_body_N(var); return body; end
///     function __gen_pred_N(var); return pred; end
///     (__gen_body_N(var) for var in iter if __gen_pred_N(var))
/// end
/// ```
///
/// For a tuple-destructuring binding (`(a, b) in pairs`) the synthetic
/// functions take a single fresh parameter and a destructuring prologue
/// (`a = arg[1]; b = arg[2]`) is injected before the lifted expression, so the
/// bound names are available to both the body and the predicate.
///
/// mirroring arrow-lambda lowering (`lower_arrow_value_as_nested_impl`): the
/// nested `FunctionDef`s inside the `LetBlock` body are discovered by
/// `collect_stmt_functions` / `collect_expr_functions`, so free variables of
/// `body` / `pred` participate in normal closure analysis and the lifted
/// callables are available when the generator is constructed.
fn lift_generator_as_nested(
    body: Expr,
    var_name: String,
    iter_expr: Expr,
    filter: Option<Box<Expr>>,
    tuple_vars: Option<Vec<String>>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> Expr {
    use crate::ir::core::{Block, Function, Literal, Stmt, TypedParam};

    // Scalar bindings reuse the loop variable directly as the single parameter;
    // tuple bindings introduce a fresh parameter that is destructured into the
    // bound names in a prologue.
    let arg_name = if tuple_vars.is_some() {
        format!("__gen_arg_{}", span.start)
    } else {
        var_name.clone()
    };

    // Build the destructuring prologue shared by every synthetic function.
    let destructuring_prologue = |stmts: &mut Vec<Stmt>| {
        if let Some(vars) = &tuple_vars {
            for (idx, bound) in vars.iter().enumerate() {
                stmts.push(Stmt::Assign {
                    var: bound.clone(),
                    value: Expr::Index {
                        array: Box::new(Expr::Var(arg_name.clone().into(), span)),
                        indices: vec![Expr::Literal(Literal::Int((idx + 1) as i64), span)],
                        span,
                    },
                    span,
                });
            }
        }
    };

    let make_func = |name: String, fn_body: Expr| -> Function {
        let mut stmts = Vec::new();
        destructuring_prologue(&mut stmts);
        stmts.push(Stmt::Return {
            value: Some(fn_body),
            span,
        });
        Function {
            name,
            params: vec![TypedParam::untyped(arg_name.clone(), span)],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block { stmts, span },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        }
        .into_lowering_helper()
    };

    // Build a plain unary call `name(arg_name)` (the shape the compiler's lazy
    // generator fast paths recognize).
    let call_arg = |name: String| Expr::Call {
        function: name.into(),
        args: vec![Expr::Var(arg_name.clone().into(), span)],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    };

    let body_name = generator_body_name(lambda_ctx, span.start, None);
    let body_func = make_func(body_name.clone(), body);

    let mut stmts = vec![Stmt::FunctionDef {
        func: Box::new(body_func),
        span,
    }];

    let gen_filter = if let Some(filter_expr) = filter {
        let pred_name = generator_predicate_name(lambda_ctx, span.start);
        let pred_func = make_func(pred_name.clone(), *filter_expr);
        stmts.push(Stmt::FunctionDef {
            func: Box::new(pred_func),
            span,
        });
        Some(Box::new(call_arg(pred_name)))
    } else {
        None
    };

    let generator = Expr::Generator {
        body: Box::new(call_arg(body_name)),
        var: arg_name.into(),
        iter: Box::new(iter_expr),
        filter: gen_filter,
        span,
    };

    stmts.push(Stmt::Expr {
        expr: generator,
        span,
    });

    Expr::LetBlock {
        bindings: vec![],
        body: Block { stmts, span },
        span,
    }
}

/// Parse ALL bindings from a for clause.
/// A single ForClause may contain multiple ForBindings when comma-separated:
///   `for i in 1:3, j in 1:3` produces one ForClause with two ForBinding children.
fn parse_for_clause_bindings<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Vec<(crate::ir::core::InternedStr, Expr)>> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

    // Collect all ForBinding children
    let mut bindings = Vec::new();
    for child in &named {
        if walker.kind(child) == NodeKind::ForBinding {
            bindings.push(parse_for_binding(walker, *child, lambda_ctx)?);
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
        let iter_expr = lower_expr_maybe_ctx(walker, iter_node, lambda_ctx)?;
        return Ok(vec![(var_name.into(), iter_expr)]);
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedForBinding,
        span,
    ))
}

/// Parse a for binding: "x in range" or "x = range"
fn parse_for_binding<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<(crate::ir::core::InternedStr, Expr)> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

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
    let iter_expr = lower_expr_maybe_ctx(walker, iter_node, lambda_ctx)?;

    Ok((var_name.into(), iter_expr))
}

fn parse_for_binding_name<'a>(walker: &CstWalker<'a>, var_node: Node<'a>) -> LowerResult<String> {
    match walker.kind(&var_node) {
        NodeKind::Identifier => Ok(walker.text(&var_node).to_string()),
        NodeKind::TupleExpression => {
            let vars: Vec<String> = walker
                .named_children(&var_node)
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
fn parse_if_clause<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty if clause"),
        );
    }

    // The if clause contains just the condition expression
    lower_expr_maybe_ctx(walker, named[0], lambda_ctx)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Literal};
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string().into(), s())
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), s())
    }

    fn array_ref() -> Expr {
        Expr::Var("arr".to_string().into(), s())
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

    // ── nd_fold / nd_cat (Issue #10190) ─────────────────────────────────────

    fn int_vals(elements: &[Expr]) -> Vec<i64> {
        elements
            .iter()
            .map(|e| match e {
                Expr::Literal(Literal::Int(v), _) => *v,
                other => panic!("expected an Int literal, got {:?}", other),
            })
            .collect()
    }

    fn row(vals: &[i64]) -> NDBlock {
        nd_block_from_row(vals.iter().map(|&v| lit_int(v)).collect())
    }

    #[test]
    fn test_nd_fold_plain_2d_matrix() {
        // [1 2; 3 4] -> shape (2, 2), column-major [1, 3, 2, 4].
        let blocks = vec![row(&[1, 2]), row(&[3, 4])];
        let merged = nd_fold(blocks, vec![1], s()).unwrap();
        assert_eq!(merged.shape, vec![2, 2]);
        assert_eq!(int_vals(&merged.elements), vec![1, 3, 2, 4]);
    }

    #[test]
    fn test_nd_fold_mwe_3d_literal() {
        // Issue #10190 MWE: [1 2; 3 4;;; 5 6; 7 8] -> Array{_,3}, size (2,2,2).
        let blocks = vec![row(&[1, 2]), row(&[3, 4]), row(&[5, 6]), row(&[7, 8])];
        let merged = nd_fold(blocks, vec![1, 3, 1], s()).unwrap();
        assert_eq!(merged.shape, vec![2, 2, 2]);
        // Verified against both upstream `julia` and this fix's `sjulia` build
        // via linear (column-major) indexing `a3[1:8]`.
        assert_eq!(int_vals(&merged.elements), vec![1, 3, 2, 4, 5, 7, 6, 8]);
    }

    #[test]
    fn test_nd_fold_4d_literal() {
        // [1 2;3 4;;;5 6;7 8;;;;9 10;11 12;;;13 14;15 16] -> (2,2,2,2).
        let blocks = vec![
            row(&[1, 2]),
            row(&[3, 4]),
            row(&[5, 6]),
            row(&[7, 8]),
            row(&[9, 10]),
            row(&[11, 12]),
            row(&[13, 14]),
            row(&[15, 16]),
        ];
        let merged = nd_fold(blocks, vec![1, 3, 1, 4, 1, 3, 1], s()).unwrap();
        assert_eq!(merged.shape, vec![2, 2, 2, 2]);
    }

    #[test]
    fn test_nd_fold_skips_unused_level() {
        // [1;2;;;3;4] skips level 2 entirely -> (2, 1, 2), matching upstream.
        let blocks = vec![row(&[1]), row(&[2]), row(&[3]), row(&[4])];
        let merged = nd_fold(blocks, vec![1, 3, 1], s()).unwrap();
        assert_eq!(merged.shape, vec![2, 1, 2]);
        assert_eq!(int_vals(&merged.elements), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_nd_fold_single_block_no_separators() {
        let blocks = vec![row(&[1, 2, 3])];
        let merged = nd_fold(blocks, vec![], s()).unwrap();
        assert_eq!(merged.shape, vec![1, 3]);
        assert_eq!(int_vals(&merged.elements), vec![1, 2, 3]);
    }

    #[test]
    fn test_nd_cat_mismatched_shape_across_level_errors() {
        // [1 2 3;;;4 5] — dim-2 sizes disagree (3 vs 2) across the `;;;` blocks.
        let blocks = vec![row(&[1, 2, 3]), row(&[4, 5])];
        let err = nd_fold(blocks, vec![3], s()).unwrap_err();
        assert_eq!(err.kind, UnsupportedFeatureKind::MalformedMatrix);
    }

    #[test]
    fn test_nd_cat_mismatched_column_count_within_level_errors() {
        // [1 2; 3 4 5] — inconsistent column count within a single `;` group.
        let blocks = vec![row(&[1, 2]), row(&[3, 4, 5])];
        let err = nd_fold(blocks, vec![1], s()).unwrap_err();
        assert_eq!(err.kind, UnsupportedFeatureKind::MalformedMatrix);
    }
}
