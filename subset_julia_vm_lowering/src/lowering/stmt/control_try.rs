//! Try statement lowering
//!
//! Handles:
//! - Try-catch: `try ... catch [e] ... end`
//! - Try-catch-else: `try ... catch ... else ... end`
//! - Try-catch-finally: `try ... catch ... finally ... end`
//! - Full form: `try ... catch ... else ... finally ... end`

use crate::ir::core::{Block, Stmt};
use crate::lowering::{type_alias, LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::{lower_block, lower_block_with_ctx};

/// Lower a try statement without lambda context.
///
/// This is kept for backwards compatibility with code paths that don't have
/// a lambda context available. It uses `lower_block` internally.
pub fn lower_try_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    lower_try_stmt_impl(walker, node, None)
}

fn lower_try_stmt_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);

    let mut try_block_node = None;
    let mut catch_clause = None;
    let mut else_clause = None;
    let mut finally_clause = None;

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Block if try_block_node.is_none() => {
                try_block_node = Some(child);
            }
            NodeKind::CatchClause => catch_clause = Some(child),
            NodeKind::ElseClause => else_clause = Some(child),
            NodeKind::FinallyClause => finally_clause = Some(child),
            _ => {}
        }
    }

    let try_block = match try_block_node {
        Some(block_node) => lower_block_maybe_ctx(walker, block_node, lambda_ctx)?,
        None => Block {
            stmts: Vec::new(),
            span,
        },
    };

    let (catch_var, catch_block) = match catch_clause {
        Some(catch_node) => {
            let mut var = None;
            let mut block_node = None;
            for child in walker.named_children(&catch_node) {
                match walker.kind(&child) {
                    NodeKind::Identifier if var.is_none() => {
                        var = Some(walker.text(&child).to_string());
                    }
                    NodeKind::Block if block_node.is_none() => {
                        block_node = Some(child);
                    }
                    _ => {}
                }
            }
            // Issue #11321: `catch NAME` introduces a fresh lexical binding
            // for NAME that shadows any outer const/type-alias spelled NAME
            // for the extent of this clause — including inside a composite
            // annotation (`Vector{T}`), not just a bare one. A same-clause
            // reassignment of NAME to a resolvable type expression is then
            // visible to later signature annotations in the SAME clause
            // (upstream evaluates every annotation eagerly against the
            // binding visible at its source position). The whole-program
            // alias pre-scan (`prescan_and_register_type_aliases`) never
            // descends into `try`/`catch` bodies — a deliberate scope
            // boundary, since a catch binder must not leak past its `end` —
            // so register the shadow directly here, at the clause's real
            // lowering, and discard it the moment this clause finishes.
            let shadow_scope = var
                .as_deref()
                .and_then(|name| shadow_catch_binder(walker, catch_node, block_node, name));
            // Restore the shadow snapshot on every exit path (including a
            // lowering error) so a failed clause never leaks its scoped
            // entries into the rest of the pass — mirroring how
            // `lower_source_file`'s outer `AliasScope` is restored before
            // its own result is returned (never behind an early `?`).
            let block_result = match block_node {
                Some(node) => lower_block_maybe_ctx(walker, node, lambda_ctx).map(Some),
                None => Ok(None),
            };
            if let Some(scope) = shadow_scope {
                scope.restore();
            }
            let block = block_result?.unwrap_or(Block {
                stmts: Vec::new(),
                span: walker.span(&catch_node),
            });
            (var, Some(block))
        }
        None => (None, None),
    };

    let else_block = match else_clause {
        Some(else_node) => {
            let block_node = walker
                .named_children(&else_node)
                .find(|child| walker.kind(child) == NodeKind::Block);
            let block = match block_node {
                Some(node) => lower_block_maybe_ctx(walker, node, lambda_ctx)?,
                None => Block {
                    stmts: Vec::new(),
                    span: walker.span(&else_node),
                },
            };
            Some(block)
        }
        None => None,
    };

    let finally_block = match finally_clause {
        Some(finally_node) => {
            let block_node = walker
                .named_children(&finally_node)
                .find(|child| walker.kind(child) == NodeKind::Block);
            let block = match block_node {
                Some(node) => lower_block_maybe_ctx(walker, node, lambda_ctx)?,
                None => Block {
                    stmts: Vec::new(),
                    span: walker.span(&finally_node),
                },
            };
            Some(block)
        }
        None => None,
    };

    Ok(Stmt::Try {
        try_block,
        catch_var,
        catch_block,
        else_block,
        finally_block,
        span,
    })
}

/// Lower a try statement with lambda context.
///
/// The context is propagated to all blocks (try, catch, else, finally)
/// to ensure proper handling of lambdas and macros within try blocks.
pub fn lower_try_stmt_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    if lambda_ctx.in_function_body() {
        lower_try_stmt_impl(walker, node, Some(lambda_ctx))
    } else {
        lambda_ctx
            .with_top_level_control_flow(|| lower_try_stmt_impl(walker, node, Some(lambda_ctx)))
    }
}

/// Register a lexically scoped alias shadow for a `catch NAME` binder
/// (Issue #11321), returning a guard that MUST be `.restore()`d once this
/// clause's lowering finishes (on every exit path, including error) so the
/// entries never leak past the clause's `end`.
///
/// Two entries are registered, in source order, so the EXISTING
/// registration-order/position visibility rules of `type_alias` (unchanged
/// here) resolve every later use correctly with zero new resolution logic:
///
/// 1. A non-alias tombstone for `name` itself, positioned at the clause's own
///    start. Any outer `const`/type-alias spelled `name` is registered at an
///    earlier position, so `resolve_alias_entry`'s "newest available entry"
///    rule now prefers this tombstone — the outer alias no longer expands
///    inside a composite annotation (`Vector{T}`) used later in this clause.
/// 2. For each direct-child `const`/plain assignment inside the clause body
///    that is itself alias-shaped (`NAME = SomeType`), a real alias entry at
///    that assignment's own position — so a same-clause reassignment to a
///    resolvable type is visible to a signature annotation appearing further
///    down in the SAME clause, matching upstream's eager, source-order
///    evaluation of signature annotations. Deliberately shallow (direct
///    children only, mirroring the whole-program pre-scan's own module/block
///    recursion depth) — a reassignment nested inside further control flow
///    within the clause is out of this fix's bounded scope.
fn shadow_catch_binder<'a>(
    walker: &CstWalker<'a>,
    catch_node: Node<'a>,
    block_node: Option<Node<'a>>,
    name: &str,
) -> Option<type_alias::AliasScope> {
    let tombstone_position = type_alias::current_source_position(walker.span(&catch_node).start)?;
    let owner = type_alias::current_module_owner();
    let scope = type_alias::snapshot();
    type_alias::register_prescanned_non_alias(name, tombstone_position, &owner);
    if let Some(block_node) = block_node {
        for child in walker.named_children(&block_node) {
            match walker.kind(&child) {
                NodeKind::ConstStatement => {
                    if let Some(alias) = super::try_extract_type_alias(walker, child) {
                        register_scoped_alias(&alias, &owner);
                    }
                }
                NodeKind::Assignment => {
                    if let Some(alias) =
                        super::try_extract_type_alias_from_assignment(walker, child)
                    {
                        register_scoped_alias(&alias, &owner);
                    }
                }
                _ => {}
            }
        }
    }
    Some(scope)
}

fn register_scoped_alias(alias: &crate::ir::core::TypeAliasDef, owner: &[String]) {
    if let Some(position) = type_alias::current_source_position(alias.span.start) {
        type_alias::register_prescanned(
            &alias.name,
            alias.params.clone(),
            &alias.target_type,
            position,
            owner,
        );
    }
}

fn lower_block_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Block> {
    match lambda_ctx {
        Some(ctx) => lower_block_with_ctx(walker, node, ctx),
        None => lower_block(walker, node),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::ir::core::Stmt;
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    fn lower_first_stmt(source: &str) -> Stmt {
        let mut parser = Parser::new().expect("Failed to init parser");
        let parse_outcome = parser.parse(source).expect("Failed to parse");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parse_outcome).expect("Failed to lower");
        assert!(!program.main.stmts.is_empty(), "No statements found");
        program.main.stmts[0].clone()
    }

    #[test]
    fn test_try_catch_basic() {
        let stmt = lower_first_stmt("try\n  1\ncatch\n  2\nend");
        assert!(
            matches!(
                stmt,
                Stmt::Try {
                    catch_block: Some(_),
                    catch_var: None,
                    ..
                }
            ),
            "Expected Try with catch block and no catch variable, got {:?}",
            stmt
        );
    }

    #[test]
    fn test_try_catch_with_variable() {
        let stmt = lower_first_stmt("try\n  1\ncatch e\n  e\nend");
        assert!(
            matches!(stmt, Stmt::Try { catch_var: Some(ref v), catch_block: Some(_), .. } if v == "e"),
            "Expected Try with catch variable 'e', got {:?}",
            stmt
        );
    }

    #[test]
    fn test_try_catch_finally() {
        let stmt = lower_first_stmt("try\n  1\ncatch\n  2\nfinally\n  3\nend");
        assert!(
            matches!(
                stmt,
                Stmt::Try {
                    catch_block: Some(_),
                    finally_block: Some(_),
                    ..
                }
            ),
            "Expected Try with both catch and finally blocks, got {:?}",
            stmt
        );
    }
}
