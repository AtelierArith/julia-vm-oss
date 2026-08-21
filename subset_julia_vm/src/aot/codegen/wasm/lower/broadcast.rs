//! Wasm lowering for single-array broadcast trees.
//!
//! Every axis extent is read from the source descriptor before the result is
//! allocated, so a malformed descriptor traps before any allocation. The result
//! is allocated once and each element is computed in place from the fused tree,
//! so a chain such as `clamp.(x .+ s .* f, lo, hi)` builds no intermediate array.
//! The source is only ever read, so noncontiguous and zero-stride host inputs
//! stay valid and unmodified.

use crate::aot::ir::{
    broadcast_plan, ArrayInit, BroadcastPlan, ConstValue, Instruction, IrFunction, VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::broadcast_loops::BroadcastLoop;
use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn broadcast(
        &mut self,
        expr: &crate::aot::ir::AotExpr,
        function: &mut IrFunction,
    ) -> AotResult<Option<VarRef>> {
        let plan = match broadcast_plan(expr) {
            Ok(None) => return Ok(None),
            Ok(Some(plan)) => plan,
            Err(reject) => return Err(unsupported(reject.message())),
        };
        self.emit_broadcast(&plan, function).map(Some)
    }

    fn emit_broadcast(
        &mut self,
        plan: &BroadcastPlan,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let source = self.expr(&plan.source, function)?;
        let StaticType::Array {
            element,
            ndims: Some(rank),
        } = source.ty.clone()
        else {
            return Err(unsupported(
                "Wasm broadcast requires a statically ranked array operand",
            ));
        };
        let mut dims = Vec::with_capacity(rank);
        for axis in 1..=rank {
            dims.push(self.size_axis(&source, axis, function)?);
        }
        let result = self.temporary(StaticType::Array {
            element: Box::new(plan.elem_ty.clone()),
            ndims: Some(rank),
        });
        self.current_block_mut(function)?
            .push(Instruction::ArrayNew {
                dest: result.clone(),
                dims: dims.clone(),
                init: ArrayInit::Zero,
            });
        let counters = (0..rank)
            .map(|_| self.temporary(StaticType::I64))
            .collect::<Vec<_>>();
        let order = (0..rank).rev().collect::<Vec<_>>();
        let context = BroadcastLoop {
            counters: &counters,
            dims: &dims,
            source: &source,
            element_ty: &element,
            plan,
            result: &result,
        };
        self.broadcast_axes(&order, &context, function)?;
        Ok(result)
    }

    fn size_axis(
        &mut self,
        array: &VarRef,
        axis: usize,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let axis =
            i64::try_from(axis).map_err(|_| unsupported("array axis exceeds Julia Int64"))?;
        let axis = self.constant(ConstValue::Int64(axis), StaticType::I64, function)?;
        let dest = self.temporary(StaticType::I64);
        self.current_block_mut(function)?.push(Instruction::Call {
            dest: Some(dest.clone()),
            func: "__sjulia_array_size_axis".to_string(),
            args: vec![array.clone(), axis],
        });
        Ok(dest)
    }

    pub(super) fn converted(
        &mut self,
        value: VarRef,
        ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        if value.ty == *ty {
            return Ok(value);
        }
        let dest = self.temporary(ty.clone());
        self.current_block_mut(function)?.push(Instruction::Copy {
            dest: dest.clone(),
            src: value,
        });
        Ok(dest)
    }
}
