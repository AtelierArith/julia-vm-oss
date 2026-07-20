use crate::bytecode::ValueType;
use crate::compile::core_compiler::ScopeCleanupContext;
use crate::ir::core::{Block, Stmt};
use std::collections::HashSet;
use subset_julia_vm_bytecode::Instr;

use super::super::type_helpers::join_type;
use super::super::CResult;
use super::super::CoreCompiler;

struct ClauseLexicalMetadataState {
    name: String,
    value_type: Option<ValueType>,
    initialized: bool,
    julia_type: Option<crate::types::JuliaType>,
    known_any_rank_array: bool,
    mixed_type: bool,
}

impl CoreCompiler<'_> {
    fn clause_locals(
        &self,
        block: &Block,
        binder: Option<&String>,
        enclosing: &HashSet<String>,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut inventory = crate::lowering::soft_scope::ScopeBindingInventory::collect(block);
        if let Some(binder) = binder {
            inventory.soft_bindings.insert(binder.clone());
        }
        let mut fresh: Vec<_> = inventory
            .soft_bindings
            .into_iter()
            .filter(|name| {
                !enclosing.contains(name)
                    && !self.initialized_locals.contains(name)
                    && !inventory.explicit_locals.contains(name)
                    && !inventory.globals.contains(name)
            })
            .collect();
        let mut explicit: Vec<_> = inventory
            .explicit_locals
            .into_iter()
            .filter(|name| !inventory.globals.contains(name))
            .collect();
        let mut globals: Vec<_> = inventory.globals.into_iter().collect();
        fresh.sort();
        explicit.sort();
        globals.sort();
        (fresh, explicit, globals)
    }

    fn compile_clause_block(
        &mut self,
        block: &Block,
        binder: Option<&String>,
        enclosing: &HashSet<String>,
        fresh: &[String],
        explicit: &[String],
        declared_globals: &[String],
        nonlocal_pop_handler: bool,
        nonlocal_pop_caught_exception: bool,
    ) -> CResult<ScopeCleanupContext> {
        let explicit_lexical = self.explicit_lexical_scopes;
        let previous_scope = self.lexical_scope_locals.clone();
        let mut clause_scope = enclosing.clone();
        clause_scope.extend(fresh.iter().cloned());
        clause_scope.extend(explicit.iter().cloned());
        if let Some(binder) = binder {
            clause_scope.insert(binder.clone());
        }
        self.lexical_scope_locals = clause_scope;
        let previous_declared_globals = self.declared_globals.clone();
        // At a module body's depth zero, `global x` is a no-op and the normal
        // module store path must retain `current_module_path` qualification.
        // `declared_globals` emits a bare frame-zero store, which is correct
        // only from a genuine local scope (function/let/testset).
        if self.strict_undefined_check || self.local_scope_depth > 0 {
            self.declared_globals
                .extend(declared_globals.iter().cloned());
        }
        for name in explicit.iter().chain(binder) {
            self.declared_globals.remove(name);
        }
        let mut shadows = Vec::new();
        let mut lexical_names: Vec<String> = fresh.iter().chain(explicit.iter()).cloned().collect();
        lexical_names.extend(binder.cloned());
        lexical_names.sort();
        lexical_names.dedup();
        let lexical_metadata = if explicit_lexical {
            lexical_names
                .iter()
                .map(|name| ClauseLexicalMetadataState {
                    name: name.clone(),
                    value_type: self.locals.get(name).cloned(),
                    initialized: self.initialized_locals.contains(name),
                    julia_type: self.julia_type_locals.get(name).cloned(),
                    known_any_rank_array: self.known_any_rank_array_locals.contains(name),
                    mixed_type: self.mixed_type_vars.contains(name),
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let entered_lexical = if explicit_lexical {
            self.enter_explicit_lexical_scope(lexical_names)
        } else {
            for name in explicit {
                shadows.push(self.shadow_local_enter(name)?);
            }
            false
        };
        for name in fresh.iter().chain(explicit.iter()) {
            self.locals.insert(name.clone(), ValueType::Any);
            self.initialized_locals.remove(name);
            self.julia_type_locals.remove(name);
            self.known_any_rank_array_locals.remove(name);
            self.mixed_type_vars.insert(name.clone());
        }
        if let Some(binder) = binder {
            // A catch binder is a fresh lexical binding even when the same
            // name already denotes an initialized module const/import. Make
            // the compiler metadata follow the physical owner before the
            // exception value is stored (Issue #11569).
            self.locals.insert(binder.clone(), ValueType::Any);
            self.initialized_locals.remove(binder);
            self.julia_type_locals.remove(binder);
            self.known_any_rank_array_locals.remove(binder);
            self.mixed_type_vars.insert(binder.clone());
            if explicit_lexical && nonlocal_pop_caught_exception {
                self.emit(Instr::PushExceptionValue);
                self.emit(Instr::StoreAny(binder.clone()));
                self.initialized_locals.insert(binder.clone());
            } else if nonlocal_pop_caught_exception {
                // The legacy frame-slot path stores the exception immediately
                // before entering this clause. Reflect that executed store in
                // compiler metadata so a nested function may capture the catch
                // binder, while still excluding merely prescanned locals.
                self.initialized_locals.insert(binder.clone());
            }
        }
        if explicit_lexical && nonlocal_pop_caught_exception {
            // Capture the pending exception into the lexical catch binder
            // before ClearError moves it to the caught-exception stack. For a
            // binder-less catch this still establishes the caught state before
            // compiling the clause body.
            self.emit(Instr::ClearError);
        }
        let cleanup = ScopeCleanupContext {
            names: if explicit_lexical {
                Vec::new()
            } else {
                fresh.to_vec()
            },
            shadows,
            lexical_scope_count: usize::from(entered_lexical),
            loop_depth: self.loop_stack.len(),
            cleanup_on_loop_exit: true,
            nonlocal_pop_handler,
            nonlocal_pop_caught_exception,
        };
        self.scope_cleanup_stack.push(cleanup.clone());
        let result = self.compile_block(block);
        self.scope_cleanup_stack.pop();
        if result.is_ok() {
            if entered_lexical {
                self.exit_explicit_lexical_scope();
            } else {
                self.emit_scope_cleanup(&cleanup);
            }
        }
        if explicit_lexical {
            if result.is_err() && entered_lexical {
                self.exit_explicit_lexical_scope();
            }
            // Restore only bindings owned by this clause. Whole-map snapshots
            // erase writes to enclosing compiler temporaries such as the
            // `__sjvm_try_result_*` value slot and make a catch expression
            // incorrectly produce `nothing` (Issue #11569).
            for previous in lexical_metadata {
                if let Some(value_type) = previous.value_type {
                    self.locals.insert(previous.name.clone(), value_type);
                } else {
                    self.locals.remove(&previous.name);
                }
                if previous.initialized {
                    self.initialized_locals.insert(previous.name.clone());
                } else {
                    self.initialized_locals.remove(&previous.name);
                }
                if let Some(julia_type) = previous.julia_type {
                    self.julia_type_locals
                        .insert(previous.name.clone(), julia_type);
                } else {
                    self.julia_type_locals.remove(&previous.name);
                }
                if previous.known_any_rank_array {
                    self.known_any_rank_array_locals
                        .insert(previous.name.clone());
                } else {
                    self.known_any_rank_array_locals.remove(&previous.name);
                }
                if previous.mixed_type {
                    self.mixed_type_vars.insert(previous.name);
                } else {
                    self.mixed_type_vars.remove(&previous.name);
                }
            }
        } else {
            for name in fresh {
                self.locals.remove(name);
                self.initialized_locals.remove(name);
                self.julia_type_locals.remove(name);
                self.known_any_rank_array_locals.remove(name);
                self.mixed_type_vars.remove(name);
            }
        }
        self.lexical_scope_locals = previous_scope;
        self.declared_globals = previous_declared_globals;
        result?;
        Ok(cleanup)
    }

    fn emit_scope_cleanup(&mut self, cleanup: &ScopeCleanupContext) {
        for _ in 0..cleanup.lexical_scope_count {
            self.emit(Instr::ExitLexicalScope);
        }
        for shadow in cleanup.shadows.iter().rev().cloned() {
            self.hard_scope_shadow_exit(shadow);
        }
        if !cleanup.names.is_empty() {
            self.emit(Instr::ForgetLetLocals(cleanup.names.clone()));
        }
    }

    /// The VM has already truncated explicit lexical scopes to the handler's
    /// recorded depth before entering an exceptional trampoline. Only legacy
    /// frame-slot cleanup remains here; emitting `ExitLexicalScope` again would
    /// pop an enclosing owner (Issues #11569/#9784).
    fn emit_scope_cleanup_after_vm_unwind(&mut self, cleanup: &ScopeCleanupContext) {
        for shadow in cleanup.shadows.iter().rev().cloned() {
            self.hard_scope_shadow_exit(shadow);
        }
        if !cleanup.names.is_empty() {
            self.emit(Instr::ForgetLetLocals(cleanup.names.clone()));
        }
    }

    fn emit_nonlocal_scope_exit(&mut self, cleanup: &ScopeCleanupContext) {
        self.emit_scope_cleanup(cleanup);
        if cleanup.nonlocal_pop_caught_exception {
            self.emit(Instr::PopCaughtException);
        }
        if cleanup.nonlocal_pop_handler {
            self.emit(Instr::PopHandler);
        }
    }

    pub(crate) fn emit_scope_cleanup_for_return(&mut self) {
        let cleanups: Vec<_> = self.scope_cleanup_stack.iter().rev().cloned().collect();
        for cleanup in &cleanups {
            self.emit_nonlocal_scope_exit(cleanup);
        }
    }

    pub(crate) fn emit_scope_cleanup_for_loop_exit(&mut self, loop_depth: usize) {
        let cleanups: Vec<_> = self
            .scope_cleanup_stack
            .iter()
            .rev()
            .filter(|ctx| ctx.loop_depth >= loop_depth && ctx.cleanup_on_loop_exit)
            .cloned()
            .collect();
        for cleanup in &cleanups {
            self.emit_nonlocal_scope_exit(cleanup);
        }
    }

    pub(crate) fn compile_pending_finally(
        &mut self,
        context: &super::super::FinallyContext,
    ) -> CResult<()> {
        self.compile_clause_block(
            &context.finally_block,
            None,
            &context.enclosing_scope_locals,
            &context.fresh_locals,
            &context.explicit_locals,
            &context.declared_globals,
            true,
            false,
        )?;
        Ok(())
    }

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
        let mut cleanup_handlers: Vec<(usize, ScopeCleanupContext, bool)> = Vec::new();
        let mut enclosing_scope = self.lexical_scope_locals.clone();
        // Main/module compilation does not pre-populate
        // `lexical_scope_locals`, but `initialized_locals` records bindings
        // that have actually executed earlier in source order. In lenient
        // top-level soft scope those bindings remain shared with each try
        // clause, while a type-pre-scanned but not-yet-initialized name is a
        // fresh clause local. Strict function compilation relies exclusively
        // on its lexical inventory and must not inherit this top-level rule.
        if !self.strict_undefined_check
            && self.current_module_path.is_none()
            && self.local_scope_depth == 0
        {
            enclosing_scope.extend(self.initialized_locals.iter().cloned());
            enclosing_scope.extend(self.preexisting_global_bindings.iter().cloned());
        }
        let (try_fresh, try_explicit, try_globals) =
            self.clause_locals(try_block, None, &enclosing_scope);
        let (catch_fresh, catch_explicit, catch_globals) = catch_block
            .as_ref()
            .map(|block| self.clause_locals(block, catch_var.as_ref(), &enclosing_scope))
            .unwrap_or_default();
        let (else_fresh, else_explicit, else_globals) = else_block
            .as_ref()
            .map(|block| self.clause_locals(block, None, &enclosing_scope))
            .unwrap_or_default();
        let (finally_fresh, finally_explicit, finally_globals) = finally_block
            .as_ref()
            .map(|block| self.clause_locals(block, None, &enclosing_scope))
            .unwrap_or_default();

        if let Some(fb) = finally_block {
            self.finally_stack.push(super::super::FinallyContext {
                finally_block: fb.clone(),
                loop_depth: self.loop_stack.len(),
                fresh_locals: finally_fresh.clone(),
                explicit_locals: finally_explicit.clone(),
                declared_globals: finally_globals.clone(),
                enclosing_scope_locals: enclosing_scope.clone(),
            });
        }

        let handler_pos = self.here();
        self.emit(Instr::PushHandler(None, None));

        self.enter_catchable_runtime_error_region();
        let try_compile_result = self.compile_clause_block(
            try_block,
            None,
            &enclosing_scope,
            &try_fresh,
            &try_explicit,
            &try_globals,
            true,
            false,
        );
        self.exit_catchable_runtime_error_region();
        let try_cleanup = try_compile_result?;
        self.emit(Instr::PopHandler);

        if has_else {
            let else_needs_cleanup = !else_fresh.is_empty() || !else_explicit.is_empty();
            let mut else_handler_pos = None;
            if has_finally || else_needs_cleanup {
                let finally_handler_pos = self.here();
                self.emit(Instr::PushHandler(None, Some(usize::MAX)));
                else_handler_pos = Some(finally_handler_pos);
                if has_finally && !else_needs_cleanup {
                    finally_handler_positions.push(finally_handler_pos);
                }
            }
            let mut else_cleanup = None;
            if let Some(else_block) = else_block {
                else_cleanup = Some(self.compile_clause_block(
                    else_block,
                    None,
                    &enclosing_scope,
                    &else_fresh,
                    &else_explicit,
                    &else_globals,
                    has_finally || else_needs_cleanup,
                    false,
                )?);
            }
            if let (Some(pos), Some(cleanup)) = (else_handler_pos, else_cleanup) {
                if else_needs_cleanup {
                    cleanup_handlers.push((pos, cleanup, has_finally));
                }
            }
            if has_finally || else_needs_cleanup {
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
        self.emit_scope_cleanup_after_vm_unwind(&try_cleanup);
        if has_catch {
            let catch_needs_cleanup = !catch_fresh.is_empty() || !catch_explicit.is_empty();
            let mut catch_handler_pos = None;
            if has_finally || catch_needs_cleanup {
                let finally_handler_pos = self.here();
                self.emit(Instr::PushHandler(None, Some(usize::MAX)));
                catch_handler_pos = Some(finally_handler_pos);
                if has_finally && !catch_needs_cleanup {
                    finally_handler_positions.push(finally_handler_pos);
                }
            }
            if let Some(var) = catch_var {
                if !self.explicit_lexical_scopes {
                    self.locals.insert(var.clone(), ValueType::Any);
                    self.emit(Instr::PushExceptionValue);
                    self.emit(Instr::StoreAny(var.clone()));
                }
            }
            if !self.explicit_lexical_scopes {
                self.emit(Instr::ClearError);
            }
            let mut catch_cleanup = None;
            if let Some(catch_block) = catch_block {
                catch_cleanup = Some(self.compile_clause_block(
                    catch_block,
                    catch_var.as_ref(),
                    &enclosing_scope,
                    &catch_fresh,
                    &catch_explicit,
                    &catch_globals,
                    has_finally || catch_needs_cleanup,
                    true,
                )?);
            }
            if let (Some(pos), Some(cleanup)) = (catch_handler_pos, catch_cleanup) {
                if catch_needs_cleanup {
                    cleanup_handlers.push((pos, cleanup, has_finally));
                }
            }
            self.emit(Instr::PopCaughtException);
            if has_finally || catch_needs_cleanup {
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
                self.locals
                    .insert(name.clone(), join_type(try_ty, &catch_ty));
            }
        }

        let finally_start = self.here();
        if let Some(finally_block) = finally_block {
            let finally_handler_pos = self.here();
            self.emit(Instr::PushHandler(None, Some(usize::MAX)));
            let finally_cleanup = self.compile_clause_block(
                finally_block,
                None,
                &enclosing_scope,
                &finally_fresh,
                &finally_explicit,
                &finally_globals,
                true,
                false,
            )?;
            cleanup_handlers.push((finally_handler_pos, finally_cleanup, false));
            self.emit(Instr::PopHandler);
            self.emit(Instr::Rethrow);
        }

        let try_needs_cleanup = !try_fresh.is_empty() || !try_explicit.is_empty();
        let primary_cleanup_handler = !has_catch && try_needs_cleanup;
        if primary_cleanup_handler {
            cleanup_handlers.push((handler_pos, try_cleanup.clone(), has_finally));
        }
        let skip_cleanup_handlers = self.here();
        self.emit(Instr::Jump(usize::MAX));
        for (handler_pos, cleanup, jump_to_finally) in cleanup_handlers {
            let cleanup_start = self.here();
            // VM handler dispatch already truncates caught-exception state to
            // the handler's recorded depth. Only lexical bindings remain for
            // this exceptional trampoline to clean up; popping here would
            // remove an enclosing catch's active exception a second time.
            self.emit_scope_cleanup_after_vm_unwind(&cleanup);
            if jump_to_finally {
                self.emit(Instr::Jump(finally_start));
            } else {
                self.emit(Instr::Rethrow);
            }
            match &mut self.code[handler_pos] {
                Instr::PushHandler(_, finally_ip) => *finally_ip = Some(cleanup_start),
                _ => unreachable!("cleanup handler patch must target PushHandler"),
            }
        }

        let end = self.here();
        self.patch_jump(skip_cleanup_handlers, end);
        for jump_pos in jump_positions {
            self.patch_jump(jump_pos, if has_finally { finally_start } else { end });
        }

        let catch_ip = if has_catch { Some(catch_start) } else { None };
        let finally_ip = if has_finally && !has_catch && !primary_cleanup_handler {
            Some(finally_start)
        } else {
            None
        };
        if !primary_cleanup_handler {
            self.code[handler_pos] = Instr::PushHandler(catch_ip, finally_ip);
        }

        for pos in finally_handler_positions.drain(..) {
            self.code[pos] = Instr::PushHandler(None, Some(finally_start));
        }

        if has_finally {
            self.finally_stack.pop();
        }

        Ok(Some(()))
    }
}
