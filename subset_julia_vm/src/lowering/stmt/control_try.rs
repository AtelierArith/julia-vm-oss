//! Try statement lowering
//!
//! Handles:
//! - Try-catch: `try ... catch [e] ... end`
//! - Try-catch-else: `try ... catch ... else ... end`
//! - Try-catch-finally: `try ... catch ... finally ... end`
//! - Full form: `try ... catch ... else ... finally ... end`

use crate::ir::core::{Block, Stmt};
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::{lower_block, lower_block_with_ctx};

/// Lower a try statement without lambda context.
///
/// This is kept for backwards compatibility with code paths that don't have
/// a lambda context available. It uses `lower_block` internally.
pub fn lower_try_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
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
        Some(block_node) => lower_block(walker, block_node)?,
        None => Block {
            stmts: Vec::new(),
            span,
        },
    };

    let (catch_var, catch_block) = match catch_clause {
        Some(catch_node) => {
            let mut var = None;
            let mut block = None;
            for child in walker.named_children(&catch_node) {
                match walker.kind(&child) {
                    NodeKind::Identifier if var.is_none() => {
                        var = Some(walker.text(&child).to_string());
                    }
                    NodeKind::Block if block.is_none() => {
                        block = Some(lower_block(walker, child)?);
                    }
                    _ => {}
                }
            }
            let block = block.unwrap_or(Block {
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
                .into_iter()
                .find(|child| walker.kind(child) == NodeKind::Block);
            let block = match block_node {
                Some(node) => lower_block(walker, node)?,
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
                .into_iter()
                .find(|child| walker.kind(child) == NodeKind::Block);
            let block = match block_node {
                Some(node) => lower_block(walker, node)?,
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
        Some(block_node) => lower_block_with_ctx(walker, block_node, lambda_ctx)?,
        None => Block {
            stmts: Vec::new(),
            span,
        },
    };

    let (catch_var, catch_block) = match catch_clause {
        Some(catch_node) => {
            let mut var = None;
            let mut block = None;
            for child in walker.named_children(&catch_node) {
                match walker.kind(&child) {
                    NodeKind::Identifier if var.is_none() => {
                        var = Some(walker.text(&child).to_string());
                    }
                    NodeKind::Block if block.is_none() => {
                        block = Some(lower_block_with_ctx(walker, child, lambda_ctx)?);
                    }
                    _ => {}
                }
            }
            let block = block.unwrap_or(Block {
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
                .into_iter()
                .find(|child| walker.kind(child) == NodeKind::Block);
            let block = match block_node {
                Some(node) => lower_block_with_ctx(walker, node, lambda_ctx)?,
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
                .into_iter()
                .find(|child| walker.kind(child) == NodeKind::Block);
            let block = match block_node {
                Some(node) => lower_block_with_ctx(walker, node, lambda_ctx)?,
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

#[cfg(test)]
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
            matches!(stmt, Stmt::Try { catch_block: Some(_), catch_var: None, .. }),
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
            matches!(stmt, Stmt::Try { catch_block: Some(_), finally_block: Some(_), .. }),
            "Expected Try with both catch and finally blocks, got {:?}",
            stmt
        );
    }
}
