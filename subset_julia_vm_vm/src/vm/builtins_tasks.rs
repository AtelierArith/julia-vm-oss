//! VM-owned cooperative task continuations and their internal builtin
//! boundaries (Issue #10349).
//!
//! The root frame (`frames[0]`) owns shared globals and is never moved.  A task
//! continuation is the rest of the interpreter state: the frame suffix,
//! operand stack, return IPs, handlers, and task-local exception state.  Every
//! restored context starts above the same root frame, so absolute `stack_base`
//! and handler depths remain valid without rebasing.

use std::collections::VecDeque;

use super::hof_exec::state::RuntimeCallableResult;
use super::{
    Frame, Handler, RngLike, RootLexicalScope, StackOps, Value, Vm, VmError, VmStackFrame,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VmTaskState {
    Runnable,
    Running,
    Blocked,
    Done,
}

pub(super) struct VmTaskContext {
    pub(super) ip: usize,
    pub(super) next_executable_ip: usize,
    pub(super) stack: Vec<Value>,
    pub(super) frames: Vec<Frame>,
    pub(super) lexical_scopes: Vec<RootLexicalScope>,
    pub(super) return_ips: Vec<usize>,
    pub(super) handlers: Vec<Handler>,
    pub(super) pending_error: Option<VmError>,
    pub(super) pending_exception_value: Option<Value>,
    pub(super) pending_backtrace: Option<Vec<VmStackFrame>>,
    pub(super) caught_exceptions: Vec<(VmError, Option<Value>, Vec<VmStackFrame>)>,
    pub(super) pending_finally_rethrows: Vec<(VmError, Option<Value>, Option<Vec<VmStackFrame>>)>,
    pub(super) last_error_ip: Option<usize>,
    pub(super) call_depth_overflow_pending: bool,
}

pub(super) struct VmTask {
    pub(super) object: Value,
    pub(super) entry: Option<Value>,
    pub(super) context: Option<VmTaskContext>,
    pub(super) state: VmTaskState,
}

impl VmTask {
    pub(super) fn main_placeholder() -> Self {
        Self {
            object: Value::Nothing,
            entry: None,
            context: None,
            state: VmTaskState::Running,
        }
    }
}

impl<R: RngLike> Vm<R> {
    pub(super) fn fresh_task_table() -> Vec<VmTask> {
        vec![VmTask::main_placeholder()]
    }

    fn ensure_suspendable_task_boundary(&self) -> Result<(), VmError> {
        // Every native re-entry currently contributes `eval_dispatch_floor`.
        // Capturing while a Rust frame awaits a particular VM depth/value would
        // resume into invalid native state, so fail catchably instead.
        if self.eval_dispatch_floor.is_some() || self.eval_dispatch_depth != 0 {
            return Err(VmError::ErrorException(
                "cannot suspend task across a native VM call boundary".to_string(),
            ));
        }
        // These state machines contain depth-indexed native continuations. They
        // are deliberately guarded until S4 moves each consumer onto the flat
        // scheduler path or gives it an explicit task-local representation.
        if !self.broadcast_states.is_empty()
            || self.composed_call_state.is_some()
            || !self.generator_iterate_state.is_empty()
            || self.sprint_state.is_some()
            || !self.redirect_states.is_empty()
            || !self.generated_expr_pending_keys.is_empty()
            || !self.generated_expr_pending_eval_frames.is_empty()
        {
            return Err(VmError::ErrorException(
                "cannot suspend task while a native continuation is active".to_string(),
            ));
        }
        Ok(())
    }

    fn capture_running_task_context(&mut self) -> VmTaskContext {
        let frames = if self.frames.len() > 1 {
            self.frames.split_off(1)
        } else {
            Vec::new()
        };
        VmTaskContext {
            ip: self.ip,
            next_executable_ip: self.next_executable_ip,
            stack: std::mem::take(&mut self.stack),
            frames,
            lexical_scopes: std::mem::take(&mut self.lexical_scopes),
            return_ips: std::mem::take(&mut self.return_ips),
            handlers: std::mem::take(&mut self.handlers),
            pending_error: self.pending_error.take(),
            pending_exception_value: self.pending_exception_value.take(),
            pending_backtrace: self.pending_backtrace.take(),
            caught_exceptions: std::mem::take(&mut self.caught_exceptions),
            pending_finally_rethrows: std::mem::take(&mut self.pending_finally_rethrows),
            last_error_ip: self.last_error_ip.take(),
            call_depth_overflow_pending: std::mem::take(&mut self.call_depth_overflow_pending),
        }
    }

    fn restore_task_context(&mut self, context: VmTaskContext) {
        debug_assert_eq!(self.frames.len(), 1);
        self.ip = context.ip;
        self.next_executable_ip = context.next_executable_ip;
        self.stack = context.stack;
        self.frames.extend(context.frames);
        self.lexical_scopes = context.lexical_scopes;
        self.return_ips = context.return_ips;
        self.handlers = context.handlers;
        self.pending_error = context.pending_error;
        self.pending_exception_value = context.pending_exception_value;
        self.pending_backtrace = context.pending_backtrace;
        self.caught_exceptions = context.caught_exceptions;
        self.pending_finally_rethrows = context.pending_finally_rethrows;
        self.last_error_ip = context.last_error_ip;
        self.call_depth_overflow_pending = context.call_depth_overflow_pending;
    }

    fn next_runnable_task(&mut self) -> Option<usize> {
        loop {
            let now = std::time::Instant::now();
            let mut pending = Vec::with_capacity(self.sleeping_tasks.len());
            for (deadline, id) in self.sleeping_tasks.drain(..) {
                if deadline <= now {
                    if let Some(task) = self.tasks.get_mut(id) {
                        if task.state == VmTaskState::Blocked {
                            task.state = VmTaskState::Runnable;
                            self.runnable_tasks.push_back(id);
                        }
                    }
                } else {
                    pending.push((deadline, id));
                }
            }
            self.sleeping_tasks = pending;

            while let Some(id) = self.runnable_tasks.pop_front() {
                if self
                    .tasks
                    .get(id)
                    .is_some_and(|task| task.state == VmTaskState::Runnable)
                {
                    return Some(id);
                }
            }
            let deadline = self
                .sleeping_tasks
                .iter()
                .map(|(deadline, _)| *deadline)
                .min()?;
            let now = std::time::Instant::now();
            if deadline > now {
                std::thread::sleep(deadline.duration_since(now));
            }
        }
    }

    fn clear_active_task_execution_state(&mut self) {
        self.frames.truncate(1);
        self.lexical_scopes.clear();
        self.stack.clear();
        self.return_ips.clear();
        self.handlers.clear();
        self.pending_error = None;
        self.pending_exception_value = None;
        self.pending_backtrace = None;
        self.caught_exceptions.clear();
        self.pending_finally_rethrows.clear();
        self.last_error_ip = None;
        self.call_depth_overflow_pending = false;
        self.ip = 0;
        self.next_executable_ip = super::executable::NO_EXECUTABLE_IP;
    }

    fn activate_task(&mut self, id: usize) -> Result<(), VmError> {
        let context = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| VmError::InternalError(format!("invalid VM task id {id}")))?
            .context
            .take();
        if let Some(context) = context {
            self.clear_active_task_execution_state();
            self.restore_task_context(context);
            self.tasks[id].state = VmTaskState::Running;
            self.current_task_id = id;
            return Ok(());
        }

        let (entry, object) = {
            let task = &self.tasks[id];
            if task.state != VmTaskState::Runnable {
                return Err(VmError::InternalError(format!(
                    "VM task {id} is not runnable"
                )));
            }
            (task.entry.clone(), task.object.clone())
        };
        let entry = entry
            .ok_or_else(|| VmError::InternalError(format!("VM task {id} has no entry callable")))?;
        debug_assert_eq!(self.frames.len(), 1);
        self.clear_active_task_execution_state();
        let activation = match self.call_runtime_callable_value(entry, vec![object]) {
            Ok(RuntimeCallableResult::StartedFrame) => {
                // The task entry is a root, not a child of the shared global
                // frame's current instruction. Its outer return must become a
                // scheduler completion instead of jumping back to bootstrap IP.
                self.return_ips.pop();
                Ok(())
            }
            Ok(RuntimeCallableResult::Immediate(_)) => Err(VmError::InternalError(
                "task entry completed without starting a VM frame".to_string(),
            )),
            Ok(RuntimeCallableResult::Raised) => {
                Err(self.pending_error.take().unwrap_or_else(|| {
                    VmError::InternalError("task entry raised without an exception".to_string())
                }))
            }
            Err(error) => Err(error),
        };
        if let Err(error) = activation {
            // Entry dispatch is fallible. Leave the target runnable and its
            // entry/object intact so the caller can transactionally restore
            // the outgoing continuation (Issue #10349).
            self.clear_active_task_execution_state();
            return Err(error);
        }
        self.tasks[id].state = VmTaskState::Running;
        self.current_task_id = id;
        Ok(())
    }

    fn save_and_switch_current(&mut self, requeue: bool) -> Result<(), VmError> {
        self.ensure_suspendable_task_boundary()?;
        // The internal primitive is the value of the Julia `yield()` / park
        // call. Store it in the suspended stack before capturing the context.
        self.stack.push(Value::Nothing);
        let current = self.current_task_id;
        let context = self.capture_running_task_context();
        let task = self.tasks.get_mut(current).ok_or_else(|| {
            VmError::InternalError(format!("invalid current VM task id {current}"))
        })?;
        task.context = Some(context);
        task.state = if requeue {
            VmTaskState::Runnable
        } else {
            VmTaskState::Blocked
        };
        if requeue {
            self.runnable_tasks.push_back(current);
        }

        let Some(next) = self.next_runnable_task() else {
            let context = self.tasks[current].context.take().ok_or_else(|| {
                VmError::InternalError("parked task context disappeared".to_string())
            })?;
            self.tasks[current].state = VmTaskState::Running;
            self.current_task_id = current;
            self.restore_task_context(context);
            // The synthetic return value belongs only to a successful park.
            // A catchable deadlock error must leave the call stack exactly as
            // it was before the intrinsic (Issue #10349).
            self.stack.pop();
            return Err(VmError::ErrorException(
                "deadlock detected: all tasks are blocked".to_string(),
            ));
        };
        if let Err(error) = self.activate_task(next) {
            if self.tasks[next].state == VmTaskState::Runnable {
                self.runnable_tasks.push_front(next);
            }
            self.runnable_tasks.retain(|id| *id != current);
            let context = self.tasks[current].context.take().ok_or_else(|| {
                VmError::InternalError("switched task context disappeared".to_string())
            })?;
            self.tasks[current].state = VmTaskState::Running;
            self.current_task_id = current;
            self.restore_task_context(context);
            self.stack.pop();
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn execute_builtin_tasks(
        &mut self,
        builtin: &crate::builtins::BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        use crate::builtins::BuiltinId;
        match builtin {
            BuiltinId::TaskRegisterMain => {
                if argc != 1 {
                    return Err(VmError::ArgumentError(
                        "_task_register_main expects one Task".to_string(),
                    ));
                }
                let task = self.stack.pop_value()?;
                self.tasks[0].object = task;
                self.stack.push(Value::I64(0));
                Ok(Some(()))
            }
            BuiltinId::TaskSchedule => {
                if argc != 2 {
                    return Err(VmError::ArgumentError(
                        "_task_schedule expects a Task and entry callable".to_string(),
                    ));
                }
                let entry = self.stack.pop_value()?;
                let object = self.stack.pop_value()?;
                let id = self.tasks.len();
                self.tasks.push(VmTask {
                    object,
                    entry: Some(entry),
                    context: None,
                    state: VmTaskState::Runnable,
                });
                self.runnable_tasks.push_back(id);
                self.stack.push(Value::I64(id as i64));
                Ok(Some(()))
            }
            BuiltinId::TaskYield => {
                if argc != 0 {
                    return Err(VmError::ArgumentError(
                        "_task_yield expects no arguments".to_string(),
                    ));
                }
                self.save_and_switch_current(true)?;
                Ok(Some(()))
            }
            BuiltinId::TaskPark => {
                if argc != 0 {
                    return Err(VmError::ArgumentError(
                        "_task_park expects no arguments".to_string(),
                    ));
                }
                self.save_and_switch_current(false)?;
                Ok(Some(()))
            }
            BuiltinId::TaskWake => {
                if argc != 1 {
                    return Err(VmError::ArgumentError(
                        "_task_wake expects one task id".to_string(),
                    ));
                }
                let id = self.stack.pop_i64()?;
                let id = usize::try_from(id).map_err(|_| {
                    VmError::ArgumentError("invalid negative VM task id".to_string())
                })?;
                let should_wake = {
                    let task = self.tasks.get_mut(id).ok_or_else(|| {
                        VmError::ArgumentError(format!("unknown VM task id {id}"))
                    })?;
                    if task.state == VmTaskState::Blocked {
                        task.state = VmTaskState::Runnable;
                        true
                    } else {
                        false
                    }
                };
                if should_wake {
                    // An explicit wake cancels a pending timer for the same
                    // task. Otherwise its stale deadline could later wake an
                    // unrelated Channel/Condition park (Issue #10349).
                    self.sleeping_tasks.retain(|(_, task_id)| *task_id != id);
                    self.runnable_tasks.push_back(id);
                }
                self.stack.push(Value::Nothing);
                Ok(Some(()))
            }
            BuiltinId::TaskCurrent => {
                if argc != 0 {
                    return Err(VmError::ArgumentError(
                        "_task_current expects no arguments".to_string(),
                    ));
                }
                let object = self
                    .tasks
                    .get(self.current_task_id)
                    .map(|task| task.object.clone())
                    .ok_or_else(|| {
                        VmError::InternalError("current VM task disappeared".to_string())
                    })?;
                self.stack.push(object);
                Ok(Some(()))
            }
            _ => Ok(None),
        }
    }

    /// Turn a root return from a non-main task into a scheduler switch. Returns
    /// `false` for the main task so the normal VM exit path remains unchanged.
    pub(super) fn finish_current_task_and_switch(&mut self) -> Result<bool, VmError> {
        if self.current_task_id == 0 {
            return Ok(false);
        }
        let finished = self.current_task_id;
        self.clear_active_task_execution_state();
        self.tasks[finished].state = VmTaskState::Done;
        self.tasks[finished].context = None;
        // The Julia Task object remains reachable through ordinary Julia
        // references. The scheduler needs only the terminal state after exit;
        // retaining these values would pin every completed task and closure.
        self.tasks[finished].object = Value::Nothing;
        self.tasks[finished].entry = None;

        let next = self.next_runnable_task().ok_or_else(|| {
            VmError::ErrorException("deadlock detected: all tasks are blocked".to_string())
        })?;
        if let Err(error) = self.activate_task(next) {
            if self.tasks[next].state == VmTaskState::Runnable {
                self.runnable_tasks.push_front(next);
            }
            return Err(error);
        }
        Ok(true)
    }

    pub(super) fn sleep_current_task(
        &mut self,
        duration: std::time::Duration,
    ) -> Result<(), VmError> {
        self.ensure_suspendable_task_boundary()?;
        let deadline = std::time::Instant::now() + duration;
        let current = self.current_task_id;
        self.sleeping_tasks.push((deadline, current));
        let result = self.save_and_switch_current(false);
        if result.is_err() {
            self.sleeping_tasks
                .retain(|(_, task_id)| *task_id != current);
        }
        result
    }
}

pub(super) fn empty_runnable_queue() -> VecDeque<usize> {
    VecDeque::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::BuiltinId;
    use crate::rng::StableRng;

    #[test]
    fn task_suspend_rejects_native_reentry_floor_without_mutation_10349() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.eval_dispatch_floor = Some(0);

        assert!(matches!(
            vm.execute_builtin_tasks(&BuiltinId::TaskYield, 0),
            Err(VmError::ErrorException(message))
                if message == "cannot suspend task across a native VM call boundary"
        ));
        assert_eq!(vm.current_task_id, 0);
        assert!(vm.stack.is_empty());
        assert!(vm.runnable_tasks.is_empty());
        assert!(vm.tasks[0].context.is_none());
        assert_eq!(vm.tasks[0].state, VmTaskState::Running);
    }

    #[test]
    fn main_task_exit_keeps_zero_task_fast_path_10349() {
        let mut vm = Vm::new(vec![], StableRng::new(0));

        assert!(matches!(vm.finish_current_task_and_switch(), Ok(false)));
        assert_eq!(vm.current_task_id, 0);
        assert_eq!(vm.tasks.len(), 1);
        assert!(vm.runnable_tasks.is_empty());
    }

    #[test]
    fn deadlock_error_restores_parked_task_without_result_value_10349() {
        let mut vm = Vm::new(vec![], StableRng::new(0));

        assert!(matches!(
            vm.execute_builtin_tasks(&BuiltinId::TaskPark, 0),
            Err(VmError::ErrorException(message))
                if message == "deadlock detected: all tasks are blocked"
        ));
        assert!(vm.stack.is_empty());
        assert!(vm.tasks[0].context.is_none());
        assert_eq!(vm.tasks[0].state, VmTaskState::Running);
    }

    #[test]
    fn activation_error_rolls_back_outgoing_task_and_queue_10349() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.tasks.push(VmTask {
            object: Value::Nothing,
            entry: None,
            context: None,
            state: VmTaskState::Runnable,
        });
        vm.runnable_tasks.push_back(1);

        assert!(matches!(
            vm.execute_builtin_tasks(&BuiltinId::TaskYield, 0),
            Err(VmError::InternalError(message))
                if message == "VM task 1 has no entry callable"
        ));
        assert_eq!(vm.current_task_id, 0);
        assert!(vm.stack.is_empty());
        assert!(vm.tasks[0].context.is_none());
        assert_eq!(vm.tasks[0].state, VmTaskState::Running);
        assert_eq!(vm.tasks[1].state, VmTaskState::Runnable);
        assert_eq!(vm.runnable_tasks, VecDeque::from([1]));
    }

    #[test]
    fn explicit_wake_cancels_stale_sleep_deadline_10349() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.tasks[0].state = VmTaskState::Blocked;
        vm.sleeping_tasks.push((
            std::time::Instant::now() + std::time::Duration::from_secs(60),
            0,
        ));
        vm.stack.push(Value::I64(0));

        assert!(matches!(
            vm.execute_builtin_tasks(&BuiltinId::TaskWake, 1),
            Ok(Some(()))
        ));
        assert!(vm.sleeping_tasks.is_empty());
        assert_eq!(vm.tasks[0].state, VmTaskState::Runnable);
        assert_eq!(vm.runnable_tasks, VecDeque::from([0]));
    }

    #[test]
    fn fresh_task_state_reset_clears_error_metadata_10349() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.last_error_ip = Some(41);
        vm.call_depth_overflow_pending = true;

        vm.clear_active_task_execution_state();

        assert_eq!(vm.last_error_ip, None);
        assert!(!vm.call_depth_overflow_pending);
    }

    #[test]
    fn task_context_owns_root_lexical_scope_stack_11569() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        assert!(vm.enter_root_lexical_scope(&["x".to_string()]).is_ok());
        assert!(vm.store_root_lexical("x", Value::I64(7)).is_ok());

        let context = vm.capture_running_task_context();
        assert!(vm.lexical_scopes.is_empty());
        vm.restore_task_context(context);
        assert!(matches!(
            vm.root_lexical_binding("x"),
            Some(Some(Value::I64(7)))
        ));

        vm.clear_active_task_execution_state();
        assert!(vm.lexical_scopes.is_empty());
    }

    #[test]
    fn active_and_suspended_lexical_roots_share_one_gc_remap_visited_set_11569() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.struct_heap = vec![
            crate::vm::value::StructInstance::with_name(
                0,
                "Dead".to_string(),
                vec![Value::Nothing],
            ),
            crate::vm::value::StructInstance::with_name(
                0,
                "Child".to_string(),
                vec![Value::Nothing],
            ),
            crate::vm::value::StructInstance::with_name(
                0,
                "Root".to_string(),
                vec![Value::StructRef(1)],
            ),
        ];
        let shared = std::rc::Rc::new(std::cell::RefCell::new(Value::StructRef(2)));

        assert!(vm.enter_root_lexical_scope(&["parked".to_string()]).is_ok());
        assert!(vm
            .store_root_lexical("parked", Value::Ref(shared.clone()))
            .is_ok());
        let parked = vm.capture_running_task_context();
        vm.tasks.push(VmTask {
            object: Value::Nothing,
            entry: None,
            context: Some(parked),
            state: VmTaskState::Blocked,
        });

        assert!(vm.enter_root_lexical_scope(&["active".to_string()]).is_ok());
        assert!(vm
            .store_root_lexical("active", Value::Ref(shared.clone()))
            .is_ok());

        let stats = vm.compact_struct_heap_for_explicit_gc();
        assert_eq!(stats.reclaimed, 1);
        assert_eq!(vm.struct_heap.len(), 2);
        assert!(matches!(*shared.borrow(), Value::StructRef(1)));
    }

    #[test]
    fn suspended_lexical_values_contribute_to_memory_waterline_11569() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let baseline = vm.estimated_memory_waterline_bytes();
        let memory = crate::vm::value::MemoryValue::undef_typed(
            &crate::vm::value::ArrayElementType::I64,
            128,
        );
        assert!(vm.enter_root_lexical_scope(&["buffer".to_string()]).is_ok());
        assert!(vm
            .store_root_lexical(
                "buffer",
                Value::Memory(crate::vm::value::new_memory_ref(memory)),
            )
            .is_ok());
        assert!(vm.estimated_memory_waterline_bytes() > baseline);

        let parked = vm.capture_running_task_context();
        vm.tasks.push(VmTask {
            object: Value::Nothing,
            entry: None,
            context: Some(parked),
            state: VmTaskState::Blocked,
        });
        assert!(vm.estimated_memory_waterline_bytes() > baseline);
    }
}
