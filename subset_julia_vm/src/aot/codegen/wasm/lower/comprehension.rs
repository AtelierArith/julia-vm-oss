//! Recognition and shape planning for primitive rectangular comprehensions.
//!
//! The result array is allocated once from the statically ranked axis lengths;
//! `comprehension_loops` then writes every element in place, so no
//! intermediate array is built.

use crate::aot::ir::{AotExpr, ArrayInit, Instruction, IrFunction, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::ops::ensure_type;
use super::Lowerer;

pub(super) struct Axis {
    pub(super) var: String,
    pub(super) start: VarRef,
    pub(super) length: VarRef,
    pub(super) counter: VarRef,
}

impl Lowerer<'_, '_> {
    pub(super) fn comprehension(
        &mut self,
        expr: &AotExpr,
        function: &mut IrFunction,
    ) -> AotResult<Option<VarRef>> {
        match expr {
            AotExpr::Convert { value, target_ty }
                if matches!(
                    value.as_ref(),
                    AotExpr::Comprehension { .. } | AotExpr::MultiComprehension { .. }
                ) && matches!(target_ty, StaticType::Array { .. }) =>
            {
                self.rectangular(value, Some(target_ty), function).map(Some)
            }
            AotExpr::Comprehension { .. } | AotExpr::MultiComprehension { .. } => {
                self.rectangular(expr, None, function).map(Some)
            }
            _ => Ok(None),
        }
    }

    fn rectangular(
        &mut self,
        expr: &AotExpr,
        result_ty: Option<&StaticType>,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let (body, iterations, filter, elem_ty) = match expr {
            AotExpr::Comprehension {
                body,
                var,
                iter,
                filter,
                elem_ty,
            } => (
                body.as_ref(),
                vec![(var.clone(), iter.as_ref().clone())],
                filter,
                elem_ty,
            ),
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                elem_ty,
            } => (body.as_ref(), iterations.clone(), filter, elem_ty),
            _ => {
                return Err(unsupported(
                    "Wasm comprehension lowering requires a comprehension expression",
                ))
            }
        };
        if filter.is_some() {
            return Err(unsupported(
                "Wasm AoT comprehensions do not support `if` filters because the result length is data dependent",
            ));
        }
        ensure_type(elem_ty)?;
        if iterations.is_empty() {
            return Err(unsupported("Wasm comprehensions require at least one axis"));
        }
        let axes = iterations
            .iter()
            .map(|(var, iter)| self.axis(var, iter, function))
            .collect::<AotResult<Vec<_>>>()?;
        let result_ty = self.comprehension_result_ty(result_ty, elem_ty, axes.len())?;
        let dims = axes.iter().map(|axis| axis.length.clone()).collect();
        let result = self.temporary(result_ty);
        self.current_block_mut(function)?
            .push(Instruction::ArrayNew {
                dest: result.clone(),
                dims,
                init: ArrayInit::Zero,
            });
        // Outermost loop is the last axis so that axis zero advances fastest.
        let order = (0..axes.len()).rev().collect::<Vec<_>>();
        let prior_bindings = axes
            .iter()
            .map(|axis| (axis.var.clone(), self.vars.get(&axis.var).cloned()))
            .collect::<Vec<_>>();
        let loop_result = self.axis_loops(&order, &axes, body, elem_ty, &result, function);
        for (name, prior) in prior_bindings {
            match prior {
                Some(value) => {
                    self.vars.insert(name, value);
                }
                None => {
                    self.vars.remove(&name);
                }
            }
        }
        loop_result?;
        Ok(result)
    }

    fn comprehension_result_ty(
        &self,
        declared: Option<&StaticType>,
        elem_ty: &StaticType,
        rank: usize,
    ) -> AotResult<StaticType> {
        let Some(declared) = declared else {
            return Ok(StaticType::Array {
                element: Box::new(elem_ty.clone()),
                ndims: Some(rank),
            });
        };
        match declared {
            StaticType::Array {
                ndims: Some(declared_rank),
                ..
            } if *declared_rank == rank => Ok(declared.clone()),
            _ => Err(unsupported(
                "Wasm comprehension result rank must match its iteration axes",
            )),
        }
    }

    fn axis(&mut self, var: &str, iter: &AotExpr, function: &mut IrFunction) -> AotResult<Axis> {
        let AotExpr::Range {
            start,
            stop,
            step,
            elem_ty,
        } = iter
        else {
            return Err(unsupported(
                "Wasm comprehensions iterate unit-step Int64 ranges",
            ));
        };
        if step.is_some() {
            return Err(unsupported(
                "Wasm comprehensions require unit-step ranges; stepped ranges are unsupported",
            ));
        }
        if *elem_ty != StaticType::I64 {
            return Err(unsupported(
                "Wasm comprehension ranges require Int64 bounds",
            ));
        }
        let start = self.expr(start, function)?;
        let stop = self.expr(stop, function)?;
        let length = self.unit_range_length(&start, &stop, function)?;
        Ok(Axis {
            var: var.to_string(),
            start,
            length,
            counter: self.temporary(StaticType::I64),
        })
    }
}
