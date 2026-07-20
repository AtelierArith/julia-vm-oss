//! Tail-call optimization for direct self recursion in AoT IR.

use crate::aot::ir::{AotExpr, AotFunction, AotProgram, AotStmt};
use crate::aot::types::StaticType;
use std::collections::HashSet;

/// Direct self-tail-recursion elimination for high-level AoT IR.
#[derive(Debug, Default)]
pub struct AotTailRecursionOptimizer {
    transform_count: usize,
    temp_counter: usize,
}

impl AotTailRecursionOptimizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn transform_count(&self) -> usize {
        self.transform_count
    }

    pub fn optimize_program(&mut self, program: &mut AotProgram) -> usize {
        let mut transforms = 0;

        for func in &mut program.functions {
            transforms += self.optimize_function(func);
        }

        transforms
    }

    fn optimize_function(&mut self, func: &mut AotFunction) -> usize {
        let self_names = HashSet::from([func.name.clone(), func.mangled_name()]);
        let mut body = func.body.clone();
        let transforms = self.rewrite_tail_calls_in_stmts(&mut body, &self_names, &func.params);

        if transforms == 0 {
            return 0;
        }

        func.body = vec![AotStmt::While {
            condition: AotExpr::LitBool(true),
            body,
        }];
        self.transform_count += transforms;
        transforms
    }

    fn rewrite_tail_calls_in_stmts(
        &mut self,
        stmts: &mut Vec<AotStmt>,
        self_names: &HashSet<String>,
        params: &[(String, StaticType)],
    ) -> usize {
        let mut transforms = 0;
        let mut i = 0;

        while i < stmts.len() {
            match &mut stmts[i] {
                AotStmt::Return(Some(expr)) => {
                    if let Some(replacement) = self.tail_call_replacement(expr, self_names, params)
                    {
                        let replacement_len = replacement.len();
                        stmts.splice(i..=i, replacement);
                        transforms += 1;
                        i += replacement_len;
                        continue;
                    }
                }
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    transforms += self.rewrite_tail_calls_in_stmts(then_branch, self_names, params);
                    if let Some(else_branch) = else_branch {
                        transforms +=
                            self.rewrite_tail_calls_in_stmts(else_branch, self_names, params);
                    }
                }
                AotStmt::While { .. } | AotStmt::ForRange { .. } | AotStmt::ForEach { .. } => {
                    // `continue` would target the nested loop, not the TCO loop.
                }
                AotStmt::Let { .. }
                | AotStmt::Assign { .. }
                | AotStmt::CompoundAssign { .. }
                | AotStmt::Expr(_)
                | AotStmt::ValueCarrier(_)
                | AotStmt::Return(None)
                | AotStmt::Break
                | AotStmt::Continue => {}
            }

            i += 1;
        }

        transforms
    }

    fn tail_call_replacement(
        &mut self,
        expr: &AotExpr,
        self_names: &HashSet<String>,
        params: &[(String, StaticType)],
    ) -> Option<Vec<AotStmt>> {
        let AotExpr::CallStatic { function, args, .. } = expr else {
            return None;
        };
        if !self_names.contains(function) || args.len() != params.len() {
            return None;
        }

        let mut replacement = Vec::with_capacity(params.len() * 2 + 1);
        let mut temp_bindings = Vec::with_capacity(params.len());

        for ((param_name, param_ty), arg) in params.iter().zip(args) {
            let temp_name = format!("__sjulia_tco_{}_{}", self.temp_counter, param_name);
            self.temp_counter += 1;
            replacement.push(AotStmt::Let {
                name: temp_name.clone(),
                ty: param_ty.clone(),
                value: arg.clone(),
                is_mutable: false,
            });
            temp_bindings.push((param_name.clone(), param_ty.clone(), temp_name));
        }

        for (param_name, param_ty, temp_name) in temp_bindings {
            replacement.push(AotStmt::Assign {
                target: AotExpr::Var {
                    name: param_name,
                    ty: param_ty.clone(),
                },
                value: AotExpr::Var {
                    name: temp_name,
                    ty: param_ty,
                },
            });
        }

        replacement.push(AotStmt::Continue);
        Some(replacement)
    }
}

pub fn optimize_aot_program_with_tail_recursion(program: &mut AotProgram) -> usize {
    let mut optimizer = AotTailRecursionOptimizer::new();
    optimizer.optimize_program(program)
}
