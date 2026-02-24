//! Strength Reduction optimization for AoT IR
//!
//! This module implements strength reduction optimizations that replace
//! expensive operations with cheaper equivalents.

use crate::aot::ir::{AotBinOp, AotExpr, AotProgram, AotStmt};
use crate::aot::types::StaticType;

/// Strength reduction optimizer for AoT IR
///
/// Performs the following transformations:
/// - Multiplication by power of 2 -> left shift (`x * 2^n` -> `x << n`)
/// - Integer division by power of 2 -> right shift (`x / 2^n` -> `x >> n`)
/// - Small integer exponentiation -> multiplication chain (`x^2` -> `x * x`)
#[derive(Debug, Default)]
pub struct AotStrengthReducer {
    /// Number of reductions performed
    reduction_count: usize,
}

impl AotStrengthReducer {
    /// Create a new strength reducer
    pub fn new() -> Self {
        Self { reduction_count: 0 }
    }

    /// Get the number of reductions performed
    pub fn reduction_count(&self) -> usize {
        self.reduction_count
    }

    /// Optimize an AoT program with strength reduction
    pub fn optimize_program(&mut self, program: &mut AotProgram) -> usize {
        let mut total_reductions = 0;

        // Reduce in functions
        for func in &mut program.functions {
            total_reductions += self.optimize_stmts(&mut func.body);
        }

        // Reduce in main block
        total_reductions += self.optimize_stmts(&mut program.main);

        total_reductions
    }

    /// Optimize a list of statements
    fn optimize_stmts(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        let mut total = 0;

        for stmt in stmts.iter_mut() {
            total += self.optimize_stmt(stmt);
        }

        total
    }

    /// Optimize a single statement
    fn optimize_stmt(&mut self, stmt: &mut AotStmt) -> usize {
        match stmt {
            AotStmt::Let { value, .. } => {
                let (new_expr, reductions) = self.reduce_expr(value);
                if reductions > 0 {
                    *value = new_expr;
                    self.reduction_count += reductions;
                }
                reductions
            }
            AotStmt::Assign { target, value } => {
                let mut reductions = 0;
                let (new_target, t_red) = self.reduce_expr(target);
                if t_red > 0 {
                    *target = new_target;
                    reductions += t_red;
                }
                let (new_value, v_red) = self.reduce_expr(value);
                if v_red > 0 {
                    *value = new_value;
                    reductions += v_red;
                }
                self.reduction_count += reductions;
                reductions
            }
            AotStmt::CompoundAssign { target, value, .. } => {
                let mut reductions = 0;
                let (new_target, t_red) = self.reduce_expr(target);
                if t_red > 0 {
                    *target = new_target;
                    reductions += t_red;
                }
                let (new_value, v_red) = self.reduce_expr(value);
                if v_red > 0 {
                    *value = new_value;
                    reductions += v_red;
                }
                self.reduction_count += reductions;
                reductions
            }
            AotStmt::Expr(expr) => {
                let (new_expr, reductions) = self.reduce_expr(expr);
                if reductions > 0 {
                    *expr = new_expr;
                    self.reduction_count += reductions;
                }
                reductions
            }
            AotStmt::Return(Some(expr)) => {
                let (new_expr, reductions) = self.reduce_expr(expr);
                if reductions > 0 {
                    *expr = new_expr;
                    self.reduction_count += reductions;
                }
                reductions
            }
            AotStmt::Return(None) => 0,
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut reductions = 0;
                let (new_cond, c_red) = self.reduce_expr(condition);
                if c_red > 0 {
                    *condition = new_cond;
                    reductions += c_red;
                }
                reductions += self.optimize_stmts(then_branch);
                if let Some(else_b) = else_branch {
                    reductions += self.optimize_stmts(else_b);
                }
                self.reduction_count += reductions;
                reductions
            }
            AotStmt::While { condition, body } => {
                let mut reductions = 0;
                let (new_cond, c_red) = self.reduce_expr(condition);
                if c_red > 0 {
                    *condition = new_cond;
                    reductions += c_red;
                }
                reductions += self.optimize_stmts(body);
                self.reduction_count += reductions;
                reductions
            }
            AotStmt::ForRange {
                start,
                stop,
                step,
                body,
                ..
            } => {
                let mut reductions = 0;
                let (new_start, s_red) = self.reduce_expr(start);
                if s_red > 0 {
                    *start = new_start;
                    reductions += s_red;
                }
                let (new_stop, st_red) = self.reduce_expr(stop);
                if st_red > 0 {
                    *stop = new_stop;
                    reductions += st_red;
                }
                if let Some(step_expr) = step {
                    let (new_step, stp_red) = self.reduce_expr(step_expr);
                    if stp_red > 0 {
                        *step_expr = new_step;
                        reductions += stp_red;
                    }
                }
                reductions += self.optimize_stmts(body);
                self.reduction_count += reductions;
                reductions
            }
            AotStmt::ForEach { iter, body, .. } => {
                let mut reductions = 0;
                let (new_iter, i_red) = self.reduce_expr(iter);
                if i_red > 0 {
                    *iter = new_iter;
                    reductions += i_red;
                }
                reductions += self.optimize_stmts(body);
                self.reduction_count += reductions;
                reductions
            }
            AotStmt::Break | AotStmt::Continue => 0,
        }
    }

    /// Reduce an expression
    fn reduce_expr(&self, expr: &AotExpr) -> (AotExpr, usize) {
        match expr {
            // Binary operations - check for strength reduction opportunities
            AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty,
            } => {
                // First recursively reduce operands
                let (reduced_left, l_red) = self.reduce_expr(left);
                let (reduced_right, r_red) = self.reduce_expr(right);

                // Try to apply strength reduction
                if let Some(reduced) =
                    self.try_reduce_binop(*op, &reduced_left, &reduced_right, result_ty)
                {
                    return (reduced, l_red + r_red + 1);
                }

                // If operands were reduced but binop wasn't, return updated expr
                if l_red + r_red > 0 {
                    return (
                        AotExpr::BinOpStatic {
                            op: *op,
                            left: Box::new(reduced_left),
                            right: Box::new(reduced_right),
                            result_ty: result_ty.clone(),
                        },
                        l_red + r_red,
                    );
                }

                (expr.clone(), 0)
            }

            AotExpr::BinOpDynamic { op, left, right } => {
                let (reduced_left, l_red) = self.reduce_expr(left);
                let (reduced_right, r_red) = self.reduce_expr(right);

                if l_red + r_red > 0 {
                    return (
                        AotExpr::BinOpDynamic {
                            op: *op,
                            left: Box::new(reduced_left),
                            right: Box::new(reduced_right),
                        },
                        l_red + r_red,
                    );
                }

                (expr.clone(), 0)
            }

            // Unary operations - recurse
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => {
                let (reduced_operand, red) = self.reduce_expr(operand);
                if red > 0 {
                    return (
                        AotExpr::UnaryOp {
                            op: *op,
                            operand: Box::new(reduced_operand),
                            result_ty: result_ty.clone(),
                        },
                        red,
                    );
                }
                (expr.clone(), 0)
            }

            // Ternary - recurse
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => {
                let (red_cond, c_red) = self.reduce_expr(condition);
                let (red_then, t_red) = self.reduce_expr(then_expr);
                let (red_else, e_red) = self.reduce_expr(else_expr);

                let total = c_red + t_red + e_red;
                if total > 0 {
                    return (
                        AotExpr::Ternary {
                            condition: Box::new(red_cond),
                            then_expr: Box::new(red_then),
                            else_expr: Box::new(red_else),
                            result_ty: result_ty.clone(),
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            // Array literal - recurse into elements
            AotExpr::ArrayLit {
                elements,
                elem_ty,
                shape,
            } => {
                let mut new_elements = Vec::with_capacity(elements.len());
                let mut total = 0;
                for elem in elements {
                    let (reduced, red) = self.reduce_expr(elem);
                    new_elements.push(reduced);
                    total += red;
                }
                if total > 0 {
                    return (
                        AotExpr::ArrayLit {
                            elements: new_elements,
                            elem_ty: elem_ty.clone(),
                            shape: shape.clone(),
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            // Tuple literal - recurse
            AotExpr::TupleLit { elements } => {
                let mut new_elements = Vec::with_capacity(elements.len());
                let mut total = 0;
                for elem in elements {
                    let (reduced, red) = self.reduce_expr(elem);
                    new_elements.push(reduced);
                    total += red;
                }
                if total > 0 {
                    return (
                        AotExpr::TupleLit {
                            elements: new_elements,
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            // Function calls - recurse into arguments
            AotExpr::CallStatic {
                function,
                args,
                return_ty,
            } => {
                let mut new_args = Vec::with_capacity(args.len());
                let mut total = 0;
                for arg in args {
                    let (reduced, red) = self.reduce_expr(arg);
                    new_args.push(reduced);
                    total += red;
                }
                if total > 0 {
                    return (
                        AotExpr::CallStatic {
                            function: function.clone(),
                            args: new_args,
                            return_ty: return_ty.clone(),
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            AotExpr::CallDynamic { function, args } => {
                let mut new_args = Vec::with_capacity(args.len());
                let mut total = 0;
                for arg in args {
                    let (reduced, red) = self.reduce_expr(arg);
                    new_args.push(reduced);
                    total += red;
                }
                if total > 0 {
                    return (
                        AotExpr::CallDynamic {
                            function: function.clone(),
                            args: new_args,
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } => {
                let mut new_args = Vec::with_capacity(args.len());
                let mut total = 0;
                for arg in args {
                    let (reduced, red) = self.reduce_expr(arg);
                    new_args.push(reduced);
                    total += red;
                }
                if total > 0 {
                    return (
                        AotExpr::CallBuiltin {
                            builtin: *builtin,
                            args: new_args,
                            return_ty: return_ty.clone(),
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            // Index - recurse into array and indices
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple,
            } => {
                let (reduced_array, a_red) = self.reduce_expr(array);
                let mut new_indices = Vec::with_capacity(indices.len());
                let mut i_total = 0;
                for idx in indices {
                    let (reduced, red) = self.reduce_expr(idx);
                    new_indices.push(reduced);
                    i_total += red;
                }
                let total = a_red + i_total;
                if total > 0 {
                    return (
                        AotExpr::Index {
                            array: Box::new(reduced_array),
                            indices: new_indices,
                            elem_ty: elem_ty.clone(),
                            is_tuple: *is_tuple,
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            // Range - recurse
            AotExpr::Range {
                start,
                stop,
                step,
                elem_ty,
            } => {
                let (red_start, s_red) = self.reduce_expr(start);
                let (red_stop, st_red) = self.reduce_expr(stop);
                let (red_step, stp_red) = if let Some(s) = step {
                    let (r, red) = self.reduce_expr(s);
                    (Some(Box::new(r)), red)
                } else {
                    (None, 0)
                };
                let total = s_red + st_red + stp_red;
                if total > 0 {
                    return (
                        AotExpr::Range {
                            start: Box::new(red_start),
                            stop: Box::new(red_stop),
                            step: red_step,
                            elem_ty: elem_ty.clone(),
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            // Struct operations - recurse
            AotExpr::StructNew { name, fields } => {
                let mut new_fields = Vec::with_capacity(fields.len());
                let mut total = 0;
                for field in fields {
                    let (reduced, red) = self.reduce_expr(field);
                    new_fields.push(reduced);
                    total += red;
                }
                if total > 0 {
                    return (
                        AotExpr::StructNew {
                            name: name.clone(),
                            fields: new_fields,
                        },
                        total,
                    );
                }
                (expr.clone(), 0)
            }

            AotExpr::FieldAccess {
                object,
                field,
                field_ty,
            } => {
                let (reduced_obj, red) = self.reduce_expr(object);
                if red > 0 {
                    return (
                        AotExpr::FieldAccess {
                            object: Box::new(reduced_obj),
                            field: field.clone(),
                            field_ty: field_ty.clone(),
                        },
                        red,
                    );
                }
                (expr.clone(), 0)
            }

            // Box/Unbox - recurse
            AotExpr::Box(inner) => {
                let (reduced, red) = self.reduce_expr(inner);
                if red > 0 {
                    return (AotExpr::Box(Box::new(reduced)), red);
                }
                (expr.clone(), 0)
            }

            AotExpr::Unbox { value, target_ty } => {
                let (reduced, red) = self.reduce_expr(value);
                if red > 0 {
                    return (
                        AotExpr::Unbox {
                            value: Box::new(reduced),
                            target_ty: target_ty.clone(),
                        },
                        red,
                    );
                }
                (expr.clone(), 0)
            }

            AotExpr::Convert { value, target_ty } => {
                let (reduced, red) = self.reduce_expr(value);
                if red > 0 {
                    return (
                        AotExpr::Convert {
                            value: Box::new(reduced),
                            target_ty: target_ty.clone(),
                        },
                        red,
                    );
                }
                (expr.clone(), 0)
            }

            // Literals and variables - no reduction
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing
            | AotExpr::Var { .. } => (expr.clone(), 0),

            // Lambda expressions - recurse into body
            AotExpr::Lambda {
                params,
                body,
                captures,
                return_ty,
            } => {
                let (reduced_body, red) = self.reduce_expr(body);
                if red > 0 {
                    return (
                        AotExpr::Lambda {
                            params: params.clone(),
                            body: Box::new(reduced_body),
                            captures: captures.clone(),
                            return_ty: return_ty.clone(),
                        },
                        red,
                    );
                }
                (expr.clone(), 0)
            }
        }
    }

    /// Try to apply strength reduction to a binary operation
    fn try_reduce_binop(
        &self,
        op: AotBinOp,
        left: &AotExpr,
        right: &AotExpr,
        result_ty: &StaticType,
    ) -> Option<AotExpr> {
        match op {
            // x * 2^n -> x << n (for integers)
            AotBinOp::Mul => {
                // Check if result is integer type
                if !self.is_integer_type(result_ty) {
                    return None;
                }

                // Check right operand for power of 2
                if let Some(shift) = self.get_power_of_two_shift(right) {
                    return Some(AotExpr::BinOpStatic {
                        op: AotBinOp::Shl,
                        left: Box::new(left.clone()),
                        right: Box::new(AotExpr::LitI64(shift)),
                        result_ty: result_ty.clone(),
                    });
                }

                // Check left operand for power of 2 (commutative)
                if let Some(shift) = self.get_power_of_two_shift(left) {
                    return Some(AotExpr::BinOpStatic {
                        op: AotBinOp::Shl,
                        left: Box::new(right.clone()),
                        right: Box::new(AotExpr::LitI64(shift)),
                        result_ty: result_ty.clone(),
                    });
                }

                None
            }

            // x / 2^n -> x >> n (for integers, unsigned division semantics)
            AotBinOp::IntDiv => {
                // Check if result is integer type
                if !self.is_integer_type(result_ty) {
                    return None;
                }

                // Check right operand for power of 2
                if let Some(shift) = self.get_power_of_two_shift(right) {
                    return Some(AotExpr::BinOpStatic {
                        op: AotBinOp::Shr,
                        left: Box::new(left.clone()),
                        right: Box::new(AotExpr::LitI64(shift)),
                        result_ty: result_ty.clone(),
                    });
                }

                None
            }

            // x^2 -> x * x, x^3 -> x * x * x
            AotBinOp::Pow => {
                if let Some(exp) = self.get_small_integer_exponent(right) {
                    match exp {
                        0 => {
                            // x^0 = 1 (handled by constant folding if x is known)
                            return Some(AotExpr::LitI64(1));
                        }
                        1 => {
                            // x^1 = x
                            return Some(left.clone());
                        }
                        2 => {
                            // x^2 = x * x
                            return Some(AotExpr::BinOpStatic {
                                op: AotBinOp::Mul,
                                left: Box::new(left.clone()),
                                right: Box::new(left.clone()),
                                result_ty: result_ty.clone(),
                            });
                        }
                        3 => {
                            // x^3 = x * x * x = (x * x) * x
                            let x_squared = AotExpr::BinOpStatic {
                                op: AotBinOp::Mul,
                                left: Box::new(left.clone()),
                                right: Box::new(left.clone()),
                                result_ty: result_ty.clone(),
                            };
                            return Some(AotExpr::BinOpStatic {
                                op: AotBinOp::Mul,
                                left: Box::new(x_squared),
                                right: Box::new(left.clone()),
                                result_ty: result_ty.clone(),
                            });
                        }
                        4 => {
                            // x^4 = (x * x) * (x * x)
                            let x_squared = AotExpr::BinOpStatic {
                                op: AotBinOp::Mul,
                                left: Box::new(left.clone()),
                                right: Box::new(left.clone()),
                                result_ty: result_ty.clone(),
                            };
                            return Some(AotExpr::BinOpStatic {
                                op: AotBinOp::Mul,
                                left: Box::new(x_squared.clone()),
                                right: Box::new(x_squared),
                                result_ty: result_ty.clone(),
                            });
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }

            _ => None,
        }
    }

    /// Check if a type is an integer type
    fn is_integer_type(&self, ty: &StaticType) -> bool {
        matches!(ty, StaticType::I64 | StaticType::I32)
    }

    /// Get the shift amount if the expression is a power of 2 literal
    fn get_power_of_two_shift(&self, expr: &AotExpr) -> Option<i64> {
        match expr {
            AotExpr::LitI64(v) if *v > 0 && (*v & (*v - 1)) == 0 => Some((*v as f64).log2() as i64),
            AotExpr::LitI32(v) if *v > 0 && (*v & (*v - 1)) == 0 => Some((*v as f64).log2() as i64),
            _ => None,
        }
    }

    /// Get small integer exponent (0-4) for power optimization
    fn get_small_integer_exponent(&self, expr: &AotExpr) -> Option<i64> {
        match expr {
            AotExpr::LitI64(v) if *v >= 0 && *v <= 4 => Some(*v),
            AotExpr::LitI32(v) if *v >= 0 && *v <= 4 => Some(*v as i64),
            AotExpr::LitF64(v) if *v >= 0.0 && *v <= 4.0 && v.fract() == 0.0 => Some(*v as i64),
            _ => None,
        }
    }
}

/// Optimize an AoT program with strength reduction
pub fn optimize_aot_program_with_strength_reduction(program: &mut AotProgram) -> usize {
    let mut reducer = AotStrengthReducer::new();
    reducer.optimize_program(program)
}
