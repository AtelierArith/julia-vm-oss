use crate::ir::core::Stmt;
use crate::vm::instr::Instr;
use crate::vm::ValueType;

use super::super::type_helpers::join_type;
use super::super::CoreCompiler;
use super::super::CResult;

impl CoreCompiler<'_> {
    pub(super) fn compile_try_stmt(&mut self, stmt: &Stmt) -> CResult<Option<()>> {
        let Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            ..
        } = stmt
        else {
            return Ok(None);
        };

        let has_catch = catch_block.is_some();
        let has_else = else_block.is_some();
        let has_finally = finally_block.is_some();
        let mut finally_handler_positions: Vec<usize> = Vec::new();
        let mut jump_positions: Vec<usize> = Vec::new();

        if let Some(fb) = finally_block {
            self.finally_stack.push(super::super::FinallyContext {
                finally_block: fb.clone(),
                loop_depth: self.loop_stack.len(),
            });
        }

        let handler_pos = self.here();
        self.emit(Instr::PushHandler(None, None));

        self.compile_block(try_block)?;
        self.emit(Instr::PopHandler);

        if has_else {
            if has_finally {
                let finally_handler_pos = self.here();
                self.emit(Instr::PushHandler(None, Some(usize::MAX)));
                finally_handler_positions.push(finally_handler_pos);
            }
            if let Some(else_block) = else_block {
                self.compile_block(else_block)?;
            }
            if has_finally {
                self.emit(Instr::PopHandler);
            }
            let j = self.here();
            self.emit(Instr::Jump(usize::MAX));
            jump_positions.push(j);
        } else {
            let j = self.here();
            self.emit(Instr::Jump(usize::MAX));
            jump_positions.push(j);
        }

        let locals_after_try = self.locals.clone();
        let catch_start = self.here();
        if has_catch {
            if has_finally {
                let finally_handler_pos = self.here();
                self.emit(Instr::PushHandler(None, Some(usize::MAX)));
                finally_handler_positions.push(finally_handler_pos);
            }
            if let Some(var) = catch_var {
                self.locals.insert(var.clone(), ValueType::Any);
                self.emit(Instr::PushExceptionValue);
                self.emit(Instr::StoreAny(var.clone()));
            }
            self.emit(Instr::ClearError);
            if let Some(catch_block) = catch_block {
                self.compile_block(catch_block)?;
            }
            if has_finally {
                self.emit(Instr::PopHandler);
            }
            let j = self.here();
            self.emit(Instr::Jump(usize::MAX));
            jump_positions.push(j);

            for (name, try_ty) in &locals_after_try {
                // At runtime either the try-path or the catch-path ran, so we cannot
                // commit to either type alone — use join_type() to widen to Any when
                // the two paths disagree. (Issue #3044)
                let catch_ty = self.locals.get(name).cloned().unwrap_or(ValueType::Any);
                self.locals.insert(name.clone(), join_type(try_ty, &catch_ty));
            }
        }

        let finally_start = self.here();
        if let Some(finally_block) = finally_block {
            self.compile_block(finally_block)?;
            self.emit(Instr::Rethrow);
        }

        let end = self.here();
        for jump_pos in jump_positions {
            self.patch_jump(jump_pos, if has_finally { finally_start } else { end });
        }

        let catch_ip = if has_catch { Some(catch_start) } else { None };
        let finally_ip = if has_finally && !has_catch {
            Some(finally_start)
        } else {
            None
        };
        self.code[handler_pos] = Instr::PushHandler(catch_ip, finally_ip);

        for pos in finally_handler_positions.drain(..) {
            self.code[pos] = Instr::PushHandler(None, Some(finally_start));
        }

        if has_finally {
            self.finally_stack.pop();
        }

        Ok(Some(()))
    }
}
