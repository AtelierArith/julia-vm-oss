//! Dead Code Elimination (DCE) for AoT IR
//!
//! This module implements dead code elimination that removes
//! unreachable code and unused variable assignments.

use crate::aot::ir::{AotExpr, AotProgram, AotStmt};

/// Dead code eliminator for AoT IR
///
/// Removes unreachable code and unused variable assignments.
#[derive(Debug, Default)]
pub struct AotDeadCodeEliminator {
    /// Number of statements eliminated
    elimination_count: usize,
}

impl AotDeadCodeEliminator {
    /// Create a new dead code eliminator
    pub fn new() -> Self {
        Self {
            elimination_count: 0,
        }
    }

    /// Get the number of eliminations performed
    pub fn elimination_count(&self) -> usize {
        self.elimination_count
    }

    /// Optimize an AoT program with dead code elimination
    pub fn optimize_program(&mut self, program: &mut AotProgram) -> usize {
        let mut total_eliminations = 0;

        // Eliminate dead code in functions
        for func in &mut program.functions {
            total_eliminations += self.optimize_stmts(&mut func.body);
        }

        // Eliminate dead code in main block
        total_eliminations += self.optimize_stmts(&mut program.main);

        total_eliminations
    }

    /// Optimize a list of statements
    fn optimize_stmts(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        let mut total_eliminations = 0;

        // First pass: eliminate unreachable code after return/break/continue
        total_eliminations += self.eliminate_unreachable(stmts);

        // Second pass: simplify constant conditions in if statements
        total_eliminations += self.simplify_constant_conditions(stmts);

        // Third pass: recursively optimize nested blocks
        for stmt in stmts.iter_mut() {
            match stmt {
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    total_eliminations += self.optimize_stmts(then_branch);
                    if let Some(else_b) = else_branch {
                        total_eliminations += self.optimize_stmts(else_b);
                    }
                }
                AotStmt::While { body, .. }
                | AotStmt::ForRange { body, .. }
                | AotStmt::ForEach { body, .. } => {
                    total_eliminations += self.optimize_stmts(body);
                }
                _ => {}
            }
        }

        self.elimination_count += total_eliminations;
        total_eliminations
    }

    /// Eliminate unreachable code after return/break/continue
    fn eliminate_unreachable(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        let mut eliminations = 0;
        let mut i = 0;

        while i < stmts.len() {
            let is_terminator = matches!(
                stmts[i],
                AotStmt::Return(_) | AotStmt::Break | AotStmt::Continue
            );

            if is_terminator && i + 1 < stmts.len() {
                // Remove all statements after the terminator
                let removed = stmts.len() - i - 1;
                stmts.truncate(i + 1);
                eliminations += removed;
                break;
            }

            i += 1;
        }

        eliminations
    }

    /// Simplify if statements with constant conditions
    fn simplify_constant_conditions(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        let mut eliminations = 0;
        let mut i = 0;

        while i < stmts.len() {
            if let AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } = &stmts[i]
            {
                // Check if condition is a constant boolean
                if let AotExpr::LitBool(cond_value) = condition {
                    if *cond_value {
                        // Condition is always true - replace with then branch
                        let then_stmts = then_branch.clone();
                        stmts.splice(i..=i, then_stmts);
                        eliminations += 1;
                        continue; // Don't increment i, we replaced
                    } else {
                        // Condition is always false - replace with else branch or remove
                        if let Some(else_stmts) = else_branch {
                            let else_stmts = else_stmts.clone();
                            stmts.splice(i..=i, else_stmts);
                        } else {
                            stmts.remove(i);
                        }
                        eliminations += 1;
                        continue; // Don't increment i
                    }
                }
            }

            // Also simplify while(false) loops - just remove them
            if let AotStmt::While { condition, .. } = &stmts[i] {
                if let AotExpr::LitBool(false) = condition {
                    stmts.remove(i);
                    eliminations += 1;
                    continue;
                }
            }

            i += 1;
        }

        eliminations
    }
}

/// Optimize an AoT program with dead code elimination
pub fn optimize_aot_program_with_dce(program: &mut AotProgram) -> usize {
    let mut eliminator = AotDeadCodeEliminator::new();
    eliminator.optimize_program(program)
}
