//! Loop-nest emission for primitive rectangular comprehensions.
//!
//! Julia evaluates `[body for i in a:b, j in c:d]` in column-major order: the
//! first iteration variable advances fastest, so it becomes the innermost loop.

use crate::aot::ir::{
    AotExpr, BasicBlock, BinOpKind, ConstValue, Instruction, IrFunction, Terminator, VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::comprehension::Axis;
use super::Lowerer;

impl Lowerer<'_, '_> {
    /// Emit the loop nest, then the element store at the innermost level.
    pub(super) fn axis_loops(
        &mut self,
        order: &[usize],
        axes: &[Axis],
        body: &AotExpr,
        elem_ty: &StaticType,
        result: &VarRef,
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let Some((axis_index, rest)) = order.split_first() else {
            return self.store_element(axes, body, elem_ty, result, function);
        };
        let axis = &axes[*axis_index];
        let one = self.constant(ConstValue::Int64(1), StaticType::I64, function)?;
        self.current_block_mut(function)?.push(Instruction::Copy {
            dest: axis.counter.clone(),
            src: one.clone(),
        });
        let cond_label = self.label("compr_cond");
        let body_label = self.label("compr_body");
        let exit_label = self.label("compr_exit");
        self.current_block_mut(function)?
            .set_terminator(Terminator::Jump(cond_label.clone()));
        function.add_block(BasicBlock::new(cond_label.clone()));
        self.current = cond_label.clone();
        let active = self.temporary(StaticType::Bool);
        self.current_block_mut(function)?.push(Instruction::BinOp {
            dest: active.clone(),
            op: BinOpKind::Le,
            left: axis.counter.clone(),
            right: axis.length.clone(),
        });
        self.current_block_mut(function)?
            .set_terminator(Terminator::Branch {
                cond: active,
                then_block: body_label.clone(),
                else_block: exit_label.clone(),
            });
        function.add_block(BasicBlock::new(body_label.clone()));
        self.current = body_label;
        self.bind_axis_value(axis, &one, function)?;
        self.axis_loops(rest, axes, body, elem_ty, result, function)?;
        let next = self.temporary(StaticType::I64);
        self.current_block_mut(function)?.push(Instruction::BinOp {
            dest: next.clone(),
            op: BinOpKind::Add,
            left: axis.counter.clone(),
            right: one,
        });
        self.current_block_mut(function)?.push(Instruction::Copy {
            dest: axis.counter.clone(),
            src: next,
        });
        self.jump_if_open(&cond_label, function)?;
        function.add_block(BasicBlock::new(exit_label.clone()));
        self.current = exit_label;
        Ok(())
    }

    /// Bind the Julia iteration variable to `start + counter - 1`.
    fn bind_axis_value(
        &mut self,
        axis: &Axis,
        one: &VarRef,
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let offset = self.temporary(StaticType::I64);
        self.current_block_mut(function)?.push(Instruction::BinOp {
            dest: offset.clone(),
            op: BinOpKind::Sub,
            left: axis.counter.clone(),
            right: one.clone(),
        });
        let value = self.temporary(StaticType::I64);
        self.current_block_mut(function)?.push(Instruction::BinOp {
            dest: value.clone(),
            op: BinOpKind::Add,
            left: axis.start.clone(),
            right: offset,
        });
        self.vars.insert(axis.var.clone(), value);
        Ok(())
    }

    fn store_element(
        &mut self,
        axes: &[Axis],
        body: &AotExpr,
        elem_ty: &StaticType,
        result: &VarRef,
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let value = self.expr(body, function)?;
        let value = if value.ty == *elem_ty {
            value
        } else {
            let converted = self.temporary(elem_ty.clone());
            self.current_block_mut(function)?.push(Instruction::Copy {
                dest: converted.clone(),
                src: value,
            });
            converted
        };
        let indices = axes.iter().map(|axis| axis.counter.clone()).collect();
        self.current_block_mut(function)?
            .push(Instruction::SetIndex {
                array: result.clone(),
                indices,
                value,
            });
        Ok(())
    }
}
