use super::error;
use super::expr::verify_expr;
use crate::aot::ir::AotStmt;
use crate::aot::pass_pipeline::AotPassStage;
use crate::aot::AotResult;

pub(super) fn verify_stmt(
    stage: AotPassStage,
    function: &str,
    index: usize,
    statement: &AotStmt,
) -> AotResult<()> {
    match statement {
        AotStmt::Let { name, value, .. } => {
            if name.trim().is_empty() {
                return error(
                    stage,
                    function,
                    &format!("statement #{} binds an empty variable name", index),
                );
            }
            verify_expr(stage, function, value)
        }
        AotStmt::Assign { target, value } | AotStmt::CompoundAssign { target, value, .. } => {
            verify_expr(stage, function, target)?;
            verify_expr(stage, function, value)
        }
        AotStmt::Return(Some(expr)) | AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => {
            verify_expr(stage, function, expr)
        }
        AotStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(stage, function, condition)?;
            verify_block(stage, function, then_branch)?;
            if let Some(branch) = else_branch {
                verify_block(stage, function, branch)?;
            }
            Ok(())
        }
        AotStmt::While {
            condition, body, ..
        } => {
            verify_expr(stage, function, condition)?;
            verify_block(stage, function, body)
        }
        AotStmt::ForRange {
            var,
            start,
            stop,
            step,
            body,
        } => {
            if var.trim().is_empty() {
                return error(stage, function, "for-range variable name is empty");
            }
            verify_expr(stage, function, start)?;
            verify_expr(stage, function, stop)?;
            if let Some(step) = step {
                verify_expr(stage, function, step)?;
            }
            verify_block(stage, function, body)
        }
        AotStmt::ForEach { var, iter, body } => {
            if var.trim().is_empty() {
                return error(stage, function, "for-each variable name is empty");
            }
            verify_expr(stage, function, iter)?;
            verify_block(stage, function, body)
        }
        AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => Ok(()),
    }
}

fn verify_block(stage: AotPassStage, function: &str, block: &[AotStmt]) -> AotResult<()> {
    for (index, statement) in block.iter().enumerate() {
        verify_stmt(stage, function, index, statement)?;
    }
    Ok(())
}
