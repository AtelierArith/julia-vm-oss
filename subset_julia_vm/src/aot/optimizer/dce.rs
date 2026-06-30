//! Dead Code Elimination (DCE) for AoT IR
//!
//! This module implements dead code elimination that removes
//! unreachable code and unused variable assignments.

use crate::aot::ir::{AotBinOp, AotExpr, AotProgram, AotStmt, AotUnaryOp};
use std::collections::HashSet;

use super::constant_folding::AotConstantFolder;

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
        self.optimize_stmts_with_dead_store_elimination(stmts, true)
    }

    fn optimize_stmts_with_dead_store_elimination(
        &mut self,
        stmts: &mut Vec<AotStmt>,
        eliminate_dead_stores: bool,
    ) -> usize {
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
                    total_eliminations +=
                        self.optimize_stmts_with_dead_store_elimination(then_branch, false);
                    if let Some(else_b) = else_branch {
                        total_eliminations +=
                            self.optimize_stmts_with_dead_store_elimination(else_b, false);
                    }
                }
                AotStmt::While { body, .. }
                | AotStmt::ForRange { body, .. }
                | AotStmt::ForEach { body, .. } => {
                    total_eliminations +=
                        self.optimize_stmts_with_dead_store_elimination(body, false);
                }
                _ => {}
            }
        }

        // Fourth pass: remove overwritten stores whose values are never read.
        if eliminate_dead_stores {
            total_eliminations += self.eliminate_dead_stores(stmts);
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
            // Check if condition is a constant boolean
            if let AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } = &stmts[i]
            {
                let Some(cond_value) = Self::const_bool_value(condition) else {
                    i += 1;
                    continue;
                };
                if cond_value {
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

            // Also simplify while(false) loops - just remove them
            if let AotStmt::While { condition, .. } = &stmts[i] {
                if Self::const_bool_value(condition) == Some(false) {
                    stmts.remove(i);
                    eliminations += 1;
                    continue;
                }
            }

            i += 1;
        }

        eliminations
    }

    /// Remove plain variable assignments that are overwritten before any read.
    ///
    /// This deliberately does not remove `Let` declarations: later `Assign`
    /// statements rely on the declaration for Rust codegen. It also only drops
    /// assignments whose RHS is side-effect-free and no-throw under the current
    /// AoT IR model; calls, indexing, conversions, division/modulo/power, and
    /// heap-shaped expressions stay in place even if the target is dead.
    fn eliminate_dead_stores(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        let mut live_vars = HashSet::new();
        let mut dead_store_indices = Vec::new();

        for idx in (0..stmts.len()).rev() {
            match &stmts[idx] {
                AotStmt::Assign {
                    target: AotExpr::Var { name, .. },
                    value,
                } => {
                    if !live_vars.contains(name) && Self::expr_is_droppable(value) {
                        dead_store_indices.push(idx);
                        continue;
                    }

                    live_vars.remove(name);
                    Self::collect_expr_vars(value, &mut live_vars);
                }
                AotStmt::Assign { target, value } => {
                    Self::collect_expr_vars(target, &mut live_vars);
                    Self::collect_expr_vars(value, &mut live_vars);
                }
                AotStmt::Let { name, value, .. } => {
                    live_vars.remove(name);
                    Self::collect_expr_vars(value, &mut live_vars);
                }
                AotStmt::CompoundAssign { target, value, .. } => {
                    Self::collect_expr_vars(target, &mut live_vars);
                    Self::collect_expr_vars(value, &mut live_vars);
                }
                AotStmt::Expr(expr) | AotStmt::Return(Some(expr)) => {
                    Self::collect_expr_vars(expr, &mut live_vars);
                }
                AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => {}
                AotStmt::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    Self::collect_expr_vars(condition, &mut live_vars);
                    Self::collect_stmt_vars_conservative(then_branch, &mut live_vars);
                    if let Some(else_branch) = else_branch {
                        Self::collect_stmt_vars_conservative(else_branch, &mut live_vars);
                    }
                }
                AotStmt::While { condition, body } => {
                    Self::collect_expr_vars(condition, &mut live_vars);
                    Self::collect_stmt_vars_conservative(body, &mut live_vars);
                }
                AotStmt::ForRange {
                    var,
                    start,
                    stop,
                    step,
                    body,
                } => {
                    live_vars.insert(var.clone());
                    Self::collect_expr_vars(start, &mut live_vars);
                    Self::collect_expr_vars(stop, &mut live_vars);
                    if let Some(step) = step {
                        Self::collect_expr_vars(step, &mut live_vars);
                    }
                    Self::collect_stmt_vars_conservative(body, &mut live_vars);
                }
                AotStmt::ForEach { var, iter, body } => {
                    live_vars.insert(var.clone());
                    Self::collect_expr_vars(iter, &mut live_vars);
                    Self::collect_stmt_vars_conservative(body, &mut live_vars);
                }
            }
        }

        let eliminations = dead_store_indices.len();
        for idx in dead_store_indices {
            stmts.remove(idx);
        }
        eliminations
    }

    fn collect_stmt_vars_conservative(stmts: &[AotStmt], vars: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                AotStmt::Let { name, value, .. } => {
                    vars.insert(name.clone());
                    Self::collect_expr_vars(value, vars);
                }
                AotStmt::Assign { target, value } => {
                    Self::collect_expr_vars(target, vars);
                    Self::collect_expr_vars(value, vars);
                }
                AotStmt::CompoundAssign { target, value, .. } => {
                    Self::collect_expr_vars(target, vars);
                    Self::collect_expr_vars(value, vars);
                }
                AotStmt::Expr(expr) | AotStmt::Return(Some(expr)) => {
                    Self::collect_expr_vars(expr, vars);
                }
                AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => {}
                AotStmt::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    Self::collect_expr_vars(condition, vars);
                    Self::collect_stmt_vars_conservative(then_branch, vars);
                    if let Some(else_branch) = else_branch {
                        Self::collect_stmt_vars_conservative(else_branch, vars);
                    }
                }
                AotStmt::While { condition, body } => {
                    Self::collect_expr_vars(condition, vars);
                    Self::collect_stmt_vars_conservative(body, vars);
                }
                AotStmt::ForRange {
                    var,
                    start,
                    stop,
                    step,
                    body,
                } => {
                    vars.insert(var.clone());
                    Self::collect_expr_vars(start, vars);
                    Self::collect_expr_vars(stop, vars);
                    if let Some(step) = step {
                        Self::collect_expr_vars(step, vars);
                    }
                    Self::collect_stmt_vars_conservative(body, vars);
                }
                AotStmt::ForEach { var, iter, body } => {
                    vars.insert(var.clone());
                    Self::collect_expr_vars(iter, vars);
                    Self::collect_stmt_vars_conservative(body, vars);
                }
            }
        }
    }

    fn collect_expr_vars(expr: &AotExpr, vars: &mut HashSet<String>) {
        match expr {
            AotExpr::Var { name, .. } => {
                vars.insert(name.clone());
            }
            AotExpr::BinOpStatic { left, right, .. }
            | AotExpr::BinOpDynamic { left, right, .. } => {
                Self::collect_expr_vars(left, vars);
                Self::collect_expr_vars(right, vars);
            }
            AotExpr::UnaryOp { operand, .. } => {
                Self::collect_expr_vars(operand, vars);
            }
            AotExpr::CallStatic { args, .. }
            | AotExpr::CallDynamic { args, .. }
            | AotExpr::CallBuiltin { args, .. }
            | AotExpr::ArrayLit { elements: args, .. }
            | AotExpr::TupleLit { elements: args }
            | AotExpr::StructNew { fields: args, .. } => {
                for arg in args {
                    Self::collect_expr_vars(arg, vars);
                }
            }
            AotExpr::SetFromIter { iter, .. } => Self::collect_expr_vars(iter, vars),
            AotExpr::NamedTupleLit { fields } => {
                for (_, field) in fields {
                    Self::collect_expr_vars(field, vars);
                }
            }
            AotExpr::Comprehension {
                body, iter, filter, ..
            }
            | AotExpr::Generator {
                body, iter, filter, ..
            } => {
                Self::collect_expr_vars(iter, vars);
                if let Some(filter) = filter {
                    Self::collect_expr_vars(filter, vars);
                }
                Self::collect_expr_vars(body, vars);
            }
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                for (_, iter) in iterations {
                    Self::collect_expr_vars(iter, vars);
                }
                if let Some(filter) = filter {
                    Self::collect_expr_vars(filter, vars);
                }
                Self::collect_expr_vars(body, vars);
            }
            AotExpr::Index { array, indices, .. } => {
                Self::collect_expr_vars(array, vars);
                for index in indices {
                    Self::collect_expr_vars(index, vars);
                }
            }
            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::collect_expr_vars(start, vars);
                Self::collect_expr_vars(stop, vars);
                if let Some(step) = step {
                    Self::collect_expr_vars(step, vars);
                }
            }
            AotExpr::FieldAccess { object, .. } => {
                Self::collect_expr_vars(object, vars);
            }
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::collect_expr_vars(condition, vars);
                Self::collect_expr_vars(then_expr, vars);
                Self::collect_expr_vars(else_expr, vars);
            }
            AotExpr::Box(inner)
            | AotExpr::Unbox { value: inner, .. }
            | AotExpr::Convert { value: inner, .. } => {
                Self::collect_expr_vars(inner, vars);
            }
            AotExpr::Lambda { body, captures, .. } => {
                for (name, _) in captures {
                    vars.insert(name.clone());
                }
                Self::collect_expr_vars(body, vars);
            }
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing
            | AotExpr::LitMissing => {}
        }
    }

    fn expr_is_droppable(expr: &AotExpr) -> bool {
        match expr {
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing
            | AotExpr::LitMissing
            | AotExpr::Var { .. } => true,
            AotExpr::UnaryOp { operand, .. } => Self::expr_is_droppable(operand),
            AotExpr::BinOpStatic {
                op, left, right, ..
            } => {
                Self::binop_is_droppable(*op)
                    && Self::expr_is_droppable(left)
                    && Self::expr_is_droppable(right)
            }
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_is_droppable(condition)
                    && Self::expr_is_droppable(then_expr)
                    && Self::expr_is_droppable(else_expr)
            }
            AotExpr::BinOpDynamic { .. }
            | AotExpr::CallStatic { .. }
            | AotExpr::CallDynamic { .. }
            | AotExpr::CallBuiltin { .. }
            | AotExpr::ArrayLit { .. }
            | AotExpr::SetFromIter { .. }
            | AotExpr::TupleLit { .. }
            | AotExpr::NamedTupleLit { .. }
            | AotExpr::Comprehension { .. }
            | AotExpr::MultiComprehension { .. }
            | AotExpr::Generator { .. }
            | AotExpr::Index { .. }
            | AotExpr::Range { .. }
            | AotExpr::StructNew { .. }
            | AotExpr::FieldAccess { .. }
            | AotExpr::Box(_)
            | AotExpr::Unbox { .. }
            | AotExpr::Convert { .. }
            | AotExpr::Lambda { .. } => false,
        }
    }

    fn binop_is_droppable(op: AotBinOp) -> bool {
        !matches!(
            op,
            AotBinOp::Div | AotBinOp::IntDiv | AotBinOp::Mod | AotBinOp::Pow | AotBinOp::Subtype
        )
    }

    fn const_bool_value(expr: &AotExpr) -> Option<bool> {
        let folder = AotConstantFolder::new();
        let (folded, folds) = folder.fold_expr(expr);
        if let AotExpr::LitBool(value) = &folded {
            return Some(*value);
        }
        let expr = if folds > 0 { &folded } else { expr };
        Self::const_bool_literal_boundary(expr)
    }

    fn const_bool_literal_boundary(expr: &AotExpr) -> Option<bool> {
        match expr {
            AotExpr::LitBool(value) => Some(*value),
            AotExpr::UnaryOp {
                op: AotUnaryOp::Not,
                operand,
                ..
            } => Some(!Self::const_bool_value(operand)?),
            AotExpr::BinOpStatic {
                op, left, right, ..
            } => Self::const_bool_binop(*op, left, right),
            _ => None,
        }
    }

    fn const_bool_binop(op: AotBinOp, left: &AotExpr, right: &AotExpr) -> Option<bool> {
        match (left, right) {
            (AotExpr::LitBool(a), AotExpr::LitBool(b)) => match op {
                AotBinOp::And => Some(*a && *b),
                AotBinOp::Or => Some(*a || *b),
                AotBinOp::Eq | AotBinOp::Egal => Some(a == b),
                AotBinOp::Ne | AotBinOp::NotEgal => Some(a != b),
                _ => None,
            },
            (AotExpr::LitI64(a), AotExpr::LitI64(b)) => Self::compare_ord(op, a, b),
            (AotExpr::LitI32(a), AotExpr::LitI32(b)) => Self::compare_ord(op, a, b),
            (AotExpr::LitChar(a), AotExpr::LitChar(b)) => Self::compare_ord(op, a, b),
            (AotExpr::LitF64(a), AotExpr::LitF64(b)) => Self::compare_partial(op, a, b),
            (AotExpr::LitF32(a), AotExpr::LitF32(b)) => Self::compare_partial(op, a, b),
            (AotExpr::LitStr(a), AotExpr::LitStr(b)) => Self::compare_ord(op, a, b),
            _ => None,
        }
    }

    fn compare_ord<T: Ord>(op: AotBinOp, left: &T, right: &T) -> Option<bool> {
        match op {
            AotBinOp::Eq | AotBinOp::Egal => Some(left == right),
            AotBinOp::Ne | AotBinOp::NotEgal => Some(left != right),
            AotBinOp::Lt => Some(left < right),
            AotBinOp::Le => Some(left <= right),
            AotBinOp::Gt => Some(left > right),
            AotBinOp::Ge => Some(left >= right),
            _ => None,
        }
    }

    fn compare_partial<T: PartialOrd>(op: AotBinOp, left: &T, right: &T) -> Option<bool> {
        match op {
            AotBinOp::Eq | AotBinOp::Egal => Some(left == right),
            AotBinOp::Ne | AotBinOp::NotEgal => Some(left != right),
            AotBinOp::Lt => Some(left < right),
            AotBinOp::Le => Some(left <= right),
            AotBinOp::Gt => Some(left > right),
            AotBinOp::Ge => Some(left >= right),
            _ => None,
        }
    }
}

/// Optimize an AoT program with dead code elimination
pub fn optimize_aot_program_with_dce(program: &mut AotProgram) -> usize {
    let mut eliminator = AotDeadCodeEliminator::new();
    eliminator.optimize_program(program)
}
