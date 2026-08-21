//! Loop-nest and fused element evaluation for single-array broadcast.
//!
//! Axis zero is the innermost loop so elements are visited in the column-major
//! order the descriptor lays out. Each operand is converted to the operation's
//! unified operand type before it is applied, matching Julia's promotion.

use crate::aot::ir::{
    broadcast_node_ty, BasicBlock, BinOpKind, BroadcastNode, BroadcastOp, BroadcastPlan,
    ConstValue, Instruction, IrFunction, Terminator, VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::Lowerer;

pub(super) struct BroadcastLoop<'a> {
    pub(super) counters: &'a [VarRef],
    pub(super) dims: &'a [VarRef],
    pub(super) source: &'a VarRef,
    pub(super) element_ty: &'a StaticType,
    pub(super) plan: &'a BroadcastPlan,
    pub(super) result: &'a VarRef,
}

impl Lowerer<'_, '_> {
    pub(super) fn broadcast_axes(
        &mut self,
        order: &[usize],
        context: &BroadcastLoop<'_>,
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let Some((axis, rest)) = order.split_first() else {
            return self.broadcast_store(context, function);
        };
        let counter = context.counters[*axis].clone();
        let one = self.constant(ConstValue::Int64(1), StaticType::I64, function)?;
        self.current_block_mut(function)?.push(Instruction::Copy {
            dest: counter.clone(),
            src: one.clone(),
        });
        let cond_label = self.label("bcast_cond");
        let body_label = self.label("bcast_body");
        let exit_label = self.label("bcast_exit");
        self.current_block_mut(function)?
            .set_terminator(Terminator::Jump(cond_label.clone()));
        function.add_block(BasicBlock::new(cond_label.clone()));
        self.current = cond_label.clone();
        let active = self.temporary(StaticType::Bool);
        self.current_block_mut(function)?.push(Instruction::BinOp {
            dest: active.clone(),
            op: BinOpKind::Le,
            left: counter.clone(),
            right: context.dims[*axis].clone(),
        });
        self.current_block_mut(function)?
            .set_terminator(Terminator::Branch {
                cond: active,
                then_block: body_label.clone(),
                else_block: exit_label.clone(),
            });
        function.add_block(BasicBlock::new(body_label.clone()));
        self.current = body_label;
        self.broadcast_axes(rest, context, function)?;
        let next = self.temporary(StaticType::I64);
        self.current_block_mut(function)?.push(Instruction::BinOp {
            dest: next.clone(),
            op: BinOpKind::Add,
            left: counter.clone(),
            right: one,
        });
        self.current_block_mut(function)?.push(Instruction::Copy {
            dest: counter,
            src: next,
        });
        self.jump_if_open(&cond_label, function)?;
        function.add_block(BasicBlock::new(exit_label.clone()));
        self.current = exit_label;
        Ok(())
    }

    fn broadcast_store(
        &mut self,
        context: &BroadcastLoop<'_>,
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let element = self.temporary(context.element_ty.clone());
        self.current_block_mut(function)?
            .push(Instruction::GetIndex {
                dest: element.clone(),
                array: context.source.clone(),
                indices: context.counters.to_vec(),
            });
        let value = self.eval_node(&context.plan.node, &element, context.element_ty, function)?;
        let value = self.converted(value, &context.plan.elem_ty, function)?;
        self.current_block_mut(function)?
            .push(Instruction::SetIndex {
                array: context.result.clone(),
                indices: context.counters.to_vec(),
                value,
            });
        Ok(())
    }

    fn eval_node(
        &mut self,
        node: &BroadcastNode,
        element: &VarRef,
        element_ty: &StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        match node {
            BroadcastNode::Element => Ok(element.clone()),
            BroadcastNode::Scalar(expr) => self.expr(expr, function),
            BroadcastNode::Apply { op, args } => {
                let arg_types = args
                    .iter()
                    .map(|arg| broadcast_node_ty(arg, element_ty))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| unsupported("Wasm broadcast operand type is not static"))?;
                let operand_ty = op.operand_ty(&arg_types).ok_or_else(|| {
                    unsupported("Wasm broadcast operand types have no Julia promotion")
                })?;
                let result_ty = op.result_ty(&arg_types).ok_or_else(|| {
                    unsupported("Wasm broadcast has no statically known Julia result type")
                })?;
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    let value = self.eval_node(arg, element, element_ty, function)?;
                    values.push(self.converted(value, &operand_ty, function)?);
                }
                self.apply_op(*op, values, result_ty, function)
            }
        }
    }

    fn apply_op(
        &mut self,
        op: BroadcastOp,
        values: Vec<VarRef>,
        result_ty: StaticType,
        function: &mut IrFunction,
    ) -> AotResult<VarRef> {
        let dest = self.temporary(result_ty);
        match op {
            BroadcastOp::Binary(kind) => {
                let [left, right] = values.as_slice() else {
                    return Err(unsupported(
                        "Wasm broadcast binary operations take exactly two operands",
                    ));
                };
                self.current_block_mut(function)?.push(Instruction::BinOp {
                    dest: dest.clone(),
                    op: kind,
                    left: left.clone(),
                    right: right.clone(),
                });
            }
            BroadcastOp::Builtin(builtin) => {
                self.current_block_mut(function)?
                    .push(Instruction::Builtin {
                        dest: dest.clone(),
                        op: builtin,
                        args: values,
                    });
            }
        }
        Ok(dest)
    }
}
