//! Common Subexpression Elimination (CSE) for AoT IR
//!
//! This module implements CSE optimization that identifies expressions
//! computed multiple times and replaces them with references to a single computation.

use crate::aot::ir::{AotBuiltinOp, AotExpr, AotProgram, AotStmt};
use crate::aot::types::StaticType;
use std::collections::{HashMap, HashSet};

/// Common Subexpression Elimination for AoT IR
///
/// This optimization identifies expressions that are computed multiple times
/// and replaces them with references to a single computation.
///
/// # Example
/// ```julia
/// x = a + b
/// y = a + b  # same expression
/// ```
/// Becomes:
/// ```julia
/// _cse_0 = a + b
/// x = _cse_0
/// y = _cse_0
/// ```
#[derive(Debug)]
pub struct AotCSE {
    /// Counter for generating unique CSE variable names
    var_counter: usize,
    /// Number of eliminations performed
    elimination_count: usize,
    /// Set of pure builtin functions that can be CSE'd
    pure_builtins: HashSet<String>,
}

impl Default for AotCSE {
    fn default() -> Self {
        Self::new()
    }
}

impl AotCSE {
    /// Create a new CSE optimizer
    pub fn new() -> Self {
        let mut pure_builtins = HashSet::new();
        // Math functions that are pure (no side effects)
        for name in &[
            "sqrt", "abs", "sin", "cos", "tan", "exp", "log", "floor", "ceil", "round", "trunc",
            "sign", "min", "max", "asin", "acos", "atan",
        ] {
            pure_builtins.insert(name.to_string());
        }
        Self {
            var_counter: 0,
            elimination_count: 0,
            pure_builtins,
        }
    }

    /// Get the number of eliminations performed
    pub fn elimination_count(&self) -> usize {
        self.elimination_count
    }

    /// Generate a unique CSE variable name
    fn gen_cse_var(&mut self) -> String {
        let name = format!("_cse_{}", self.var_counter);
        self.var_counter += 1;
        name
    }

    /// Optimize an AoT program with CSE
    pub fn optimize_program(&mut self, program: &mut AotProgram) -> usize {
        let mut total_eliminations = 0;

        // Optimize each function
        for func in &mut program.functions {
            total_eliminations += self.optimize_stmts(&mut func.body);
        }

        // Optimize main block
        total_eliminations += self.optimize_stmts(&mut program.main);

        total_eliminations
    }

    /// Optimize a list of statements
    fn optimize_stmts(&mut self, stmts: &mut Vec<AotStmt>) -> usize {
        // Map from expression canonical form to (variable name, type)
        let mut expr_map: HashMap<String, (String, StaticType)> = HashMap::new();
        // Track which variables have been modified (invalidates expressions using them)
        let mut modified_vars: HashSet<String> = HashSet::new();

        let mut eliminations = 0;
        let mut i = 0;

        while i < stmts.len() {
            // Clone the statement to avoid borrow conflicts
            let stmt = stmts[i].clone();

            match stmt {
                AotStmt::Let {
                    ref name,
                    ref ty,
                    ref value,
                    is_mutable,
                } => {
                    // Check if this expression can be CSE'd
                    if let Some(canonical) = self.expr_canonical_form(value, &modified_vars) {
                        if let Some((existing_var, _)) = expr_map.get(&canonical) {
                            // Replace with reference to existing computation
                            let new_value = AotExpr::Var {
                                name: existing_var.clone(),
                                ty: ty.clone(),
                            };
                            stmts[i] = AotStmt::Let {
                                name: name.clone(),
                                ty: ty.clone(),
                                value: new_value,
                                is_mutable,
                            };
                            eliminations += 1;
                            self.elimination_count += 1;
                        } else {
                            // Record this expression for future CSE
                            expr_map.insert(canonical, (name.clone(), ty.clone()));
                        }
                    }

                    // If mutable, track it
                    if is_mutable {
                        modified_vars.insert(name.clone());
                        // Invalidate any expressions using this variable
                        self.invalidate_expr_map(&mut expr_map, name);
                    }
                }
                AotStmt::Assign {
                    ref target,
                    ref value,
                } => {
                    // Track variable modification
                    if let AotExpr::Var { ref name, .. } = target {
                        modified_vars.insert(name.clone());
                        // Invalidate expressions using this variable
                        self.invalidate_expr_map(&mut expr_map, name);
                    }

                    // Check if value expression can be CSE'd
                    if let Some(canonical) = self.expr_canonical_form(value, &modified_vars) {
                        if let Some((existing_var, ty)) = expr_map.get(&canonical).cloned() {
                            // Replace value with reference
                            let new_value = AotExpr::Var {
                                name: existing_var,
                                ty,
                            };
                            stmts[i] = AotStmt::Assign {
                                target: target.clone(),
                                value: new_value,
                            };
                            eliminations += 1;
                            self.elimination_count += 1;
                        }
                    }
                }
                AotStmt::CompoundAssign { ref target, .. } => {
                    // Compound assignment modifies the target
                    if let AotExpr::Var { ref name, .. } = target {
                        modified_vars.insert(name.clone());
                        self.invalidate_expr_map(&mut expr_map, name);
                    }
                }
                AotStmt::If {
                    ref condition,
                    ref then_branch,
                    ref else_branch,
                } => {
                    // Recursively optimize branches (with fresh scope)
                    let mut then_stmts = then_branch.clone();
                    eliminations += self.optimize_stmts(&mut then_stmts);

                    let mut else_stmts = else_branch.clone().unwrap_or_default();
                    if !else_stmts.is_empty() {
                        eliminations += self.optimize_stmts(&mut else_stmts);
                    }

                    stmts[i] = AotStmt::If {
                        condition: condition.clone(),
                        then_branch: then_stmts,
                        else_branch: if else_stmts.is_empty() {
                            None
                        } else {
                            Some(else_stmts)
                        },
                    };

                    // After if/else, we can't rely on previous expressions
                    // (variables might have been modified in branches)
                    expr_map.clear();
                }
                AotStmt::While {
                    ref condition,
                    ref body,
                } => {
                    // Optimize loop body with fresh scope
                    let mut body_stmts = body.clone();
                    eliminations += self.optimize_stmts(&mut body_stmts);

                    stmts[i] = AotStmt::While {
                        condition: condition.clone(),
                        body: body_stmts,
                    };

                    // After loop, clear expression map
                    expr_map.clear();
                }
                AotStmt::ForRange {
                    ref var,
                    ref start,
                    ref stop,
                    ref step,
                    ref body,
                } => {
                    // Loop variable is modified
                    modified_vars.insert(var.clone());

                    // Optimize loop body
                    let mut body_stmts = body.clone();
                    eliminations += self.optimize_stmts(&mut body_stmts);

                    stmts[i] = AotStmt::ForRange {
                        var: var.clone(),
                        start: start.clone(),
                        stop: stop.clone(),
                        step: step.clone(),
                        body: body_stmts,
                    };

                    // After loop, clear expression map
                    expr_map.clear();
                }
                AotStmt::ForEach {
                    ref var,
                    ref iter,
                    ref body,
                } => {
                    modified_vars.insert(var.clone());

                    let mut body_stmts = body.clone();
                    eliminations += self.optimize_stmts(&mut body_stmts);

                    stmts[i] = AotStmt::ForEach {
                        var: var.clone(),
                        iter: iter.clone(),
                        body: body_stmts,
                    };

                    expr_map.clear();
                }
                AotStmt::Expr(_) | AotStmt::Return(_) | AotStmt::Break | AotStmt::Continue => {
                    // No variables modified, no CSE opportunity for standalone expr
                }
            }

            i += 1;
        }

        eliminations
    }

    /// Generate a canonical string form of an expression for comparison
    /// Returns None if the expression cannot be CSE'd (has side effects or uses modified vars)
    fn expr_canonical_form(
        &self,
        expr: &AotExpr,
        modified_vars: &HashSet<String>,
    ) -> Option<String> {
        match expr {
            // Literals are not worth CSE'ing
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing => None,

            // Variables - not worth CSE'ing alone
            AotExpr::Var { name, .. } => {
                if modified_vars.contains(name) {
                    None
                } else {
                    None // Variables alone don't need CSE
                }
            }

            // Binary operations - good CSE candidates
            AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty: _,
            } => {
                let left_form = self.expr_operand_form(left, modified_vars)?;
                let right_form = self.expr_operand_form(right, modified_vars)?;
                Some(format!("binop({:?},{},{})", op, left_form, right_form))
            }

            AotExpr::BinOpDynamic { op, left, right } => {
                let left_form = self.expr_operand_form(left, modified_vars)?;
                let right_form = self.expr_operand_form(right, modified_vars)?;
                Some(format!("binop_dyn({:?},{},{})", op, left_form, right_form))
            }

            // Unary operations
            AotExpr::UnaryOp {
                op,
                operand,
                result_ty: _,
            } => {
                let operand_form = self.expr_operand_form(operand, modified_vars)?;
                Some(format!("unary({:?},{})", op, operand_form))
            }

            // Static function calls - only if pure
            AotExpr::CallStatic {
                function,
                args,
                return_ty: _,
            } => {
                if !self.pure_builtins.contains(function) {
                    return None;
                }
                let mut args_form = Vec::new();
                for arg in args {
                    args_form.push(self.expr_operand_form(arg, modified_vars)?);
                }
                Some(format!("call({},{})", function, args_form.join(",")))
            }

            // Builtin calls - check if pure
            AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty: _,
            } => {
                if !self.is_pure_builtin(builtin) {
                    return None;
                }
                let mut args_form = Vec::new();
                for arg in args {
                    args_form.push(self.expr_operand_form(arg, modified_vars)?);
                }
                Some(format!("builtin({:?},{})", builtin, args_form.join(",")))
            }

            // Dynamic calls - not safe to CSE (may have side effects)
            AotExpr::CallDynamic { .. } => None,

            // Array/Index operations - not safe to CSE (array might be modified)
            AotExpr::ArrayLit { .. } | AotExpr::Index { .. } | AotExpr::TupleLit { .. } => None,

            // Field access - could be CSE'd but complex
            AotExpr::FieldAccess { .. } => None,

            // Other expressions - skip
            _ => None,
        }
    }

    /// Generate canonical form for an operand (subexpression)
    fn expr_operand_form(&self, expr: &AotExpr, modified_vars: &HashSet<String>) -> Option<String> {
        match expr {
            AotExpr::LitI64(v) => Some(format!("i64:{}", v)),
            AotExpr::LitI32(v) => Some(format!("i32:{}", v)),
            AotExpr::LitF64(v) => Some(format!("f64:{}", v)),
            AotExpr::LitF32(v) => Some(format!("f32:{}", v)),
            AotExpr::LitBool(v) => Some(format!("bool:{}", v)),
            AotExpr::LitStr(v) => Some(format!("str:{}", v)),
            AotExpr::LitChar(v) => Some(format!("char:{}", v)),
            AotExpr::LitNothing => Some("nothing".to_string()),
            AotExpr::Var { name, .. } => {
                if modified_vars.contains(name) {
                    None
                } else {
                    Some(format!("var:{}", name))
                }
            }
            // For complex subexpressions, use their canonical form
            _ => self.expr_canonical_form(expr, modified_vars),
        }
    }

    /// Check if a builtin operation is pure (no side effects)
    fn is_pure_builtin(&self, builtin: &AotBuiltinOp) -> bool {
        matches!(
            builtin,
            AotBuiltinOp::Sqrt
                | AotBuiltinOp::Abs
                | AotBuiltinOp::Sin
                | AotBuiltinOp::Cos
                | AotBuiltinOp::Tan
                | AotBuiltinOp::Exp
                | AotBuiltinOp::Log
                | AotBuiltinOp::Floor
                | AotBuiltinOp::Ceil
                | AotBuiltinOp::Round
                | AotBuiltinOp::Min
                | AotBuiltinOp::Max
                | AotBuiltinOp::Length
        )
    }

    /// Invalidate expressions in the map that use a modified variable
    fn invalidate_expr_map(&self, expr_map: &mut HashMap<String, (String, StaticType)>, var: &str) {
        let var_pattern = format!("var:{}", var);
        expr_map.retain(|canonical, _| !canonical.contains(&var_pattern));
    }
}

/// Optimize an AoT program with Common Subexpression Elimination
pub fn optimize_aot_program_with_cse(program: &mut AotProgram) -> usize {
    let mut cse = AotCSE::new();
    cse.optimize_program(program)
}
