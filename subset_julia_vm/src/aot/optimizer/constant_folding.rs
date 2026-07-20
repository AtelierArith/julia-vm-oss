//! Constant Folding optimization for AoT IR
//!
//! This module implements constant folding that evaluates operations
//! on literal values at compile time.
#![allow(clippy::cast_sign_loss)] // known-safe constant casts (i64->u32)

use crate::aot::ir::{AotBinOp, AotBuiltinOp, AotExpr, AotProgram, AotStmt, AotUnaryOp};

/// Julia `<<` on `Int64`: left shift with over-shift clamped to 0 and a
/// negative amount delegating to an arithmetic right shift. Mirrors the runtime
/// `SjuliaShift` helpers so constant-folded shifts match unfolded ones
/// (Issue #7057).
fn julia_shl_i64(x: i64, k: i64) -> i64 {
    if k < 0 {
        return julia_ashr_i64(x, k.unsigned_abs().min(64) as i64);
    }
    if k >= 64 {
        0
    } else {
        (x as u64).wrapping_shl(k as u32) as i64
    }
}

/// Julia `>>` on `Int64`: arithmetic right shift with over-shift sign fill and a
/// negative amount delegating to a left shift (Issue #7057).
fn julia_ashr_i64(x: i64, k: i64) -> i64 {
    if k < 0 {
        return julia_shl_i64(x, k.unsigned_abs().min(64) as i64);
    }
    if k >= 64 {
        if x < 0 {
            -1
        } else {
            0
        }
    } else {
        x >> (k as u32)
    }
}

/// Constant folder for AoT IR
///
/// Performs constant folding on expressions, evaluating operations
/// on literal values at compile time.
#[derive(Debug, Default)]
pub struct AotConstantFolder {
    /// Number of folds performed
    fold_count: usize,
}

impl AotConstantFolder {
    /// Create a new constant folder
    pub fn new() -> Self {
        Self { fold_count: 0 }
    }

    /// Get the number of folds performed
    pub fn fold_count(&self) -> usize {
        self.fold_count
    }

    /// Optimize an AoT program with constant folding
    pub fn optimize_program(&mut self, program: &mut AotProgram) -> usize {
        let mut total_folds = 0;

        // Fold constants in functions
        for func in &mut program.functions {
            total_folds += self.optimize_stmts(&mut func.body);
        }

        // Fold constants in main block
        total_folds += self.optimize_stmts(&mut program.main);

        total_folds
    }

    /// Optimize a list of statements
    fn optimize_stmts(&mut self, stmts: &mut [AotStmt]) -> usize {
        let mut total_folds = 0;

        for stmt in stmts.iter_mut() {
            total_folds += self.optimize_stmt(stmt);
        }

        total_folds
    }

    /// Optimize a single statement
    fn optimize_stmt(&mut self, stmt: &mut AotStmt) -> usize {
        match stmt {
            AotStmt::Let { value, .. } => {
                let (new_expr, folds) = self.fold_expr(value);
                if folds > 0 {
                    *value = new_expr;
                    self.fold_count += folds;
                }
                folds
            }
            AotStmt::Assign { target, value } => {
                let mut folds = 0;
                let (new_target, t_folds) = self.fold_expr(target);
                if t_folds > 0 {
                    *target = new_target;
                    folds += t_folds;
                }
                let (new_value, v_folds) = self.fold_expr(value);
                if v_folds > 0 {
                    *value = new_value;
                    folds += v_folds;
                }
                self.fold_count += folds;
                folds
            }
            AotStmt::CompoundAssign { target, value, .. } => {
                let mut folds = 0;
                let (new_target, t_folds) = self.fold_expr(target);
                if t_folds > 0 {
                    *target = new_target;
                    folds += t_folds;
                }
                let (new_value, v_folds) = self.fold_expr(value);
                if v_folds > 0 {
                    *value = new_value;
                    folds += v_folds;
                }
                self.fold_count += folds;
                folds
            }
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => {
                let (new_expr, folds) = self.fold_expr(expr);
                if folds > 0 {
                    *expr = new_expr;
                    self.fold_count += folds;
                }
                folds
            }
            AotStmt::Return(Some(expr)) => {
                let (new_expr, folds) = self.fold_expr(expr);
                if folds > 0 {
                    *expr = new_expr;
                    self.fold_count += folds;
                }
                folds
            }
            AotStmt::Return(None) => 0,
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut folds = 0;
                let (new_cond, c_folds) = self.fold_expr(condition);
                if c_folds > 0 {
                    *condition = new_cond;
                    folds += c_folds;
                }
                folds += self.optimize_stmts(then_branch);
                if let Some(else_b) = else_branch {
                    folds += self.optimize_stmts(else_b);
                }
                self.fold_count += folds;
                folds
            }
            AotStmt::While { condition, body } => {
                let mut folds = 0;
                let (new_cond, c_folds) = self.fold_expr(condition);
                if c_folds > 0 {
                    *condition = new_cond;
                    folds += c_folds;
                }
                folds += self.optimize_stmts(body);
                self.fold_count += folds;
                folds
            }
            AotStmt::ForRange {
                start,
                stop,
                step,
                body,
                ..
            } => {
                let mut folds = 0;
                let (new_start, s_folds) = self.fold_expr(start);
                if s_folds > 0 {
                    *start = new_start;
                    folds += s_folds;
                }
                let (new_stop, st_folds) = self.fold_expr(stop);
                if st_folds > 0 {
                    *stop = new_stop;
                    folds += st_folds;
                }
                if let Some(step_expr) = step {
                    let (new_step, stp_folds) = self.fold_expr(step_expr);
                    if stp_folds > 0 {
                        *step_expr = new_step;
                        folds += stp_folds;
                    }
                }
                folds += self.optimize_stmts(body);
                self.fold_count += folds;
                folds
            }
            AotStmt::ForEach { iter, body, .. } => {
                let mut folds = 0;
                let (new_iter, i_folds) = self.fold_expr(iter);
                if i_folds > 0 {
                    *iter = new_iter;
                    folds += i_folds;
                }
                folds += self.optimize_stmts(body);
                self.fold_count += folds;
                folds
            }
            AotStmt::Break | AotStmt::Continue => 0,
        }
    }

    /// Fold an expression, returning the folded expression and number of folds
    pub(super) fn fold_expr(&self, expr: &AotExpr) -> (AotExpr, usize) {
        match expr {
            // Binary operations on literals
            AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty,
            } => {
                // First, recursively fold the operands
                let (folded_left, left_folds) = self.fold_expr(left);
                let (folded_right, right_folds) = self.fold_expr(right);

                // Try to fold the operation itself
                if let Some(result) = self.try_fold_binop(*op, &folded_left, &folded_right) {
                    return (result, left_folds + right_folds + 1);
                }

                // If we can't fold but operands changed, return new expr
                if left_folds + right_folds > 0 {
                    return (
                        AotExpr::BinOpStatic {
                            op: *op,
                            left: Box::new(folded_left),
                            right: Box::new(folded_right),
                            result_ty: result_ty.clone(),
                        },
                        left_folds + right_folds,
                    );
                }

                (expr.clone(), 0)
            }

            // Unary operations on literals
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => {
                let (folded_operand, op_folds) = self.fold_expr(operand);

                if let Some(result) = self.try_fold_unary(*op, &folded_operand) {
                    return (result, op_folds + 1);
                }

                if op_folds > 0 {
                    return (
                        AotExpr::UnaryOp {
                            op: *op,
                            operand: Box::new(folded_operand),
                            result_ty: result_ty.clone(),
                        },
                        op_folds,
                    );
                }

                (expr.clone(), 0)
            }

            // Ternary with constant condition
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => {
                let (folded_cond, cond_folds) = self.fold_expr(condition);
                let (folded_then, then_folds) = self.fold_expr(then_expr);
                let (folded_else, else_folds) = self.fold_expr(else_expr);

                // If condition is a constant bool, we can eliminate the ternary
                if let AotExpr::LitBool(b) = folded_cond {
                    if b {
                        return (folded_then, cond_folds + then_folds + else_folds + 1);
                    } else {
                        return (folded_else, cond_folds + then_folds + else_folds + 1);
                    }
                }

                let total_folds = cond_folds + then_folds + else_folds;
                if total_folds > 0 {
                    return (
                        AotExpr::Ternary {
                            condition: Box::new(folded_cond),
                            then_expr: Box::new(folded_then),
                            else_expr: Box::new(folded_else),
                            result_ty: result_ty.clone(),
                        },
                        total_folds,
                    );
                }

                (expr.clone(), 0)
            }

            // Fold inside array literals
            AotExpr::ArrayLit {
                elements,
                elem_ty,
                shape,
            } => {
                let mut new_elements = Vec::with_capacity(elements.len());
                let mut total_folds = 0;
                for elem in elements {
                    let (folded, folds) = self.fold_expr(elem);
                    new_elements.push(folded);
                    total_folds += folds;
                }
                if total_folds > 0 {
                    return (
                        AotExpr::ArrayLit {
                            elements: new_elements,
                            elem_ty: elem_ty.clone(),
                            shape: shape.clone(),
                        },
                        total_folds,
                    );
                }
                (expr.clone(), 0)
            }

            AotExpr::SetFromIter { iter, elem_ty } => {
                let (folded_iter, folds) = self.fold_expr(iter);
                if folds > 0 {
                    return (
                        AotExpr::SetFromIter {
                            iter: Box::new(folded_iter),
                            elem_ty: elem_ty.clone(),
                        },
                        folds,
                    );
                }
                (expr.clone(), 0)
            }

            // Fold inside tuple literals
            AotExpr::TupleLit { elements } => {
                let mut new_elements = Vec::with_capacity(elements.len());
                let mut total_folds = 0;
                for elem in elements {
                    let (folded, folds) = self.fold_expr(elem);
                    new_elements.push(folded);
                    total_folds += folds;
                }
                if total_folds > 0 {
                    return (
                        AotExpr::TupleLit {
                            elements: new_elements,
                        },
                        total_folds,
                    );
                }
                (expr.clone(), 0)
            }

            // Fold inside function calls
            AotExpr::CallStatic {
                function,
                args,
                return_ty,
                inline_policy,
            } => {
                let mut new_args = Vec::with_capacity(args.len());
                let mut total_folds = 0;
                for arg in args {
                    let (folded, folds) = self.fold_expr(arg);
                    new_args.push(folded);
                    total_folds += folds;
                }
                if total_folds > 0 {
                    return (
                        AotExpr::CallStatic {
                            function: function.clone(),
                            args: new_args,
                            return_ty: return_ty.clone(),
                            inline_policy: *inline_policy,
                        },
                        total_folds,
                    );
                }
                (expr.clone(), 0)
            }

            // Fold inside builtin calls
            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } => {
                let mut new_args = Vec::with_capacity(args.len());
                let mut total_folds = 0;
                for arg in args {
                    let (folded, folds) = self.fold_expr(arg);
                    new_args.push(folded);
                    total_folds += folds;
                }

                // Try to fold builtin calls with constant args
                if let Some(result) = self.try_fold_builtin(*builtin, &new_args) {
                    return (result, total_folds + 1);
                }

                if total_folds > 0 {
                    return (
                        AotExpr::CallBuiltin {
                            builtin: *builtin,
                            args: new_args,
                            return_ty: return_ty.clone(),
                        },
                        total_folds,
                    );
                }
                (expr.clone(), 0)
            }

            // Literals and other expressions don't need folding
            _ => (expr.clone(), 0),
        }
    }

    /// Try to fold a binary operation on two expressions
    fn try_fold_binop(&self, op: AotBinOp, left: &AotExpr, right: &AotExpr) -> Option<AotExpr> {
        match (left, right) {
            // Integer operations
            (AotExpr::LitI64(a), AotExpr::LitI64(b)) => self.fold_i64_binop(op, *a, *b),
            (AotExpr::LitI32(a), AotExpr::LitI32(b)) => self.fold_i32_binop(op, *a, *b),

            // Float operations
            (AotExpr::LitF64(a), AotExpr::LitF64(b)) => self.fold_f64_binop(op, *a, *b),
            (AotExpr::LitF32(a), AotExpr::LitF32(b)) => self.fold_f32_binop(op, *a, *b),

            // Boolean operations
            (AotExpr::LitBool(a), AotExpr::LitBool(b)) => self.fold_bool_binop(op, *a, *b),

            // Mixed int/float (promote to float)
            (AotExpr::LitI64(a), AotExpr::LitF64(b)) => self.fold_f64_binop(op, *a as f64, *b),
            (AotExpr::LitF64(a), AotExpr::LitI64(b)) => self.fold_f64_binop(op, *a, *b as f64),

            // String concatenation
            (AotExpr::LitStr(a), AotExpr::LitStr(b)) if op == AotBinOp::Add => {
                Some(AotExpr::LitStr(format!("{}{}", a, b)))
            }
            (AotExpr::LitStr(a), AotExpr::LitStr(b)) if op == AotBinOp::Mul => {
                Some(AotExpr::LitStr(format!("{}{}", a, b)))
            }
            (AotExpr::LitStr(a), AotExpr::LitChar(b)) if op == AotBinOp::Mul => {
                Some(AotExpr::LitStr(format!("{}{}", a, b)))
            }
            (AotExpr::LitChar(a), AotExpr::LitStr(b)) if op == AotBinOp::Mul => {
                Some(AotExpr::LitStr(format!("{}{}", a, b)))
            }
            (AotExpr::LitChar(a), AotExpr::LitChar(b)) if op == AotBinOp::Mul => {
                Some(AotExpr::LitStr(format!("{}{}", a, b)))
            }

            _ => None,
        }
    }

    /// Fold i64 binary operation
    fn fold_i64_binop(&self, op: AotBinOp, a: i64, b: i64) -> Option<AotExpr> {
        match op {
            AotBinOp::Add => Some(AotExpr::LitI64(a.wrapping_add(b))),
            AotBinOp::Sub => Some(AotExpr::LitI64(a.wrapping_sub(b))),
            AotBinOp::Mul => Some(AotExpr::LitI64(a.wrapping_mul(b))),
            AotBinOp::Div => {
                if b != 0 {
                    Some(AotExpr::LitF64(a as f64 / b as f64))
                } else {
                    None
                }
            }
            AotBinOp::IntDiv => {
                if b != 0 {
                    Some(AotExpr::LitI64(a / b))
                } else {
                    None
                }
            }
            AotBinOp::Mod => {
                if b != 0 {
                    Some(AotExpr::LitI64(a % b))
                } else {
                    None
                }
            }
            AotBinOp::Pow => {
                if (0..=63).contains(&b) {
                    Some(AotExpr::LitI64(a.wrapping_pow(b as u32)))
                } else {
                    Some(AotExpr::LitF64((a as f64).powf(b as f64)))
                }
            }
            AotBinOp::Lt => Some(AotExpr::LitBool(a < b)),
            AotBinOp::Gt => Some(AotExpr::LitBool(a > b)),
            AotBinOp::Le => Some(AotExpr::LitBool(a <= b)),
            AotBinOp::Ge => Some(AotExpr::LitBool(a >= b)),
            AotBinOp::Eq => Some(AotExpr::LitBool(a == b)),
            AotBinOp::Ne => Some(AotExpr::LitBool(a != b)),
            AotBinOp::Egal => Some(AotExpr::LitBool(a == b)),
            AotBinOp::NotEgal => Some(AotExpr::LitBool(a != b)),
            AotBinOp::BitAnd => Some(AotExpr::LitI64(a & b)),
            AotBinOp::BitOr => Some(AotExpr::LitI64(a | b)),
            AotBinOp::BitXor => Some(AotExpr::LitI64(a ^ b)),
            // Julia shift semantics: over-shift → 0 (or sign fill for `>>`),
            // negative amount shifts the other direction. NOT Rust's masked
            // `b & 63` (Issue #7057). Must match the runtime SjuliaShift helpers
            // so constant-folded and unfolded shifts agree.
            AotBinOp::Shl => Some(AotExpr::LitI64(julia_shl_i64(a, b))),
            AotBinOp::Shr => Some(AotExpr::LitI64(julia_ashr_i64(a, b))),
            AotBinOp::And | AotBinOp::Or | AotBinOp::Subtype => None, // Not applicable to integers
        }
    }

    /// Fold i32 binary operation
    fn fold_i32_binop(&self, op: AotBinOp, a: i32, b: i32) -> Option<AotExpr> {
        match op {
            AotBinOp::Add => Some(AotExpr::LitI32(a.wrapping_add(b))),
            AotBinOp::Sub => Some(AotExpr::LitI32(a.wrapping_sub(b))),
            AotBinOp::Mul => Some(AotExpr::LitI32(a.wrapping_mul(b))),
            AotBinOp::Div => {
                if b != 0 {
                    Some(AotExpr::LitF64(a as f64 / b as f64))
                } else {
                    None
                }
            }
            AotBinOp::IntDiv => {
                if b != 0 {
                    Some(AotExpr::LitI32(a / b))
                } else {
                    None
                }
            }
            AotBinOp::Mod => {
                if b != 0 {
                    Some(AotExpr::LitI32(a % b))
                } else {
                    None
                }
            }
            AotBinOp::Lt => Some(AotExpr::LitBool(a < b)),
            AotBinOp::Gt => Some(AotExpr::LitBool(a > b)),
            AotBinOp::Le => Some(AotExpr::LitBool(a <= b)),
            AotBinOp::Ge => Some(AotExpr::LitBool(a >= b)),
            AotBinOp::Eq => Some(AotExpr::LitBool(a == b)),
            AotBinOp::Ne => Some(AotExpr::LitBool(a != b)),
            _ => None,
        }
    }

    /// Fold f64 binary operation
    fn fold_f64_binop(&self, op: AotBinOp, a: f64, b: f64) -> Option<AotExpr> {
        match op {
            AotBinOp::Add => Some(AotExpr::LitF64(a + b)),
            AotBinOp::Sub => Some(AotExpr::LitF64(a - b)),
            AotBinOp::Mul => Some(AotExpr::LitF64(a * b)),
            AotBinOp::Div => Some(AotExpr::LitF64(a / b)),
            AotBinOp::Pow => Some(AotExpr::LitF64(a.powf(b))),
            AotBinOp::Lt => Some(AotExpr::LitBool(a < b)),
            AotBinOp::Gt => Some(AotExpr::LitBool(a > b)),
            AotBinOp::Le => Some(AotExpr::LitBool(a <= b)),
            AotBinOp::Ge => Some(AotExpr::LitBool(a >= b)),
            AotBinOp::Eq => Some(AotExpr::LitBool(a == b)),
            AotBinOp::Ne => Some(AotExpr::LitBool(a != b)),
            _ => None,
        }
    }

    /// Fold f32 binary operation
    fn fold_f32_binop(&self, op: AotBinOp, a: f32, b: f32) -> Option<AotExpr> {
        match op {
            AotBinOp::Add => Some(AotExpr::LitF32(a + b)),
            AotBinOp::Sub => Some(AotExpr::LitF32(a - b)),
            AotBinOp::Mul => Some(AotExpr::LitF32(a * b)),
            AotBinOp::Div => Some(AotExpr::LitF32(a / b)),
            AotBinOp::Pow => Some(AotExpr::LitF32(a.powf(b))),
            AotBinOp::Lt => Some(AotExpr::LitBool(a < b)),
            AotBinOp::Gt => Some(AotExpr::LitBool(a > b)),
            AotBinOp::Le => Some(AotExpr::LitBool(a <= b)),
            AotBinOp::Ge => Some(AotExpr::LitBool(a >= b)),
            AotBinOp::Eq => Some(AotExpr::LitBool(a == b)),
            AotBinOp::Ne => Some(AotExpr::LitBool(a != b)),
            _ => None,
        }
    }

    /// Fold boolean binary operation
    fn fold_bool_binop(&self, op: AotBinOp, a: bool, b: bool) -> Option<AotExpr> {
        match op {
            AotBinOp::And => Some(AotExpr::LitBool(a && b)),
            AotBinOp::Or => Some(AotExpr::LitBool(a || b)),
            AotBinOp::Eq => Some(AotExpr::LitBool(a == b)),
            AotBinOp::Ne => Some(AotExpr::LitBool(a != b)),
            AotBinOp::Egal => Some(AotExpr::LitBool(a == b)),
            AotBinOp::NotEgal => Some(AotExpr::LitBool(a != b)),
            _ => None,
        }
    }

    /// Try to fold a unary operation
    fn try_fold_unary(&self, op: AotUnaryOp, operand: &AotExpr) -> Option<AotExpr> {
        match (op, operand) {
            (AotUnaryOp::Neg, AotExpr::LitI64(v)) => Some(AotExpr::LitI64(-*v)),
            (AotUnaryOp::Neg, AotExpr::LitI32(v)) => Some(AotExpr::LitI32(-*v)),
            (AotUnaryOp::Neg, AotExpr::LitF64(v)) => Some(AotExpr::LitF64(-*v)),
            (AotUnaryOp::Neg, AotExpr::LitF32(v)) => Some(AotExpr::LitF32(-*v)),
            (AotUnaryOp::Not, AotExpr::LitBool(v)) => Some(AotExpr::LitBool(!*v)),
            (AotUnaryOp::BitNot, AotExpr::LitI64(v)) => Some(AotExpr::LitI64(!*v)),
            (AotUnaryOp::BitNot, AotExpr::LitI32(v)) => Some(AotExpr::LitI32(!*v)),
            _ => None,
        }
    }

    /// Try to fold a builtin call with constant arguments
    fn try_fold_builtin(&self, builtin: AotBuiltinOp, args: &[AotExpr]) -> Option<AotExpr> {
        match builtin {
            AotBuiltinOp::Abs => {
                if args.len() == 1 {
                    match &args[0] {
                        AotExpr::LitI64(v) => Some(AotExpr::LitI64(v.abs())),
                        AotExpr::LitI32(v) => Some(AotExpr::LitI32(v.abs())),
                        AotExpr::LitF64(v) => Some(AotExpr::LitF64(v.abs())),
                        AotExpr::LitF32(v) => Some(AotExpr::LitF32(v.abs())),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            AotBuiltinOp::Sqrt => {
                if args.len() == 1 {
                    match &args[0] {
                        AotExpr::LitF64(v) if *v >= 0.0 => Some(AotExpr::LitF64(v.sqrt())),
                        AotExpr::LitI64(v) if *v >= 0 => Some(AotExpr::LitF64((*v as f64).sqrt())),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            AotBuiltinOp::Floor => {
                if args.len() == 1 {
                    match &args[0] {
                        AotExpr::LitF64(v) => Some(AotExpr::LitF64(v.floor())),
                        AotExpr::LitF32(v) => Some(AotExpr::LitF32(v.floor())),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            AotBuiltinOp::Ceil => {
                if args.len() == 1 {
                    match &args[0] {
                        AotExpr::LitF64(v) => Some(AotExpr::LitF64(v.ceil())),
                        AotExpr::LitF32(v) => Some(AotExpr::LitF32(v.ceil())),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            AotBuiltinOp::Round => {
                if args.len() == 1 {
                    match &args[0] {
                        // Julia's default RoundNearest is round-half-to-even.
                        AotExpr::LitF64(v) => Some(AotExpr::LitF64(v.round_ties_even())),
                        AotExpr::LitF32(v) => Some(AotExpr::LitF32(v.round_ties_even())),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            AotBuiltinOp::Min => {
                if args.len() == 2 {
                    match (&args[0], &args[1]) {
                        (AotExpr::LitI64(a), AotExpr::LitI64(b)) => {
                            Some(AotExpr::LitI64((*a).min(*b)))
                        }
                        (AotExpr::LitF64(a), AotExpr::LitF64(b)) => {
                            Some(AotExpr::LitF64(a.min(*b)))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            AotBuiltinOp::Max => {
                if args.len() == 2 {
                    match (&args[0], &args[1]) {
                        (AotExpr::LitI64(a), AotExpr::LitI64(b)) => {
                            Some(AotExpr::LitI64((*a).max(*b)))
                        }
                        (AotExpr::LitF64(a), AotExpr::LitF64(b)) => {
                            Some(AotExpr::LitF64(a.max(*b)))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            AotBuiltinOp::Length => {
                if args.len() == 1 {
                    match &args[0] {
                        AotExpr::ArrayLit { elements, .. } => {
                            Some(AotExpr::LitI64(elements.len() as i64))
                        }
                        AotExpr::TupleLit { elements } => {
                            Some(AotExpr::LitI64(elements.len() as i64))
                        }
                        AotExpr::LitStr(s) => Some(AotExpr::LitI64(s.len() as i64)),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Optimize an AoT program with constant folding
pub fn optimize_aot_program_with_constant_folding(program: &mut AotProgram) -> usize {
    let mut folder = AotConstantFolder::new();
    folder.optimize_program(program)
}
