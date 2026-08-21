use crate::aot::ir::{AotExpr, AotStmt, BasicBlock, Instruction, IrFunction, Terminator, VarRef};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

use super::super::types::unsupported;
use super::ops::ensure_type;
use super::Lowerer;

impl Lowerer<'_, '_> {
    pub(super) fn stmt(&mut self, stmt: &AotStmt, function: &mut IrFunction) -> AotResult<()> {
        match stmt {
            AotStmt::Let {
                name, ty, value, ..
            } => self.bind(name, ty, value, function),
            AotStmt::Assign {
                target: AotExpr::Var { name, ty },
                value,
            } => self.bind(name, ty, value, function),
            AotStmt::Assign {
                target:
                    AotExpr::Index {
                        array,
                        indices,
                        is_tuple: false,
                        ..
                    },
                value,
            } if array.get_type() == StaticType::Str => Err(unsupported(
                "Wasm AoT string literals are immutable; dynamic string mutation is not supported",
            )),
            AotStmt::Assign {
                target:
                    AotExpr::Index {
                        array,
                        indices,
                        is_tuple: false,
                        ..
                    },
                value,
            } => {
                let element_type = match array.get_type() {
                    StaticType::Array { element, .. } => *element,
                    other => {
                        return Err(unsupported(format!(
                            "Wasm indexed assignment requires an array, got `{}`",
                            other.julia_type_name()
                        )))
                    }
                };
                let array = self.expr(array, function)?;
                let indices = indices
                    .iter()
                    .map(|index| self.expr(index, function))
                    .collect::<AotResult<Vec<_>>>()?;
                let source = self.expr(value, function)?;
                let value = if source.ty == element_type {
                    source
                } else {
                    let converted = self.temporary(element_type);
                    self.current_block_mut(function)?.push(Instruction::Copy {
                        dest: converted.clone(),
                        src: source,
                    });
                    converted
                };
                self.current_block_mut(function)?
                    .push(Instruction::SetIndex {
                        array,
                        indices,
                        value,
                    });
                Ok(())
            }
            AotStmt::Assign {
                target: AotExpr::Index { is_tuple: true, .. },
                ..
            }
            | AotStmt::Assign {
                target: AotExpr::FieldAccess { .. },
                ..
            } => Err(unsupported(
                "Wasm tuple and isbits-struct values are immutable",
            )),
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => {
                self.expr(expr, function).map(|_| ())
            }
            AotStmt::Return(value) => {
                let value = value
                    .as_ref()
                    .map(|expr| self.expr(expr, function))
                    .transpose()?;
                self.current_block_mut(function)?
                    .set_terminator(Terminator::Return(value));
                Ok(())
            }
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => self.if_stmt(
                condition,
                then_branch,
                else_branch.as_deref().unwrap_or(&[]),
                function,
            ),
            AotStmt::While { condition, body } => self.while_stmt(condition, body, function),
            _ => Err(unsupported(format!(
                "Wasm AoT cannot lower statement `{stmt:?}`"
            ))),
        }
    }

    fn bind(
        &mut self,
        name: &str,
        ty: &StaticType,
        value: &AotExpr,
        function: &mut IrFunction,
    ) -> AotResult<()> {
        ensure_type(ty)?;
        let src = self.expr(value, function)?;
        let dest = VarRef::new(name.to_string(), ty.clone());
        self.current_block_mut(function)?.push(Instruction::Copy {
            dest: dest.clone(),
            src,
        });
        self.vars.insert(name.to_string(), dest);
        Ok(())
    }

    fn if_stmt(
        &mut self,
        condition: &AotExpr,
        then_stmts: &[AotStmt],
        else_stmts: &[AotStmt],
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let cond = self.expr(condition, function)?;
        let then_label = self.label("if_then");
        let else_label = self.label("if_else");
        let join_label = self.label("if_join");
        self.current_block_mut(function)?
            .set_terminator(Terminator::Branch {
                cond,
                then_block: then_label.clone(),
                else_block: else_label.clone(),
            });
        function.add_block(BasicBlock::new(then_label.clone()));
        self.current = then_label;
        self.stmts(then_stmts, function)?;
        let then_falls_through = self.current_block(function)?.terminator.is_none();
        if then_falls_through {
            self.jump_if_open(&join_label, function)?;
        }
        function.add_block(BasicBlock::new(else_label.clone()));
        self.current = else_label;
        self.stmts(else_stmts, function)?;
        let else_falls_through = self.current_block(function)?.terminator.is_none();
        if else_falls_through {
            self.jump_if_open(&join_label, function)?;
        }
        if then_falls_through || else_falls_through {
            function.add_block(BasicBlock::new(join_label.clone()));
            self.current = join_label;
        }
        Ok(())
    }

    fn while_stmt(
        &mut self,
        condition: &AotExpr,
        body: &[AotStmt],
        function: &mut IrFunction,
    ) -> AotResult<()> {
        let cond_label = self.label("while_cond");
        let body_label = self.label("while_body");
        let exit_label = self.label("while_exit");
        self.current_block_mut(function)?
            .set_terminator(Terminator::Jump(cond_label.clone()));
        function.add_block(BasicBlock::new(cond_label.clone()));
        self.current = cond_label.clone();
        let cond = self.expr(condition, function)?;
        self.current_block_mut(function)?
            .set_terminator(Terminator::Branch {
                cond,
                then_block: body_label.clone(),
                else_block: exit_label.clone(),
            });
        function.add_block(BasicBlock::new(body_label.clone()));
        self.current = body_label;
        self.stmts(body, function)?;
        self.jump_if_open(&cond_label, function)?;
        function.add_block(BasicBlock::new(exit_label.clone()));
        self.current = exit_label;
        Ok(())
    }
}
