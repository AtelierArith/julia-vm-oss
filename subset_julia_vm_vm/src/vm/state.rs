//! VM lifecycle, value/state accessors, and runtime-support helpers.
//!
//! Split out of `vm/mod.rs` (Issue #6826). These `impl Vm<R>` methods cover the
//! `Vm` constructors (`new`, `new_program`), local/global/output accessors,
//! value type queries, error handling/`raise`, the call-site inline/dispatch
//! caches, and the small stack/compare execution helpers. The `Vm` struct
//! definition itself stays in `vm/mod.rs`.

use super::*;
use crate::types::JuliaType;
use crate::vm::executable::ExecutableProgram;
use crate::vm::hof_exec::state::RuntimeCallableResult;
use crate::vm::value::GeneratorCallable;
use std::collections::HashSet;
use subset_julia_vm_bytecode::FunctionInfo;

const MEMORY_BUDGET_ENV: &str = "SJULIA_MEMORY_BUDGET_BYTES";
const MEMORY_WATERLINE_CHECK_INTERVAL: usize = 1024;

fn memory_budget_from_env() -> Option<usize> {
    std::env::var(MEMORY_BUDGET_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
}

fn memory_budget_default() -> (Option<usize>, bool) {
    if let Some(bytes) = memory_budget_bytes_default() {
        (Some(bytes), true)
    } else {
        (memory_budget_from_env(), false)
    }
}

/// Base (unqualified) generic-function name used to bucket runtime dispatch
/// cache invalidation (Issue #9197 S6). A module-qualified definition
/// (`MyMod.f`) and a bare call site (`f`) can alias across the recording sites,
/// so — like the #8554 inference backedge walk (`function_base_name`) — the
/// comparison uses the segment after the last `.` and errs toward
/// over-invalidation.
fn dispatch_generic_base_name(name: &str) -> &str {
    name.rfind('.').map_or(name, |idx| &name[idx + 1..])
}

fn runtime_nominal_base_name(name: &str) -> &str {
    name.as_bytes()
        .iter()
        .position(|byte| *byte == 123)
        .map_or(name, |index| &name[..index])
}

fn register_active_enum_definitions(
    program: &CompiledProgram,
    hidden_type_ids: &HashSet<usize>,
) -> HashMap<String, usize> {
    let active = program
        .enum_defs
        .iter()
        .enumerate()
        .filter(|(index, _)| !hidden_type_ids.contains(index))
        .map(|(index, definition)| (definition.name.clone(), index))
        .collect::<HashMap<_, _>>();
    for (index, definition) in program.enum_defs.iter().enumerate() {
        if !hidden_type_ids.contains(&index) {
            crate::vm::value::enum_registry::register_enum(&definition.name, &definition.members);
        }
    }
    active
}

fn keep_reserved_nominal_ids(program: &CompiledProgram) -> bool {
    program
        .code
        .get(program.entry..)
        .unwrap_or_default()
        .iter()
        .any(|instruction| matches!(instruction, Instr::DefineRuntimeNominal(_)))
}

fn runtime_nominal_activation_counts(
    activations: &[RuntimeNominalActivation],
) -> (usize, usize, usize, usize) {
    let mut counts = (0, 0, 0, 0);
    for activation in activations {
        if activation.coalesced_root {
            continue;
        }
        match &activation.definition {
            RuntimeNominalDefInfo::Struct(_) => counts.0 += 1,
            RuntimeNominalDefInfo::AbstractType(_) => counts.1 += 1,
            RuntimeNominalDefInfo::PrimitiveType(_) => counts.2 += 1,
            RuntimeNominalDefInfo::Enum(_) => counts.3 += 1,
        }
    }
    counts
}

fn collect_runtime_constructor_indices(
    templates: &[DefineRuntimeNominalOperands],
    function_count: usize,
    activation_members: &HashSet<usize>,
) -> Option<HashSet<usize>> {
    let mut indices = HashSet::new();
    for template in templates {
        for &index in &template.constructor_function_indices {
            if index >= function_count
                || activation_members.contains(&index)
                || !indices.insert(index)
            {
                return None;
            }
        }
    }
    Some(indices)
}

fn reached_reserved_runtime_struct_count(
    activations: &[RuntimeNominalActivation],
    templates: &[DefineRuntimeNominalOperands],
) -> usize {
    activations
        .iter()
        .filter(|activation| {
            templates.iter().any(|template| {
                template.site_id == activation.site_id && template.reserved_struct_type_id.is_some()
            })
        })
        .count()
}

fn build_function_name_indices(
    functions: &[Rc<FunctionInfo>],
) -> (HashMap<String, Vec<usize>>, HashMap<String, Vec<usize>>) {
    let mut function_name_index: HashMap<String, Vec<usize>> = HashMap::new();
    let mut lowering_helper_name_index: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, func) in functions.iter().enumerate() {
        let name_index = if func.is_lowering_helper {
            &mut lowering_helper_name_index
        } else {
            &mut function_name_index
        };
        name_index.entry(func.name.clone()).or_default().push(idx);
        // A module-body `let`/`@testset`-root function (Issue #10236) is a
        // lexically-scoped local, so only its qualified name is reachable.
        if !func.suppress_short_name_alias {
            if let Some((_, short_name)) = func.name.rsplit_once('.') {
                name_index
                    .entry(short_name.to_string())
                    .or_default()
                    .push(idx);
            }
        }
    }
    (function_name_index, lowering_helper_name_index)
}

/// Whether a cached dispatch decision resolving to `func_index` may change
/// because generic function `target` (a base name) was (re)defined
/// (Issue #9197 S6).
///
/// A runtime dispatch cache entry's implicit backedge is its resolved function
/// index; the reverse "callee name → affected entries" map is computed on
/// demand by this predicate — the minimal runtime analogue of upstream's
/// `invalidate_backedges` (`julia/src/gf.c`), without a persisted reverse
/// index. Returns `true` (drop the entry) for: `usize::MAX` (the builtin/native
/// fallback sentinel — a fresh user method for `target` may now capture the
/// site), an out-of-range index (defensive), or a method whose own
/// generic-function base name equals `target`. Every other resolved method
/// belongs to an unrelated generic function and is provably unaffected, so its
/// entry survives.
fn dispatch_decision_affected(
    functions: &[Rc<FunctionInfo>],
    target: &str,
    func_index: usize,
) -> bool {
    if func_index == usize::MAX {
        return true;
    }
    match functions.get(func_index) {
        Some(func) => dispatch_generic_base_name(&func.name) == target,
        None => true,
    }
}

fn estimated_array_storage_bytes(elem_type: &ArrayElementType, length: usize) -> usize {
    let bytes_per_element = match elem_type {
        ArrayElementType::F64
        | ArrayElementType::I64
        | ArrayElementType::U64
        | ArrayElementType::ComplexF32 => 8,
        ArrayElementType::F32
        | ArrayElementType::I32
        | ArrayElementType::U32
        | ArrayElementType::Char => 4,
        ArrayElementType::I16 | ArrayElementType::U16 => 2,
        ArrayElementType::I8 | ArrayElementType::U8 | ArrayElementType::Bool => 1,
        ArrayElementType::ComplexF64 => 16,
        ArrayElementType::TupleOf(field_types) => field_types
            .len()
            .max(1)
            .saturating_mul(std::mem::size_of::<Value>()),
        ArrayElementType::StructInlineOf(_, field_count) => (*field_count)
            .max(1)
            .saturating_mul(std::mem::size_of::<Value>()),
        // Contiguous all-`Float64` isbits struct: `field_count` unboxed f64
        // (8 B each), not boxed `Value`s (Issue #9198 S4).
        ArrayElementType::StructInlineF64(_, field_count) => (*field_count).max(1) * 8,
        _ => std::mem::size_of::<Value>(),
    };
    length.saturating_mul(bytes_per_element)
}

fn split_pending_eval_struct_defs(
    program: &mut CompiledProgram,
    keep_reserved: bool,
) -> (VecDeque<(usize, StructDefInfo)>, HashSet<usize>) {
    let marker_type_ids = program
        .code
        .get(program.entry..)
        .unwrap_or_default()
        .iter()
        .filter_map(|instr| match instr {
            Instr::DefineEvalStruct(type_id) => Some(*type_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    let runtime_marker_type_ids = program
        .code
        .get(program.entry..)
        .unwrap_or_default()
        .iter()
        .filter_map(|instr| match instr {
            Instr::DefineRuntimeNominal(operands) => operands.reserved_struct_type_id,
            _ => None,
        })
        .collect::<HashSet<_>>();
    if keep_reserved && marker_type_ids.is_empty() {
        return (VecDeque::new(), runtime_marker_type_ids);
    }
    marker_type_ids.first().map_or_else(
        || (VecDeque::new(), runtime_marker_type_ids.clone()),
        |first_type_id| {
            if keep_reserved {
                let pending = marker_type_ids
                    .iter()
                    .filter_map(|type_id| {
                        program
                            .struct_defs
                            .get(*type_id)
                            .cloned()
                            .map(|definition| (*type_id, definition))
                    })
                    .collect();
                let mut hidden = marker_type_ids
                    .iter()
                    .copied()
                    .filter(|type_id| *type_id < program.struct_defs.len())
                    .collect::<HashSet<_>>();
                hidden.extend(runtime_marker_type_ids.iter().copied());
                return (pending, hidden);
            }
            let expected = (*first_type_id..program.struct_defs.len()).collect::<Vec<_>>();
            if marker_type_ids != expected {
                let hidden = marker_type_ids
                    .iter()
                    .copied()
                    .filter(|type_id| *type_id < program.struct_defs.len())
                    .collect();
                return (VecDeque::new(), hidden);
            }
            let pending = program
                .struct_defs
                .split_off(*first_type_id)
                .into_iter()
                .enumerate()
                .map(|(offset, def)| (first_type_id + offset, def))
                .collect();
            (pending, HashSet::new())
        },
    )
}

fn split_pending_eval_abstract_types(
    program: &mut CompiledProgram,
    keep_reserved: bool,
) -> (VecDeque<(usize, AbstractTypeDefInfo)>, HashSet<usize>) {
    let marker_ids = program
        .code
        .get(program.entry..)
        .unwrap_or_default()
        .iter()
        .filter_map(|instr| match instr {
            Instr::DefineEvalAbstractType(type_id) => Some(*type_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    // Split from the smallest referenced suffix index even when marker order is
    // malformed. The queue-front check then raises InternalError at execution;
    // leaving the registry active here would publish compiler-known names
    // before any valid source marker (Issue #11635).
    let Some(first_type_id) = marker_ids.iter().copied().min() else {
        return (VecDeque::new(), HashSet::new());
    };
    if first_type_id >= program.abstract_types.len() {
        return (VecDeque::new(), HashSet::new());
    }
    if keep_reserved {
        let pending = marker_ids
            .iter()
            .filter_map(|type_id| {
                program
                    .abstract_types
                    .get(*type_id)
                    .cloned()
                    .map(|definition| (*type_id, definition))
            })
            .collect();
        let hidden = marker_ids
            .iter()
            .copied()
            .filter(|type_id| *type_id < program.abstract_types.len())
            .collect();
        return (pending, hidden);
    }
    let pending = program
        .abstract_types
        .split_off(first_type_id)
        .into_iter()
        .enumerate()
        .map(|(offset, definition)| (first_type_id + offset, definition))
        .collect();
    (pending, HashSet::new())
}

fn split_pending_eval_primitive_types(
    program: &mut CompiledProgram,
    keep_reserved: bool,
) -> (VecDeque<(usize, PrimitiveTypeDefInfo)>, HashSet<usize>) {
    let marker_ids = program
        .code
        .get(program.entry..)
        .unwrap_or_default()
        .iter()
        .filter_map(|instr| match instr {
            Instr::DefineEvalPrimitiveType(type_id) => Some(*type_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    // See the abstract-type path above: malformed marker order must fail closed
    // with the whole referenced suffix private (Issue #11635).
    let Some(first_type_id) = marker_ids.iter().copied().min() else {
        return (VecDeque::new(), HashSet::new());
    };
    if first_type_id >= program.primitive_types.len() {
        return (VecDeque::new(), HashSet::new());
    }
    if keep_reserved {
        let pending = marker_ids
            .iter()
            .filter_map(|type_id| {
                program
                    .primitive_types
                    .get(*type_id)
                    .cloned()
                    .map(|definition| (*type_id, definition))
            })
            .collect();
        let hidden = marker_ids
            .iter()
            .copied()
            .filter(|type_id| *type_id < program.primitive_types.len())
            .collect();
        return (pending, hidden);
    }
    let pending = program
        .primitive_types
        .split_off(first_type_id)
        .into_iter()
        .enumerate()
        .map(|(offset, definition)| (first_type_id + offset, definition))
        .collect();
    (pending, HashSet::new())
}

fn split_pending_eval_enum_defs(
    program: &mut CompiledProgram,
    keep_reserved: bool,
) -> (VecDeque<(usize, EnumDefInfo)>, HashSet<usize>) {
    let markers = program
        .code
        .get(program.entry..)
        .unwrap_or_default()
        .iter()
        .filter_map(|instruction| match instruction {
            Instr::RegisterEnum(operands) => {
                Some((operands.type_name.as_str(), operands.members.as_slice()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    // A malformed first marker must not make the complete enum table active.
    // Reserve from the earliest metadata row named by any marker; execution's
    // queue-front/name/member validation reports the mismatch in source order.
    let Some(first_index) = markers
        .iter()
        .filter_map(|(name, _)| {
            program
                .enum_defs
                .iter()
                .position(|definition| definition.name == *name)
        })
        .min()
    else {
        return (VecDeque::new(), HashSet::new());
    };
    if keep_reserved {
        let marker_indices = markers
            .iter()
            .filter_map(|(name, members)| {
                program.enum_defs.iter().position(|definition| {
                    definition.name == *name && definition.members == *members
                })
            })
            .collect::<Vec<_>>();
        let pending = marker_indices
            .iter()
            .filter_map(|index| {
                program
                    .enum_defs
                    .get(*index)
                    .cloned()
                    .map(|definition| (*index, definition))
            })
            .collect();
        let hidden = marker_indices.into_iter().collect();
        return (pending, hidden);
    }
    let pending = program
        .enum_defs
        .split_off(first_index)
        .into_iter()
        .enumerate()
        .map(|(offset, definition)| (first_index + offset, definition))
        .collect();
    (pending, HashSet::new())
}

fn display_method_map(
    entries: &[subset_julia_vm_bytecode::ShowMethodEntry],
) -> HashMap<String, usize> {
    entries
        .iter()
        .map(|entry| (entry.type_name.clone(), entry.func_index))
        .collect()
}

fn display_method_candidates(
    entries: &[subset_julia_vm_bytecode::ShowMethodEntry],
) -> HashMap<String, Vec<usize>> {
    let mut candidates: HashMap<String, Vec<usize>> = HashMap::new();
    for entry in entries {
        candidates
            .entry(entry.type_name.clone())
            .or_default()
            .push(entry.func_index);
    }
    candidates
}

impl<R: RngLike> Vm<R> {
    /// Enter a (possibly nested) `@testset` (Issue #10338): save the counts
    /// the enclosing scope accumulated so far — plus its testset name — as a
    /// [`TestSetFrame`], then reset the scalar counters so they track only
    /// the new set. Shared by the `_testset_begin!` builtin and the legacy
    /// `Instr::TestSetBegin` lane so both aggregate identically.
    pub(in crate::vm) fn testset_begin_frame(&mut self, name: String) {
        self.testset_stack.push(TestSetFrame {
            enclosing_name: self.current_testset.take(),
            saved_pass: self.test_pass_count,
            saved_fail: self.test_fail_count,
            saved_broken: self.test_broken_count,
            saved_error: self.test_error_count,
        });
        self.current_testset = Some(name);
        self.test_pass_count = 0;
        self.test_fail_count = 0;
        self.test_broken_count = 0;
        self.test_error_count = 0;
    }

    /// Finish the innermost `@testset` (Issue #10338): return
    /// `(pass, fail, error, broken)` for the set that just ended (the caller
    /// prints its summary from these), then restore the enclosing scope's
    /// counters WITH this set's results folded in — upstream
    /// `Test.finish`'s `record(parent, child)` aggregation, so an outer
    /// testset's own `_testset_end!` reports the aggregated totals instead
    /// of echoing the last inner set. An unbalanced end (empty stack — e.g.
    /// an exception unwound past a `_testset_end!`) degrades to the pre-stack
    /// behavior: counters keep the current values and `current_testset`
    /// clears.
    pub(in crate::vm) fn testset_end_frame(&mut self) -> (usize, usize, usize, usize) {
        let finished = (
            self.test_pass_count,
            self.test_fail_count,
            self.test_error_count,
            self.test_broken_count,
        );
        match self.testset_stack.pop() {
            Some(frame) => {
                self.test_pass_count += frame.saved_pass;
                self.test_fail_count += frame.saved_fail;
                self.test_broken_count += frame.saved_broken;
                self.test_error_count += frame.saved_error;
                self.current_testset = frame.enclosing_name;
            }
            None => {
                self.current_testset = None;
            }
        }
        finished
    }

    /// Run a native operation with a nestable GC-root frame.
    ///
    /// The callback must store every authoritative `Value` that survives a
    /// synchronous Julia call in this root stack and retain only
    /// [`TransientRootId`] handles in Rust aggregates. Cleanup happens after
    /// every normal callback result, including `Err` and handled-raise control
    /// outcomes represented inside that result (Issue #11372).
    #[cfg(test)]
    pub(in crate::vm) fn with_transient_root_frame<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let base = self.transient_roots.len();
        let result = operation(self);
        self.transient_roots.truncate(base);
        result
    }

    /// Split-form scope API for large opcode arms whose existing early returns
    /// are captured inside a local closure.
    pub(in crate::vm) fn begin_transient_root_frame(&self) -> usize {
        self.transient_roots.len()
    }

    pub(in crate::vm) fn end_transient_root_frame(&mut self, base: usize) {
        assert!(
            base <= self.transient_roots.len(),
            "transient GC root frame underflow"
        );
        self.transient_roots.truncate(base);
    }

    pub(in crate::vm) fn push_transient_root(
        &mut self,
        value: Value,
    ) -> Result<TransientRootId, VmError> {
        let generation = self.next_transient_root_generation;
        self.next_transient_root_generation = generation.checked_add(1).ok_or_else(|| {
            VmError::InternalError("transient GC root generation exhausted".to_string())
        })?;
        let id = TransientRootId {
            index: self.transient_roots.len(),
            generation,
        };
        self.transient_roots
            .push(TransientRootSlot { generation, value });
        Ok(id)
    }

    pub(in crate::vm) fn clone_transient_root(
        &self,
        id: TransientRootId,
    ) -> Result<Value, VmError> {
        self.transient_roots
            .get(id.index)
            .filter(|slot| slot.generation == id.generation)
            .map(|slot| slot.value.clone())
            .ok_or_else(|| {
                VmError::InternalError(format!(
                    "stale transient GC root {}@{}",
                    id.index, id.generation
                ))
            })
    }

    pub(in crate::vm) fn replace_transient_root(
        &mut self,
        id: TransientRootId,
        value: Value,
    ) -> Result<(), VmError> {
        let root = self
            .transient_roots
            .get_mut(id.index)
            .filter(|slot| slot.generation == id.generation)
            .ok_or_else(|| {
                VmError::InternalError(format!(
                    "stale transient GC root {}@{}",
                    id.index, id.generation
                ))
            })?;
        root.value = value;
        Ok(())
    }

    pub(in crate::vm) fn clone_transient_roots(
        &self,
        ids: &[TransientRootId],
    ) -> Result<Vec<Value>, VmError> {
        ids.iter()
            .map(|&id| self.clone_transient_root(id))
            .collect()
    }

    pub fn debug_current_instruction(&self) -> Option<(usize, Instr)> {
        self.code
            .get(self.ip)
            .cloned()
            .map(|instr| (self.ip, instr))
    }

    pub fn debug_instruction_at(&self, ip: usize) -> Option<Instr> {
        self.code.get(ip).cloned()
    }

    /// Create a new VM with a flat instruction list and an RNG instance.
    ///
    /// Use this constructor when you have a raw `Vec<Instr>` (e.g., from incremental
    /// compilation). For compiled programs with entry points and metadata, prefer
    /// [`Vm::new_program`].
    pub fn new(code: Vec<Instr>, rng: R) -> Self {
        let call_site_caches = vec![CallSiteCache::default(); code.len()];
        let (type_intern, call_site_type_id_tables) = build_call_site_intern_tables();
        let (memory_budget_bytes, memory_waterline_enabled) = memory_budget_default();
        Self {
            ip: 0,
            stack: Vec::with_capacity(256),
            transient_roots: Vec::new(),
            next_transient_root_generation: 1,
            frames: vec![Frame::new()],
            lexical_scopes: Vec::new(),
            frame_pool: Vec::new(),
            arg_vec_pool: Vec::new(),
            return_ips: Vec::new(),
            handlers: Vec::new(),
            tasks: Self::fresh_task_table(),
            runnable_tasks: builtins_tasks::empty_runnable_queue(),
            sleeping_tasks: Vec::new(),
            current_task_id: 0,
            code: Rc::new(code),
            executable: executable::ExecutableProgram::empty(),
            next_executable_ip: executable::NO_EXECUTABLE_IP,
            functions: Vec::new(),
            base_function_count: 0,
            native_array_exempt_functions: Vec::new(),
            function_slot_maps: Vec::new(),
            binary_signature_cache: HashMap::new(),
            typed_signature_cache: HashMap::new(),
            struct_defs: Vec::new(),
            pending_eval_struct_defs: VecDeque::new(),
            pending_eval_abstract_types: VecDeque::new(),
            pending_eval_primitive_types: VecDeque::new(),
            enum_defs: Vec::new(),
            pending_eval_enum_defs: VecDeque::new(),
            active_enum_name_index: HashMap::new(),
            pending_eval_enum_member_bindings: VecDeque::new(),
            hidden_eval_struct_type_ids: HashSet::new(),
            hidden_eval_abstract_type_ids: HashSet::new(),
            hidden_eval_primitive_type_ids: HashSet::new(),
            hidden_eval_enum_type_ids: HashSet::new(),
            published_eval_nominal_type_names: HashSet::new(),
            repl_definition_activations: Vec::new(),
            repl_using_activations: Vec::new(),
            repl_module_activations: Vec::new(),
            repl_runtime_function_indices: Vec::new(),
            repl_written_globals: HashSet::new(),
            repl_explicit_global_writes: HashSet::new(),
            repl_function_refresh_groups: HashMap::new(),
            repl_specializable_updates: HashMap::new(),
            repl_world_sensitive_specializable_indices: HashSet::new(),
            abstract_types: Vec::new(),
            show_methods: std::collections::HashMap::new(),
            print_methods: std::collections::HashMap::new(),
            show_method_candidates: std::collections::HashMap::new(),
            print_method_candidates: std::collections::HashMap::new(),
            struct_heap: Vec::new(),
            weak_refs: Vec::new(),
            finalizers: Vec::new(),
            pending_finalizers: Vec::new(),
            in_finalizer: false,
            rng,
            output: String::new(),
            stderr_output: String::new(),
            stdin_stream: IOValue::stdin_ref(),
            current_stdout: IOValue::stdout_ref(),
            current_stderr: IOValue::stderr_ref(),
            devnull_stream: IOValue::devnull_ref(),
            output_callback: None,
            output_callback_context: std::ptr::null_mut(),
            broadcast_states: Vec::new(),
            composed_call_state: None,
            generator_iterate_state: Vec::new(),
            sprint_state: None,
            redirect_states: Vec::new(),
            pending_error: None,
            pending_exception_value: None,
            pending_backtrace: None,
            caught_exceptions: Vec::new(),
            pending_finally_rethrows: Vec::new(),
            test_pass_count: 0,
            test_fail_count: 0,
            test_broken_count: 0,
            test_error_count: 0,
            current_testset: None,
            testset_stack: Vec::new(),
            any_test_failed: false,
            test_throws_state: None,
            // Lazy AoT fields
            specializable_functions: Vec::new(),
            specializable_callable_registry_cache: None,
            specialization_cache: HashMap::new(),
            specialization_failure_cache: HashSet::new(),
            specialization_i64_cache: HashMap::new(),
            specialization_i64_fast_cache: Vec::new(),
            specialization_f64_cache: HashMap::new(),
            specialization_f64_fast_cache: Vec::new(),
            specialization_mixed_cache: HashMap::new(),
            i64_function_cache: HashMap::new(),
            f64_function_cache: HashMap::new(),
            typed_function_cache: HashMap::new(),
            binary_method_cache: HashMap::new(),
            compile_context: None,
            macro_bindings: HashMap::new(),
            module_registry: subset_julia_vm_bytecode::ModuleInternTable::new(),
            global_slot_names: Vec::new(),
            global_slot_map: HashMap::new(),
            gensym_counter: 0,
            runtime_typevar_counter: 0,
            runtime_typevar_projection_identities: HashMap::new(),
            cached_cartesian_index_type_id: Cell::new(None),
            cached_pair_type_id: Cell::new(None),
            cached_complex_type_id: Cell::new(None),
            cached_array_type_id: Cell::new(None),
            struct_def_name_index: HashMap::new(),
            abstract_type_name_index: HashMap::new(),
            dispatch_cache: HashMap::new(),
            binary_both_dispatch_cache: HashMap::new(),
            call_site_caches,
            type_intern,
            call_site_type_id_tables,
            dispatch_generation: 0,
            dispatch_cache_entry_limit: dispatch_cache_entry_limit_default(),
            specialization_cache_entry_limit: specialization_cache_entry_limit_default(),
            cache_clear_count: 0,
            cache_cleared_entry_count: 0,
            memory_budget_bytes,
            memory_waterline_enabled,
            memory_waterline_check_countdown: MEMORY_WATERLINE_CHECK_INTERVAL,
            call_site_inline_cache_disabled: call_site_inline_cache_disabled_from_env(),
            method_dispatch_cache: HashMap::new(),
            generated_expr_cache: HashMap::new(),
            generated_expr_pending_keys: HashMap::new(),
            generated_expr_pending_eval_frames: HashMap::new(),
            module_eval_scope_floors: Vec::new(),
            lexical_eval_scope_floors: Vec::new(),
            function_name_index: HashMap::new(),
            lowering_helper_name_index: HashMap::new(),
            eval_defined_bodies: HashMap::new(),
            pending_exception_payload: Default::default(),
            eval_defined_struct_names: HashSet::new(),
            current_world: 1,
            source_map: Vec::new(),
            last_error_ip: None,
            type_ancestors: HashMap::new(),
            struct_hierarchy: StructHierarchy::new(),
            eval_dispatch_depth: 0,
            eval_dispatch_floor: None,
            call_depth_overflow_pending: false,
            register_gate: register_gate::RegisterGateState::from_env(),
            stack_metrics: stack_metrics::StackVmMetrics::from_env(),
            // Display stack starts inactive; graphical hosts opt in via
            // `enable_graphical_display()` before `run()` (Issue #9262).
            graphical_display_active: false,
            display_artifacts: Vec::new(),
            #[cfg(feature = "vm-handler-table")]
            handler_table: exec::handler_table::HandlerTableState::from_env(),
        }
    }

    /// Create a new VM from a fully compiled program.
    ///
    /// `CompiledProgram` carries the entry point IP, all function/struct definitions,
    /// global slot layout, and optional lazy-AoT context produced by the compiler.
    /// This is the primary constructor used after calling [`compile_and_run_str`] or
    /// the two-phase compile pipeline.
    pub fn new_program(mut program: CompiledProgram, rng: R) -> Self {
        let (memory_budget_bytes, memory_waterline_enabled) = memory_budget_default();
        // Build display lookup maps from the CompiledProgram's registry Vecs.
        // The single-index maps preserve the historical fallback behavior; the
        // candidate maps keep duplicate keys so runtime method specificity can
        // choose between overlapping display methods (Issue #9564).
        let show_methods = program.show_methods.as_slice();
        let print_entries = program.print_methods.as_slice();
        let show_method_candidates = display_method_candidates(show_methods);
        let print_method_candidates = display_method_candidates(print_entries);
        let show_methods = display_method_map(show_methods);
        let print_methods = display_method_map(print_entries);

        let global_slot_map = program
            .global_slot_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect::<HashMap<_, _>>();

        // Pre-compute the native-array boundary exemptions once at install so
        // runtime dispatch consults a flag instead of re-matching function name
        // strings per candidate (Issue #6336).
        let native_array_exempt_functions = program
            .functions
            .iter()
            .map(|func| base_function_accepts_native_array_value(&func.name))
            .collect::<Vec<_>>();

        // Pre-compute per-function `name -> slot` maps so the string-keyed
        // `Load*/Store*` paths resolve `name -> slot` in O(1) instead of a
        // linear scan over `slot_names` on every execution (Issue #5179).
        let function_slot_maps = program
            .functions
            .iter()
            .map(|func| {
                func.slot_names
                    .iter()
                    .enumerate()
                    .map(|(idx, name)| (name.clone(), idx))
                    .collect::<HashMap<String, usize>>()
            })
            .collect::<Vec<_>>();

        let entry_ip = program.entry;
        // Runtime-conditional definitions append registry rows when reached.
        // Keep later root rows reserved at their compiler-assigned IDs so the
        // two families cannot claim the same slot before the root marker runs.
        // Projecting an unreached reserved suffix after an intervening uncaught
        // REPL error requires ID remapping and remains tracked by Issue #11683.
        let keep_reserved_nominal_ids = keep_reserved_nominal_ids(&program);
        let (pending_eval_struct_defs, hidden_eval_struct_type_ids) =
            split_pending_eval_struct_defs(&mut program, keep_reserved_nominal_ids);
        let (pending_eval_abstract_types, hidden_eval_abstract_type_ids) =
            split_pending_eval_abstract_types(&mut program, keep_reserved_nominal_ids);
        let (pending_eval_primitive_types, hidden_eval_primitive_type_ids) =
            split_pending_eval_primitive_types(&mut program, keep_reserved_nominal_ids);
        let (pending_eval_enum_defs, hidden_eval_enum_type_ids) =
            split_pending_eval_enum_defs(&mut program, keep_reserved_nominal_ids);
        if let Some(context) = program.compile_context.as_mut() {
            context.primitive_types = program.primitive_types.clone();
        }
        let active_enum_name_index =
            register_active_enum_definitions(&program, &hidden_eval_enum_type_ids);

        let struct_def_name_index = program
            .struct_defs
            .iter()
            .enumerate()
            .map(|(idx, def)| (def.name.clone(), idx))
            .collect::<HashMap<_, _>>();

        let abstract_type_name_index = program
            .abstract_types
            .iter()
            .enumerate()
            .filter(|(index, _)| !hidden_eval_abstract_type_ids.contains(index))
            .map(|(idx, at)| (at.name.clone(), idx))
            .collect::<HashMap<_, _>>();

        let struct_hierarchy = build_struct_hierarchy_from_program(&program);
        let base_function_count = program.base_function_count;
        let executable = executable::ExecutableProgram::from_bytecode(
            &program.code,
            &program.functions,
            base_function_count,
        );
        let next_executable_ip = executable.next_ip_from(entry_ip);
        let call_site_caches = vec![CallSiteCache::default(); program.code.len()];

        // Parametric user structs are not in `struct_defs` (they instantiate
        // lazily), so surface their base names while the declared parents stay
        // centralized in `struct_hierarchy` (Issue #5052, #5920).
        let parametric_struct_names: Vec<String> = program
            .compile_context
            .as_ref()
            .map(|ctx| ctx.parametric_structs.keys().cloned().collect())
            .unwrap_or_default();

        // Pre-compute transitive closure of abstract type hierarchy (Issue #3356)
        let type_ancestors = compute_type_ancestors(
            &program.struct_defs,
            &program.abstract_types,
            &abstract_type_name_index,
            &struct_hierarchy,
            &parametric_struct_names,
        );

        // Build function name → indices lookups for O(1) dispatch (Issue #3361).
        let (function_name_index, lowering_helper_name_index) =
            build_function_name_indices(&program.functions);

        // The reflection `Method` struct exposes a `.module::Module` field, but
        // `module` is a reserved keyword the parser cannot accept as a field
        // name, so the pure-Julia definition declares it as `mod`. Rename it to
        // `module` here so `m.module` field access (compiled to a
        // `GetFieldByName("module")`) resolves and `fieldnames(Method)` reports
        // `:module`, matching upstream (Issue #5125).
        normalize_method_struct_def(&mut program.struct_defs);

        let (type_intern, call_site_type_id_tables) = build_call_site_intern_tables();

        Self {
            ip: entry_ip,
            stack: Vec::with_capacity(256),
            transient_roots: Vec::new(),
            next_transient_root_generation: 1,
            frames: vec![Frame::new_with_slots(program.global_slot_count, None)],
            lexical_scopes: Vec::new(),
            frame_pool: Vec::new(),
            arg_vec_pool: Vec::new(),
            return_ips: Vec::new(),
            handlers: Vec::new(),
            tasks: Self::fresh_task_table(),
            runnable_tasks: builtins_tasks::empty_runnable_queue(),
            sleeping_tasks: Vec::new(),
            current_task_id: 0,
            code: Rc::new(program.code),
            executable,
            next_executable_ip,
            // Issue #9140: CompiledProgram already carries Rc<FunctionInfo>;
            // move the Rcs directly (base entries stay shared with the cache).
            functions: program.functions,
            base_function_count,
            native_array_exempt_functions,
            function_slot_maps,
            binary_signature_cache: HashMap::new(),
            typed_signature_cache: HashMap::new(),
            struct_defs: program.struct_defs,
            pending_eval_struct_defs,
            pending_eval_abstract_types,
            pending_eval_primitive_types,
            enum_defs: program.enum_defs,
            pending_eval_enum_defs,
            active_enum_name_index,
            pending_eval_enum_member_bindings: VecDeque::new(),
            hidden_eval_struct_type_ids,
            hidden_eval_abstract_type_ids,
            hidden_eval_primitive_type_ids,
            hidden_eval_enum_type_ids,
            published_eval_nominal_type_names: HashSet::new(),
            repl_definition_activations: Vec::new(),
            repl_using_activations: Vec::new(),
            repl_module_activations: Vec::new(),
            repl_runtime_function_indices: Vec::new(),
            repl_written_globals: HashSet::new(),
            repl_explicit_global_writes: HashSet::new(),
            repl_function_refresh_groups: HashMap::new(),
            repl_specializable_updates: HashMap::new(),
            repl_world_sensitive_specializable_indices: HashSet::new(),
            abstract_types: program.abstract_types,
            show_methods,
            print_methods,
            show_method_candidates,
            print_method_candidates,
            struct_heap: Vec::new(),
            weak_refs: Vec::new(),
            finalizers: Vec::new(),
            pending_finalizers: Vec::new(),
            in_finalizer: false,
            rng,
            output: String::new(),
            stderr_output: String::new(),
            stdin_stream: IOValue::stdin_ref(),
            current_stdout: IOValue::stdout_ref(),
            current_stderr: IOValue::stderr_ref(),
            devnull_stream: IOValue::devnull_ref(),
            output_callback: None,
            output_callback_context: std::ptr::null_mut(),
            broadcast_states: Vec::new(),
            composed_call_state: None,
            generator_iterate_state: Vec::new(),
            sprint_state: None,
            redirect_states: Vec::new(),
            pending_error: None,
            pending_exception_value: None,
            pending_backtrace: None,
            caught_exceptions: Vec::new(),
            pending_finally_rethrows: Vec::new(),
            test_pass_count: 0,
            test_fail_count: 0,
            test_broken_count: 0,
            test_error_count: 0,
            current_testset: None,
            testset_stack: Vec::new(),
            any_test_failed: false,
            test_throws_state: None,
            // Lazy AoT fields
            specializable_functions: program.specializable_functions,
            specializable_callable_registry_cache: None,
            specialization_cache: HashMap::new(),
            specialization_failure_cache: HashSet::new(),
            specialization_i64_cache: HashMap::new(),
            specialization_i64_fast_cache: Vec::new(),
            specialization_f64_cache: HashMap::new(),
            specialization_f64_fast_cache: Vec::new(),
            specialization_mixed_cache: HashMap::new(),
            i64_function_cache: HashMap::new(),
            f64_function_cache: HashMap::new(),
            typed_function_cache: HashMap::new(),
            binary_method_cache: HashMap::new(),
            compile_context: program.compile_context,
            macro_bindings: program.macro_bindings,
            module_registry: program.module_registry,
            global_slot_names: program.global_slot_names,
            global_slot_map,
            gensym_counter: 0,
            runtime_typevar_counter: 0,
            runtime_typevar_projection_identities: HashMap::new(),
            cached_cartesian_index_type_id: Cell::new(None),
            cached_pair_type_id: Cell::new(None),
            cached_complex_type_id: Cell::new(None),
            cached_array_type_id: Cell::new(None),
            struct_def_name_index,
            abstract_type_name_index,
            dispatch_cache: HashMap::new(),
            binary_both_dispatch_cache: HashMap::new(),
            call_site_caches,
            type_intern,
            call_site_type_id_tables,
            dispatch_generation: 0,
            dispatch_cache_entry_limit: dispatch_cache_entry_limit_default(),
            specialization_cache_entry_limit: specialization_cache_entry_limit_default(),
            cache_clear_count: 0,
            cache_cleared_entry_count: 0,
            memory_budget_bytes,
            memory_waterline_enabled,
            memory_waterline_check_countdown: MEMORY_WATERLINE_CHECK_INTERVAL,
            call_site_inline_cache_disabled: call_site_inline_cache_disabled_from_env(),
            method_dispatch_cache: HashMap::new(),
            generated_expr_cache: HashMap::new(),
            generated_expr_pending_keys: HashMap::new(),
            generated_expr_pending_eval_frames: HashMap::new(),
            module_eval_scope_floors: Vec::new(),
            lexical_eval_scope_floors: Vec::new(),
            function_name_index,
            lowering_helper_name_index,
            eval_defined_bodies: HashMap::new(),
            pending_exception_payload: Default::default(),
            eval_defined_struct_names: HashSet::new(),
            current_world: 1,
            source_map: program.source_map,
            last_error_ip: None,
            type_ancestors,
            struct_hierarchy,
            eval_dispatch_depth: 0,
            eval_dispatch_floor: None,
            call_depth_overflow_pending: false,
            register_gate: register_gate::RegisterGateState::from_env(),
            stack_metrics: stack_metrics::StackVmMetrics::from_env(),
            // Display stack starts inactive; graphical hosts opt in via
            // `enable_graphical_display()` before `run()` (Issue #9262).
            graphical_display_active: false,
            display_artifacts: Vec::new(),
            #[cfg(feature = "vm-handler-table")]
            handler_table: exec::handler_table::HandlerTableState::from_env(),
        }
    }

    /// Inject an `Int64` variable into the current frame before execution.
    ///
    /// If `name` maps to a slot in the global slot layout, the slot is updated
    /// directly; otherwise the value is stored in `locals_any`.
    /// This is used by the REPL and FFI layer to pass integer inputs into Julia code
    /// without going through compilation.
    pub fn set_local_i64(&mut self, name: &str, v: i64) {
        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(frame) = self.frames.last_mut() {
                if frame.set_slot_i64(slot, v) {
                    return;
                }
            }
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.locals_any.insert(name.to_string(), Value::I64(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::I64);
        }
    }

    /// Inject a `Float64` variable into the current frame before execution.
    ///
    /// Mirrors [`Vm::set_local_i64`] but for floating-point values. The slot-based
    /// fast path is tried first; `locals_any` is used as fallback.
    pub fn set_local_f64(&mut self, name: &str, v: f64) {
        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(frame) = self.frames.last_mut() {
                if frame.set_slot_f64(slot, v) {
                    return;
                }
            }
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.locals_any.insert(name.to_string(), Value::F64(v));
            frame
                .var_types
                .insert(name.to_string(), frame::VarTypeTag::F64);
        }
    }

    /// Get the accumulated output from println calls
    pub fn get_output(&self) -> &str {
        &self.output
    }

    /// Get the accumulated stderr output from `print(stderr, ...)` calls (Issue #3573).
    pub fn get_stderr_output(&self) -> &str {
        &self.stderr_output
    }

    /// Set the source map that maps instruction IPs to source spans (Issue #2856).
    ///
    /// The source map is a parallel vector to `code` — `source_map[ip]` gives the
    /// source span for instruction at `ip`. Populated by the compiler; empty by default.
    pub fn set_source_map(&mut self, source_map: Vec<Option<crate::span::Span>>) {
        self.source_map = source_map;
    }

    /// Get the source span for the instruction that caused the last error (Issue #2856).
    ///
    /// Returns `None` if no error has occurred, or if the source map is not populated.
    pub fn last_error_span(&self) -> Option<crate::span::Span> {
        self.last_error_ip
            .and_then(|ip| self.source_map.get(ip).copied().flatten())
    }

    /// Source span of the most recent call site that entered the function where
    /// an error escaped. Pure-Julia helpers such as `error(...)` can raise from
    /// cached Base code with no source-map entry; the caller's return IP still
    /// identifies the user statement that was reached (#11761).
    pub fn last_error_callsite_span(&self) -> Option<crate::span::Span> {
        self.return_ips
            .last()
            .and_then(|return_ip| return_ip.checked_sub(1))
            .and_then(|call_ip| self.source_map.get(call_ip).copied().flatten())
    }

    /// Create a [`SpannedVmError`] from a `VmError`, attaching the source span
    /// of the last error instruction if available (Issue #2856).
    pub fn spanned_error(&self, error: VmError) -> SpannedVmError {
        SpannedVmError {
            error,
            span: self.last_error_span(),
        }
    }

    pub fn runtime_stack_trace(&self) -> Vec<VmStackFrame> {
        if self.frames.len() <= 1 {
            return Vec::new();
        }

        let last_function_frame = self.frames.len() - 1;
        let mut frames = Vec::new();
        for frame_idx in (1..=last_function_frame).rev() {
            let Some(func_index) = self.frames[frame_idx].func_index else {
                continue;
            };
            let Some(func) = self.functions.get(func_index) else {
                continue;
            };
            let span = if frame_idx == last_function_frame {
                self.last_error_span()
            } else {
                self.return_ips
                    .get(frame_idx)
                    .and_then(|return_ip| return_ip.checked_sub(1))
                    .and_then(|call_ip| self.source_map.get(call_ip).copied().flatten())
            };
            frames.push(VmStackFrame {
                function: func.name.clone(),
                span,
            });
        }
        frames
    }

    /// Get the type_id for Complex struct from struct_defs (cached).
    /// Returns the first type_id for a struct named "Complex" or "Complex{...}"
    pub(super) fn get_complex_type_id(&self) -> usize {
        if let Some(id) = self.cached_complex_type_id.get() {
            return id;
        }
        let id = self
            .struct_defs
            .iter()
            .enumerate()
            .find_map(|(idx, def)| {
                if is_complex_type_name(&def.name) {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        self.cached_complex_type_id.set(Some(id));
        id
    }

    /// Create a Complex struct instance with the correct struct_name from struct_defs.
    /// This ensures the struct_name matches what's registered in struct_defs for proper dispatch.
    pub(super) fn create_complex(&mut self, type_id: usize, re: f64, im: f64) -> Value {
        let struct_name = self
            .struct_defs
            .get(type_id)
            .map(|def| def.name.clone())
            .unwrap_or_else(|| "Complex{Float64}".to_string());
        let s =
            StructInstance::with_name(type_id, struct_name, vec![Value::F64(re), Value::F64(im)]);
        let idx = self.struct_heap.len();
        self.struct_heap.push(s);
        Value::StructRef(idx)
    }

    /// Get the ValueType from a Value (for Lazy AoT specialization)
    pub(super) fn get_value_type(&self, val: &Value) -> ValueType {
        // Route the legacy native-array carrier through the shared
        // `native_array_value_ref` helper so the match below no longer holds
        // a native-array arm (Issue #3908). The catch-all
        // `_ => ValueType::Any` would otherwise map the native carrier to
        // `Any`, so the early return is required for behavior preservation.
        if let Some(arr) = native_array_value_ref(val) {
            return ValueType::ArrayOf(arr.borrow().element_type(), None);
        }
        match val {
            Value::I64(_) => ValueType::I64,
            Value::F64(_) => ValueType::F64,
            Value::Str(_) | Value::StrBytes(_) => ValueType::Str,
            Value::Char(_) | Value::CharMalformed(_) => ValueType::Char,
            Value::Bool(_) => ValueType::Bool,
            Value::Nothing => ValueType::Nothing,
            Value::Missing => ValueType::Missing,
            Value::Memory(mem) => ValueType::ArrayOf(mem.borrow().element_type().clone(), None),
            Value::MemoryRef(_) => ValueType::Any,
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    value_type_for_struct_instance(s)
                } else {
                    ValueType::Any
                }
            }
            Value::Struct(s) => value_type_for_struct_instance(s),
            Value::Tuple(_) => ValueType::Tuple,
            Value::NamedTuple(_) => ValueType::Tuple,
            Value::Range(_) => ValueType::Range,
            Value::DataType(_) | Value::RuntimeTypeVar(_) => ValueType::DataType,
            Value::Rng(_) => ValueType::Rng,
            Value::Generator(_) => ValueType::Generator,
            _ => ValueType::Any,
        }
    }

    /// Get the JuliaType for a Value (for type parameter binding)
    pub(super) fn get_value_julia_type(&self, val: &Value) -> crate::types::JuliaType {
        // Route the legacy native-array carrier through the shared
        // `native_array_value_ref` helper so the match below no longer holds
        // a native-array arm (Issue #3908). The catch-all
        // `_ => crate::types::JuliaType::Any` would otherwise map the native
        // carrier to `Any`, so the early return is required for behavior
        // preservation.
        if let Some(arr) = native_array_value_ref(val) {
            let arr_ref = arr.borrow();
            if let Some(container_type) = arr_ref.array_type_override() {
                return crate::types::JuliaType::Struct(container_type.to_string());
            }
            let elem_jtype = self.array_value_logical_element_julia_type(&arr_ref);
            return julia_array_type_for_ndims(elem_jtype, arr_ref.shape.len());
        }
        match val {
            Value::I8(_) => crate::types::JuliaType::Int8,
            Value::I16(_) => crate::types::JuliaType::Int16,
            Value::I32(_) => crate::types::JuliaType::Int32,
            Value::I64(_) => crate::types::JuliaType::Int64,
            Value::I128(_) => crate::types::JuliaType::Int128,
            Value::BigInt(_) => crate::types::JuliaType::BigInt,
            Value::U8(_) => crate::types::JuliaType::UInt8,
            Value::U16(_) => crate::types::JuliaType::UInt16,
            Value::U32(_) => crate::types::JuliaType::UInt32,
            Value::U64(_) => crate::types::JuliaType::UInt64,
            Value::U128(_) => crate::types::JuliaType::UInt128,
            Value::F16(_) => crate::types::JuliaType::Float16,
            Value::F32(_) => crate::types::JuliaType::Float32,
            Value::F64(_) => crate::types::JuliaType::Float64,
            Value::BigFloat(_) => crate::types::JuliaType::BigFloat,
            Value::Str(_) | Value::StrBytes(_) => crate::types::JuliaType::String,
            Value::Char(_) | Value::CharMalformed(_) => crate::types::JuliaType::Char,
            Value::Bool(_) => crate::types::JuliaType::Bool,
            Value::Nothing => crate::types::JuliaType::Nothing,
            Value::Missing => crate::types::JuliaType::Missing,
            Value::Regex(_) => crate::types::JuliaType::Struct("Regex".to_string()),
            Value::RegexMatch(_) => crate::types::JuliaType::Struct("RegexMatch".to_string()),
            Value::WeakRef(_) => crate::types::JuliaType::Struct("WeakRef".to_string()),
            // Report the concrete RNG struct type so method dispatch on
            // `::Xoshiro`, `::StableRNG`, `::AbstractRNG` selects the right
            // method when a Value::Rng is passed as an argument (Issue #7231).
            // The global handle (default_rng()/GLOBAL_RNG) reports as
            // TaskLocalRNG (Issue #7230).
            Value::Rng(rng) => crate::types::JuliaType::Struct(
                match rng {
                    crate::rng::RngInstance::Stable(_) => "StableRNG",
                    crate::rng::RngInstance::Xoshiro(_) => "Xoshiro",
                    crate::rng::RngInstance::Mersenne(_) => "MersenneTwister",
                    crate::rng::RngInstance::Global => "TaskLocalRNG",
                }
                .to_string(),
            ),
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    // Issue #8025: resolve a user-struct array element type
                    // (`StructOf(type_id)`) to its concrete struct name through
                    // `struct_defs`, mirroring `typeof`/reflection (Issue #7304).
                    // The registry-free `array_wrapper_julia_type()` reports `Any`
                    // for such an eltype, so dispatch saw a `Matrix{MyNum}` as
                    // `Matrix{Any}` and a parametric `AbstractMatrix{<:MyNum}`
                    // method failed to match against its bare `AbstractMatrix`
                    // sibling.
                    self.array_wrapper_julia_type_resolved(s)
                        .unwrap_or_else(|| self.get_parametric_struct_name(s))
                } else {
                    crate::types::JuliaType::Any
                }
            }
            Value::Struct(s) => self
                .array_wrapper_julia_type_resolved(s)
                .unwrap_or_else(|| self.get_parametric_struct_name(s)),
            Value::DataType(jt) => *jt.clone(),
            Value::RuntimeTypeVar(tv) => tv.projection(),
            Value::Memory(mem) => {
                let mem = mem.borrow();
                let elem_type_name = self.memory_element_type_name(mem.element_type());
                crate::types::JuliaType::Struct(format!("Memory{{{}}}", elem_type_name))
            }
            Value::MemoryRef(memref) => {
                crate::types::JuliaType::Struct(self.memory_ref_type_name(memref))
            }
            Value::Tuple(items) => crate::types::JuliaType::TupleOf(
                items
                    .elements
                    .iter()
                    .map(|item| self.get_value_julia_type(item))
                    .collect(),
            ),
            Value::NamedTuple(nt) => {
                let fields: Vec<String> = nt
                    .names
                    .iter()
                    .zip(nt.values.iter())
                    .map(|(name, val)| format!("{}::{}", name, self.get_type_name(val)))
                    .collect();
                crate::types::JuliaType::Struct(format!("@NamedTuple{{{}}}", fields.join(", ")))
            }
            Value::SimpleVector(_) => {
                crate::types::JuliaType::Struct("Core.SimpleVector".to_string())
            }
            // Keep runtime dispatch in sync with `typeof` / reflection for index
            // wrappers. Official Julia dispatches range and colon indexing through
            // `getindex(A::Array, I::AbstractUnitRange)` / `getindex(A::Array, ::Colon)`.
            Value::Range(_) => val.runtime_type(),
            Value::SliceAll => crate::types::JuliaType::Struct("Colon".to_string()),
            // Base.RefValue{T}: report the concrete struct type so method dispatch
            // on `::Ref`, `::RefValue`, and `::Ref{T}` selects correctly (Issue #5130).
            Value::Ref(inner) => {
                let inner_ty = self.get_value_julia_type(&inner.borrow());
                crate::types::JuliaType::Struct(format!("Base.RefValue{{{}}}", inner_ty))
            }
            Value::IO(_) => crate::types::JuliaType::IOBuffer,
            // Closures carry a per-definition-site singleton type
            // `typeof(<qualified nested name>)`, mirroring the
            // `Value::Function` arm below (Issue #9106); the shared
            // `Function` type made `::typeof(f)` dispatch unresolvable.
            Value::Closure(cv) => crate::types::JuliaType::Struct(cv.singleton_type_name()),
            Value::ComposedFunction(_) => crate::types::JuliaType::Function,
            Value::Generator(_) => crate::types::JuliaType::Generator,
            Value::Module(_) => crate::types::JuliaType::Module,
            Value::Symbol(_) => crate::types::JuliaType::Symbol,
            Value::Expr(_) => crate::types::JuliaType::Expr,
            Value::QuoteNode(_) => crate::types::JuliaType::QuoteNode,
            Value::LineNumberNode(_) => crate::types::JuliaType::LineNumberNode,
            Value::GlobalRef(_) => crate::types::JuliaType::GlobalRef,
            Value::Binding(_) => crate::types::JuliaType::Struct("Core.Binding".to_string()),
            Value::Pairs(_) => crate::types::JuliaType::Pairs,
            Value::Enum { type_name, .. } => crate::types::JuliaType::Enum(type_name.clone()),
            // Each generic function has its own singleton type `typeof(f)`, a
            // subtype of `Function` (Issue #5128). Report it here so a
            // `where {F}` / `where {F<:Function}` parameter matched against a
            // function value binds `F` to `typeof(f)` instead of falling
            // through to the `Any` catch-all. This mirrors the `typeof(f)`
            // projection in `BuiltinId::TypeOf` (builtins_types.rs).
            Value::Function(f) => crate::types::JuliaType::Struct(f.singleton_type_name()),
            // StaticArray variants: report the concrete parametric type name so
            // where-clause binding (e.g. `size(x::SMatrix{M,N,T}) where {M,N,T}`)
            // can extract M, N, T from the type string (Issue #7964).
            Value::StaticArray(sv) => {
                crate::types::JuliaType::Struct(sv.julia_type_name().to_string())
            }
            Value::StaticArrayInline(sv) => {
                crate::types::JuliaType::Struct(sv.julia_type_name_owned().to_string())
            }
            _ => crate::types::JuliaType::Any,
        }
    }

    pub(in crate::vm) fn array_value_logical_element_julia_type(
        &self,
        arr: &ArrayValue,
    ) -> crate::types::JuliaType {
        match arr.element_type() {
            ArrayElementType::StructOf(type_id)
            | ArrayElementType::StructInlineOf(type_id, _)
            | ArrayElementType::StructInlineF64(type_id, _) => self
                .struct_defs
                .get(type_id)
                .map(|def| crate::types::JuliaType::Struct(def.name.clone()))
                .unwrap_or(crate::types::JuliaType::Any),
            ArrayElementType::Struct => {
                if let ArrayData::StructRefs(refs) = &arr.data {
                    refs.first()
                        .and_then(|idx| self.struct_heap.get(*idx))
                        .map(|s| crate::types::JuliaType::Struct(s.struct_name.to_string()))
                        .unwrap_or(crate::types::JuliaType::Any)
                } else {
                    crate::types::JuliaType::Any
                }
            }
            ArrayElementType::Any => {
                if let ArrayData::Any(values) = &arr.data {
                    values
                        .first()
                        .and_then(|first| match first {
                            Value::StructRef(idx) => self.struct_heap.get(*idx).map(|s| {
                                crate::types::JuliaType::Struct(s.struct_name.to_string())
                            }),
                            Value::Struct(s) => {
                                Some(crate::types::JuliaType::Struct(s.struct_name.to_string()))
                            }
                            _ => None,
                        })
                        .unwrap_or(crate::types::JuliaType::Any)
                } else {
                    crate::types::JuliaType::Any
                }
            }
            element_type => array_element_type_to_julia_type(&element_type),
        }
    }

    pub(in crate::vm) fn array_value_declared_element_julia_type(
        &self,
        arr: &ArrayValue,
    ) -> crate::types::JuliaType {
        match arr.element_type() {
            ArrayElementType::StructOf(type_id)
            | ArrayElementType::StructInlineOf(type_id, _)
            | ArrayElementType::StructInlineF64(type_id, _) => self
                .struct_defs
                .get(type_id)
                .map(|def| crate::types::JuliaType::Struct(def.name.clone()))
                .unwrap_or(crate::types::JuliaType::Any),
            element_type => array_element_type_to_julia_type(&element_type),
        }
    }

    /// Get the full parametric struct name for a struct instance.
    /// Preserves actual type parameters (e.g., "Complex{Bool}", "Complex{Int64}").
    ///
    /// Routes through [`Vm::concrete_struct_type_name`] so this DISPATCH-facing
    /// projection agrees with the `typeof`/`isa` projection (Issue #10577):
    /// `Pair` is modeled as a non-parametric struct at the value level
    /// (`struct Pair; first; second; end`), so its bare `struct_name` alone
    /// cannot distinguish `Pair{Int64,Int64}` from `Pair{Int64,Float64}`.
    /// Before this, `f(x::Pair{Int,Int}) = 1; f(x::Pair{Int,Float64}) = 2`
    /// dispatched on the identical bare `Pair` runtime type for every Pair
    /// value and reported the two methods as ambiguous instead of selecting
    /// the exact match upstream picks (Issue #11551). Every other struct
    /// already carries its concrete `Name{...}` in `struct_name`, so this is a
    /// no-op for them.
    pub(super) fn get_parametric_struct_name(&self, s: &StructInstance) -> crate::types::JuliaType {
        crate::types::JuliaType::Struct(self.concrete_struct_type_name(s))
    }

    /// Try to load a variable from a specific frame index.
    /// Returns true if the variable was found and pushed onto the stack.
    pub(super) fn try_load_from_frame(&mut self, name: &str, frame_idx: usize) -> bool {
        if let Some(value) = self.lookup_frame_binding(name, frame_idx) {
            self.stack.push(value);
            true
        } else {
            false
        }
    }

    /// Authoritative non-mutating projection of every value namespace readable
    /// from one frame. Keep the namespace list and precedence here only: stack
    /// loads, closure snapshots, reflection, and eval all delegate to this
    /// method (Issue #11051).
    fn lookup_frame_binding(&self, name: &str, frame_idx: usize) -> Option<Value> {
        let frame = self.frames.get(frame_idx)?;
        self.load_slot_value_by_name(frame, name)
            // A local introduced in this frame shadows an outer captured value.
            .or_else(|| frame.get_local(name))
            // A `where` binder introduced by this frame shadows an inherited
            // same-named capture from an outer lexical frame (Issue #11070).
            .or_else(|| {
                frame
                    .type_bindings
                    .get(name)
                    .cloned()
                    .map(|ty| Value::DataType(Box::new(ty)))
            })
            .or_else(|| frame.captured_vars.get(name).cloned())
    }

    /// Get a variable value from a specific frame without pushing to stack.
    /// Returns None if the variable is not found.
    pub(super) fn get_value_from_frame(&self, name: &str, frame_idx: usize) -> Option<Value> {
        self.lookup_frame_binding(name, frame_idx)
    }

    /// Check if a variable is defined in a specific frame.
    /// Returns true if the variable exists in that frame.
    pub(super) fn is_var_defined_in_frame(&self, name: &str, frame_idx: usize) -> bool {
        self.get_value_from_frame(name, frame_idx).is_some()
    }

    /// Get a variable value by name, checking current frame first, then global.
    /// Used by eval to resolve symbols at runtime.
    pub fn get_variable_value(&self, name: &str) -> Option<Value> {
        // First check current frame
        let current_frame_idx = self.frames.len().saturating_sub(1);
        if let Some(val) = self.get_value_from_frame(name, current_frame_idx) {
            return Some(val);
        }
        // Try global frame if not in current frame
        if self.frames.len() > 1 {
            if let Some(val) = self.get_value_from_frame(name, 0) {
                return Some(val);
            }
        }
        None
    }

    /// Resolve a symbol for the runtime module-level `eval` interpreter.
    ///
    /// Eval may see its own temporary lexical frames (created at or above the
    /// innermost eval floor) and frame 0 globals, but not the compiled caller's
    /// locals, captures, or type bindings below that floor. Outside module eval,
    /// generated-expression and eval-defined-method consumers retain the
    /// historical current-frame lookup (Issue #11071).
    pub(super) fn get_eval_variable_value(&self, name: &str) -> Option<Value> {
        let Some(&floor) = self
            .module_eval_scope_floors
            .last()
            .or_else(|| self.lexical_eval_scope_floors.last())
        else {
            return self.get_variable_value(name);
        };

        for frame_idx in (floor..self.frames.len()).rev() {
            if let Some(value) = self.get_value_from_frame(name, frame_idx) {
                return Some(value);
            }
        }
        self.get_value_from_frame(name, 0)
    }

    /// Resolve a symbol for an explicit module-target eval. Eval-owned lexical
    /// frames win first, followed by the target module binding. A user-defined
    /// Main global is never an implicit parent of another module (#11072).
    /// Imported Base bindings in a user module require explicit runtime import
    /// provenance and are tracked separately by #11073.
    pub(super) fn get_module_eval_variable_value(
        &self,
        module_name: &str,
        name: &str,
    ) -> Option<Value> {
        let floor = *self.module_eval_scope_floors.last()?;
        for frame_idx in (floor..self.frames.len()).rev() {
            if let Some(value) = self.get_value_from_frame(name, frame_idx) {
                return Some(value);
            }
        }

        let qualified = format!("{module_name}.{name}");
        if let Some(value) = self.get_value_from_frame(&qualified, 0) {
            return Some(value);
        }

        if util::is_root_module_name(module_name) {
            return self.get_value_from_frame(name, 0);
        }
        None
    }

    /// Store an assignment evaluated by module-level `eval`. An evaluated
    /// lexical construct such as `let` owns a temporary frame above the eval
    /// floor; otherwise the assignment is a module/global binding (#11071).
    pub(super) fn set_eval_variable_value(
        &mut self,
        name: &str,
        val: Value,
        module_name: Option<&str>,
    ) {
        let Some(&floor) = self
            .module_eval_scope_floors
            .last()
            .or_else(|| self.lexical_eval_scope_floors.last())
        else {
            // Eval-defined method bodies use the same tree walker without a
            // module-eval boundary and retain ordinary function-local writes.
            self.set_variable_value(name, val);
            return;
        };
        if self.frames.len() > floor {
            // Assignment updates the nearest eval-owned lexical binding. A
            // nested `let`/`catch` only creates a new binding when no enclosing
            // eval scope already owns that name (Issue #11071).
            let target_frame_idx = (floor..self.frames.len())
                .rev()
                .find(|frame_idx| self.is_var_defined_in_frame(name, *frame_idx))
                .unwrap_or_else(|| self.frames.len().saturating_sub(1));
            if let Some(frame) = self.frames.get_mut(target_frame_idx) {
                util::bind_value_to_frame(frame, name, ValueType::Any, val, &mut self.struct_heap);
            }
        } else if let Some(module) = module_name {
            self.store_global_value(&format!("{module}.{name}"), val);
        } else {
            self.store_global_value(name, val);
        }
    }

    /// Set a variable value by name in the current frame.
    /// Used by eval to support assignment expressions.
    pub fn set_variable_value(&mut self, name: &str, val: Value) {
        if let Some(frame) = self.frames.last_mut() {
            util::bind_value_to_frame(frame, name, ValueType::Any, val, &mut self.struct_heap);
        }
    }

    pub(super) fn slot_index_for_frame(&self, frame: &Frame, name: &str) -> Option<usize> {
        if let Some(func_index) = frame.func_index {
            // Fast path: O(1) probe of the pre-computed `name -> slot` map
            // (Issue #5179). Falls back to scanning `slot_names` only when the
            // map is absent — e.g. functions appended after construction by
            // unit-test harnesses that do not refresh `function_slot_maps`.
            if let Some(slot_map) = self.function_slot_maps.get(func_index) {
                return slot_map.get(name).copied();
            }
            self.functions.get(func_index).and_then(|func| {
                func.slot_names
                    .iter()
                    .position(|slot_name| slot_name == name)
            })
        } else {
            self.global_slot_map.get(name).copied()
        }
    }

    pub(super) fn load_slot_value_by_name(&self, frame: &Frame, name: &str) -> Option<Value> {
        let slot = self.slot_index_for_frame(frame, name)?;
        frame.locals_slots.get(slot).and_then(|v| v.clone())
    }

    pub(super) fn slot_name_for_frame(&self, frame: &Frame, slot: usize) -> String {
        if let Some(func_index) = frame.func_index {
            if let Some(name) = self
                .functions
                .get(func_index)
                .and_then(|func| func.slot_names.get(slot))
            {
                return name.clone();
            }
        } else if let Some(name) = self.global_slot_names.get(slot) {
            return name.clone();
        }
        format!("slot {}", slot)
    }

    /// Name (and, when available, declared static slot tag) of the function
    /// owning `frame` — used to make typed-slot `InternalError`s
    /// (`LoadSlotI64: expected numeric in x, got ...`) self-identifying so a
    /// dispatch-/cache-order-dependent miscompile can be traced to the exact
    /// method and slot (Issue #9724).
    pub(super) fn slot_debug_context_for_frame(&self, frame: &Frame, slot: usize) -> String {
        let func = frame.func_index.and_then(|i| self.functions.get(i));
        let fn_name = func.map(|f| f.name.as_str()).unwrap_or("<main>");
        let tag = func
            .and_then(|f| f.slot_types.get(slot))
            .and_then(|t| *t)
            .map(|t| format!("{:?}", t))
            .unwrap_or_else(|| "none".to_string());
        format!("fn `{}` slot#{} tag={}", fn_name, slot, tag)
    }

    /// Set the output callback for streaming output.
    /// The callback will be called for each output line with the context pointer.
    pub fn set_output_callback(&mut self, callback: OutputCallback, context: *mut c_void) {
        self.output_callback = Some(callback);
        self.output_callback_context = context;
    }

    /// Emit output to the buffer and optionally to the callback.
    /// This is the central method for all output operations.
    ///
    /// When inside a sprint call, output is redirected to the sprint's IOBuffer
    /// instead of stdout/the main output buffer.
    pub(super) fn emit_output(&mut self, s: &str, newline: bool) {
        // Check if we're inside a sprint call - if so, redirect output to the sprint buffer
        if let Some(ref state) = self.sprint_state {
            let mut io = state.io.borrow_mut();
            let _ = io.write_buffer_str(s);
            if newline {
                let _ = io.write_buffer_str("\n");
            }
            return;
        }

        let sink = self.current_stdout.clone();
        self.emit_stdout_text_to_sink(&sink, s, newline);
    }

    pub(super) fn emit_stdout_text_to_sink(&mut self, sink: &value::IORef, s: &str, newline: bool) {
        let kind = sink.borrow().kind.clone();
        match kind {
            IOKind::Buffer | IOKind::Pipe => {
                let mut io = sink.borrow_mut();
                let _ = io.write_buffer_str(s);
                if newline {
                    let _ = io.write_buffer_str("\n");
                }
            }
            IOKind::Devnull => {}
            IOKind::Stderr => self.emit_stderr_to_buffer(s, newline),
            IOKind::File => {
                if let Some(handle) = sink.borrow().file_handle.clone() {
                    let _ = handle.borrow_mut().write_str(s);
                    if newline {
                        let _ = handle.borrow_mut().write_str("\n");
                    }
                }
            }
            IOKind::Stdout | IOKind::Stdin => self.emit_stdout_to_buffer(s, newline),
        }
    }

    fn emit_stdout_to_buffer(&mut self, s: &str, newline: bool) {
        self.output.push_str(s);
        if newline {
            self.output.push('\n');
        }

        // Call the streaming callback if set
        if let Some(callback) = self.output_callback {
            let line = if newline {
                format!("{}\n", s)
            } else {
                s.to_string()
            };
            if let Ok(cstr) = CString::new(line) {
                callback(self.output_callback_context, cstr.as_ptr());
            }
        }
    }

    /// Emit captured stderr output (Issue #3573).
    ///
    /// Mirrors `emit_output` but writes to a separate buffer that the runner
    /// (or FFI consumer) is expected to forward to the user's actual stderr
    /// on exit. Inside a `sprint` call we route to the sprint buffer too so
    /// that `sprint(io -> print(stderr, x))` is well-defined.
    pub(super) fn emit_stderr(&mut self, s: &str, newline: bool) {
        if let Some(ref state) = self.sprint_state {
            let mut io = state.io.borrow_mut();
            let _ = io.write_buffer_str(s);
            if newline {
                let _ = io.write_buffer_str("\n");
            }
            return;
        }
        let sink = self.current_stderr.clone();
        let kind = sink.borrow().kind.clone();
        match kind {
            IOKind::Buffer | IOKind::Pipe => {
                let mut io = sink.borrow_mut();
                let _ = io.write_buffer_str(s);
                if newline {
                    let _ = io.write_buffer_str("\n");
                }
            }
            IOKind::Devnull => {}
            IOKind::Stdout => self.emit_stdout_to_buffer(s, newline),
            IOKind::File => {
                if let Some(handle) = sink.borrow().file_handle.clone() {
                    let _ = handle.borrow_mut().write_str(s);
                    if newline {
                        let _ = handle.borrow_mut().write_str("\n");
                    }
                }
            }
            IOKind::Stderr | IOKind::Stdin => self.emit_stderr_to_buffer(s, newline),
        }
    }

    fn emit_stderr_to_buffer(&mut self, s: &str, newline: bool) {
        self.stderr_output.push_str(s);
        if newline {
            self.stderr_output.push('\n');
        }
    }

    /// Get a global variable by name from the top-level frame.
    /// Used by REPL session to extract variables after execution.
    pub fn get_global(&self, name: &str) -> Option<Value> {
        // Look in the first (global) frame
        let frame = self.frames.first()?;

        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(Some(val)) = frame.locals_slots.get(slot) {
                return Some(val.clone());
            }
        }

        // Check fallback locals for TypedArray, StructRef, and other dynamic types.
        if let Some(v) = frame.locals_any.get(name) {
            return Some(v.clone());
        }

        None
    }

    /// Resolve definition-owned bindings that live in the Julia global
    /// namespace without occupying a mutable frame-0 slot. An explicit
    /// `global T` read must see nominal types and generic functions just like an
    /// ordinary top-level name lookup (Issue #11655).
    pub(super) fn get_global_definition_value(&mut self, name: &str) -> Option<Value> {
        if self.eval_nominal_type_name_is_unpublished(name) {
            return None;
        }

        if self.published_eval_nominal_type_names.contains(name) {
            return Some(Value::DataType(Box::new(
                self.datatype_from_name_or_partial_unionall(name),
            )));
        }

        // Qualified spellings carry their declaration owner. Bare compiler
        // registries do not: module collection intentionally installs short
        // aliases for imported lookup, including aliases whose `using` has not
        // executed yet. Never treat those aliases as Main-owned globals.
        let qualified_definition = name.contains('.');
        let alias_target = self
            .compile_context
            .as_ref()
            .and_then(|context| context.type_aliases.get(name))
            .cloned();
        if qualified_definition {
            if let Some(target) = alias_target {
                return Some(Value::DataType(Box::new(
                    self.datatype_from_name_or_partial_unionall(&target),
                )));
            }
        }

        if qualified_definition && self.active_enum_name_index.contains_key(name) {
            return Some(Value::DataType(Box::new(JuliaType::Enum(name.to_string()))));
        }

        let is_qualified_nominal_type = qualified_definition
            && (self
                .struct_defs
                .iter()
                .any(|definition| definition.name == name)
                || self
                    .abstract_types
                    .iter()
                    .any(|definition| definition.name == name)
                || self.compile_context.as_ref().is_some_and(|context| {
                    context.parametric_structs.contains_key(name)
                        || context
                            .primitive_types
                            .iter()
                            .any(|definition| definition.name == name)
                }));
        if is_qualified_nominal_type {
            return Some(Value::DataType(Box::new(
                self.datatype_from_name_or_partial_unionall(name),
            )));
        }

        let world = self.current_dispatch_world();
        self.get_function_indices_by_name(name)
            .iter()
            .any(|&index| {
                self.functions.get(index).is_some_and(|function| {
                    function.name == name && self.function_visible_in_world(index, world)
                })
            })
            .then(|| Value::Function(FunctionValue::new(name.to_string())))
    }

    /// Resolve only a nominal binding whose source-position publication has
    /// already executed. Signature probes must not use the broader compiler
    /// registry lookup above: that registry contains source-later root types
    /// before their Julia binding exists (Issue #11025). Runtime-conditional
    /// declarations enter this set when reached, which makes them available to
    /// an immediately following method signature (Issue #11688).
    pub(super) fn get_published_eval_nominal_type_value(&mut self, name: &str) -> Option<Value> {
        self.published_eval_nominal_type_names
            .contains(name)
            .then(|| Value::DataType(Box::new(self.datatype_from_name_or_partial_unionall(name))))
    }

    /// Get a reference to the struct heap (for REPL display)
    pub fn get_struct_heap(&self) -> &[StructInstance] {
        &self.struct_heap
    }

    /// Number of bytecode instructions currently installed (Issue #9199 LV1).
    ///
    /// Read by the REPL to check the live-VM append invariant: a delta whose
    /// `CompiledProgram.entry` equals this length has its new `main` emitted at
    /// exactly the offset where the live `code` ends, so appending that `main`
    /// makes the live `code` byte-identical to the delta's whole program and the
    /// VM can re-enter at `entry` without any renumbering.
    pub fn code_len(&self) -> usize {
        self.code.len()
    }

    /// Return the innermost lexical declaration owner for `name`, preserving
    /// declared-but-uninitialized state separately from an absent declaration.
    pub(super) fn root_lexical_binding(&self, name: &str) -> Option<&Option<Value>> {
        self.lexical_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
    }

    fn root_lexical_binding_mut(&mut self, name: &str) -> Option<&mut Option<Value>> {
        self.lexical_scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(name))
    }

    pub(super) fn enter_root_lexical_scope(&mut self, names: &[String]) -> Result<(), VmError> {
        if self.frames.len() != 1 {
            return Err(VmError::InternalError(
                "EnterLexicalScope executed outside module/main frame".to_string(),
            ));
        }
        self.lexical_scopes.push(RootLexicalScope::new(names));
        Ok(())
    }

    pub(super) fn store_root_lexical(&mut self, name: &str, value: Value) -> Result<(), VmError> {
        // Lexical bindings have the same Julia storage semantics as frame
        // slots: mutable structs must be heap-indirected so aliases keep one
        // object identity instead of cloning independent inline values.
        let value = self.value_for_slot_storage(value);
        let Some(binding) = self.root_lexical_binding_mut(name) else {
            return Err(VmError::InternalError(format!(
                "StoreLexical has no declaration owner for {name}"
            )));
        };
        *binding = Some(value);
        Ok(())
    }

    pub(super) fn exit_root_lexical_scope(&mut self) -> Result<(), VmError> {
        if self.frames.len() != 1 {
            return Err(VmError::InternalError(
                "ExitLexicalScope executed outside module/main frame".to_string(),
            ));
        }
        self.lexical_scopes.pop().map(|_| ()).ok_or_else(|| {
            VmError::InternalError("ExitLexicalScope without matching enter".to_string())
        })
    }

    /// Restore the per-run VM state to its fresh-entry baseline while keeping
    /// the live REPL world intact. Frame 0, the heap, installed definitions,
    /// module state, method worlds, and dispatch caches remain authoritative;
    /// only state owned by the completed/failed invocation is cleared.
    fn reset_repl_transient_state(&mut self, rng: R) {
        self.stack.clear();
        self.transient_roots.clear();
        self.frames.truncate(1); // keep frame-0 (module globals)
        self.lexical_scopes.clear();
        self.return_ips.clear();
        self.handlers.clear();
        self.tasks = Self::fresh_task_table();
        self.runnable_tasks.clear();
        self.sleeping_tasks.clear();
        self.current_task_id = 0;
        self.output.clear();
        self.stderr_output.clear();
        self.broadcast_states.clear();
        self.composed_call_state = None;
        self.generator_iterate_state.clear();
        self.sprint_state = None;
        // A failed redirect thunk returns before `handle_redirect_return` can
        // restore its previous stream. Unwind in LIFO order; simply clearing the
        // states would leave `current_stdout` / `current_stderr` redirected in
        // the next REPL input (Issue #9784).
        while let Some(state) = self.redirect_states.pop() {
            self.restore_redirect_stream(state);
        }
        self.pending_error = None;
        self.pending_exception_value = None;
        self.pending_backtrace = None;
        self.clear_pending_exception_payloads();
        self.caught_exceptions.clear();
        self.pending_finally_rethrows.clear();
        self.pending_eval_enum_member_bindings.clear();
        self.test_pass_count = 0;
        self.test_fail_count = 0;
        self.test_broken_count = 0;
        self.test_error_count = 0;
        self.current_testset = None;
        self.testset_stack.clear();
        self.any_test_failed = false;
        self.test_throws_state = None;
        self.last_error_ip = None;
        self.display_artifacts.clear();
        self.graphical_display_active = false;
        self.eval_dispatch_depth = 0;
        self.eval_dispatch_floor = None;
        self.generated_expr_pending_keys.clear();
        self.generated_expr_pending_eval_frames.clear();
        self.module_eval_scope_floors.clear();
        self.lexical_eval_scope_floors.clear();
        self.call_depth_overflow_pending = false;
        self.rng = rng;
    }

    /// Recover a live REPL VM after an unhandled runtime error without replacing
    /// its persistent world (Issue #9784). Unlike normal re-entry, an error may
    /// legitimately leave native transient roots registered, so recovery clears
    /// them instead of asserting that the failed invocation already released
    /// them. The next appended main chooses the new entry IP.
    pub fn recover_repl_toplevel_after_error(&mut self, rng: R) {
        self.reset_repl_transient_state(rng);
    }

    /// Splice an expression-only REPL delta's freshly compiled `main` onto the
    /// live VM and reset the transient per-run state, so the next `run()`
    /// executes the delta while PRESERVING the persistent state it must observe
    /// (Issue #9199 LV1; see `docs/vm/ADR_REPL_EVAL_MODEL.md` §"Live-VM slice
    /// decomposition"). This is the runtime half of the relocatable-delta
    /// contract: the caller has compiled `main_code` against the SAME accumulated
    /// index space this VM already holds (function indices, global slots, struct
    /// type-ids), and `main_code` was emitted at offset `self.code_len()`, so no
    /// reference needs renumbering — the append is a pure splice.
    ///
    /// Grows in place: `code` (copy-on-write via `Rc::make_mut`), the derived
    /// `executable` predecode table (`append_bytecode`), the IP-indexed
    /// `call_site_caches`, and `source_map`. Resets to the fresh-`run()` baseline:
    /// the operand stack, call frames above frame-0, the output buffers, the
    /// error/exception state, the per-run test counters, the display sink, and
    /// the RNG (re-seeded by the caller for per-eval determinism, matching the
    /// fresh-VM path). LEAVES UNTOUCHED (the whole point): frame-0 module globals,
    /// the struct heap, every dispatch / specialization cache, the interned type
    /// ids, and the `current_world` / `dispatch_generation` counters.
    ///
    /// Preconditions the caller guarantees (an LV1 delta defines nothing and adds
    /// no global): `main_start == self.code.len()`, `self.functions` unchanged,
    /// and the frame-0 global layout unchanged. Returns the entry IP for `run()`.
    ///
    /// This is the LV1 runtime primitive, WIRED by LV2 (Issue #9199): the REPL's
    /// `try_live_delta_run` feeds it an isolated, prefix-aligned, slot-seeded delta
    /// `main` produced by `repl_relocatable_delta_compile` (the relocatable-delta
    /// compiler contract LV1 lacked). Also exercised directly by
    /// `repl::session::lv1_live_vm_tests`.
    pub fn reenter_appended_main(
        &mut self,
        main_code: &[Instr],
        main_source_map: &[Option<crate::span::Span>],
        rng: R,
    ) -> usize {
        let main_start = self.code.len();
        self.repl_definition_activations.clear();
        self.repl_using_activations.clear();
        self.repl_module_activations.clear();
        self.repl_runtime_function_indices.clear();
        self.repl_written_globals.clear();
        self.repl_explicit_global_writes.clear();
        self.repl_function_refresh_groups.clear();
        self.repl_specializable_updates.clear();
        self.repl_world_sensitive_specializable_indices.clear();

        // Grow the shared bytecode vector. The previous `run()` dropped its
        // `Rc::clone` snapshot, so this is normally a unique `Rc` mutated in
        // place (the same copy-on-write append `CallSpecialize` / `@eval` use).
        let code = std::rc::Rc::make_mut(&mut self.code);
        code.extend_from_slice(main_code);
        let code_end = code.len();

        // Grow the predecoded hot-block table and the IP-indexed inline call-site
        // cache to cover the appended range. A brand-new call site starts cold;
        // the preserved caches for the prefix stay warm.
        self.executable.append_bytecode(
            &self.code,
            &self.functions,
            self.base_function_count,
            main_start,
            code_end,
        );
        self.call_site_caches
            .resize(code_end, CallSiteCache::default());

        // Keep the source map aligned with `code` (used for error spans). The
        // live source map covers `[0, main_start)`; append the delta main's spans
        // and pad any shortfall so `source_map.len() == code.len()`.
        self.source_map.truncate(main_start);
        self.source_map.resize(main_start, None);
        self.source_map.extend_from_slice(main_source_map);
        self.source_map.resize(code_end, None);

        // Enter the appended main and refresh the executable-block cursor.
        self.ip = main_start;
        self.next_executable_ip = self.executable.next_ip_from(main_start);

        // Reset the transient per-run state to the fresh-`run()` baseline. This
        // mirrors the non-persistent field initializers of `new_program`; the
        // persistent fields (frame-0, struct_heap, functions, all caches, world,
        // interner) are deliberately left as-is.
        debug_assert!(
            self.transient_roots.is_empty(),
            "native transient roots leaked across a completed VM run"
        );
        self.reset_repl_transient_state(rng);

        main_start
    }

    /// Number of installed functions (Issue #9199 LV3). The REPL checks this
    /// equals the compile prefix's function count before a compiled-definition
    /// live-append, so each new function's delta index equals its live index and
    /// no function-index relocation is needed.
    pub fn functions_len(&self) -> usize {
        self.functions.len()
    }

    /// Whether any persistent runtime root refers to a function installed at or
    /// after `first_function`. This detects observable helpers nested inside
    /// mutable containers, structs, generators, closures, tasks, and lexical
    /// roots even when no `StoreGlobal` instruction executed (#9784).
    pub fn repl_roots_reference_function_suffix(&self, first_function: usize) -> bool {
        if first_function >= self.functions.len() {
            return false;
        }
        let mut visited = CallableReachVisited::default();
        let mut inspect = |value: &Value| {
            value_references_function_suffix(value, &self.struct_heap, first_function, &mut visited)
        };

        self.stack.iter().any(&mut inspect)
            || self.frames.iter().any(|frame| {
                frame.locals_slots.iter().flatten().any(&mut inspect)
                    || frame.locals_any.values().any(&mut inspect)
                    || frame.captured_vars.values().any(&mut inspect)
            })
            || self
                .lexical_scopes
                .iter()
                .flat_map(RootLexicalScope::values)
                .any(&mut inspect)
            || self.tasks.iter().any(|task| {
                inspect(&task.object)
                    || task.entry.as_ref().is_some_and(&mut inspect)
                    || task.context.as_ref().is_some_and(|context| {
                        context.stack.iter().any(&mut inspect)
                            || context.frames.iter().any(|frame| {
                                frame.locals_slots.iter().flatten().any(&mut inspect)
                                    || frame.locals_any.values().any(&mut inspect)
                                    || frame.captured_vars.values().any(&mut inspect)
                            })
                            || context
                                .lexical_scopes
                                .iter()
                                .flat_map(RootLexicalScope::values)
                                .any(&mut inspect)
                            || context
                                .pending_exception_value
                                .as_ref()
                                .is_some_and(&mut inspect)
                            || context
                                .caught_exceptions
                                .iter()
                                .filter_map(|(_, value, _)| value.as_ref())
                                .any(&mut inspect)
                    })
            })
            || self.transient_roots.iter().any(|root| inspect(&root.value))
            || self.finalizers.iter().any(|entry| {
                entry.active && (inspect(&entry.callback) || inspect(&entry.object_snapshot))
            })
            || self
                .pending_finalizers
                .iter()
                .any(|(callback, object)| inspect(callback) || inspect(object))
    }

    /// Roll back a function/code/global-slot suffix that no persistent value
    /// observes. Source methods must still be dormant (`min_world == MAX`);
    /// private helpers are immediately callable but may be removed only when no
    /// root carries them. This preserves recovered frame-0/heap state while
    /// preventing repeated pre-definition failures from growing forever.
    pub fn rollback_unobserved_repl_function_append(
        &mut self,
        first_function: usize,
        first_global_slot: usize,
    ) -> bool {
        if first_function >= self.functions.len()
            || self.functions[first_function..]
                .iter()
                .any(|function| !function.is_lowering_helper && function.min_world != u64::MAX)
            || self.repl_roots_reference_function_suffix(first_function)
        {
            return false;
        }

        let code_start = self.functions[first_function].entry;
        if code_start > self.code.len() || first_global_slot > self.global_slot_names.len() {
            return false;
        }

        let removed_global_names = self.global_slot_names.split_off(first_global_slot);
        if let Some(frame0) = self.frames.first_mut() {
            for (offset, name) in removed_global_names.iter().enumerate() {
                if let Some(value) = frame0
                    .locals_slots
                    .get(first_global_slot + offset)
                    .and_then(Clone::clone)
                {
                    frame0.locals_any.insert(name.clone(), value);
                    frame0
                        .var_types
                        .insert(name.clone(), crate::vm::frame::VarTypeTag::Any);
                }
            }
            frame0.locals_slots.truncate(first_global_slot);
        }
        self.global_slot_map
            .retain(|_, index| *index < first_global_slot);

        self.functions.truncate(first_function);
        self.function_slot_maps.truncate(first_function);
        self.native_array_exempt_functions.truncate(first_function);
        self.function_name_index
            .values_mut()
            .for_each(|indices| indices.retain(|index| *index < first_function));
        self.function_name_index
            .retain(|_, indices| !indices.is_empty());
        self.lowering_helper_name_index
            .values_mut()
            .for_each(|indices| indices.retain(|index| *index < first_function));
        self.lowering_helper_name_index
            .retain(|_, indices| !indices.is_empty());
        self.eval_defined_bodies
            .retain(|index, _| *index < first_function);
        self.specializable_functions
            .retain(|function| function.fallback_index < first_function);
        self.specializable_callable_registry_cache = None;

        std::rc::Rc::make_mut(&mut self.code).truncate(code_start);
        self.source_map.truncate(code_start);
        self.call_site_caches.truncate(code_start);
        self.executable =
            ExecutableProgram::from_bytecode(&self.code, &self.functions, self.base_function_count);
        self.ip = code_start;
        self.next_executable_ip = self.executable.next_ip_from(code_start);
        self.repl_definition_activations.clear();
        self.repl_using_activations.clear();
        self.repl_module_activations.clear();
        self.repl_runtime_function_indices.clear();
        self.repl_function_refresh_groups.clear();
        self.repl_specializable_updates.clear();
        self.repl_world_sensitive_specializable_indices.clear();
        self.clear_runtime_caches();
        true
    }

    pub fn prepare_repl_append_setup(
        &self,
        counts: ReplAppendDefinitionCounts,
        new_specializable_functions: Vec<SpecializableFunction>,
        activations: &[ReplDefinitionActivation],
        specializable_updates: &[(usize, SpecializableFunction)],
    ) -> Option<PreparedReplAppendSetup> {
        if !self.pending_eval_struct_defs.is_empty()
            || !self.pending_eval_abstract_types.is_empty()
            || !self.pending_eval_primitive_types.is_empty()
            || !self.pending_eval_enum_defs.is_empty()
            || (counts.primitive_types > 0 && self.compile_context.is_none())
        {
            return None;
        }

        let first_function = self.functions.len();
        let projected_functions_len = self.functions.len().checked_add(counts.function_bodies)?;
        let projected_specializable_len = self
            .specializable_functions
            .len()
            .checked_add(new_specializable_functions.len())?;
        if new_specializable_functions
            .iter()
            .any(|function| function.fallback_index >= projected_functions_len)
        {
            return None;
        }

        let first_struct = self.struct_defs.len();
        let first_abstract = self.abstract_types.len();
        let first_primitive = self
            .compile_context
            .as_ref()
            .map_or(0, |context| context.primitive_types.len());
        let first_enum = self.enum_defs.len();
        let struct_end = first_struct.checked_add(counts.structs)?;
        let abstract_end = first_abstract.checked_add(counts.abstract_types)?;
        let primitive_end = first_primitive.checked_add(counts.primitive_types)?;
        let enum_end = first_enum.checked_add(counts.enums)?;
        let mut function_members = HashSet::new();
        let mut struct_indices = HashSet::new();
        let mut abstract_indices = HashSet::new();
        let mut primitive_indices = HashSet::new();
        let mut enum_indices = HashSet::new();
        for activation in activations {
            match activation {
                ReplDefinitionActivation::Function(index) => {
                    if !(first_function..projected_functions_len).contains(index)
                        || !function_members.insert(*index)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::FunctionGroup { primary, refresh } => {
                    if refresh.is_empty()
                        || !(first_function..projected_functions_len).contains(primary)
                        || !function_members.insert(*primary)
                        || refresh.iter().any(|index| {
                            !(first_function..projected_functions_len).contains(index)
                                || !function_members.insert(*index)
                        })
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::Struct(index) => {
                    if !(first_struct..struct_end).contains(index) || !struct_indices.insert(*index)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::AbstractType(index) => {
                    if !(first_abstract..abstract_end).contains(index)
                        || !abstract_indices.insert(*index)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::PrimitiveType(index) => {
                    if !(first_primitive..primitive_end).contains(index)
                        || !primitive_indices.insert(*index)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::Enum(index) => {
                    if !(first_enum..enum_end).contains(index) || !enum_indices.insert(*index) {
                        return None;
                    }
                }
                ReplDefinitionActivation::RuntimeNominal(_) => {}
            }
        }
        // Marker-less helpers may occupy any appended function index, but every
        // Julia-visible source method has exactly one primary activation.
        // Nominal declarations likewise require exactly one marker.
        if function_members.len() < counts.source_functions
            || activations
                .iter()
                .filter(|activation| {
                    matches!(
                        activation,
                        ReplDefinitionActivation::Function(_)
                            | ReplDefinitionActivation::FunctionGroup { .. }
                    )
                })
                .count()
                != counts.source_functions
            || struct_indices.len() != counts.structs
            || abstract_indices.len() != counts.abstract_types
            || primitive_indices.len() != counts.primitive_types
            || enum_indices.len() != counts.enums
        {
            return None;
        }

        let (refresh_groups, prepared_updates, world_sensitive) =
            Self::build_repl_function_activation_state(
                projected_functions_len,
                projected_specializable_len,
                activations,
                specializable_updates,
            )?;
        Some(PreparedReplAppendSetup {
            expected_functions_len: projected_functions_len,
            expected_specializable_prefix_len: self.specializable_functions.len(),
            new_specializable_functions,
            refresh_groups,
            specializable_updates: prepared_updates,
            world_sensitive_specializable_indices: world_sensitive,
        })
    }

    pub fn install_prepared_repl_append_setup(&mut self, setup: PreparedReplAppendSetup) {
        debug_assert_eq!(self.functions.len(), setup.expected_functions_len);
        debug_assert_eq!(
            self.specializable_functions.len(),
            setup.expected_specializable_prefix_len
        );
        self.specializable_functions
            .extend(setup.new_specializable_functions);
        self.specializable_callable_registry_cache = None;
        self.repl_function_refresh_groups = setup.refresh_groups;
        self.repl_specializable_updates = setup.specializable_updates;
        self.repl_world_sensitive_specializable_indices =
            setup.world_sensitive_specializable_indices;
    }

    /// Append the compiler-verified contiguous specialization registry tail.
    /// Every fallback body must already be installed at its aligned function
    /// index; method world visibility remains authoritative at execution.
    pub fn install_appended_specializable_functions(
        &mut self,
        functions: Vec<SpecializableFunction>,
    ) -> bool {
        if functions
            .iter()
            .any(|function| function.fallback_index >= self.functions.len())
        {
            return false;
        }
        self.specializable_functions.extend(functions);
        self.specializable_callable_registry_cache = None;
        true
    }

    /// Install the compiler-proved caller refresh groups for the appended main.
    /// A group is keyed by its source `DefineEvalFunction` marker and every
    /// member must already be installed as a dormant function body.
    pub fn configure_repl_function_activation_state(
        &mut self,
        activations: &[ReplDefinitionActivation],
        specializable_updates: &[(usize, SpecializableFunction)],
    ) -> bool {
        let Some((refresh_groups, prepared_updates, world_sensitive)) =
            Self::build_repl_function_activation_state(
                self.functions.len(),
                self.specializable_functions.len(),
                activations,
                specializable_updates,
            )
        else {
            return false;
        };
        self.repl_function_refresh_groups = refresh_groups;
        self.repl_specializable_updates = prepared_updates;
        self.repl_world_sensitive_specializable_indices = world_sensitive;
        true
    }

    fn build_repl_function_activation_state(
        functions_len: usize,
        specializable_functions_len: usize,
        activations: &[ReplDefinitionActivation],
        specializable_updates: &[(usize, SpecializableFunction)],
    ) -> Option<(
        HashMap<usize, Vec<usize>>,
        HashMap<usize, Vec<(usize, SpecializableFunction)>>,
        HashSet<usize>,
    )> {
        let mut refresh_groups = HashMap::new();
        let mut prepared_updates: HashMap<usize, Vec<(usize, SpecializableFunction)>> =
            HashMap::new();
        let mut world_sensitive = HashSet::new();
        let mut members = HashSet::new();
        for activation in activations {
            match activation {
                ReplDefinitionActivation::Function(index) => {
                    if *index >= functions_len || !members.insert(*index) {
                        return None;
                    }
                }
                ReplDefinitionActivation::FunctionGroup { primary, refresh } => {
                    if refresh.is_empty()
                        || *primary >= functions_len
                        || !members.insert(*primary)
                        || refresh.iter().any(|index| {
                            *index >= functions_len || *index == *primary || !members.insert(*index)
                        })
                    {
                        return None;
                    }
                    refresh_groups.insert(*primary, refresh.clone());
                }
                ReplDefinitionActivation::Struct(_)
                | ReplDefinitionActivation::AbstractType(_)
                | ReplDefinitionActivation::PrimitiveType(_)
                | ReplDefinitionActivation::Enum(_)
                | ReplDefinitionActivation::RuntimeNominal(_) => {}
            }
        }
        for (index, update) in specializable_updates {
            if *index >= specializable_functions_len || !members.contains(&update.fallback_index) {
                return None;
            }
            prepared_updates
                .entry(update.fallback_index)
                .or_default()
                .push((*index, update.clone()));
            world_sensitive.insert(*index);
        }
        Some((refresh_groups, prepared_updates, world_sensitive))
    }

    /// Number of installed concrete struct definitions (Issue #9199 LV4). The
    /// REPL checks this equals the compile prefix's struct-def count before a
    /// compiled-struct live-append, so each new struct installs at the `type_id`
    /// (== its index in `struct_defs`) that the relocatable-delta compiler baked
    /// into every `NewStruct(type_id, ..)` — no type-id relocation is needed.
    pub fn struct_defs_len(&self) -> usize {
        self.struct_defs
            .len()
            .saturating_sub(self.hidden_eval_struct_type_ids.len())
    }

    pub fn abstract_types_len(&self) -> usize {
        self.abstract_types
            .len()
            .saturating_sub(self.hidden_eval_abstract_type_ids.len())
    }

    pub fn primitive_types_len(&self) -> usize {
        self.compile_context
            .as_ref()
            .map_or(0, |context| context.primitive_types.len())
            .saturating_sub(self.hidden_eval_primitive_type_ids.len())
    }

    pub fn enum_defs_len(&self) -> usize {
        self.enum_defs
            .len()
            .saturating_sub(self.hidden_eval_enum_type_ids.len())
    }

    /// Runtime definition state that must stay aligned with the REPL compiler
    /// snapshot when an errored live VM is retained.
    ///
    /// A source delta with no visible definitions can still execute `@eval`
    /// inside a called function. That appends/redefines runtime methods and
    /// advances `current_world` without producing a corresponding persistent
    /// compiler snapshot. The REPL compares this fingerprint around `run()` and
    /// conservatively drops such an errored VM (Issue #9784).
    pub fn repl_definition_world_fingerprint(&self) -> ReplDefinitionWorldFingerprint {
        ReplDefinitionWorldFingerprint {
            functions_len: self.functions.len(),
            active_structs_len: self.struct_defs_len(),
            pending_structs_len: self.pending_eval_struct_defs.len(),
            active_abstract_types_len: self.abstract_types_len(),
            pending_abstract_types_len: self.pending_eval_abstract_types.len(),
            active_primitive_types_len: self.primitive_types_len(),
            pending_primitive_types_len: self.pending_eval_primitive_types.len(),
            active_enums_len: self.enum_defs_len(),
            pending_enums_len: self.pending_eval_enum_defs.len(),
            current_world: self.current_world,
        }
    }

    pub(super) fn record_repl_runtime_function(&mut self, index: usize) {
        if !self.repl_runtime_function_indices.contains(&index) {
            self.repl_runtime_function_indices.push(index);
        }
    }

    /// Validate and return the exact interleaved prefix of newly appended
    /// function and concrete-struct definitions published by this run.
    ///
    /// `before` is captured after all dormant bodies were installed and before
    /// the appended main ran. A valid run may only activate those bodies in
    /// their contiguous source order: every reached `DefineEvalFunction` bumps
    /// the world once and stamps the matching method with that world, while the
    /// unreached suffix remains at `u64::MAX`. Any function/type registry drift,
    /// unrelated world mutation, skipped/duplicate marker, or out-of-order
    /// activation rejects recovery so the session never pairs a live runtime
    /// with a different compiler snapshot (Issues #9784 and #11477).
    pub fn repl_reached_appended_definition_prefix(
        &self,
        before: ReplDefinitionWorldFingerprint,
        expected: &[ReplDefinitionActivation],
        runtime_nominal_templates: &[DefineRuntimeNominalOperands],
        starts: ReplAppendDefinitionStarts,
        counts: ReplAppendDefinitionCounts,
        source_function_indices: &[usize],
    ) -> Option<ReachedReplDefinitionPrefix> {
        if self.functions.len() != before.functions_len
            || before.pending_structs_len != counts.structs
            || before.pending_abstract_types_len != counts.abstract_types
            || before.pending_primitive_types_len != counts.primitive_types
            || before.pending_enums_len != counts.enums
            || starts.functions.checked_add(counts.function_bodies)? > before.functions_len
            || counts.source_functions > counts.function_bodies
            || counts.source_functions != source_function_indices.len()
        {
            return None;
        }
        let observed_root_activations = self
            .repl_definition_activations
            .iter()
            .filter(|activation| !matches!(activation, ReplDefinitionActivation::RuntimeNominal(_)))
            .collect::<Vec<_>>();
        if observed_root_activations.len() > expected.len()
            || expected
                .iter()
                .zip(observed_root_activations.iter())
                .any(|(expected, observed)| expected != *observed)
        {
            return None;
        }
        let templates_by_site = runtime_nominal_templates
            .iter()
            .map(|template| (template.site_id, template))
            .collect::<HashMap<_, _>>();
        if templates_by_site.len() != runtime_nominal_templates.len() {
            return None;
        }

        let first_appended_function = starts.functions;
        let first_appended_struct = starts.structs;
        let appended_struct_end = first_appended_struct.checked_add(counts.structs)?;
        let first_appended_abstract_type = starts.abstract_types;
        let appended_abstract_type_end =
            first_appended_abstract_type.checked_add(counts.abstract_types)?;
        let first_appended_primitive_type = starts.primitive_types;
        let appended_primitive_type_end =
            first_appended_primitive_type.checked_add(counts.primitive_types)?;
        let first_appended_enum = starts.enums;
        let appended_enum_end = first_appended_enum.checked_add(counts.enums)?;
        let mut expected_function_primaries = Vec::new();
        let mut all_activation_members = HashSet::new();
        let mut expected_structs = HashSet::new();
        let mut expected_abstract_types = HashSet::new();
        let mut expected_primitive_types = HashSet::new();
        let mut expected_enums = HashSet::new();
        for activation in expected {
            match activation {
                ReplDefinitionActivation::Function(index) => {
                    if !(first_appended_function..before.functions_len).contains(index)
                        || !all_activation_members.insert(*index)
                    {
                        return None;
                    }
                    expected_function_primaries.push(*index);
                }
                ReplDefinitionActivation::FunctionGroup { primary, refresh } => {
                    if !(first_appended_function..before.functions_len).contains(primary)
                        || !all_activation_members.insert(*primary)
                        || refresh.is_empty()
                        || refresh.iter().any(|index| {
                            !(first_appended_function..before.functions_len).contains(index)
                                || !all_activation_members.insert(*index)
                        })
                    {
                        return None;
                    }
                    expected_function_primaries.push(*primary);
                }
                ReplDefinitionActivation::Struct(type_id) => {
                    if !(first_appended_struct..appended_struct_end).contains(type_id)
                        || !expected_structs.insert(*type_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::AbstractType(type_id) => {
                    if !(first_appended_abstract_type..appended_abstract_type_end).contains(type_id)
                        || !expected_abstract_types.insert(*type_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::PrimitiveType(type_id) => {
                    if !(first_appended_primitive_type..appended_primitive_type_end)
                        .contains(type_id)
                        || !expected_primitive_types.insert(*type_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::Enum(enum_id) => {
                    if !(first_appended_enum..appended_enum_end).contains(enum_id)
                        || !expected_enums.insert(*enum_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::RuntimeNominal(_) => {}
            }
        }
        if expected_function_primaries != source_function_indices
            || expected_structs.len() != counts.structs
            || expected_abstract_types.len() != counts.abstract_types
            || expected_primitive_types.len() != counts.primitive_types
            || expected_enums.len() != counts.enums
        {
            return None;
        }
        let all_runtime_constructor_indices = collect_runtime_constructor_indices(
            runtime_nominal_templates,
            before.functions_len,
            &all_activation_members,
        )?;
        let mut reached_functions = Vec::new();
        let mut reached_runtime_constructors = Vec::new();
        let mut reached_function_worlds = HashMap::new();
        let mut reached_refresh_worlds = HashMap::new();
        let mut reached_world_activation_count = 0usize;
        let mut reached_structs = HashSet::new();
        let mut reached_abstract_types = HashSet::new();
        let mut reached_primitive_types = HashSet::new();
        let mut reached_enums = HashSet::new();
        let mut reached_runtime_sites = HashSet::new();
        let mut runtime_nominal_activations = Vec::new();
        for activation in &self.repl_definition_activations {
            match activation {
                ReplDefinitionActivation::Function(index)
                | ReplDefinitionActivation::FunctionGroup { primary: index, .. } => {
                    if !(first_appended_function..before.functions_len).contains(index)
                        || reached_functions.contains(index)
                    {
                        return None;
                    }
                    reached_world_activation_count =
                        reached_world_activation_count.checked_add(1)?;
                    let world = before
                        .current_world
                        .checked_add(u64::try_from(reached_world_activation_count).ok()?)?;
                    reached_functions.push(*index);
                    reached_function_worlds.insert(*index, world);
                    if let ReplDefinitionActivation::FunctionGroup { refresh, .. } = activation {
                        for member in refresh {
                            if !all_activation_members.contains(member)
                                || reached_refresh_worlds.insert(*member, world).is_some()
                            {
                                return None;
                            }
                        }
                    }
                }
                ReplDefinitionActivation::Struct(type_id) => {
                    if !(first_appended_struct..appended_struct_end).contains(type_id)
                        || !reached_structs.insert(*type_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::AbstractType(type_id) => {
                    if !(first_appended_abstract_type..appended_abstract_type_end).contains(type_id)
                        || !reached_abstract_types.insert(*type_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::PrimitiveType(type_id) => {
                    if !(first_appended_primitive_type..appended_primitive_type_end)
                        .contains(type_id)
                        || !reached_primitive_types.insert(*type_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::Enum(enum_id) => {
                    if !(first_appended_enum..appended_enum_end).contains(enum_id)
                        || !reached_enums.insert(*enum_id)
                    {
                        return None;
                    }
                }
                ReplDefinitionActivation::RuntimeNominal(activation) => {
                    let template = templates_by_site.get(&activation.site_id)?;
                    if !reached_runtime_sites.insert(activation.site_id)
                        || activation.span != template.span
                        || activation.definition != template.definition
                    {
                        return None;
                    }
                    for &index in &template.constructor_function_indices {
                        if !all_runtime_constructor_indices.contains(&index)
                            || reached_function_worlds.contains_key(&index)
                        {
                            return None;
                        }
                        reached_world_activation_count =
                            reached_world_activation_count.checked_add(1)?;
                        let world = before
                            .current_world
                            .checked_add(u64::try_from(reached_world_activation_count).ok()?)?;
                        reached_function_worlds.insert(index, world);
                        reached_runtime_constructors.push(index);
                    }
                    runtime_nominal_activations.push(activation.clone());
                }
            }
        }

        let reached_function_count = reached_functions.len();
        let reached_struct_count = reached_structs.len();
        let reached_abstract_type_count = reached_abstract_types.len();
        let reached_primitive_type_count = reached_primitive_types.len();
        let reached_enum_count = reached_enums.len();
        let (
            runtime_struct_count,
            runtime_abstract_type_count,
            runtime_primitive_type_count,
            runtime_enum_count,
        ) = runtime_nominal_activation_counts(&runtime_nominal_activations);
        let world_delta =
            usize::try_from(self.current_world.checked_sub(before.current_world)?).ok()?;
        let reached_reserved_runtime_struct_count = reached_reserved_runtime_struct_count(
            &runtime_nominal_activations,
            runtime_nominal_templates,
        );
        if world_delta != reached_world_activation_count
            || self.struct_defs_len()
                != before
                    .active_structs_len
                    .checked_add(reached_struct_count)?
                    .checked_add(runtime_struct_count)?
            || self.pending_eval_struct_defs.len()
                != before
                    .pending_structs_len
                    .checked_sub(reached_struct_count.checked_add(
                        reached_reserved_runtime_struct_count.min(before.pending_structs_len),
                    )?)?
            || self.abstract_types_len()
                != before
                    .active_abstract_types_len
                    .checked_add(reached_abstract_type_count)?
                    .checked_add(runtime_abstract_type_count)?
            || self.pending_eval_abstract_types.len()
                != before
                    .pending_abstract_types_len
                    .checked_sub(reached_abstract_type_count)?
            || self.primitive_types_len()
                != before
                    .active_primitive_types_len
                    .checked_add(reached_primitive_type_count)?
                    .checked_add(runtime_primitive_type_count)?
            || self.pending_eval_primitive_types.len()
                != before
                    .pending_primitive_types_len
                    .checked_sub(reached_primitive_type_count)?
            || self.enum_defs_len()
                != before
                    .active_enums_len
                    .checked_add(reached_enum_count)?
                    .checked_add(runtime_enum_count)?
            || self.pending_eval_enum_defs.len()
                != before.pending_enums_len.checked_sub(reached_enum_count)?
        {
            return None;
        }

        let appended_function_end = starts.functions.checked_add(counts.function_bodies)?;
        if !self.repl_function_worlds_match(
            first_appended_function,
            appended_function_end,
            &reached_function_worlds,
            &reached_refresh_worlds,
            &all_activation_members,
            &all_runtime_constructor_indices,
        ) {
            return None;
        }

        Some(ReachedReplDefinitionPrefix {
            function_count: reached_function_count,
            runtime_constructor_indices: reached_runtime_constructors,
            struct_count: reached_struct_count,
            abstract_type_count: reached_abstract_type_count,
            primitive_type_count: reached_primitive_type_count,
            enum_count: reached_enum_count,
            runtime_nominal_activations,
            runtime_function_indices: self.repl_runtime_function_indices.clone(),
        })
    }

    fn repl_function_worlds_match(
        &self,
        first_appended_function: usize,
        appended_function_end: usize,
        reached_function_worlds: &HashMap<usize, u64>,
        reached_refresh_worlds: &HashMap<usize, u64>,
        all_activation_members: &HashSet<usize>,
        all_runtime_constructor_indices: &HashSet<usize>,
    ) -> bool {
        for index in first_appended_function..appended_function_end {
            let expected_world = reached_function_worlds
                .get(&index)
                .or_else(|| reached_refresh_worlds.get(&index))
                .copied()
                .unwrap_or_else(|| {
                    if all_activation_members.contains(&index)
                        || all_runtime_constructor_indices.contains(&index)
                    {
                        u64::MAX
                    } else {
                        1
                    }
                });
            if self.functions[index].min_world != expected_world {
                return false;
            }
        }
        all_runtime_constructor_indices.iter().all(|index| {
            (first_appended_function..appended_function_end).contains(index)
                || self.functions[*index].min_world
                    == reached_function_worlds
                        .get(index)
                        .copied()
                        .unwrap_or(u64::MAX)
        })
    }

    /// Drop the private, unreached concrete-type suffix after the typed prefix
    /// has been validated and projected into the compiler snapshot.
    pub fn discard_unreached_repl_struct_defs(&mut self) {
        self.pending_eval_struct_defs.clear();
        self.pending_eval_abstract_types.clear();
        self.pending_eval_primitive_types.clear();
        self.pending_eval_enum_defs.clear();
    }

    /// Reserve compiled concrete definitions at their already-aligned type IDs.
    /// They stay private until `DefineEvalStruct` reaches each declaration.
    /// A non-empty prior reservation means the runtime/compiler snapshots are
    /// already misaligned, so the caller must restore the VM and fall back.
    pub fn reserve_appended_types(&mut self, new_struct_defs: Vec<StructDefInfo>) -> bool {
        self.reserve_appended_nominal_types(new_struct_defs, Vec::new(), Vec::new(), Vec::new())
    }

    pub fn reserve_appended_nominal_types(
        &mut self,
        new_struct_defs: Vec<StructDefInfo>,
        new_abstract_types: Vec<AbstractTypeDefInfo>,
        new_primitive_types: Vec<PrimitiveTypeDefInfo>,
        new_enum_defs: Vec<EnumDefInfo>,
    ) -> bool {
        if !self.pending_eval_struct_defs.is_empty()
            || !self.pending_eval_abstract_types.is_empty()
            || !self.pending_eval_primitive_types.is_empty()
            || !self.pending_eval_enum_defs.is_empty()
            || (!new_primitive_types.is_empty() && self.compile_context.is_none())
        {
            return false;
        }
        let first_type_id = self.struct_defs.len();
        self.pending_eval_struct_defs.extend(
            new_struct_defs
                .into_iter()
                .enumerate()
                .map(|(offset, def)| (first_type_id + offset, def)),
        );
        let first_abstract_id = self.abstract_types.len();
        self.pending_eval_abstract_types.extend(
            new_abstract_types
                .into_iter()
                .enumerate()
                .map(|(offset, def)| (first_abstract_id + offset, def)),
        );
        let first_primitive_id = self
            .compile_context
            .as_ref()
            .map_or(0, |context| context.primitive_types.len());
        self.pending_eval_primitive_types.extend(
            new_primitive_types
                .into_iter()
                .enumerate()
                .map(|(offset, def)| (first_primitive_id + offset, def)),
        );
        let first_enum_id = self.enum_defs.len();
        self.pending_eval_enum_defs.extend(
            new_enum_defs
                .into_iter()
                .enumerate()
                .map(|(offset, definition)| (first_enum_id + offset, definition)),
        );
        true
    }

    pub(crate) fn eval_struct_type_name_is_pending(&self, type_name: &str) -> bool {
        self.pending_eval_struct_defs
            .iter()
            .any(|(_, definition)| definition.name == type_name)
            || self.hidden_eval_struct_type_ids.iter().any(|type_id| {
                self.struct_defs
                    .get(*type_id)
                    .is_some_and(|definition| definition.name == type_name)
            })
            || self
                .pending_eval_abstract_types
                .iter()
                .any(|(_, definition)| definition.name == type_name)
            || self.hidden_eval_abstract_type_ids.iter().any(|type_id| {
                self.abstract_types
                    .get(*type_id)
                    .is_some_and(|definition| definition.name == type_name)
            })
            || self
                .pending_eval_primitive_types
                .iter()
                .any(|(_, definition)| definition.name == type_name)
            || self.hidden_eval_primitive_type_ids.iter().any(|type_id| {
                self.compile_context
                    .as_ref()
                    .and_then(|context| context.primitive_types.get(*type_id))
                    .is_some_and(|definition| definition.name == type_name)
            })
            || self
                .pending_eval_enum_defs
                .iter()
                .any(|(_, definition)| definition.name == type_name)
            || self.hidden_eval_enum_type_ids.iter().any(|type_id| {
                self.enum_defs
                    .get(*type_id)
                    .is_some_and(|definition| definition.name == type_name)
            })
    }

    pub(crate) fn eval_nominal_type_name_is_unpublished(&self, type_name: &str) -> bool {
        if !self.eval_struct_type_name_is_pending(type_name) {
            return false;
        }
        let concrete_is_active =
            self.struct_defs
                .iter()
                .enumerate()
                .any(|(type_id, definition)| {
                    definition.name == type_name
                        && !self.hidden_eval_struct_type_ids.contains(&type_id)
                });
        let primitive_is_active = self.compile_context.as_ref().is_some_and(|context| {
            context
                .primitive_types
                .iter()
                .enumerate()
                .any(|(type_id, definition)| {
                    definition.name == type_name
                        && !self.hidden_eval_primitive_type_ids.contains(&type_id)
                })
        });
        !(concrete_is_active
            || self.abstract_type_name_index.contains_key(type_name)
            || primitive_is_active
            || self.active_enum_name_index.contains_key(type_name))
    }

    pub(crate) fn pending_eval_struct_name(&self, type_id: usize) -> Option<String> {
        self.pending_eval_struct_defs
            .iter()
            .find_map(|(pending_type_id, definition)| {
                (*pending_type_id == type_id
                    && !self
                        .published_eval_nominal_type_names
                        .contains(&definition.name))
                .then(|| definition.name.clone())
            })
            .or_else(|| {
                self.hidden_eval_struct_type_ids
                    .contains(&type_id)
                    .then(|| {
                        self.struct_defs
                            .get(type_id)
                            .map(|definition| definition.name.clone())
                    })
                    .flatten()
            })
    }

    fn ensure_eval_nominal_parent_is_published(&self, parent: Option<&str>) -> Result<(), VmError> {
        let Some(parent) = parent else {
            return Ok(());
        };
        if !self.eval_nominal_type_name_is_unpublished(parent) {
            return Ok(());
        }

        let local_name = parent.rsplit('.').next().unwrap_or(parent).to_string();
        Err(VmError::UndefVarError(local_name))
    }

    fn runtime_nominal_name_is_defined(&self, name: &str) -> bool {
        self.get_global(name).is_some()
            || self.published_eval_nominal_type_names.contains(name)
            || self
                .struct_defs
                .iter()
                .enumerate()
                .any(|(type_id, definition)| {
                    definition.name == name && !self.hidden_eval_struct_type_ids.contains(&type_id)
                })
            || self
                .abstract_types
                .iter()
                .enumerate()
                .any(|(type_id, definition)| {
                    definition.name == name
                        && !self.hidden_eval_abstract_type_ids.contains(&type_id)
                })
            || self.compile_context.as_ref().is_some_and(|context| {
                context.parametric_structs.contains_key(name)
                    || context
                        .primitive_types
                        .iter()
                        .enumerate()
                        .any(|(type_id, definition)| {
                            definition.name == name
                                && !self.hidden_eval_primitive_type_ids.contains(&type_id)
                        })
                    || context.type_aliases.contains_key(name)
            })
            || self.active_enum_name_index.contains_key(name)
            || self.function_name_index.contains_key(name)
    }

    fn ensure_runtime_nominal_name_is_available(&self, name: &str) -> Result<(), VmError> {
        if self.runtime_nominal_name_is_defined(name) {
            return Err(VmError::TypeError(format!(
                "invalid redefinition of constant {name}"
            )));
        }
        Ok(())
    }

    fn runtime_nominal_parent_is_defined(&self, parent: &str) -> bool {
        let base = runtime_nominal_base_name(parent);
        if self.eval_nominal_type_name_is_unpublished(parent)
            || self.eval_nominal_type_name_is_unpublished(base)
        {
            return false;
        }
        JuliaType::from_name(parent).is_some()
            || JuliaType::from_name(base).is_some()
            || self.struct_hierarchy.contains_name(parent)
            || self.struct_hierarchy.contains_name(base)
            || self.published_eval_nominal_type_names.contains(parent)
            || self.published_eval_nominal_type_names.contains(base)
            || self
                .struct_defs
                .iter()
                .enumerate()
                .any(|(type_id, definition)| {
                    (definition.name == parent || definition.name == base)
                        && !self.hidden_eval_struct_type_ids.contains(&type_id)
                })
            || self
                .abstract_types
                .iter()
                .enumerate()
                .any(|(type_id, definition)| {
                    (definition.name == parent || definition.name == base)
                        && !self.hidden_eval_abstract_type_ids.contains(&type_id)
                })
            || self.compile_context.as_ref().is_some_and(|context| {
                context.parametric_structs.contains_key(parent)
                    || context.parametric_structs.contains_key(base)
                    || context
                        .primitive_types
                        .iter()
                        .enumerate()
                        .any(|(type_id, definition)| {
                            (definition.name == parent || definition.name == base)
                                && !self.hidden_eval_primitive_type_ids.contains(&type_id)
                        })
                    || context.type_aliases.contains_key(parent)
                    || context.type_aliases.contains_key(base)
            })
            || self.active_enum_name_index.contains_key(parent)
            || self.active_enum_name_index.contains_key(base)
    }

    fn runtime_nominal_parent_is_abstract(&self, parent: &str) -> bool {
        fn is_abstract<R: RngLike>(
            vm: &Vm<R>,
            parent: &str,
            visited: &mut HashSet<String>,
        ) -> bool {
            let base = runtime_nominal_base_name(parent);
            if !visited.insert(base.to_string()) {
                return false;
            }
            if CoreType::is_builtin_abstract_datatype_for_julia_name(parent)
                || CoreType::is_builtin_abstract_datatype_for_julia_name(base)
                || vm.abstract_type_name_index.contains_key(parent)
                || vm.abstract_type_name_index.contains_key(base)
            {
                return true;
            }
            vm.compile_context
                .as_ref()
                .and_then(|context| {
                    context
                        .type_aliases
                        .get(parent)
                        .or_else(|| context.type_aliases.get(base))
                })
                .is_some_and(|target| is_abstract(vm, target, visited))
        }

        is_abstract(self, parent, &mut HashSet::new())
    }

    fn ensure_runtime_nominal_parent_is_defined(
        &self,
        parent: Option<&str>,
    ) -> Result<(), VmError> {
        let Some(parent) = parent else {
            return Ok(());
        };
        if self.runtime_nominal_parent_is_defined(parent) {
            return self
                .runtime_nominal_parent_is_abstract(parent)
                .then_some(())
                .ok_or_else(|| {
                    VmError::ErrorException(format!(
                        "invalid subtyping in definition: expected {parent} to be an abstract type"
                    ))
                });
        }
        if self.get_global(parent).is_some() {
            return Err(VmError::ErrorException(format!(
                "invalid subtyping in definition: expected {parent} to be a type"
            )));
        }
        let local_name = parent.rsplit('.').next().unwrap_or(parent).to_string();
        Err(VmError::UndefVarError(local_name))
    }

    fn ensure_runtime_nominal_field_type_is_defined(
        &self,
        type_expr: &crate::types::TypeExpr,
        type_params: &HashSet<&str>,
    ) -> Result<(), VmError> {
        match type_expr {
            crate::types::TypeExpr::Concrete(julia_type) => {
                self.ensure_runtime_nominal_julia_type_is_defined(julia_type)
            }
            crate::types::TypeExpr::TypeVar(name) => {
                if type_params.contains(name.as_str())
                    || matches!(name.as_str(), "true" | "false")
                    || name.starts_with(':')
                    || name.chars().all(|ch| ch.is_ascii_digit())
                {
                    return Ok(());
                }
                self.ensure_runtime_nominal_type_name_is_defined(name)
            }
            crate::types::TypeExpr::Parameterized { base, params } => {
                self.ensure_runtime_nominal_type_name_is_defined(base)?;
                for param in params {
                    self.ensure_runtime_nominal_field_type_is_defined(param, type_params)?;
                }
                Ok(())
            }
            crate::types::TypeExpr::RuntimeExpr(source) => {
                let source = source.trim();
                if source.split('.').all(|part| {
                    !part.is_empty()
                        && part
                            .chars()
                            .next()
                            .is_some_and(|ch| ch.is_alphabetic() || ch == '_')
                        && part.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
                }) {
                    self.ensure_runtime_nominal_type_name_is_defined(source)
                } else {
                    Err(VmError::NotImplemented(
                        "runtime-computed field type annotations in top-level control-flow struct declarations (Issue #11697)"
                            .to_string(),
                    ))
                }
            }
        }
    }

    fn ensure_runtime_nominal_type_parameter_bounds_are_defined(
        &self,
        type_params: &[crate::types::TypeParam],
    ) -> Result<(), VmError> {
        let mut preceding_parameter_names = HashSet::new();
        for parameter in type_params {
            for bound in parameter
                .lower_bound
                .iter()
                .chain(parameter.upper_bound.iter())
            {
                let Some(type_expr) = crate::types::parse_single_type_expr(bound) else {
                    return Err(VmError::TypeError(format!(
                        "invalid type parameter bound {bound}"
                    )));
                };
                if matches!(
                    &type_expr,
                    crate::types::TypeExpr::TypeVar(name)
                        if matches!(name.as_str(), "true" | "false")
                            || name.starts_with(':')
                            || name.chars().all(|ch| ch.is_ascii_digit())
                ) {
                    return Err(VmError::TypeError(format!(
                        "invalid type parameter bound {bound}"
                    )));
                }
                self.ensure_runtime_nominal_field_type_is_defined(
                    &type_expr,
                    &preceding_parameter_names,
                )?;
            }
            preceding_parameter_names.insert(parameter.name.as_str());
        }
        Ok(())
    }

    fn ensure_runtime_nominal_julia_type_is_defined(
        &self,
        julia_type: &JuliaType,
    ) -> Result<(), VmError> {
        match julia_type {
            JuliaType::Struct(name) | JuliaType::Enum(name) => {
                self.ensure_runtime_nominal_type_name_is_defined(name)
            }
            JuliaType::AbstractUser(name, parent) => {
                self.ensure_runtime_nominal_type_name_is_defined(name)?;
                if let Some(parent) = parent {
                    self.ensure_runtime_nominal_type_name_is_defined(parent)?;
                }
                Ok(())
            }
            JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
                self.ensure_runtime_nominal_julia_type_is_defined(inner)
            }
            JuliaType::TupleOf(types) | JuliaType::Union(types) => {
                for julia_type in types {
                    self.ensure_runtime_nominal_julia_type_is_defined(julia_type)?;
                }
                Ok(())
            }
            JuliaType::RuntimeParametric { base, params } => {
                self.ensure_runtime_nominal_type_name_is_defined(base)?;
                for param in params {
                    self.ensure_runtime_nominal_julia_type_is_defined(param)?;
                }
                Ok(())
            }
            JuliaType::UnionAll { body, .. } | JuliaType::RuntimeUnionAll { body, .. } => {
                self.ensure_runtime_nominal_julia_type_is_defined(body)
            }
            _ => Ok(()),
        }
    }

    fn ensure_runtime_nominal_type_name_is_defined(&self, name: &str) -> Result<(), VmError> {
        if self.runtime_nominal_parent_is_defined(name) {
            return Ok(());
        }
        if self.get_global(name).is_some() {
            return Err(VmError::TypeError(format!("expected {name} to be a type")));
        }
        Err(VmError::UndefVarError(
            name.rsplit('.').next().unwrap_or(name).to_string(),
        ))
    }

    fn record_runtime_nominal_activation(
        &mut self,
        operands: &DefineRuntimeNominalOperands,
        registry_id: usize,
        coalesced_root: bool,
        published_members: Option<Vec<String>>,
    ) {
        self.repl_definition_activations
            .push(ReplDefinitionActivation::RuntimeNominal(
                RuntimeNominalActivation {
                    site_id: operands.site_id,
                    span: operands.span,
                    registry_id,
                    definition: operands.definition.clone(),
                    coalesced_root,
                    published_members,
                },
            ));
    }

    pub(crate) fn install_runtime_struct_definition(
        &mut self,
        definition: StructDefInfo,
    ) -> Result<usize, VmError> {
        self.ensure_runtime_nominal_name_is_available(&definition.name)?;
        self.ensure_runtime_nominal_parent_is_defined(definition.parent_type.as_deref())?;
        let type_id = self.struct_defs.len();
        self.struct_def_name_index
            .insert(definition.name.clone(), type_id);
        self.struct_hierarchy.insert_if_absent(
            &definition.name,
            definition.parent_type.clone(),
            Vec::new(),
        );
        let name = definition.name.clone();
        self.struct_defs.push(definition);
        self.published_eval_nominal_type_names.insert(name.clone());
        append_type_ancestors(
            &mut self.type_ancestors,
            std::slice::from_ref(&name),
            &self.abstract_types,
            &self.abstract_type_name_index,
            &self.struct_hierarchy,
        );
        self.note_method_table_mutation();
        Ok(type_id)
    }

    /// Allocate and publish one reached runtime-conditional nominal definition.
    /// Every family validates all failure-prone prerequisites before mutating
    /// registries. Enum member stores intentionally follow type publication so
    /// a caught collision retains Julia's exact successfully-published prefix.
    pub(crate) fn define_runtime_nominal(
        &mut self,
        operands: &DefineRuntimeNominalOperands,
    ) -> Result<(), VmError> {
        if let Some(existing) = self
            .repl_definition_activations
            .iter()
            .find_map(|activation| match activation {
                ReplDefinitionActivation::RuntimeNominal(activation)
                    if activation.site_id == operands.site_id =>
                {
                    Some(activation)
                }
                _ => None,
            })
        {
            return if existing.definition == operands.definition {
                Ok(())
            } else {
                Err(VmError::InternalError(format!(
                    "runtime nominal site {} changed definition while executing",
                    operands.site_id
                )))
            };
        }

        let name = match &operands.definition {
            RuntimeNominalDefInfo::Struct(definition) => definition.layout.name.as_str(),
            RuntimeNominalDefInfo::AbstractType(definition) => definition.name.as_str(),
            RuntimeNominalDefInfo::PrimitiveType(definition) => definition.name.as_str(),
            RuntimeNominalDefInfo::Enum(definition) => definition.name.as_str(),
        };
        if operands.reserved_struct_type_id.is_some() {
            let RuntimeNominalDefInfo::Struct(definition) = &operands.definition else {
                return Err(VmError::InternalError(
                    "reserved concrete type attached to non-struct runtime declaration".to_string(),
                ));
            };
            self.validate_runtime_struct_definition(definition)?;
            self.publish_reserved_runtime_struct(operands, definition)?;
            self.activate_runtime_nominal_constructors(operands);
            return Ok(());
        }
        if operands.coalesce_with_root {
            self.coalesce_runtime_nominal_with_root(operands)?;
            self.activate_runtime_nominal_constructors(operands);
            return Ok(());
        }
        self.ensure_runtime_nominal_name_is_available(name)?;

        match &operands.definition {
            RuntimeNominalDefInfo::Struct(definition) => {
                self.validate_runtime_struct_definition(definition)?;
                if !definition.source.inner_constructors.is_empty() {
                    return Err(VmError::InternalError(
                        "runtime inner constructor has no reserved concrete type".to_string(),
                    ));
                }
                let type_id = self.install_runtime_struct_definition(definition.layout.clone())?;
                self.record_runtime_nominal_activation(operands, type_id, false, None);
            }
            RuntimeNominalDefInfo::AbstractType(definition) => {
                self.ensure_runtime_nominal_type_parameter_bounds_are_defined(
                    &definition.type_params,
                )?;
                self.ensure_runtime_nominal_parent_is_defined(definition.parent.as_deref())?;
                let type_id = self.abstract_types.len();
                self.abstract_type_name_index
                    .insert(definition.name.clone(), type_id);
                self.struct_hierarchy.insert_if_absent(
                    &definition.name,
                    definition.parent.clone(),
                    definition
                        .type_params
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect(),
                );
                self.abstract_types.push(definition.clone());
                self.published_eval_nominal_type_names
                    .insert(definition.name.clone());
                append_type_ancestors(
                    &mut self.type_ancestors,
                    std::slice::from_ref(&definition.name),
                    &self.abstract_types,
                    &self.abstract_type_name_index,
                    &self.struct_hierarchy,
                );
                self.note_method_table_mutation();
                self.record_runtime_nominal_activation(operands, type_id, false, None);
            }
            RuntimeNominalDefInfo::PrimitiveType(definition) => {
                if definition.bits == 0 || definition.bits % 8 != 0 {
                    return Err(VmError::ErrorException(format!(
                        "invalid number of bits in primitive type definition; expected a positive multiple of 8, got {}",
                        definition.bits
                    )));
                }
                self.ensure_runtime_nominal_parent_is_defined(definition.parent.as_deref())?;
                let type_id = self
                    .compile_context
                    .as_ref()
                    .map(|context| context.primitive_types.len())
                    .ok_or_else(|| {
                        VmError::InternalError(
                            "runtime primitive definition requires a compile context".to_string(),
                        )
                    })?;
                self.compile_context
                    .as_mut()
                    .ok_or_else(|| {
                        VmError::InternalError(
                            "runtime primitive definition lost its compile context".to_string(),
                        )
                    })?
                    .primitive_types
                    .push(definition.clone());
                self.struct_hierarchy.insert_if_absent(
                    &definition.name,
                    definition.parent.clone(),
                    Vec::new(),
                );
                self.published_eval_nominal_type_names
                    .insert(definition.name.clone());
                append_type_ancestors(
                    &mut self.type_ancestors,
                    std::slice::from_ref(&definition.name),
                    &self.abstract_types,
                    &self.abstract_type_name_index,
                    &self.struct_hierarchy,
                );
                self.note_method_table_mutation();
                self.record_runtime_nominal_activation(operands, type_id, false, None);
            }
            RuntimeNominalDefInfo::Enum(definition) => {
                let enum_id = self.enum_defs.len();
                let parent = Some(format!("Enum{{{}}}", definition.base_type));
                self.active_enum_name_index
                    .insert(definition.name.clone(), enum_id);
                self.struct_hierarchy
                    .insert_if_absent(&definition.name, parent, Vec::new());
                crate::vm::value::enum_registry::register_enum(
                    &definition.name,
                    &definition.members,
                );
                self.enum_defs.push(definition.clone());
                self.published_eval_nominal_type_names
                    .insert(definition.name.clone());
                append_type_ancestors(
                    &mut self.type_ancestors,
                    std::slice::from_ref(&definition.name),
                    &self.abstract_types,
                    &self.abstract_type_name_index,
                    &self.struct_hierarchy,
                );
                self.note_method_table_mutation();
                self.record_runtime_nominal_activation(operands, enum_id, false, Some(Vec::new()));
                self.publish_runtime_nominal_enum_members(operands, definition)?;
            }
        }
        Ok(())
    }

    fn validate_runtime_struct_definition(
        &self,
        definition: &crate::bytecode::RuntimeStructDefInfo,
    ) -> Result<(), VmError> {
        if !definition.source.type_params.is_empty() {
            return Err(VmError::NotImplemented(
                "parametric struct declarations in top-level control flow".to_string(),
            ));
        }
        self.ensure_runtime_nominal_parent_is_defined(definition.layout.parent_type.as_deref())?;
        self.ensure_runtime_nominal_type_parameter_bounds_are_defined(
            &definition.source.type_params,
        )?;
        let type_params: HashSet<&str> = definition
            .source
            .type_params
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect();
        for field in &definition.source.fields {
            if let Some(type_expr) = &field.type_expr {
                self.ensure_runtime_nominal_field_type_is_defined(type_expr, &type_params)?;
            }
        }
        Ok(())
    }

    fn publish_reserved_runtime_struct(
        &mut self,
        operands: &DefineRuntimeNominalOperands,
        definition: &crate::bytecode::RuntimeStructDefInfo,
    ) -> Result<(), VmError> {
        let type_id = operands.reserved_struct_type_id.ok_or_else(|| {
            VmError::InternalError("runtime struct reservation lost its type id".to_string())
        })?;
        if self.struct_defs.get(type_id) == Some(&definition.layout) {
            if !self.hidden_eval_struct_type_ids.remove(&type_id) {
                return Err(VmError::InternalError(format!(
                    "runtime struct reservation {type_id} was already visible"
                )));
            }
        } else {
            let Some((pending_type_id, pending_definition)) =
                self.pending_eval_struct_defs.pop_front()
            else {
                return Err(VmError::InternalError(format!(
                    "runtime struct reservation {type_id} disappeared"
                )));
            };
            if pending_type_id != type_id
                || type_id != self.struct_defs.len()
                || pending_definition != definition.layout
            {
                self.pending_eval_struct_defs
                    .push_front((pending_type_id, pending_definition));
                return Err(VmError::InternalError(format!(
                    "out-of-order runtime struct reservation: marker={type_id}, active={}",
                    self.struct_defs.len()
                )));
            }
            self.struct_defs.push(pending_definition);
        }
        self.struct_def_name_index
            .insert(definition.layout.name.clone(), type_id);
        self.struct_hierarchy.insert_if_absent(
            &definition.layout.name,
            definition.layout.parent_type.clone(),
            Vec::new(),
        );
        self.published_eval_nominal_type_names
            .insert(definition.layout.name.clone());
        append_type_ancestors(
            &mut self.type_ancestors,
            std::slice::from_ref(&definition.layout.name),
            &self.abstract_types,
            &self.abstract_type_name_index,
            &self.struct_hierarchy,
        );
        self.note_method_table_mutation();
        self.record_runtime_nominal_activation(operands, type_id, false, None);
        Ok(())
    }

    fn activate_runtime_nominal_constructors(&mut self, operands: &DefineRuntimeNominalOperands) {
        for &index in &operands.constructor_function_indices {
            self.current_world = self.current_world.saturating_add(1);
            let function_name = self
                .functions
                .get(index)
                .map(|function| function.name.clone());
            if let Some(function) = self.functions.get_mut(index) {
                std::rc::Rc::make_mut(function).min_world = self.current_world;
            }
            if let Some(function_name) = function_name {
                self.note_method_table_mutation_for(&function_name);
            } else {
                self.note_method_table_mutation();
            }
        }
    }

    fn coalesce_runtime_nominal_with_root(
        &mut self,
        operands: &DefineRuntimeNominalOperands,
    ) -> Result<(), VmError> {
        let registry_id = match &operands.definition {
            RuntimeNominalDefInfo::Struct(definition) => {
                let Some(type_id) = self
                    .struct_defs
                    .iter()
                    .rposition(|root| root == &definition.layout)
                else {
                    return Err(VmError::InternalError(
                        "compatible runtime/root struct target disappeared".to_string(),
                    ));
                };
                self.struct_def_name_index
                    .insert(definition.layout.name.clone(), type_id);
                self.hidden_eval_struct_type_ids.remove(&type_id);
                self.struct_hierarchy.insert_if_absent(
                    &definition.layout.name,
                    definition.layout.parent_type.clone(),
                    Vec::new(),
                );
                self.published_eval_nominal_type_names
                    .insert(definition.layout.name.clone());
                type_id
            }
            RuntimeNominalDefInfo::AbstractType(definition) => {
                let Some(type_id) = self
                    .abstract_types
                    .iter()
                    .rposition(|root| root == definition)
                else {
                    return Err(VmError::InternalError(
                        "compatible runtime/root abstract target disappeared".to_string(),
                    ));
                };
                self.abstract_type_name_index
                    .insert(definition.name.clone(), type_id);
                self.hidden_eval_abstract_type_ids.remove(&type_id);
                self.struct_hierarchy.insert_if_absent(
                    &definition.name,
                    definition.parent.clone(),
                    definition
                        .type_params
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect(),
                );
                self.published_eval_nominal_type_names
                    .insert(definition.name.clone());
                type_id
            }
            RuntimeNominalDefInfo::PrimitiveType(definition) => {
                let Some(type_id) = self.compile_context.as_ref().and_then(|context| {
                    context
                        .primitive_types
                        .iter()
                        .rposition(|root| root == definition)
                }) else {
                    return Err(VmError::InternalError(
                        "compatible runtime/root primitive target disappeared".to_string(),
                    ));
                };
                self.struct_hierarchy.insert_if_absent(
                    &definition.name,
                    definition.parent.clone(),
                    Vec::new(),
                );
                self.published_eval_nominal_type_names
                    .insert(definition.name.clone());
                type_id
            }
            RuntimeNominalDefInfo::Enum(definition) => {
                let Some(enum_id) = self.enum_defs.iter().rposition(|root| root == definition)
                else {
                    return Err(VmError::InternalError(
                        "compatible runtime/root enum target disappeared".to_string(),
                    ));
                };
                self.active_enum_name_index
                    .insert(definition.name.clone(), enum_id);
                self.hidden_eval_enum_type_ids.remove(&enum_id);
                self.struct_hierarchy.insert_if_absent(
                    &definition.name,
                    Some(format!("Enum{{{}}}", definition.base_type)),
                    Vec::new(),
                );
                crate::vm::value::enum_registry::register_enum(
                    &definition.name,
                    &definition.members,
                );
                self.published_eval_nominal_type_names
                    .insert(definition.name.clone());
                enum_id
            }
        };
        self.note_method_table_mutation();
        let published_members =
            matches!(&operands.definition, RuntimeNominalDefInfo::Enum(_)).then(Vec::new);
        self.record_runtime_nominal_activation(operands, registry_id, true, published_members);
        if let RuntimeNominalDefInfo::Enum(definition) = &operands.definition {
            self.publish_runtime_nominal_enum_members(operands, definition)?;
        }
        Ok(())
    }

    fn publish_runtime_nominal_enum_members(
        &mut self,
        operands: &DefineRuntimeNominalOperands,
        definition: &EnumDefInfo,
    ) -> Result<(), VmError> {
        for member_index in crate::bytecode::julia_enum_member_binding_order(&definition.members) {
            let (member_name, value) = &definition.members[member_index];
            if operands
                .published_members
                .as_ref()
                .is_some_and(|published| !published.contains(member_name))
            {
                continue;
            }
            let matching_root_member = operands.coalesce_with_root
                && self.get_global(member_name).is_some_and(|existing| {
                    matches!(
                        existing,
                        Value::Enum {
                            type_name: existing_type,
                            value: existing_value,
                        } if existing_type == definition.name && existing_value == *value
                    )
                });
            if self.runtime_nominal_name_is_defined(member_name) && !matching_root_member {
                return Err(VmError::ErrorException(format!(
                    "cannot declare Main.{member_name} constant; it was already declared global"
                )));
            }
            if !matching_root_member {
                self.store_global_value(
                    member_name,
                    Value::Enum {
                        type_name: definition.name.clone(),
                        value: *value,
                    },
                );
            }
            if let Some(ReplDefinitionActivation::RuntimeNominal(activation)) =
                self.repl_definition_activations.last_mut()
            {
                activation
                    .published_members
                    .get_or_insert_with(Vec::new)
                    .push(member_name.clone());
            }
        }
        Ok(())
    }

    pub(crate) fn eval_enum_type_name_is_unpublished(&self, type_name: &str) -> bool {
        self.eval_nominal_type_name_is_unpublished(type_name)
            && self
                .pending_eval_enum_defs
                .iter()
                .any(|(_, definition)| definition.name == type_name)
    }

    pub(crate) fn runtime_nominal_enum_type_name_is_unpublished(&self, type_name: &str) -> bool {
        !self.active_enum_name_index.contains_key(type_name)
            && self.code.iter().any(|instruction| {
                matches!(
                    instruction,
                    Instr::DefineRuntimeNominal(operands)
                        if matches!(
                            &operands.definition,
                            RuntimeNominalDefInfo::Enum(definition)
                                if definition.name == type_name
                        )
                )
            })
    }

    fn begin_eval_enum_member_bindings(&mut self, operands: &RegisterEnumOperands) {
        self.pending_eval_enum_member_bindings.clear();
        self.pending_eval_enum_member_bindings.extend(
            crate::bytecode::julia_enum_member_binding_order(&operands.members)
                .into_iter()
                .filter_map(|member_index| {
                    let (member_name, value) = &operands.members[member_index];
                    operands
                        .published_members
                        .as_ref()
                        .is_none_or(|published| published.contains(member_name))
                        .then(|| (operands.type_name.clone(), member_name.clone(), *value))
                }),
        );
    }

    pub(crate) fn validate_eval_enum_member_push(
        &mut self,
        type_name: &str,
        value: i64,
    ) -> Result<(), VmError> {
        let Some((expected_type, member_name, expected_value)) =
            self.pending_eval_enum_member_bindings.front()
        else {
            return Ok(());
        };
        if expected_type != type_name || *expected_value != value {
            return Ok(());
        }
        let member_name = member_name.clone();
        self.pending_eval_enum_member_bindings.pop_front();
        if self.get_global(&member_name).is_some_and(|existing| {
            !matches!(
                existing,
                Value::Enum {
                    type_name: existing_type,
                    value: existing_value,
                } if existing_type == type_name && existing_value == value
            )
        }) {
            return Err(VmError::ErrorException(format!(
                "cannot declare Main.{member_name} constant; it was already declared global"
            )));
        }
        Ok(())
    }

    /// Publish the next reserved concrete definition at its source marker.
    pub(crate) fn activate_eval_struct(&mut self, type_id: usize) -> Result<(), VmError> {
        if !self.hidden_eval_struct_type_ids.contains(&type_id)
            && self
                .pending_eval_struct_defs
                .front()
                .is_some_and(|(pending_type_id, definition)| {
                    *pending_type_id == type_id
                        && self.struct_defs.get(type_id) == Some(definition)
                        && self
                            .published_eval_nominal_type_names
                            .contains(&definition.name)
                })
        {
            self.pending_eval_struct_defs.pop_front();
            self.repl_definition_activations
                .push(ReplDefinitionActivation::Struct(type_id));
            return Ok(());
        }
        if self.hidden_eval_struct_type_ids.contains(&type_id) {
            // A runtime-nominal program keeps its root suffix both hidden and
            // pending so reached-prefix validation can count activations. The
            // older fail-closed non-contiguous-marker path keeps the definition
            // in `struct_defs` and hides only the referenced marker IDs, with no
            // pending queue entry. Preserve both representations (Issue #11654).
            let pending_definition = match self.pending_eval_struct_defs.front() {
                Some((pending_type_id, _)) if *pending_type_id == type_id => {
                    self.pending_eval_struct_defs.pop_front()
                }
                Some((pending_type_id, _)) => {
                    return Err(VmError::InternalError(format!(
                        "out-of-order reserved struct activation: marker={type_id}, pending={pending_type_id}"
                    )));
                }
                None => None,
            };
            let (name, parent) = self
                .struct_defs
                .get(type_id)
                .map(|definition| (definition.name.clone(), definition.parent_type.clone()))
                .ok_or_else(|| {
                    VmError::InternalError(format!(
                        "DefineEvalStruct({type_id}) references no reserved definition"
                    ))
                })?;
            if let Err(error) = self.ensure_eval_nominal_parent_is_published(parent.as_deref()) {
                if let Some(pending_definition) = pending_definition {
                    self.pending_eval_struct_defs.push_front(pending_definition);
                }
                return Err(error);
            }
            self.hidden_eval_struct_type_ids.remove(&type_id);
            self.published_eval_nominal_type_names.insert(name);
            self.repl_definition_activations
                .push(ReplDefinitionActivation::Struct(type_id));
            return Ok(());
        }
        let Some((pending_type_id, def)) = self.pending_eval_struct_defs.pop_front() else {
            return Err(VmError::InternalError(format!(
                "DefineEvalStruct({type_id}) has no pending definition"
            )));
        };
        if pending_type_id != type_id || type_id != self.struct_defs.len() {
            self.pending_eval_struct_defs
                .push_front((pending_type_id, def));
            return Err(VmError::InternalError(format!(
                "out-of-order struct activation: marker={type_id}, pending={pending_type_id}, active={}",
                self.struct_defs.len()
            )));
        }
        if let Err(error) = self.ensure_eval_nominal_parent_is_published(def.parent_type.as_deref())
        {
            self.pending_eval_struct_defs
                .push_front((pending_type_id, def));
            return Err(error);
        }

        self.struct_def_name_index.insert(def.name.clone(), type_id);
        self.struct_hierarchy
            .insert_if_absent(&def.name, def.parent_type.clone(), Vec::new());
        let name = def.name.clone();
        self.struct_defs.push(def);
        self.published_eval_nominal_type_names.insert(name.clone());
        append_type_ancestors(
            &mut self.type_ancestors,
            &[name],
            &self.abstract_types,
            &self.abstract_type_name_index,
            &self.struct_hierarchy,
        );
        self.note_method_table_mutation();
        self.repl_definition_activations
            .push(ReplDefinitionActivation::Struct(type_id));
        Ok(())
    }

    pub(crate) fn activate_eval_abstract_type(&mut self, type_id: usize) -> Result<(), VmError> {
        let Some((pending_type_id, definition)) = self.pending_eval_abstract_types.pop_front()
        else {
            return Err(VmError::InternalError(format!(
                "DefineEvalAbstractType({type_id}) has no pending definition"
            )));
        };
        let is_hidden = self.hidden_eval_abstract_type_ids.contains(&type_id);
        if !is_hidden
            && self.abstract_types.get(type_id) == Some(&definition)
            && self
                .published_eval_nominal_type_names
                .contains(&definition.name)
        {
            self.repl_definition_activations
                .push(ReplDefinitionActivation::AbstractType(type_id));
            return Ok(());
        }
        if pending_type_id != type_id || (!is_hidden && type_id != self.abstract_types.len()) {
            self.pending_eval_abstract_types
                .push_front((pending_type_id, definition));
            return Err(VmError::InternalError(format!(
                "out-of-order abstract type activation: marker={type_id}, pending={pending_type_id}, active={}",
                self.abstract_types.len()
            )));
        }
        if let Err(error) =
            self.ensure_eval_nominal_parent_is_published(definition.parent.as_deref())
        {
            self.pending_eval_abstract_types
                .push_front((pending_type_id, definition));
            return Err(error);
        }

        let name = definition.name.clone();
        if is_hidden {
            let Some(reserved) = self.abstract_types.get(type_id) else {
                return Err(VmError::InternalError(format!(
                    "DefineEvalAbstractType({type_id}) references no reserved definition"
                )));
            };
            if reserved != &definition {
                return Err(VmError::InternalError(format!(
                    "DefineEvalAbstractType({type_id}) reserved definition mismatch"
                )));
            }
            self.hidden_eval_abstract_type_ids.remove(&type_id);
        }
        self.abstract_type_name_index.insert(name.clone(), type_id);
        self.struct_hierarchy.insert_if_absent(
            &name,
            definition.parent.clone(),
            definition
                .type_params
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
        );
        if !is_hidden {
            self.abstract_types.push(definition);
        }
        self.published_eval_nominal_type_names.insert(name.clone());
        append_type_ancestors(
            &mut self.type_ancestors,
            &[name],
            &self.abstract_types,
            &self.abstract_type_name_index,
            &self.struct_hierarchy,
        );
        self.note_method_table_mutation();
        self.repl_definition_activations
            .push(ReplDefinitionActivation::AbstractType(type_id));
        Ok(())
    }

    pub(crate) fn activate_eval_primitive_type(&mut self, type_id: usize) -> Result<(), VmError> {
        let Some((pending_type_id, definition)) = self.pending_eval_primitive_types.pop_front()
        else {
            return Err(VmError::InternalError(format!(
                "DefineEvalPrimitiveType({type_id}) has no pending definition"
            )));
        };
        let active_len = self
            .compile_context
            .as_ref()
            .map_or(0, |context| context.primitive_types.len());
        let is_hidden = self.hidden_eval_primitive_type_ids.contains(&type_id);
        if !is_hidden
            && self
                .compile_context
                .as_ref()
                .is_some_and(|context| context.primitive_types.get(type_id) == Some(&definition))
            && self
                .published_eval_nominal_type_names
                .contains(&definition.name)
        {
            self.repl_definition_activations
                .push(ReplDefinitionActivation::PrimitiveType(type_id));
            return Ok(());
        }
        if pending_type_id != type_id || (!is_hidden && type_id != active_len) {
            self.pending_eval_primitive_types
                .push_front((pending_type_id, definition));
            return Err(VmError::InternalError(format!(
                "out-of-order primitive type activation: marker={type_id}, pending={pending_type_id}, active={active_len}"
            )));
        }
        if let Err(error) =
            self.ensure_eval_nominal_parent_is_published(definition.parent.as_deref())
        {
            self.pending_eval_primitive_types
                .push_front((pending_type_id, definition));
            return Err(error);
        }

        let name = definition.name.clone();
        self.struct_hierarchy
            .insert_if_absent(&name, definition.parent.clone(), Vec::new());
        let Some(context) = self.compile_context.as_mut() else {
            self.pending_eval_primitive_types
                .push_front((pending_type_id, definition));
            return Err(VmError::InternalError(
                "primitive activation requires a runtime compile context".to_string(),
            ));
        };
        if is_hidden {
            if context.primitive_types.get(type_id) != Some(&definition) {
                return Err(VmError::InternalError(format!(
                    "DefineEvalPrimitiveType({type_id}) reserved definition mismatch"
                )));
            }
            self.hidden_eval_primitive_type_ids.remove(&type_id);
        } else {
            context.primitive_types.push(definition);
        }
        self.published_eval_nominal_type_names.insert(name.clone());
        append_type_ancestors(
            &mut self.type_ancestors,
            &[name],
            &self.abstract_types,
            &self.abstract_type_name_index,
            &self.struct_hierarchy,
        );
        self.note_method_table_mutation();
        self.repl_definition_activations
            .push(ReplDefinitionActivation::PrimitiveType(type_id));
        Ok(())
    }

    pub(crate) fn activate_eval_enum(
        &mut self,
        operands: &RegisterEnumOperands,
    ) -> Result<(), VmError> {
        let Some((enum_id, definition)) = self.pending_eval_enum_defs.pop_front() else {
            crate::vm::value::enum_registry::register_enum(&operands.type_name, &operands.members);
            self.begin_eval_enum_member_bindings(operands);
            return Ok(());
        };
        let is_hidden = self.hidden_eval_enum_type_ids.contains(&enum_id);
        if !is_hidden
            && self.enum_defs.get(enum_id) == Some(&definition)
            && self
                .published_eval_nominal_type_names
                .contains(&definition.name)
        {
            self.repl_definition_activations
                .push(ReplDefinitionActivation::Enum(enum_id));
            self.begin_eval_enum_member_bindings(operands);
            return Ok(());
        }
        if (!is_hidden && enum_id != self.enum_defs.len())
            || definition.name != operands.type_name
            || definition.members != operands.members
        {
            self.pending_eval_enum_defs
                .push_front((enum_id, definition));
            return Err(VmError::InternalError(format!(
                "out-of-order enum activation: marker={}, pending={}, active={}",
                operands.type_name,
                self.pending_eval_enum_defs
                    .front()
                    .map_or("<none>", |(_, definition)| definition.name.as_str()),
                self.enum_defs.len()
            )));
        }

        let name = definition.name.clone();
        if is_hidden {
            if self.enum_defs.get(enum_id) != Some(&definition) {
                return Err(VmError::InternalError(format!(
                    "RegisterEnum({}) reserved definition mismatch",
                    operands.type_name
                )));
            }
            self.hidden_eval_enum_type_ids.remove(&enum_id);
        }
        let parent = Some(format!("Enum{{{}}}", definition.base_type));
        self.active_enum_name_index.insert(name.clone(), enum_id);
        self.struct_hierarchy
            .insert_if_absent(&name, parent, Vec::new());
        crate::vm::value::enum_registry::register_enum(&name, &definition.members);
        if !is_hidden {
            self.enum_defs.push(definition);
        }
        self.published_eval_nominal_type_names.insert(name.clone());
        append_type_ancestors(
            &mut self.type_ancestors,
            &[name],
            &self.abstract_types,
            &self.abstract_type_name_index,
            &self.struct_hierarchy,
        );
        self.note_method_table_mutation();
        self.repl_definition_activations
            .push(ReplDefinitionActivation::Enum(enum_id));
        self.begin_eval_enum_member_bindings(operands);
        Ok(())
    }

    pub(crate) fn eval_struct_binding_is_pending(
        &self,
        module_name: &str,
        field_name: &str,
    ) -> bool {
        let qualified_name = format!("{module_name}.{field_name}");
        let top_level_lookup = util::is_top_level_module_binding_scope(module_name);
        let matches_name =
            |name: &str| name == qualified_name || (top_level_lookup && name == field_name);
        self.pending_eval_struct_defs
            .iter()
            .any(|(_, definition)| matches_name(&definition.name))
            || self.hidden_eval_struct_type_ids.iter().any(|type_id| {
                self.struct_defs
                    .get(*type_id)
                    .is_some_and(|definition| matches_name(&definition.name))
            })
            || self
                .pending_eval_abstract_types
                .iter()
                .any(|(_, definition)| matches_name(&definition.name))
            || self.hidden_eval_abstract_type_ids.iter().any(|type_id| {
                self.abstract_types
                    .get(*type_id)
                    .is_some_and(|definition| matches_name(&definition.name))
            })
            || self
                .pending_eval_primitive_types
                .iter()
                .any(|(_, definition)| matches_name(&definition.name))
            || self.hidden_eval_primitive_type_ids.iter().any(|type_id| {
                self.compile_context
                    .as_ref()
                    .and_then(|context| context.primitive_types.get(*type_id))
                    .is_some_and(|definition| matches_name(&definition.name))
            })
            || self
                .pending_eval_enum_defs
                .iter()
                .any(|(_, definition)| matches_name(&definition.name))
            || self.hidden_eval_enum_type_ids.iter().any(|type_id| {
                self.enum_defs
                    .get(*type_id)
                    .is_some_and(|definition| matches_name(&definition.name))
            })
    }

    /// Append a COMPILED brand-new generic function to the live VM (Issue #9199
    /// LV3 — the compiled upgrade of the tree-walked `@eval`
    /// [`Vm::eval_define_function_from_expr`] append). `body` is the function's
    /// bytecode with jumps ALREADY relocated onto the live code tail, and
    /// `info.entry`/`code_start`/`code_end` are its final live positions (the
    /// relocatable-delta compiler produced both, see
    /// `repl_relocatable_delta_compile`); this method only splices the body and
    /// registers it into every per-function table. The source-ordered
    /// `DefineEvalFunction` instruction is the sole operation that publishes the
    /// generic binding and advances its method world (Issues #9784 and #11477).
    ///
    /// MUST be called BEFORE [`Vm::reenter_appended_main`] (the function bodies
    /// precede the user main in the appended region). Caller guarantees
    /// `self.functions_len()` equals the compile prefix's function count, so the
    /// returned index equals the delta index the body's self/sibling calls were
    /// compiled against.
    pub fn install_appended_function_body(
        &mut self,
        info: FunctionInfo,
        body: &[Instr],
        source: &[Option<crate::span::Span>],
    ) -> usize {
        let entry = self.code.len();
        debug_assert_eq!(
            entry, info.entry,
            "LV3: appended function `entry` ({}) must match the live code tail ({})",
            info.entry, entry
        );

        // Grow the shared bytecode vector (copy-on-write, like `@eval` /
        // `CallSpecialize` / `reenter_appended_main`).
        let code = std::rc::Rc::make_mut(&mut self.code);
        code.extend_from_slice(body);
        let code_end = code.len();

        // Refresh the predecoded hot-block table + IP-indexed inline call-site
        // cache over the appended body range.
        self.executable.append_bytecode(
            &self.code,
            &self.functions,
            self.base_function_count,
            entry,
            code_end,
        );
        self.call_site_caches
            .resize(code_end, CallSiteCache::default());

        // Keep the source map aligned with `code`.
        self.source_map.resize(entry, None);
        self.source_map.extend_from_slice(source);
        self.source_map.resize(code_end, None);

        // Register into every per-function-indexed table (the ones
        // `Vm::new_program` derives from `program.functions`): `functions`,
        // `function_name_index` (bare + qualified short name), `function_slot_maps`,
        // and `native_array_exempt_functions`.
        let idx = self.functions.len();
        let name = info.name.clone();
        let slot_map: HashMap<String, usize> = info
            .slot_names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let exempt = base_function_accepts_native_array_value(&name);
        let suppress_short_name_alias = info.suppress_short_name_alias;
        let is_lowering_helper = info.is_lowering_helper;
        self.functions.push(std::rc::Rc::new(info));
        let name_index = if is_lowering_helper {
            &mut self.lowering_helper_name_index
        } else {
            &mut self.function_name_index
        };
        name_index.entry(name.clone()).or_default().push(idx);
        // See the matching guard in `Vm::new_program`'s `function_name_index`
        // construction (Issue #10236): a module-body `let`/`@testset`-root
        // function must not also surface under its bare short name.
        if !suppress_short_name_alias {
            if let Some((_, short_name)) = name.rsplit_once('.') {
                name_index
                    .entry(short_name.to_string())
                    .or_default()
                    .push(idx);
            }
        }
        while self.function_slot_maps.len() <= idx {
            self.function_slot_maps.push(HashMap::new());
        }
        self.function_slot_maps[idx] = slot_map;
        while self.native_array_exempt_functions.len() <= idx {
            self.native_array_exempt_functions.push(false);
        }
        self.native_array_exempt_functions[idx] = exempt;

        idx
    }

    /// The live VM's frame-0 global-slot layout (index → name), Issue #9199 LV2.
    /// The REPL feeds this to the relocatable-delta compile as the seed so the
    /// compiled delta main's global slots align with this VM's frame-0.
    pub fn global_slot_names(&self) -> &[String] {
        &self.global_slot_names
    }

    /// Names of every currently defined binding in the REPL's module frame.
    ///
    /// Most top-level bindings live in `locals_slots`, but `global x = ...`
    /// executed inside a function may create a binding after compilation and
    /// therefore stores it in frame 0's dynamic `locals_any` map.  The REPL
    /// session needs both sources when synchronizing the live VM back to its
    /// full-recompile fallback snapshot (Issue #9784).
    pub fn defined_repl_global_names(&self) -> Vec<String> {
        let Some(frame) = self.frames.first() else {
            return Vec::new();
        };

        let mut names: Vec<String> = self
            .global_slot_names
            .iter()
            .enumerate()
            .filter(|(slot, _)| frame.locals_slots.get(*slot).is_some_and(Option::is_some))
            .map(|(_, name)| name.clone())
            .collect();
        names.extend(frame.locals_any.keys().cloned());
        names.sort_unstable();
        names.dedup();
        names
    }

    pub(super) fn record_repl_using_activation(
        &mut self,
        owner_module: &str,
        program_index: usize,
    ) {
        let activation = (owner_module.to_string(), program_index);
        if !self.repl_using_activations.contains(&activation) {
            self.repl_using_activations.push(activation);
        }
    }

    /// Source-ordered `(owner module, local usings index)` identities whose
    /// statement markers reached completion in the current main (#11748).
    pub fn repl_reached_using_activations(&self) -> &[(String, usize)] {
        &self.repl_using_activations
    }

    pub(super) fn record_repl_module_activation(&mut self, module_path: &str) {
        let mut path = String::new();
        for component in module_path.split('.') {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(component);
            if !self
                .repl_module_activations
                .iter()
                .any(|reached| reached == &path)
            {
                self.repl_module_activations.push(path.clone());
            }
        }
    }

    /// Qualified module paths whose binding was published before their body
    /// began executing in the current main (#11761).
    pub fn repl_reached_module_activations(&self) -> &[String] {
        &self.repl_module_activations
    }

    /// Frame-0 bindings whose store instruction actually executed in the
    /// current appended main. The set is reset at every live re-entry.
    pub fn repl_written_global_names(&self) -> &HashSet<String> {
        &self.repl_written_globals
    }

    /// Module-global stores that actually executed during the current REPL run.
    pub fn repl_explicit_global_write_names(&self) -> &HashSet<String> {
        &self.repl_explicit_global_writes
    }

    /// Grow frame-0 in place to admit brand-new module globals (Issue #9199 LV2).
    ///
    /// Appends each name to `global_slot_names` / `global_slot_map` at the next
    /// index and extends frame-0's slot store to match, WITHOUT disturbing any
    /// existing global's slot, value, heap ref, or the dispatch/world state. This
    /// is LV2's "frame-0 slot growth": a delta that binds a new global gets a
    /// fresh top slot that the relocatable-delta main's `StoreSlot` writes, seeded
    /// so existing globals keep their index. Names already present are skipped
    /// (idempotent). Call BEFORE `reenter_appended_main` (which keeps frame-0).
    pub fn grow_global_slots(&mut self, new_names: &[String]) {
        for name in new_names {
            if self.global_slot_map.contains_key(name) {
                continue;
            }
            // A function-level `global x = ...` can create a frame-0 binding
            // dynamically before a later compiled delta first allocates `x` a
            // slot. Move that authoritative value into the new slot; leaving it
            // only in `locals_any` would make the slotized delta read `undef`
            // and shadow the recovered binding (Issue #9784).
            let carried = self.frames.first_mut().and_then(|frame0| {
                let value = frame0.locals_any.remove(name);
                if value.is_some() {
                    frame0.var_types.remove(name);
                }
                value
            });
            let idx = self.global_slot_names.len();
            self.global_slot_names.push(name.clone());
            self.global_slot_map.insert(name.clone(), idx);
            if let Some(frame0) = self.frames.first_mut() {
                if frame0.locals_slots.len() <= idx {
                    frame0.locals_slots.resize(idx + 1, None);
                }
                if let Some(value) = carried {
                    let stored = frame0.set_slot_value(idx, value);
                    debug_assert!(stored, "new REPL global slot must be in range");
                }
            }
        }
        let count = self.global_slot_names.len();
        if let Some(frame0) = self.frames.first_mut() {
            if frame0.locals_slots.len() < count {
                frame0.locals_slots.resize(count, None);
            }
        }
    }

    /// Activate the host "graphical display" for this run (Issue #9262).
    ///
    /// Graphical hosts (iOS/web REPL, Editor, `sjulia --emit-artifact`) call this
    /// before `run()` so that `display(x)` routes a renderable value (a `Plot`,
    /// animation, etc.) into the display-artifact sink instead of printing its
    /// text form. Left off for a plain CLI script / terminal REPL, where
    /// `display(x)` falls back to text — matching a headless Julia session whose
    /// display stack holds only a `TextDisplay`.
    pub fn enable_graphical_display(&mut self) {
        self.graphical_display_active = true;
    }

    /// Whether the host graphical display is active (Issue #9262). Read by the
    /// `_display_artifact` builtin to decide between artifact emission and the
    /// pure-Julia text fallback.
    pub(crate) fn graphical_display_active(&self) -> bool {
        self.graphical_display_active
    }

    /// Buffer a display artifact emitted by `display(x)` during the run
    /// (Issue #9262).
    pub(crate) fn push_display_artifact(&mut self, artifact: crate::plotting::DisplayArtifact) {
        self.display_artifacts.push(artifact);
    }

    /// Take the display artifacts emitted by `display(x)` calls during the run,
    /// leaving the sink empty (Issue #9262). Hosts read this after `run()` and
    /// prefer the last emitted artifact over the trailing-value render.
    pub fn take_display_artifacts(&mut self) -> Vec<crate::plotting::DisplayArtifact> {
        std::mem::take(&mut self.display_artifacts)
    }

    /// Return lightweight memory/cache counters for long-running hosts (Issue #8453).
    pub fn memory_stats(&self) -> VmMemoryStats {
        VmMemoryStats {
            struct_heap_len: self.struct_heap.len(),
            struct_heap_capacity: self.struct_heap.capacity(),
            frame_pool_len: self.frame_pool.len(),
            frame_pool_capacity: self.frame_pool.capacity(),
            dispatch_cache_entries: self.dispatch_cache.values().map(HashMap::len).sum(),
            binary_both_dispatch_cache_entries: self
                .binary_both_dispatch_cache
                .values()
                .map(HashMap::len)
                .sum(),
            method_dispatch_cache_entries: self.method_dispatch_cache.len(),
            specialization_cache_entries: self.specialization_cache.len(),
            specialization_i64_cache_entries: self.specialization_i64_cache.len(),
            specialization_f64_cache_entries: self.specialization_f64_cache.len(),
            i64_function_cache_entries: self.i64_function_cache.len(),
            f64_function_cache_entries: self.f64_function_cache.len(),
            binary_method_cache_entries: self.binary_method_cache.len(),
            generated_expr_cache_entries: self.generated_expr_cache.len(),
            cache_clears: self.cache_clear_count,
            cache_cleared_entries: self.cache_cleared_entry_count,
            dispatch_cache_entry_limit: self.dispatch_cache_entry_limit,
            specialization_cache_entry_limit: self.specialization_cache_entry_limit,
            memory_budget_bytes: self.memory_budget_bytes,
            estimated_memory_waterline_bytes: self.estimated_memory_waterline_bytes(),
        }
    }

    /// Set or clear this VM's host memory budget in bytes (Issue #8703).
    pub fn set_memory_budget_bytes(&mut self, bytes: Option<usize>) {
        self.memory_budget_bytes = bytes.filter(|&n| n > 0);
        self.memory_waterline_enabled = self.memory_budget_bytes.is_some();
    }

    /// Approximate reachable VM memory in bytes for intermittent budget checks
    /// (Issue #8703). This is not an allocator hook: it samples obvious VM-owned
    /// containers and cache entry counts, allowing bounded overshoot.
    pub fn estimated_memory_waterline_bytes(&self) -> usize {
        let mut total = 0usize;
        total =
            total.saturating_add(self.struct_heap.len() * std::mem::size_of::<StructInstance>());
        total = total.saturating_add(self.frame_pool.len() * std::mem::size_of::<Frame>());

        let cache_entries = self
            .dispatch_cache
            .values()
            .map(HashMap::len)
            .sum::<usize>()
            + self
                .binary_both_dispatch_cache
                .values()
                .map(HashMap::len)
                .sum::<usize>()
            + self.method_dispatch_cache.len()
            + self.specialization_cache.len()
            + self.specialization_failure_cache.len()
            + self.specialization_i64_cache.len()
            + self.specialization_f64_cache.len()
            + self.specialization_mixed_cache.len()
            + self.i64_function_cache.len()
            + self.f64_function_cache.len()
            + self.typed_function_cache.len()
            + self.binary_method_cache.len()
            + self.generated_expr_cache.len();
        total = total.saturating_add(cache_entries.saturating_mul(128));

        let mut seen_memories = HashSet::new();
        for value in &self.stack {
            total =
                total.saturating_add(self.estimated_value_memory_bytes(value, &mut seen_memories));
        }
        for value in self
            .struct_heap
            .iter()
            .flat_map(|instance| instance.values.iter())
        {
            total =
                total.saturating_add(self.estimated_value_memory_bytes(value, &mut seen_memories));
        }
        for frame in &self.frames {
            for value in frame.locals_slots.iter().flatten() {
                total = total
                    .saturating_add(self.estimated_value_memory_bytes(value, &mut seen_memories));
            }
            for value in frame.locals_any.values() {
                total = total
                    .saturating_add(self.estimated_value_memory_bytes(value, &mut seen_memories));
            }
            for value in frame.captured_vars.values() {
                total = total
                    .saturating_add(self.estimated_value_memory_bytes(value, &mut seen_memories));
            }
        }
        total =
            total.saturating_add(self.estimated_root_lexical_scope_memory_bytes(
                &self.lexical_scopes,
                &mut seen_memories,
            ));
        for context in self.tasks.iter().filter_map(|task| task.context.as_ref()) {
            total = total.saturating_add(self.estimated_root_lexical_scope_memory_bytes(
                &context.lexical_scopes,
                &mut seen_memories,
            ));
        }

        total
    }

    fn estimated_root_lexical_scope_memory_bytes(
        &self,
        scopes: &[RootLexicalScope],
        seen_memories: &mut HashSet<usize>,
    ) -> usize {
        let mut total = scopes
            .len()
            .saturating_mul(std::mem::size_of::<RootLexicalScope>());
        for (name, value) in scopes.iter().flat_map(RootLexicalScope::entries) {
            total = total.saturating_add(name.capacity());
            total = total.saturating_add(std::mem::size_of::<Option<Value>>());
            if let Some(value) = value {
                total =
                    total.saturating_add(self.estimated_value_memory_bytes(value, seen_memories));
            }
        }
        total
    }

    fn estimated_value_memory_bytes(
        &self,
        value: &Value,
        seen_memories: &mut HashSet<usize>,
    ) -> usize {
        match value {
            Value::Memory(memory) => {
                let key = std::rc::Rc::as_ptr(memory) as usize;
                if !seen_memories.insert(key) {
                    return 0;
                }
                let memory = memory.borrow();
                estimated_array_storage_bytes(memory.element_type(), memory.len())
            }
            Value::MemoryRef(memref) => {
                let key = std::rc::Rc::as_ptr(&memref.memory) as usize;
                if !seen_memories.insert(key) {
                    return 0;
                }
                let memory = memref.memory.borrow();
                estimated_array_storage_bytes(memory.element_type(), memory.len())
            }
            Value::ExprArgs(carrier) => {
                let array = carrier.as_array_ref();
                let key = std::rc::Rc::as_ptr(array) as usize;
                if !seen_memories.insert(key) {
                    return 0;
                }
                let array = array.borrow();
                estimated_array_storage_bytes(&array.element_type(), array.element_count())
            }
            Value::Struct(instance) => instance
                .values
                .iter()
                .map(|value| self.estimated_value_memory_bytes(value, seen_memories))
                .sum(),
            Value::Tuple(tuple) | Value::SimpleVector(tuple) => tuple
                .elements
                .iter()
                .map(|value| self.estimated_value_memory_bytes(value, seen_memories))
                .sum(),
            Value::NamedTuple(named) => named
                .values
                .iter()
                .map(|value| self.estimated_value_memory_bytes(value, seen_memories))
                .sum(),
            Value::Pairs(pairs) => pairs
                .data
                .values
                .iter()
                .map(|value| self.estimated_value_memory_bytes(value, seen_memories))
                .sum(),
            Value::Closure(closure) => closure
                .captures
                .iter()
                .map(|(_, value)| self.estimated_value_memory_bytes(value, seen_memories))
                .sum(),
            Value::Ref(cell) => self.estimated_value_memory_bytes(&cell.borrow(), seen_memories),
            Value::QuoteNode(inner) => self.estimated_value_memory_bytes(inner, seen_memories),
            _ => 0,
        }
    }

    pub(crate) fn check_memory_waterline_safepoint(&mut self) -> Result<(), VmError> {
        if !self.memory_waterline_enabled || self.memory_budget_bytes.is_none() {
            return Ok(());
        }
        if self.memory_waterline_check_countdown > 0 {
            self.memory_waterline_check_countdown -= 1;
            return Ok(());
        }
        self.memory_waterline_check_countdown = MEMORY_WATERLINE_CHECK_INTERVAL;
        self.check_memory_waterline_now()
    }

    pub(crate) fn check_memory_waterline_now(&mut self) -> Result<(), VmError> {
        let Some(limit) = self.memory_budget_bytes else {
            return Ok(());
        };
        if !self.memory_waterline_enabled {
            return Ok(());
        }
        if self.estimated_memory_waterline_bytes() > limit {
            self.raise(VmError::OutOfMemory)?;
        }
        Ok(())
    }

    /// Set (or restore defaults with `None`) this VM's runtime cache entry
    /// caps (Issue #8625). A long-running host on a memory-constrained device
    /// can lower these to trade dispatch-cache hit rate for a smaller
    /// footprint; a roomy host can raise them. Each cap is a per-cache entry
    /// count; the cache is cleared wholesale once it exceeds its cap
    /// (the #8610 hard-cap mechanism).
    pub fn set_cache_entry_limits(
        &mut self,
        dispatch: Option<usize>,
        specialization: Option<usize>,
    ) {
        self.dispatch_cache_entry_limit = dispatch
            .filter(|&n| n > 0)
            .unwrap_or(RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT);
        self.specialization_cache_entry_limit = specialization
            .filter(|&n| n > 0)
            .unwrap_or(RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT);
    }

    /// Record a hard-cap clear for #8625 observability.
    #[inline]
    fn note_cache_clear(&mut self, dropped: usize) {
        self.cache_clear_count = self.cache_clear_count.saturating_add(1);
        self.cache_cleared_entry_count = self
            .cache_cleared_entry_count
            .saturating_add(dropped as u64);
    }

    /// Bound the L2 dispatch cache by evicting only the overflow, not the whole
    /// cache (Issue #9197 S3).
    ///
    /// The pre-S3 policy `.clear()`-ed every call site's entries on overflow — a
    /// cliff that discarded all resolved-method decisions at once. Instead this
    /// drops exactly `entries - limit` entries (in `HashMap` iteration order ≈
    /// random replacement, O(1)-cheap), so hot call sites keep their entries
    /// across an overflow and only the excess is shed. This is **capacity
    /// management only**: the generation-counter invalidation in
    /// [`Self::note_method_table_mutation`] (which still whole-clears on a
    /// method-table mutation) is unchanged — precise per-edge flushing is slice
    /// S6.
    pub(crate) fn enforce_dispatch_cache_limit(&mut self) {
        let entries: usize = self.dispatch_cache.values().map(HashMap::len).sum();
        let limit = self.dispatch_cache_entry_limit;
        if entries <= limit {
            return;
        }
        let dropped = entries - limit;
        let mut to_drop = dropped;
        let mut emptied: Vec<usize> = Vec::new();
        for (ip, bucket) in self.dispatch_cache.iter_mut() {
            if to_drop == 0 {
                break;
            }
            if bucket.len() <= to_drop {
                to_drop -= bucket.len();
                emptied.push(*ip);
            } else {
                let mut remove_here = to_drop;
                bucket.retain(|_, _| {
                    if remove_here > 0 {
                        remove_here -= 1;
                        false
                    } else {
                        true
                    }
                });
                to_drop = 0;
            }
        }
        for ip in emptied {
            self.dispatch_cache.remove(&ip);
        }
        self.note_cache_clear(dropped);
    }

    pub(crate) fn enforce_binary_both_dispatch_cache_limit(&mut self) {
        let entries: usize = self
            .binary_both_dispatch_cache
            .values()
            .map(HashMap::len)
            .sum();
        if entries > self.dispatch_cache_entry_limit {
            self.binary_both_dispatch_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_method_dispatch_cache_limit(&mut self) {
        let entries = self.method_dispatch_cache.len();
        if entries > self.dispatch_cache_entry_limit {
            self.method_dispatch_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_specialization_cache_limit(&mut self) {
        let entries = self.specialization_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.specialization_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_specialization_failure_cache_limit(&mut self) {
        let entries = self.specialization_failure_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.specialization_failure_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_specialization_i64_cache_limit(&mut self) {
        let entries = self.specialization_i64_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.specialization_i64_cache.clear();
            self.specialization_i64_fast_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    /// Float64 mirror of [`Self::enforce_specialization_i64_cache_limit`]
    /// (Issue #10491).
    pub(crate) fn enforce_specialization_f64_cache_limit(&mut self) {
        let entries = self.specialization_f64_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.specialization_f64_cache.clear();
            self.specialization_f64_fast_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    /// Narrow mixed-arg mirror of [`Self::enforce_specialization_f64_cache_limit`]
    /// (Issue #10567 round 2). No fast-cache twin to clear (see the field doc
    /// on `Vm::specialization_mixed_cache`).
    pub(crate) fn enforce_specialization_mixed_cache_limit(&mut self) {
        let entries = self.specialization_mixed_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.specialization_mixed_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_i64_function_cache_limit(&mut self) {
        let entries = self.i64_function_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.i64_function_cache.clear();
            self.typed_function_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_f64_function_cache_limit(&mut self) {
        let entries = self.f64_function_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.f64_function_cache.clear();
            self.typed_function_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_typed_function_cache_limit(&mut self) {
        let entries = self.typed_function_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.typed_function_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_binary_method_cache_limit(&mut self) {
        let entries = self.binary_method_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.binary_method_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    pub(crate) fn enforce_generated_expr_cache_limit(&mut self) {
        let entries = self.generated_expr_cache.len();
        if entries > self.specialization_cache_entry_limit {
            self.generated_expr_cache.clear();
            self.note_cache_clear(entries);
        }
    }

    #[cfg(test)]
    pub(crate) fn enforce_runtime_cache_limits(&mut self) {
        self.enforce_dispatch_cache_limit();
        self.enforce_binary_both_dispatch_cache_limit();
        self.enforce_method_dispatch_cache_limit();
        self.enforce_specialization_cache_limit();
        self.enforce_specialization_failure_cache_limit();
        self.enforce_specialization_i64_cache_limit();
        self.enforce_specialization_f64_cache_limit();
        self.enforce_specialization_mixed_cache_limit();
        self.enforce_i64_function_cache_limit();
        self.enforce_f64_function_cache_limit();
        self.enforce_typed_function_cache_limit();
        self.enforce_binary_method_cache_limit();
        self.enforce_generated_expr_cache_limit();
    }

    /// Clear runtime dispatch/specialization caches without changing program state (Issue #8453).
    pub fn clear_runtime_caches(&mut self) {
        self.binary_signature_cache.clear();
        self.typed_signature_cache.clear();
        self.specialization_cache.clear();
        self.specialization_failure_cache.clear();
        // Issue #10749: the cache tracks `(functions.len(), specializable_functions.len())`
        // so it will rebuild itself lazily even without this, but a method-table
        // mutation can also change which names are ambiguous without changing
        // either length (e.g. reusing a previously-freed slot never happens in
        // this append-only model, but clearing here keeps this cache invalidated
        // by the exact same trigger as every other dispatch cache instead of
        // relying on that invariant continuing to hold).
        self.specializable_callable_registry_cache = None;
        self.specialization_i64_cache.clear();
        self.specialization_i64_fast_cache.clear();
        self.specialization_f64_cache.clear();
        self.specialization_f64_fast_cache.clear();
        self.specialization_mixed_cache.clear();
        self.i64_function_cache.clear();
        self.f64_function_cache.clear();
        self.binary_method_cache.clear();
        self.dispatch_cache.clear();
        self.binary_both_dispatch_cache.clear();
        self.method_dispatch_cache.clear();
        self.generated_expr_cache.clear();
        // Invalidate the IP-indexed call-site inline caches in O(1) by
        // bumping the dispatch generation (Issue #8561).
        self.dispatch_generation = self.dispatch_generation.saturating_add(1);
    }

    pub(crate) fn register_weak_ref(&mut self, cell: &WeakRefCell) {
        self.weak_refs.push(std::rc::Rc::downgrade(cell));
    }

    pub(crate) fn weakref_value(&self, value: Value) -> Result<Value, VmError> {
        match value {
            Value::WeakRef(cell) => Ok(cell.borrow().clone()),
            other => Err(VmError::TypeError(format!(
                "_weakref_value: expected WeakRef, got {}",
                self.get_type_name(&other)
            ))),
        }
    }

    pub(crate) fn weakref_set_value(
        &mut self,
        weakref: Value,
        new_value: Value,
    ) -> Result<Value, VmError> {
        match weakref {
            Value::WeakRef(cell) => {
                *cell.borrow_mut() = new_value;
                Ok(Value::Nothing)
            }
            other => Err(VmError::TypeError(format!(
                "_weakref_set_value!: expected WeakRef, got {}",
                self.get_type_name(&other)
            ))),
        }
    }

    pub(crate) fn register_finalizer(
        &mut self,
        callback: Value,
        object: Value,
    ) -> Result<Value, VmError> {
        let target = self.finalizer_target_for_value(&object).ok_or_else(|| {
            VmError::ErrorException(format!(
                "objects of type {} cannot be finalized because they are not mutable",
                self.get_type_name(&object)
            ))
        })?;
        let object_snapshot = match object {
            Value::StructRef(idx) => {
                let instance = self.struct_heap.get(idx).ok_or_else(|| {
                    VmError::TypeError(format!(
                        "Invalid struct reference: index {} out of bounds",
                        idx
                    ))
                })?;
                Value::Struct(instance.clone())
            }
            _ => object.clone(),
        };
        self.finalizers.push(FinalizerEntry {
            target,
            callback,
            object_snapshot,
            active: true,
        });
        Ok(object)
    }

    pub(crate) fn finalize_value(&mut self, object: Value) -> Result<Value, VmError> {
        let Some(target) = self.finalizer_target_for_value(&object) else {
            return Ok(Value::Nothing);
        };
        let mut callbacks = Vec::new();
        for entry in &mut self.finalizers {
            if entry.active && entry.target == target {
                entry.active = false;
                callbacks.push((entry.callback.clone(), object.clone()));
            }
        }
        self.run_finalizer_callbacks(callbacks)?;
        Ok(Value::Nothing)
    }

    pub(crate) fn gc_collect(&mut self) -> Result<Value, VmError> {
        self.compact_struct_heap_for_explicit_gc();
        self.clear_weak_refs_without_stack_roots();
        self.run_pending_finalizers()?;
        Ok(Value::Nothing)
    }

    pub(crate) fn gc_in_finalizer(&self) -> Value {
        Value::Bool(self.in_finalizer)
    }

    pub(crate) fn run_exit_finalizers(&mut self) -> Result<(), VmError> {
        loop {
            let mut callbacks = Vec::new();
            for entry in &mut self.finalizers {
                if !entry.active {
                    continue;
                }
                entry.active = false;
                let object = match entry.target {
                    FinalizerTarget::Struct(idx) if idx < self.struct_heap.len() => {
                        Value::StructRef(idx)
                    }
                    _ => entry.object_snapshot.clone(),
                };
                callbacks.push((entry.callback.clone(), object));
            }
            if callbacks.is_empty() {
                return Ok(());
            }
            self.run_finalizer_callbacks(callbacks)?;
        }
    }

    fn finalizer_target_for_value(&self, value: &Value) -> Option<FinalizerTarget> {
        match value {
            Value::StructRef(idx) if *idx < self.struct_heap.len() => {
                Some(FinalizerTarget::Struct(*idx))
            }
            Value::Ref(cell) => Some(FinalizerTarget::Shared(rc_id(cell))),
            Value::WeakRef(cell) => Some(FinalizerTarget::Shared(rc_id(cell))),
            Value::Memory(mem) => Some(FinalizerTarget::Shared(rc_id(mem))),
            Value::ExprArgs(carrier) => {
                Some(FinalizerTarget::Shared(rc_id(carrier.as_array_ref())))
            }
            Value::Expr(expr) => Some(FinalizerTarget::Shared(rc_id(&expr.args))),
            Value::Str(s) => Some(FinalizerTarget::Shared(s.as_ptr() as usize)),
            Value::StrBytes(bytes) => Some(FinalizerTarget::Shared(bytes.as_ptr() as usize)),
            _ => None,
        }
    }

    fn run_pending_finalizers(&mut self) -> Result<(), VmError> {
        while !self.pending_finalizers.is_empty() {
            let callbacks = std::mem::take(&mut self.pending_finalizers);
            self.run_finalizer_callbacks(callbacks)?;
        }
        Ok(())
    }

    fn run_finalizer_callbacks(&mut self, callbacks: Vec<(Value, Value)>) -> Result<(), VmError> {
        for (callback, object) in callbacks {
            self.invoke_finalizer_callback(callback, object)?;
        }
        Ok(())
    }

    fn invoke_finalizer_callback(&mut self, callback: Value, object: Value) -> Result<(), VmError> {
        let target_depth = self.frames.len();
        let saved_ip = self.ip;
        let was_in_finalizer = self.in_finalizer;
        self.in_finalizer = true;
        let result = match self.call_runtime_callable_value(callback, vec![object]) {
            Ok(RuntimeCallableResult::Immediate(value)) => Ok(value),
            Ok(RuntimeCallableResult::StartedFrame) => self.run_until_frame_return(target_depth),
            Ok(RuntimeCallableResult::Raised) => {
                Err(self.pending_error.take().unwrap_or_else(|| {
                    VmError::InternalError("finalizer callback raised".to_string())
                }))
            }
            Err(err) => Err(err),
        };
        self.in_finalizer = was_in_finalizer;
        self.ip = saved_ip;
        result.map(|_| ())
    }

    fn update_weak_refs_after_compaction(
        &mut self,
        remap: &[Option<usize>],
        visited: &mut RemapVisited,
    ) {
        self.weak_refs.retain(|weak| {
            let Some(cell) = weak.upgrade() else {
                return false;
            };
            // The same weak cell may also be reachable through a stack/frame/
            // transient root. Reuse the WHOLE-pass visited set so its
            // non-idempotent StructRef remap is applied exactly once (#11378).
            if !visited.insert(rc_id(&cell)) {
                return true;
            }
            let mut value = cell.borrow_mut();
            if let Value::StructRef(idx) = *value {
                *value = remap
                    .get(idx)
                    .and_then(|entry| *entry)
                    .map(Value::StructRef)
                    .unwrap_or(Value::Nothing);
            } else {
                remap_value_struct_refs(&mut value, remap, visited);
            }
            true
        });
    }

    fn clear_weak_refs_without_stack_roots(&mut self) {
        let mut live = std::collections::HashSet::new();
        let mut visited = MarkVisited::new();
        for frame in &self.frames {
            mark_frame_struct_refs(frame, &self.struct_heap, &mut live, &mut visited);
        }
        mark_root_lexical_scope_struct_refs(
            &self.lexical_scopes,
            &self.struct_heap,
            &mut live,
            &mut visited,
        );
        for task in &self.tasks {
            mark_task_struct_refs(task, &self.struct_heap, &mut live, &mut visited);
        }
        for value in &self.transient_roots {
            mark_value_struct_refs(&value.value, &self.struct_heap, &mut live, &mut visited);
        }
        self.weak_refs.retain(|weak| {
            let Some(cell) = weak.upgrade() else {
                return false;
            };
            let mut value = cell.borrow_mut();
            if let Value::StructRef(idx) = *value {
                if !live.contains(&idx) {
                    *value = Value::Nothing;
                }
            }
            true
        });
    }

    fn update_finalizers_after_compaction(
        &mut self,
        remap: &[Option<usize>],
        visited: &mut RemapVisited,
    ) {
        for entry in &mut self.finalizers {
            if !entry.active {
                continue;
            }
            remap_value_struct_refs(&mut entry.callback, remap, visited);
            remap_value_struct_refs(&mut entry.object_snapshot, remap, visited);
            if let FinalizerTarget::Struct(idx) = entry.target {
                match remap.get(idx).and_then(|entry| *entry) {
                    Some(new_idx) => entry.target = FinalizerTarget::Struct(new_idx),
                    None => {
                        entry.active = false;
                        self.pending_finalizers
                            .push((entry.callback.clone(), entry.object_snapshot.clone()));
                    }
                }
            }
        }
    }

    /// Compact `struct_heap` when the VM is at a safe point (Issue #8453).
    ///
    /// The pass is conservative: it only runs when no callee frames, exception
    /// handlers, HOF continuations, or generated-eval continuations are live. It
    /// marks from VM roots, follows nested `StructRef`s through heap fields, then
    /// rewrites retained heap indices densely.
    pub fn compact_struct_heap_at_safe_point(&mut self) -> StructHeapCompaction {
        self.compact_struct_heap_at_safe_point_with_return_internal(None, false)
    }

    /// Compact the struct heap at a completed top-level boundary while keeping
    /// an externally held return value as an additional root.
    ///
    /// REPL callers use this after a successful live transaction, before they
    /// project frame-0 globals and the returned value into session state.
    pub fn compact_struct_heap_at_safe_point_with_return(
        &mut self,
        return_value: Option<&mut Value>,
    ) -> StructHeapCompaction {
        self.compact_struct_heap_at_safe_point_with_return_internal(return_value, false)
    }

    pub(crate) fn compact_struct_heap_for_explicit_gc(&mut self) -> StructHeapCompaction {
        self.compact_struct_heap_at_safe_point_with_return_internal(None, true)
    }

    fn compact_struct_heap_at_safe_point_with_return_internal(
        &mut self,
        return_value: Option<&mut Value>,
        explicit_gc: bool,
    ) -> StructHeapCompaction {
        let before_len = self.struct_heap.len();
        if before_len == 0 {
            return StructHeapCompaction {
                before_len,
                after_len: 0,
                reclaimed: 0,
                compacted: true,
            };
        }
        let can_compact = if explicit_gc {
            self.can_compact_struct_heap_for_explicit_gc()
        } else {
            self.can_compact_struct_heap_at_safe_point()
        };
        if !can_compact {
            return StructHeapCompaction {
                before_len,
                after_len: before_len,
                reclaimed: 0,
                compacted: false,
            };
        }

        let mut live = std::collections::HashSet::new();
        let mut visited = MarkVisited::new();
        for value in &self.stack {
            mark_value_struct_refs(value, &self.struct_heap, &mut live, &mut visited);
        }
        for frame in &self.frames {
            mark_frame_struct_refs(frame, &self.struct_heap, &mut live, &mut visited);
        }
        mark_root_lexical_scope_struct_refs(
            &self.lexical_scopes,
            &self.struct_heap,
            &mut live,
            &mut visited,
        );
        for task in &self.tasks {
            mark_task_struct_refs(task, &self.struct_heap, &mut live, &mut visited);
        }
        for value in &self.transient_roots {
            mark_value_struct_refs(&value.value, &self.struct_heap, &mut live, &mut visited);
        }
        if let Some(value) = return_value.as_deref() {
            mark_value_struct_refs(value, &self.struct_heap, &mut live, &mut visited);
        }

        let mut remap = vec![None; self.struct_heap.len()];
        let mut compacted_heap = Vec::with_capacity(live.len());
        for (old_idx, instance) in self.struct_heap.iter().cloned().enumerate() {
            if live.contains(&old_idx) {
                remap[old_idx] = Some(compacted_heap.len());
                compacted_heap.push(instance);
            }
        }

        // One visited set for the WHOLE pass so a shared `Rc` array/memory/`Ref`
        // reached from several roots (a heap field AND a frame slot, two slots, …)
        // is remapped exactly once (Issue #9787).
        let mut visited = RemapVisited::new();
        for instance in &mut compacted_heap {
            for value in &mut instance.values {
                remap_value_struct_refs(value, &remap, &mut visited);
            }
        }
        for value in &mut self.stack {
            remap_value_struct_refs(value, &remap, &mut visited);
        }
        for frame in &mut self.frames {
            remap_frame_struct_refs(frame, &remap, &mut visited);
        }
        remap_root_lexical_scope_struct_refs(&mut self.lexical_scopes, &remap, &mut visited);
        for task in &mut self.tasks {
            remap_task_struct_refs(task, &remap, &mut visited);
        }
        for value in &mut self.transient_roots {
            remap_value_struct_refs(&mut value.value, &remap, &mut visited);
        }
        if let Some(value) = return_value {
            remap_value_struct_refs(value, &remap, &mut visited);
        }
        self.update_weak_refs_after_compaction(&remap, &mut visited);
        self.update_finalizers_after_compaction(&remap, &mut visited);

        self.struct_heap = compacted_heap;
        let after_len = self.struct_heap.len();
        StructHeapCompaction {
            before_len,
            after_len,
            reclaimed: before_len.saturating_sub(after_len),
            compacted: true,
        }
    }

    fn can_compact_struct_heap_at_safe_point(&self) -> bool {
        self.frames.len() == 1
            && self.return_ips.is_empty()
            && self.handlers.is_empty()
            && self.broadcast_states.is_empty()
            && self.composed_call_state.is_none()
            && self.generator_iterate_state.is_empty()
            && self.sprint_state.is_none()
            && self.pending_error.is_none()
            && self.pending_exception_value.is_none()
            && self.pending_backtrace.is_none()
            && self.caught_exceptions.is_empty()
            && self.generated_expr_pending_keys.is_empty()
            && self.generated_expr_pending_eval_frames.is_empty()
            && self.eval_dispatch_depth == 0
    }

    fn can_compact_struct_heap_for_explicit_gc(&self) -> bool {
        self.handlers.is_empty()
            && self.broadcast_states.is_empty()
            && self.composed_call_state.is_none()
            && self.generator_iterate_state.is_empty()
            && self.sprint_state.is_none()
            && self.pending_error.is_none()
            && self.pending_exception_value.is_none()
            && self.pending_backtrace.is_none()
            && self.caught_exceptions.is_empty()
            && self.generated_expr_pending_keys.is_empty()
            && self.generated_expr_pending_eval_frames.is_empty()
            && self.eval_dispatch_depth == 0
    }

    /// Pop a numeric value as f64 from stack, handling Rational and BigInt.
    /// Uses StackOpsExt with struct_heap context.
    #[inline]
    pub fn pop_f64_or_i64(&mut self) -> Result<f64, VmError> {
        StackOpsExt::pop_f64_or_i64(&mut self.stack, &self.struct_heap)
    }

    /// Pop a numeric-or-Char value as f64. For `Value::Char(c)`, returns
    /// the Unicode codepoint as `f64`. Used by `Instr::MakeRangeLazy`
    /// so Char ranges (`'a':'e'`) work — the resulting `RangeValue`
    /// stores the codepoint and `RangeElementType::Char` converts back
    /// via `char::from_u32` on element materialization (Issue #4795).
    #[inline]
    pub fn pop_f64_or_i64_or_char(&mut self) -> Result<f64, VmError> {
        if let Some(crate::vm::Value::Char(c)) = self.stack.last() {
            let cp = *c as u32 as f64;
            self.stack.pop();
            return Ok(cp);
        }
        StackOpsExt::pop_f64_or_i64(&mut self.stack, &self.struct_heap)
    }

    /// Pop a complex number from stack, handling promotion from real numbers.
    /// Uses StackOpsExt with struct_heap context.
    #[inline]
    pub fn pop_complex(&mut self) -> Result<(f64, f64), VmError> {
        StackOpsExt::pop_complex(&mut self.stack, &self.struct_heap)
    }

    /// Pop exception handlers that were pushed by the current function.
    /// This should be called before returning from a function to clean up
    /// any try-catch handlers that are still active.
    ///
    /// Handlers store `return_ip_len` which is the length of return_ips
    /// when the handler was pushed. After a callee returns, its handlers have
    /// a greater return_ip_len than the caller's current return_ips length.
    /// Caller handlers have the same length and must remain active.
    pub(crate) fn pop_handlers_for_return(&mut self) {
        let current_return_ip_len = self.return_ips.len();
        // Pop handlers that were pushed in the current function frame
        // (their return_ip_len >= current_return_ip_len means they were
        // pushed after we entered this function)
        while let Some(handler) = self.handlers.last() {
            if handler.return_ip_len > current_return_ip_len {
                self.handlers.pop();
            } else {
                break;
            }
        }
    }

    /// Drop any in-flight driven-callable control state whose owning callable
    /// frame is discarded when an exception unwinds to a handler installed at
    /// frame depth `frame_floor` (Issue #9319).
    ///
    /// The VM drives higher-order / broadcast / generator callables (`map`,
    /// `collect(f(x) for x in it if p(x))`, lazy `iterate(::Generator)`, `∘`
    /// composition, `sprint`) by parking a state record keyed to the frame
    /// depth of the callable frame and re-entering it when that frame *returns*.
    /// When an exception thrown inside such a driven callable is caught by an
    /// ancestor `try`/`catch`, `handle_error` truncates `self.frames` back to
    /// the handler's floor — but the parked state used to survive, keyed to a
    /// frame depth that no longer exists. A later, unrelated function that
    /// happened to return at that same depth was then misread as the driven
    /// callable returning, silently re-entering the stale driver (observed as a
    /// phantom "Division by zero" from a previous generator body). Mirror the
    /// frame truncation for every driven-callable carrier: keep only the states
    /// whose callable frame is at or below the surviving floor.
    pub(in crate::vm) fn unwind_driven_callable_state(&mut self, frame_floor: usize) {
        // Value-mode HOF / broadcast drivers (`map`, filtered-generator
        // `FilterMap`, `ntuple`, ...). `hof_frame_depth` is the frame count
        // while the driven callable frame is live, so a state with
        // `hof_frame_depth > frame_floor` was started inside the unwound `try`.
        self.broadcast_states
            .retain(|bc| bc.hof_frame_depth <= frame_floor);
        // Lazy `iterate(::Generator)` continuations (plain + filtered generator
        // drive) keyed the same way via `call_frame_depth`.
        self.generator_iterate_state
            .retain(|st| st.call_frame_depth <= frame_floor);
        // `(f ∘ g)(x)` composition: the inner callee frame sits at
        // `call_frame_depth + 1` (see `handle_composed_call_return`).
        if self
            .composed_call_state
            .as_ref()
            .is_some_and(|cs| cs.call_frame_depth + 1 > frame_floor)
        {
            self.composed_call_state = None;
        }
        // `sprint(f, args...)`: the `f(io, ...)` frame sits at
        // `call_frame_depth + 1` (see `handle_sprint_return`).
        if self
            .sprint_state
            .as_ref()
            .is_some_and(|ss| ss.call_frame_depth + 1 > frame_floor)
        {
            self.sprint_state = None;
        }
        // `redirect_stdout` / `redirect_stderr`: the thunk frame sits at
        // `call_frame_depth + 1`, so an exception caught below that frame must
        // restore the previous sink while dropping the parked redirect state.
        while self
            .redirect_states
            .last()
            .is_some_and(|state| state.call_frame_depth + 1 > frame_floor)
        {
            if let Some(state) = self.redirect_states.pop() {
                self.restore_redirect_stream(state);
            }
        }
    }

    pub(super) fn handle_error(&mut self, err: VmError) -> Result<(), VmError> {
        // During an `eval`-driven nested dispatch, do not route an error to a
        // handler installed by an *ancestor* frame (Issue #5972). Such a handler
        // (`frame_len <= eval_dispatch_floor`) lives in a `try` opened *outside*
        // the nested `run_until_frame_return` loop; catching it here would
        // truncate `self.frames` below the floor and make that loop return
        // mid-catch, swallowing the exception. Propagate the error as `Err`
        // instead: it unwinds out of `run_until_frame_return`/`eval_dispatch_call`
        // and the outer `run()` loop's `CallBuiltin` arm re-`raise`s it (with the
        // floor restored to the ancestor's level), routing it correctly. Handlers
        // installed *within* the nested dispatch (`frame_len > floor`) are caught
        // here as usual, so a `try`/`catch` inside the eval'd code still works.
        if let Some(floor) = self.eval_dispatch_floor {
            if self.handlers.last().is_some_and(|h| h.frame_len <= floor) {
                return Err(err);
            }
        }
        if let Some(handler) = self.handlers.pop() {
            if self.pending_backtrace.is_none() {
                self.pending_backtrace = Some(self.runtime_stack_trace());
            }
            // Freeze Rust-raised exception fields at the moment this handler
            // captures the error. `ClearError` may archive it for a later
            // `rethrow()` while the catch body raises another error; leaving
            // the typed side-channel parked until `PushExceptionValue` would
            // let that nested raise overwrite the outer payload (#11632).
            // Explicitly-thrown Julia exception values are already frozen;
            // consume any unrelated carrier so it cannot leak forward.
            if self.pending_exception_value.is_none() {
                self.pending_exception_value = self.vm_error_to_exception_value(&err);
            } else {
                self.clear_pending_exception_payloads();
            }
            // Stash a copy of the unwinding exception for this handler's
            // finally-only route BEFORE `err` moves into `pending_error`
            // below (Issue #11306). This must be captured here, not derived
            // later from `pending_error`, because an explicit `rethrow()`
            // inside the finally body — caught by its own nested try/catch —
            // legitimately drains `pending_error` as part of handling ITS OWN
            // (structurally distinct) raise. Without a separately preserved
            // copy, the compiler-emitted trailing `Rethrow` that closes this
            // finally would find nothing left to re-raise and the original
            // exception would silently vanish instead of reaching the
            // enclosing `catch`.
            let finally_only_stash = (handler.catch_ip.is_none() && handler.finally_ip.is_some())
                .then(|| {
                    (
                        err.clone(),
                        self.pending_exception_value.clone(),
                        self.pending_backtrace.clone(),
                    )
                });
            self.pending_error = Some(err);
            self.stack.truncate(handler.stack_len);
            self.frames.truncate(handler.frame_len);
            self.lexical_scopes.truncate(handler.lexical_scope_len);
            self.caught_exceptions
                .truncate(handler.caught_exception_len);
            // Any pending-finally-rethrow marker pushed by a scope nested
            // *inside* this handler (e.g. a further-nested `try`/`finally`
            // entered while unwinding through this one) is being unwound past
            // now, so it is discarded along with the rest of that scope's
            // state. A marker belonging to an *enclosing* finally — pushed
            // before this handler existed — sits at or below
            // `finally_pending_len` and survives untouched (Issue #11306).
            self.pending_finally_rethrows
                .truncate(handler.finally_pending_len);
            if let Some(entry) = finally_only_stash {
                self.pending_finally_rethrows.push(entry);
            }
            self.generated_expr_pending_keys
                .retain(|depth, _| *depth < handler.frame_len);
            self.generated_expr_pending_eval_frames
                .retain(|depth, _| *depth < handler.frame_len);
            self.return_ips.truncate(handler.return_ip_len);
            self.unwind_driven_callable_state(handler.frame_len);

            if let Some(catch_ip) = handler.catch_ip {
                self.ip = catch_ip;
            } else if let Some(finally_ip) = handler.finally_ip {
                self.ip = finally_ip;
            } else {
                let err = self
                    .pending_error
                    .take()
                    .unwrap_or(VmError::InvalidInstruction);
                return Err(err);
            }
            Ok(())
        } else {
            Err(err)
        }
    }

    pub(super) fn error_code(err: &VmError) -> i64 {
        match err {
            VmError::ErrorException(_) => 0, // User-thrown error
            VmError::ArgumentError(_) => 37,
            VmError::AssertionFailed(_) => 1,
            VmError::Cancelled => 17,
            VmError::DivisionByZero => 2,
            VmError::OutOfMemory => 34,
            VmError::StackOverflow => 3,
            VmError::StackUnderflow => 4,
            VmError::InvalidInstruction => 5,
            VmError::IndexOutOfBounds { .. } => 6,
            VmError::DimensionMismatch { .. } => 7,
            // Same Julia exception class (DimensionMismatch), free-form message
            // (Issue #11146), so it shares the error code.
            VmError::DimensionMismatchMsg(_) => 7,
            VmError::MatMulDimensionMismatch { .. } => 8,
            VmError::BroadcastDimensionMismatch { .. } => 9,
            VmError::EmptyArrayPop => 10,
            VmError::TypeError(_) => 11,
            VmError::DomainError(_) => 12,
            VmError::UnknownBroadcastOp(_) => 13,
            VmError::FieldIndexOutOfBounds { .. } => 14,
            VmError::ImmutableFieldAssign(_) => 15,
            VmError::NotImplemented(_) => 16,
            // Tuple/NamedTuple/Dict errors
            VmError::TupleIndexOutOfBounds { .. } => 18,
            VmError::EmptyTuple => 19,
            VmError::TupleDestructuringMismatch { .. } => 20,
            VmError::NamedTupleFieldNotFound(_) => 21,
            VmError::NamedTupleLengthMismatch { .. } => 22,
            VmError::DictKeyNotFound(_) => 23,
            VmError::InvalidDictKey(_) => 24,
            VmError::RangeIndexOutOfBounds { .. } => 25,
            VmError::EmptyRange => 26,
            VmError::UndefVarError(_) => 27,
            // Issue #10318: module-scoped undef is the same Julia exception type
            // (UndefVarError), so it shares the error code.
            VmError::UndefVarErrorInModule { .. } => 27,
            VmError::StringIndexError { .. } => 28,
            VmError::MethodError(_) => 29,
            VmError::InexactError(_) => 30,
            VmError::UndefKeywordError(_) => 31,
            VmError::OverflowError(_) => 32,
            VmError::InternalError(_) => 33,
            // Issue #10067: Core.Binding field access classification.
            VmError::UndefRefError => 35,
            VmError::FieldError { .. } => 36,
            // Issue #11146: runtime Julia-source parse failure (Meta.parse etc.).
            VmError::ParseError(_) => 38,
        }
    }

    pub(super) fn raise(&mut self, err: VmError) -> Result<(), VmError> {
        if self.handle_error(err.clone()).is_ok() {
            Ok(())
        } else {
            Err(err)
        }
    }

    /// Clone the top `n` values of the operand stack, preserving their
    /// bottom-to-top order. Returns fewer than `n` values if the stack is
    /// shorter. Used to build diagnostics (e.g. argument type names for a
    /// `MethodError`) without consuming the stack (Issue #5493).
    pub(crate) fn peek_stack_top(&self, n: usize) -> Vec<Value> {
        let len = self.stack.len();
        let start = len.saturating_sub(n);
        self.stack[start..].to_vec()
    }

    pub(super) fn try_or_handle<T>(
        &mut self,
        result: Result<T, VmError>,
    ) -> Result<Option<T>, VmError> {
        match result {
            Ok(val) => Ok(Some(val)),
            Err(err) => {
                if self.handle_error(err.clone()).is_ok() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Get a function by index, returning an error if the index is out of bounds.
    ///
    /// This is the single source of truth for function index lookups.
    /// All execution modules should use this method instead of raw
    /// `self.functions.get(idx).ok_or_else(...)` or `match self.functions.get(idx)`.
    pub(super) fn get_function_checked(&self, index: usize) -> Result<&FunctionInfo, VmError> {
        self.functions
            .get(index)
            .map(|func| func.as_ref())
            .ok_or_else(|| {
                VmError::InternalError(format!(
                    "Function index {} out of bounds (have {} functions)",
                    index,
                    self.functions.len()
                ))
            })
    }

    /// Get function indices by name using the pre-built index (Issue #3361).
    /// Returns an empty slice if no functions match.
    #[inline]
    pub(crate) fn get_function_indices_by_name(&self, name: &str) -> &[usize] {
        self.function_name_index
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    #[inline]
    pub(crate) fn current_dispatch_world(&self) -> u64 {
        self.frames
            .last()
            .and_then(|frame| frame.func_index.map(|_| frame.world_age))
            .unwrap_or(self.current_world)
    }

    pub(crate) fn function_visible_in_world(&self, index: usize, world: u64) -> bool {
        self.functions
            .get(index)
            .is_some_and(|func| func.min_world <= world)
    }

    pub(crate) fn activate_eval_function(&mut self, index: usize) {
        let refresh = self
            .repl_function_refresh_groups
            .remove(&index)
            .unwrap_or_default();
        self.current_world = self.current_world.saturating_add(1);
        let mut updated_specializable = false;
        for member in std::iter::once(index).chain(refresh.iter().copied()) {
            let func_name = self.functions.get(member).map(|func| func.name.clone());
            if let Some(func) = self.functions.get_mut(member) {
                std::rc::Rc::make_mut(func).min_world = self.current_world;
            }
            if let Some(updates) = self.repl_specializable_updates.remove(&member) {
                for (specializable_index, update) in updates {
                    if let Some(row) = self.specializable_functions.get_mut(specializable_index) {
                        *row = update;
                        updated_specializable = true;
                    }
                }
            }
            match func_name {
                Some(func_name) => {
                    // A `global function f(...)` defined inside a `let` binds a
                    // CLOSURE value carrying that scope's captured locals to the
                    // module-level name `f` (Issue #11015) — upstream bakes the
                    // captured `Core.Box` into the method itself. Rebinding `f` to a
                    // plain, capture-less `Value::Function` here would strip the
                    // environment the method body loads with `LoadCaptured`, so keep
                    // the closure of the SAME generic function in place.
                    let bound_closure = matches!(
                        self.get_value_from_frame(&func_name, 0),
                        Some(Value::Closure(cv)) if cv.name.as_str() == func_name
                    );
                    if let Some(frame) = self.frames.first_mut() {
                        if !bound_closure {
                            util::bind_value_to_frame(
                                frame,
                                &func_name,
                                ValueType::Any,
                                Value::Function(FunctionValue::new(func_name.clone())),
                                &mut self.struct_heap,
                            );
                        }
                    }
                    self.note_method_table_mutation_for(&func_name);
                }
                None => self.note_method_table_mutation(),
            }
        }
        if updated_specializable {
            self.specialization_cache.clear();
            self.specialization_failure_cache.clear();
            self.specialization_i64_cache.clear();
            self.specialization_i64_fast_cache.clear();
            self.specialization_f64_cache.clear();
            self.specialization_f64_fast_cache.clear();
            self.specialization_mixed_cache.clear();
            self.specializable_callable_registry_cache = None;
        }
        self.repl_definition_activations
            .push(if refresh.is_empty() {
                ReplDefinitionActivation::Function(index)
            } else {
                ReplDefinitionActivation::FunctionGroup {
                    primary: index,
                    refresh,
                }
            });
    }

    /// Coarse whole-clear of every runtime dispatch decision cache after a
    /// method-table mutation whose mutated generic function is **not** known to
    /// the caller (Issue #8561).
    ///
    /// The `HashMap`-backed decision caches are cleared eagerly; the IP-indexed
    /// `call_site_caches` side table is invalidated in O(1) by bumping
    /// `dispatch_generation` — every `CallSiteCache` entry stores the generation
    /// it was filled in and a stale generation is a miss. This is the
    /// deliberately coarse, sound fallback; the sole production mutation path
    /// (`activate_eval_function`) knows the mutated name and instead calls
    /// [`Self::note_method_table_mutation_for`] for precise per-name
    /// invalidation (Issue #9197 S6). Kept for tests and any future nameless
    /// caller.
    pub(crate) fn note_method_table_mutation(&mut self) {
        self.dispatch_generation = self.dispatch_generation.saturating_add(1);
        self.method_dispatch_cache.clear();
        self.dispatch_cache.clear();
        self.binary_both_dispatch_cache.clear();
        // A new method/struct definition can turn a previously failed
        // specialization attempt into a success (e.g. a struct the specializer
        // reported as unknown), so retire negative entries here (Issue #8603).
        self.specialization_failure_cache.clear();
    }

    /// Precisely invalidate only the runtime dispatch-cache decisions that a
    /// (re)definition of generic function `name` can change (Issue #9197 S6),
    /// leaving every unrelated warm call site cached.
    ///
    /// Upstream Julia's `jl_method_table_insert` bumps the world counter and
    /// walks backedges to cap only the `CodeInstance`s whose dispatch could
    /// change (`invalidate_backedges`, `julia/src/gf.c`), rather than flushing
    /// the whole method cache. This is the runtime-dispatch-cache analogue,
    /// adapted to sjulia's single-threaded flat function table: a cached
    /// decision can only change if it resolved to a **method of the mutated
    /// generic function** (same base name) or to the builtin/native fallback
    /// sentinel (which a new user method may now capture). The reverse
    /// "callee name → dependent entries" edge set is not materialized; it is
    /// recomputed on demand from each entry's resolved function index via
    /// [`dispatch_decision_affected`] (the minimal runtime edge set — the
    /// finer signature-intersection precision the #8554 overlap machinery
    /// enables is left for Issue #9197 S7).
    ///
    /// Correctness vs. the coarse whole-clear: the set of entries dropped for
    /// `name` is exactly the set the whole-clear would drop-and-recompute for
    /// `name`, so any re-resolution of a call site dispatching `name` is
    /// byte-identical; entries kept belong to unrelated generic functions whose
    /// dispatch is independent of this mutation. Unlike the whole-clear this
    /// does **not** bump `dispatch_generation`, so the L1 inline-cache slots of
    /// unrelated call sites stay live; only the affected L1 ways are vacated.
    pub(crate) fn note_method_table_mutation_for(&mut self, name: &str) {
        let target = dispatch_generic_base_name(name);
        let Vm {
            functions,
            dispatch_cache,
            binary_both_dispatch_cache,
            method_dispatch_cache,
            specialization_failure_cache,
            call_site_caches,
            ..
        } = self;
        let functions: &[Rc<FunctionInfo>] = functions;

        // L2 dispatch cache (per-IP → interned arg-id sequence → func_index).
        dispatch_cache.retain(|_ip, inner| {
            inner.retain(|_key, func_index| {
                !dispatch_decision_affected(functions, target, *func_index)
            });
            !inner.is_empty()
        });
        // Per-IP binary-both struct-operand resolver decisions (value = Option;
        // a `None` negative decision is dropped conservatively).
        binary_both_dispatch_cache.retain(|_ip, inner| {
            inner.retain(|_key, decision| match *decision {
                Some(idx) => !dispatch_decision_affected(functions, target, idx),
                None => false,
            });
            !inner.is_empty()
        });
        // Global name+argtype method dispatch cache. The key hashes the
        // dispatched name(s), so a `None` negative entry — whose name we cannot
        // recover from the hash — is dropped conservatively.
        method_dispatch_cache.retain(|_key, decision| match *decision {
            Some(idx) => !dispatch_decision_affected(functions, target, idx),
            None => false,
        });
        // Negative specialization attempts are correctness-neutral to re-run and
        // a new definition can turn a failure into a success (Issue #8603);
        // retire them wholesale as the coarse path does (small, negative-only).
        specialization_failure_cache.clear();
        // L1 IP-indexed inline cache: vacate only the ways whose resolved target
        // is affected, keeping unrelated (and, at a polymorphic-callable site,
        // the other function's) way warm. No generation bump.
        for slot in call_site_caches.iter_mut() {
            slot.invalidate_ways(|idx| dispatch_decision_affected(functions, target, idx));
        }
    }

    /// Compute the L1 call-site inline-cache key for a single value: its interned
    /// dispatch-type id (Issue #9197 slice 2, replacing the old #9108/#9113 hash).
    ///
    /// Interns the value's dispatch identity into the session-scoped
    /// [`TypeInternTable`] (so `&mut self`); `struct_heap` resolves a `StructRef`
    /// index to its instance. `None` ⇒ the value kind has no tracked dispatch
    /// identity, so the call site skips L1.
    #[inline]
    pub(crate) fn call_site_arg_fingerprint(&mut self, value: &Value) -> Option<CallSiteArgIds> {
        call_site_arg_type_ids(
            &[value],
            &self.struct_heap,
            &mut self.type_intern,
            &self.call_site_type_id_tables,
        )
    }

    /// Compute the L1 call-site inline-cache key (interned per-argument id
    /// sequence) for a slice of argument values (Issue #9197 slice 2).
    #[inline]
    pub(crate) fn call_site_arg_fingerprints(
        &mut self,
        values: &[&Value],
    ) -> Option<CallSiteArgIds> {
        call_site_arg_type_ids(
            values,
            &self.struct_heap,
            &mut self.type_intern,
            &self.call_site_type_id_tables,
        )
    }

    /// Look up the L1 call-site inline cache directly by bytecode IP
    /// (Issues #6345, #8561).
    ///
    /// `arg_fingerprint` is the interned arg-id sequence (Issue #9197 slice 2); a
    /// hit requires exact id-sequence equality, not just a hash match. Entries
    /// filled before the last method-table mutation
    /// ([`Self::note_method_table_mutation`]) are stale and report a miss.
    /// Records the opt-in #8559 hit/miss counters: a miss here is exactly one
    /// full run of the per-family dispatch resolver at a cache-eligible site.
    #[inline]
    pub(crate) fn lookup_call_site_inline_cache(
        &mut self,
        call_site_ip: usize,
        arg_fingerprint: &[ConcreteTypeId],
    ) -> Option<usize> {
        let generation = self.dispatch_generation;
        let cached = if self.call_site_inline_cache_disabled {
            // Benchmark/debug baseline (Issue #8561): behave as an
            // always-miss cache so every dispatch runs the resolver.
            None
        } else {
            self.call_site_caches
                .get_mut(call_site_ip)
                .and_then(|cache| cache.lookup(arg_fingerprint, generation))
        };
        if let Some(metrics) = self.stack_metrics.as_deref_mut() {
            if cached.is_some() {
                metrics.dispatch_inline_cache_hits += 1;
            } else {
                metrics.dispatch_inline_cache_misses += 1;
            }
        }
        if cached.is_some() {
            crate::vm::profiler::record_event("CallSiteDispatchCacheHit");
        }
        cached
    }

    /// Store the L1 call-site inline cache directly by bytecode IP, tagged
    /// with the current dispatch generation (Issues #6345, #8561).
    #[inline]
    pub(crate) fn store_call_site_inline_cache(
        &mut self,
        call_site_ip: usize,
        arg_fingerprint: Option<&[ConcreteTypeId]>,
        func_index: usize,
    ) {
        if self.call_site_inline_cache_disabled {
            return;
        }
        let Some(arg_fingerprint) = arg_fingerprint else {
            return;
        };
        let generation = self.dispatch_generation;
        if let Some(cache) = self.call_site_caches.get_mut(call_site_ip) {
            cache.store(arg_fingerprint, func_index, generation);
        }
    }

    /// Look up an L2 call-site polymorphic dispatch cache entry (Issue #5079).
    ///
    /// `key` is the interned per-argument [`ConcreteTypeId`] sequence (Issue
    /// #9197 S3) — the same [`CallSiteArgIds`] the L1 inline cache keys on, so a
    /// hit is deterministic exact-sequence equality, not the pre-S3 unverified
    /// type-name hash. The `HashMap` hashes the id slice internally.
    ///
    /// The cache stores `usize::MAX` as a negative/sentinel entry for call
    /// forms that fall back to a builtin or native boundary.
    #[inline]
    pub(crate) fn lookup_call_site_dispatch_cache(
        &self,
        call_site_ip: usize,
        key: &[ConcreteTypeId],
    ) -> Option<usize> {
        let cached = self
            .dispatch_cache
            .get(&call_site_ip)
            .and_then(|m| m.get(key))
            .copied();
        match cached {
            Some(usize::MAX) => {
                crate::vm::profiler::record_event("CallSiteDispatchNegativeCacheHit");
                Some(usize::MAX)
            }
            Some(idx) => {
                crate::vm::profiler::record_event("CallSiteDispatchCacheHit");
                Some(idx)
            }
            None => {
                crate::vm::profiler::record_event("CallSiteDispatchCacheMiss");
                None
            }
        }
    }

    /// Store an L2 call-site polymorphic dispatch cache entry (Issue #5079),
    /// keyed by the interned arg-id sequence `key` (Issue #9197 S3).
    #[inline]
    pub(crate) fn store_call_site_dispatch_cache(
        &mut self,
        call_site_ip: usize,
        key: &[ConcreteTypeId],
        func_index: usize,
    ) {
        crate::vm::profiler::record_event("CallSiteDispatchCacheFill");
        self.dispatch_cache
            .entry(call_site_ip)
            .or_default()
            .insert(CallSiteArgIds::from_slice(key), func_index);
        self.enforce_dispatch_cache_limit();
    }

    /// Convert a value to the representation used by local slots (Issue #5173).
    ///
    /// Mutable structs keep Julia reference identity, so slots store a
    /// `StructRef` into `struct_heap`. Immutable structs have value semantics and
    /// can stay inline in the slot, avoiding the unbounded heap growth that
    /// tight loops saw when every `StoreSlot` cloned an immutable value into
    /// `struct_heap`.
    #[inline]
    pub(crate) fn value_for_slot_storage(&mut self, val: Value) -> Value {
        match val {
            Value::Struct(s)
                if self
                    .struct_defs
                    .get(s.type_id)
                    .map(|def| def.is_mutable)
                    .unwrap_or(false) =>
            {
                let idx = self.struct_heap.len();
                self.struct_heap.push(s);
                Value::StructRef(idx)
            }
            other => other,
        }
    }

    /// Get a cloned function by index, raising through error handling if not found.
    ///
    /// Returns `Ok(Some(func))` if the function was found, `Ok(None)` if the index
    /// was invalid but the error was caught by a try-catch handler, or `Err` if
    /// the error propagated.
    /// Clone the `Rc` handle (O(1) refcount bump) for the function at `index`,
    /// routing an out-of-bounds index through the VM's exception handling.
    ///
    /// The returned `Rc<FunctionInfo>` lets the caller drop its borrow of
    /// `self.functions` and take `&mut self` for frame setup without cloning the
    /// whole (multi-`Vec`/`String`) `FunctionInfo` on every dynamic call (Issue
    /// #6853).
    pub(super) fn get_function_cloned_or_raise(
        &mut self,
        index: usize,
    ) -> Result<Option<Rc<FunctionInfo>>, VmError> {
        let result = self.functions.get(index).cloned().ok_or_else(|| {
            VmError::InternalError(format!(
                "Function index {} out of bounds (have {} functions)",
                index,
                self.functions.len()
            ))
        });
        self.try_or_handle(result)
    }

    // ==================== Boolean Context Helpers ====================

    /// Check if a value is a boolean and return its value.
    /// Returns Err(TypeError) if the value is not a boolean (Julia semantics).
    #[inline]
    pub(super) fn expect_bool(&self, v: &Value) -> Result<bool, VmError> {
        match v {
            Value::Bool(b) => Ok(*b),
            _ => {
                let type_name = self.get_type_name(v);
                Err(VmError::TypeError(format!(
                    "non-boolean ({}) used in boolean context",
                    type_name
                )))
            }
        }
    }

    /// Execute JumpIfZero instruction: jump to target if condition is false.
    /// Returns Some(target) if should jump, None if should continue.
    /// Returns Err if condition is not a boolean.
    #[inline]
    pub(super) fn execute_jump_if_zero(&mut self, target: usize) -> Result<Option<usize>, VmError> {
        let v = self.stack.pop_value()?;
        let cond = self.expect_bool(&v)?;
        Ok(if !cond { Some(target) } else { None })
    }

    // ==================== Comparison Helpers (return Bool) ====================

    /// Execute floating-point comparison, returns Bool
    #[inline]
    pub(super) fn cmp_f64<F: Fn(f64, f64) -> bool>(&mut self, op: F) -> Result<(), VmError> {
        let b = self.pop_f64_or_i64()?;
        let a = self.pop_f64_or_i64()?;
        self.stack.push(Value::Bool(op(a, b)));
        Ok(())
    }

    /// Execute integer comparison, returns Bool
    #[inline]
    pub(super) fn cmp_i64<F: Fn(i64, i64) -> bool>(&mut self, op: F) -> Result<(), VmError> {
        let b = self.stack.pop_i64()?;
        let a = self.stack.pop_i64()?;
        self.stack.push(Value::Bool(op(a, b)));
        Ok(())
    }

    /// Execute lexicographic string comparison, returns Bool.
    ///
    /// Mirrors [`Self::cmp_i64`]/[`Self::cmp_f64`]: pops the two operands (right
    /// then left), compares their raw string bytes with `op`, and pushes the
    /// `Bool` result. Non-string operands (either side missing string bytes)
    /// compare as `false`, matching the previous per-instruction inline logic
    /// (Issue #2025, consolidated in Issue #10260).
    #[inline]
    pub(super) fn cmp_str<F: Fn(&[u8], &[u8]) -> bool>(&mut self, op: F) -> Result<(), VmError> {
        let right = self.stack.pop_value()?;
        let left = self.stack.pop_value()?;
        let result = match (left.string_bytes(), right.string_bytes()) {
            (Some(a), Some(b)) => op(a, b),
            _ => false,
        };
        self.stack.push(Value::Bool(result));
        Ok(())
    }
}

fn mark_frame_struct_refs(
    frame: &Frame,
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    for value in frame.locals_slots.iter().flatten() {
        mark_value_struct_refs(value, heap, live, visited);
    }
    for value in frame.locals_any.values() {
        mark_value_struct_refs(value, heap, live, visited);
    }
    for value in frame.captured_vars.values() {
        mark_value_struct_refs(value, heap, live, visited);
    }
}

fn mark_root_lexical_scope_struct_refs(
    scopes: &[RootLexicalScope],
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    for value in scopes.iter().flat_map(RootLexicalScope::values) {
        mark_value_struct_refs(value, heap, live, visited);
    }
}

/// Suspended task segments are full GC roots (Issue #10349 S4). The task
/// object/entry callable live outside every frame while parked, and the saved
/// stack/frame suffix may be the sole owner of arbitrary mutable structs.
fn mark_task_struct_refs(
    task: &builtins_tasks::VmTask,
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    mark_value_struct_refs(&task.object, heap, live, visited);
    if let Some(entry) = &task.entry {
        mark_value_struct_refs(entry, heap, live, visited);
    }
    let Some(context) = &task.context else {
        return;
    };
    for value in &context.stack {
        mark_value_struct_refs(value, heap, live, visited);
    }
    for frame in &context.frames {
        mark_frame_struct_refs(frame, heap, live, visited);
    }
    mark_root_lexical_scope_struct_refs(&context.lexical_scopes, heap, live, visited);
    if let Some(value) = &context.pending_exception_value {
        mark_value_struct_refs(value, heap, live, visited);
    }
    for (_, value, _) in &context.caught_exceptions {
        if let Some(value) = value {
            mark_value_struct_refs(value, heap, live, visited);
        }
    }
}

/// Shared mutable carrier identities already traversed by a mark pass. One set
/// spans every root so aliases and cycles are visited exactly once.
type MarkVisited = std::collections::HashSet<usize>;

fn mark_array_data_struct_refs(
    data: &ArrayData,
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    match data {
        ArrayData::StructRefs(indices) => {
            for idx in indices {
                mark_value_struct_refs(&Value::StructRef(*idx), heap, live, visited);
            }
        }
        ArrayData::Any(values) => {
            for value in values {
                mark_value_struct_refs(value, heap, live, visited);
            }
        }
        _ => {}
    }
}

fn mark_array_ref_struct_refs(
    arr: &ArrayRef,
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    if !visited.insert(rc_id(arr)) {
        return;
    }
    let arr = arr.borrow();
    mark_array_data_struct_refs(&arr.data, heap, live, visited);
    if let Some(parent) = &arr.shared_parent {
        mark_array_ref_struct_refs(parent, heap, live, visited);
    }
}

fn mark_memory_ref_struct_refs(
    memory: &MemoryRef,
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    if !visited.insert(rc_id(memory)) {
        return;
    }
    mark_array_data_struct_refs(&memory.borrow().data, heap, live, visited);
}

fn mark_value_struct_refs(
    value: &Value,
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    match value {
        Value::StructRef(idx) if *idx < heap.len() && live.insert(*idx) => {
            for field in &heap[*idx].values {
                mark_value_struct_refs(field, heap, live, visited);
            }
        }
        Value::StructRef(_) => {}
        Value::Struct(instance) => {
            for field in &instance.values {
                mark_value_struct_refs(field, heap, live, visited);
            }
        }
        Value::Tuple(tuple) | Value::SimpleVector(tuple) => {
            for element in &tuple.elements {
                mark_value_struct_refs(element, heap, live, visited);
            }
        }
        Value::NamedTuple(named) => {
            for element in &named.values {
                mark_value_struct_refs(element, heap, live, visited);
            }
        }
        Value::Pairs(pairs) => {
            for element in &pairs.data.values {
                mark_value_struct_refs(element, heap, live, visited);
            }
        }
        Value::Ref(inner) if visited.insert(rc_id(inner)) => {
            mark_value_struct_refs(&inner.borrow(), heap, live, visited);
        }
        Value::Ref(_) | Value::WeakRef(_) => {}
        Value::Generator(generator) => {
            mark_generator_callable_struct_refs(&generator.callable, heap, live, visited);
            mark_value_struct_refs(&generator.iter, heap, live, visited);
        }
        Value::Expr(expr) => {
            mark_array_ref_struct_refs(&expr.args, heap, live, visited);
        }
        Value::QuoteNode(inner) => {
            mark_value_struct_refs(inner, heap, live, visited);
        }
        Value::ExprArgs(carrier) => {
            mark_array_ref_struct_refs(carrier.as_array_ref(), heap, live, visited);
        }
        Value::Memory(memory) => {
            mark_memory_ref_struct_refs(memory, heap, live, visited);
        }
        Value::MemoryRef(memory_ref) => {
            mark_memory_ref_struct_refs(&memory_ref.memory, heap, live, visited);
        }
        Value::Closure(closure) => {
            for (_, captured) in closure.captures.iter() {
                mark_value_struct_refs(captured, heap, live, visited);
            }
        }
        Value::ComposedFunction(composed) => {
            mark_value_struct_refs(&composed.outer, heap, live, visited);
            mark_value_struct_refs(&composed.inner, heap, live, visited);
        }
        _ => {}
    }
}

fn mark_generator_callable_struct_refs(
    callable: &GeneratorCallable,
    heap: &[StructInstance],
    live: &mut std::collections::HashSet<usize>,
    visited: &mut MarkVisited,
) {
    match callable {
        GeneratorCallable::RuntimeValue(value)
        | GeneratorCallable::TupleSplatRuntimeValue(value) => {
            mark_value_struct_refs(value, heap, live, visited);
        }
        GeneratorCallable::FilteredRuntimeValue { map, predicate } => {
            mark_value_struct_refs(map, heap, live, visited);
            mark_value_struct_refs(predicate, heap, live, visited);
        }
        _ => {}
    }
}

/// A set of already-remapped shared-`Rc` container identities (by `Rc::as_ptr`
/// address), so each shared array / memory / `Ref` is rewritten EXACTLY once
/// within a single compaction pass (Issue #9787).
///
/// `remap_value_struct_refs` rewrites a `StructRef` index *in place* through the
/// old→new table, and that table is NOT idempotent (`3→1` then `1→0`). When two
/// roots — or a root and a retained heap field, or the same value reached twice —
/// alias the same `Rc<RefCell<ArrayValue/MemoryValue/Value>>`, a second visit
/// would remap the already-remapped indices again and corrupt them. Threading a
/// visited set through the whole pass makes the rewrite safe under `Rc` aliasing
/// for BOTH callers (the cross-eval transplant and the in-VM safe-point GC).
pub(super) type RemapVisited = std::collections::HashSet<usize>;

#[inline]
fn rc_id<T>(rc: &std::rc::Rc<T>) -> usize {
    std::rc::Rc::as_ptr(rc) as *const () as usize
}

#[derive(Default)]
struct CallableReachVisited {
    shared: HashSet<usize>,
    struct_refs: HashSet<usize>,
}

fn array_data_references_function_suffix(
    data: &ArrayData,
    heap: &[StructInstance],
    first_function: usize,
    visited: &mut CallableReachVisited,
) -> bool {
    match data {
        ArrayData::Any(values) | ArrayData::String(values) => values
            .iter()
            .any(|value| value_references_function_suffix(value, heap, first_function, visited)),
        ArrayData::StructRefs(indices) => indices.iter().any(|index| {
            value_references_function_suffix(
                &Value::StructRef(*index),
                heap,
                first_function,
                visited,
            )
        }),
        _ => false,
    }
}

fn value_references_function_suffix(
    value: &Value,
    heap: &[StructInstance],
    first_function: usize,
    visited: &mut CallableReachVisited,
) -> bool {
    match value {
        Value::Function(function) => function
            .candidate_indices
            .as_ref()
            .is_some_and(|indices| indices.iter().any(|index| *index >= first_function)),
        Value::Closure(closure) => {
            closure
                .candidate_indices
                .as_ref()
                .is_some_and(|indices| indices.iter().any(|index| *index >= first_function))
                || closure.captures.iter().any(|(_, captured)| {
                    value_references_function_suffix(captured, heap, first_function, visited)
                })
        }
        Value::StructRef(index) => {
            *index < heap.len()
                && visited.struct_refs.insert(*index)
                && heap[*index].values.iter().any(|field| {
                    value_references_function_suffix(field, heap, first_function, visited)
                })
        }
        Value::Struct(instance) => instance
            .values
            .iter()
            .any(|field| value_references_function_suffix(field, heap, first_function, visited)),
        Value::Tuple(tuple) | Value::SimpleVector(tuple) => tuple.elements.iter().any(|element| {
            value_references_function_suffix(element, heap, first_function, visited)
        }),
        Value::NamedTuple(named) => named.values.iter().any(|element| {
            value_references_function_suffix(element, heap, first_function, visited)
        }),
        Value::Pairs(pairs) => pairs.data.values.iter().any(|element| {
            value_references_function_suffix(element, heap, first_function, visited)
        }),
        Value::Ref(inner) | Value::WeakRef(inner) if visited.shared.insert(rc_id(inner)) => {
            value_references_function_suffix(&inner.borrow(), heap, first_function, visited)
        }
        Value::Ref(_) | Value::WeakRef(_) => false,
        Value::Generator(generator) => {
            let callable_references = match &generator.callable {
                GeneratorCallable::FunctionIndex(index)
                | GeneratorCallable::TupleSplatFunctionIndex(index) => *index >= first_function,
                GeneratorCallable::FilteredFunctionIndex {
                    map_func_index,
                    predicate_func_index,
                } => *map_func_index >= first_function || *predicate_func_index >= first_function,
                GeneratorCallable::RuntimeValue(callable)
                | GeneratorCallable::TupleSplatRuntimeValue(callable) => {
                    value_references_function_suffix(callable, heap, first_function, visited)
                }
                GeneratorCallable::FilteredRuntimeValue { map, predicate } => {
                    value_references_function_suffix(map, heap, first_function, visited)
                        || value_references_function_suffix(
                            predicate,
                            heap,
                            first_function,
                            visited,
                        )
                }
                _ => false,
            };
            callable_references
                || value_references_function_suffix(&generator.iter, heap, first_function, visited)
        }
        Value::Expr(expr) => {
            array_ref_references_function_suffix(&expr.args, heap, first_function, visited)
        }
        Value::ExprArgs(carrier) => array_ref_references_function_suffix(
            carrier.as_array_ref(),
            heap,
            first_function,
            visited,
        ),
        Value::Memory(memory) if visited.shared.insert(rc_id(memory)) => {
            array_data_references_function_suffix(
                &memory.borrow().data,
                heap,
                first_function,
                visited,
            )
        }
        Value::Memory(_) => false,
        Value::MemoryRef(memory_ref) if visited.shared.insert(rc_id(&memory_ref.memory)) => {
            array_data_references_function_suffix(
                &memory_ref.memory.borrow().data,
                heap,
                first_function,
                visited,
            )
        }
        Value::MemoryRef(_) => false,
        Value::QuoteNode(inner) => {
            value_references_function_suffix(inner, heap, first_function, visited)
        }
        Value::ComposedFunction(composed) => {
            value_references_function_suffix(&composed.outer, heap, first_function, visited)
                || value_references_function_suffix(&composed.inner, heap, first_function, visited)
        }
        _ => false,
    }
}

fn array_ref_references_function_suffix(
    array: &ArrayRef,
    heap: &[StructInstance],
    first_function: usize,
    visited: &mut CallableReachVisited,
) -> bool {
    if !visited.shared.insert(rc_id(array)) {
        return false;
    }
    let array = array.borrow();
    array_data_references_function_suffix(&array.data, heap, first_function, visited)
        || array.shared_parent.as_ref().is_some_and(|parent| {
            array_ref_references_function_suffix(parent, heap, first_function, visited)
        })
}

fn remap_frame_struct_refs(frame: &mut Frame, remap: &[Option<usize>], visited: &mut RemapVisited) {
    for value in frame.locals_slots.iter_mut().flatten() {
        remap_value_struct_refs(value, remap, visited);
    }
    for value in frame.locals_any.values_mut() {
        remap_value_struct_refs(value, remap, visited);
    }
    for value in frame.captured_vars.values_mut() {
        remap_value_struct_refs(value, remap, visited);
    }
}

fn remap_root_lexical_scope_struct_refs(
    scopes: &mut [RootLexicalScope],
    remap: &[Option<usize>],
    visited: &mut RemapVisited,
) {
    for value in scopes.iter_mut().flat_map(RootLexicalScope::values_mut) {
        remap_value_struct_refs(value, remap, visited);
    }
}

fn remap_task_struct_refs(
    task: &mut builtins_tasks::VmTask,
    remap: &[Option<usize>],
    visited: &mut RemapVisited,
) {
    remap_value_struct_refs(&mut task.object, remap, visited);
    if let Some(entry) = &mut task.entry {
        remap_value_struct_refs(entry, remap, visited);
    }
    let Some(context) = &mut task.context else {
        return;
    };
    for value in &mut context.stack {
        remap_value_struct_refs(value, remap, visited);
    }
    for frame in &mut context.frames {
        remap_frame_struct_refs(frame, remap, visited);
    }
    remap_root_lexical_scope_struct_refs(&mut context.lexical_scopes, remap, visited);
    if let Some(value) = &mut context.pending_exception_value {
        remap_value_struct_refs(value, remap, visited);
    }
    for (_, value, _) in &mut context.caught_exceptions {
        if let Some(value) = value {
            remap_value_struct_refs(value, remap, visited);
        }
    }
}

fn remap_array_data_struct_refs(
    data: &mut ArrayData,
    remap: &[Option<usize>],
    visited: &mut RemapVisited,
) {
    match data {
        ArrayData::StructRefs(indices) => {
            for idx in indices {
                if let Some(new_idx) = remap.get(*idx).and_then(|entry| *entry) {
                    *idx = new_idx;
                }
            }
        }
        ArrayData::Any(values) => {
            for value in values {
                remap_value_struct_refs(value, remap, visited);
            }
        }
        _ => {}
    }
}

fn remap_array_ref_struct_refs(
    arr: &ArrayRef,
    remap: &[Option<usize>],
    visited: &mut RemapVisited,
) {
    // Dedup by Rc identity: a shared array reached twice must be remapped once.
    if !visited.insert(rc_id(arr)) {
        return;
    }
    let mut arr = arr.borrow_mut();
    remap_array_data_struct_refs(&mut arr.data, remap, visited);
    if let Some(parent) = &arr.shared_parent {
        remap_array_ref_struct_refs(parent, remap, visited);
    }
}

fn remap_memory_ref_struct_refs(
    memory: &MemoryRef,
    remap: &[Option<usize>],
    visited: &mut RemapVisited,
) {
    if !visited.insert(rc_id(memory)) {
        return;
    }
    remap_array_data_struct_refs(&mut memory.borrow_mut().data, remap, visited);
}

fn remap_value_struct_refs(value: &mut Value, remap: &[Option<usize>], visited: &mut RemapVisited) {
    match value {
        Value::StructRef(idx) => {
            if let Some(new_idx) = remap.get(*idx).and_then(|entry| *entry) {
                *idx = new_idx;
            }
        }
        Value::Struct(instance) => {
            for field in &mut instance.values {
                remap_value_struct_refs(field, remap, visited);
            }
        }
        Value::Tuple(tuple) | Value::SimpleVector(tuple) => {
            for element in &mut tuple.elements {
                remap_value_struct_refs(element, remap, visited);
            }
        }
        Value::NamedTuple(named) => {
            for element in &mut named.values {
                remap_value_struct_refs(element, remap, visited);
            }
        }
        Value::Pairs(pairs) => {
            for element in &mut pairs.data.values {
                remap_value_struct_refs(element, remap, visited);
            }
        }
        Value::Ref(inner) if visited.insert(rc_id(inner)) => {
            // `Ref` is a shared `Rc<RefCell<Value>>`; dedup like the array carriers.
            remap_value_struct_refs(&mut inner.borrow_mut(), remap, visited);
        }
        Value::Ref(_) => {}
        Value::WeakRef(inner) if visited.insert(rc_id(inner)) => {
            let mut target = inner.borrow_mut();
            if let Value::StructRef(idx) = *target {
                *target = remap
                    .get(idx)
                    .and_then(|entry| *entry)
                    .map(Value::StructRef)
                    .unwrap_or(Value::Nothing);
            } else {
                remap_value_struct_refs(&mut target, remap, visited);
            }
        }
        Value::WeakRef(_) => {}
        Value::Generator(generator) => {
            remap_generator_callable_struct_refs(&mut generator.callable, remap, visited);
            remap_value_struct_refs(&mut generator.iter, remap, visited);
        }
        Value::Expr(expr) => {
            remap_array_ref_struct_refs(&expr.args, remap, visited);
        }
        Value::QuoteNode(inner) => {
            remap_value_struct_refs(inner, remap, visited);
        }
        Value::ExprArgs(carrier) => {
            remap_array_ref_struct_refs(carrier.as_array_ref(), remap, visited);
        }
        Value::Memory(memory) => {
            remap_memory_ref_struct_refs(memory, remap, visited);
        }
        Value::MemoryRef(memory_ref) => {
            remap_memory_ref_struct_refs(&memory_ref.memory, remap, visited);
        }
        Value::Closure(closure) => {
            // `make_mut` copy-on-writes the captures map so a shared closure's
            // captures are remapped independently; the visited set still dedups
            // any array/memory shared *inside* the captured values.
            for (_, captured) in std::rc::Rc::make_mut(&mut closure.captures).iter_mut() {
                remap_value_struct_refs(captured, remap, visited);
            }
        }
        Value::ComposedFunction(composed) => {
            remap_value_struct_refs(&mut composed.outer, remap, visited);
            remap_value_struct_refs(&mut composed.inner, remap, visited);
        }
        _ => {}
    }
}

fn remap_generator_callable_struct_refs(
    callable: &mut GeneratorCallable,
    remap: &[Option<usize>],
    visited: &mut RemapVisited,
) {
    match callable {
        GeneratorCallable::RuntimeValue(value)
        | GeneratorCallable::TupleSplatRuntimeValue(value) => {
            remap_value_struct_refs(value, remap, visited);
        }
        GeneratorCallable::FilteredRuntimeValue { map, predicate } => {
            remap_value_struct_refs(map, remap, visited);
            remap_value_struct_refs(predicate, remap, visited);
        }
        _ => {}
    }
}

fn remap_persisted_candidate_indices(
    old_index: usize,
    prior_identities: &[super::repl_support::PersistedCallableIdentity],
    next_functions: &[Rc<FunctionInfo>],
    cache: &mut std::collections::HashMap<usize, Vec<usize>>,
) -> Vec<usize> {
    if let Some(mapped) = cache.get(&old_index) {
        return mapped.clone();
    }
    let mapped = prior_identities
        .get(old_index)
        .map_or_else(Vec::new, |prior| {
            next_functions
                .iter()
                .enumerate()
                .filter_map(|(index, next)| {
                    (super::repl_support::PersistedCallableIdentity::from_function(next) == *prior)
                        .then_some(index)
                })
                .collect()
        });
    cache.insert(old_index, mapped.clone());
    mapped
}

fn persisted_function_value_for_index(
    old_index: usize,
    prior_identities: &[super::repl_support::PersistedCallableIdentity],
    next_functions: &[Rc<FunctionInfo>],
    cache: &mut std::collections::HashMap<usize, Vec<usize>>,
) -> Value {
    let prior_identity = prior_identities.get(old_index);
    let name = prior_identity
        .map(|identity| identity.name().to_string())
        .unwrap_or_else(|| "<stale persisted callable>".to_string());
    let singleton_identity = prior_identity.map_or_else(
        || crate::vm::value::CallableSingletonIdentity::source(name.clone()),
        super::repl_support::PersistedCallableIdentity::singleton_identity,
    );
    let candidates =
        remap_persisted_candidate_indices(old_index, prior_identities, next_functions, cache);
    Value::Function(FunctionValue::with_candidates_and_identity(
        name,
        candidates,
        singleton_identity,
    ))
}

fn remap_persisted_callable_candidates(
    candidate_indices: &mut Option<Vec<usize>>,
    prior_identities: &[super::repl_support::PersistedCallableIdentity],
    next_functions: &[Rc<FunctionInfo>],
    cache: &mut std::collections::HashMap<usize, Vec<usize>>,
) {
    let Some(indices) = candidate_indices.as_ref() else {
        return;
    };
    let mut remapped = Vec::with_capacity(indices.len());
    for old_index in indices {
        let mapped =
            remap_persisted_candidate_indices(*old_index, prior_identities, next_functions, cache);
        if mapped.is_empty() {
            // A frozen candidate set is an authority boundary. If its stable
            // identity disappeared, fail closed rather than fall back by name.
            *candidate_indices = Some(Vec::new());
            return;
        }
        for new_index in mapped {
            if !remapped.contains(&new_index) {
                remapped.push(new_index);
            }
        }
    }
    *candidate_indices = Some(remapped);
}

pub(super) fn remap_persisted_callable_value(
    value: &mut Value,
    prior_identities: &[super::repl_support::PersistedCallableIdentity],
    next_functions: &[Rc<FunctionInfo>],
    cache: &mut std::collections::HashMap<usize, Vec<usize>>,
    visited: &mut RemapVisited,
) {
    match value {
        Value::Function(function) => {
            remap_persisted_callable_candidates(
                &mut function.candidate_indices,
                prior_identities,
                next_functions,
                cache,
            );
        }
        Value::Struct(instance) => {
            for field in &mut instance.values {
                remap_persisted_callable_value(
                    field,
                    prior_identities,
                    next_functions,
                    cache,
                    visited,
                );
            }
        }
        Value::Tuple(tuple) | Value::SimpleVector(tuple) => {
            for element in &mut tuple.elements {
                remap_persisted_callable_value(
                    element,
                    prior_identities,
                    next_functions,
                    cache,
                    visited,
                );
            }
        }
        Value::NamedTuple(named) => {
            for element in &mut named.values {
                remap_persisted_callable_value(
                    element,
                    prior_identities,
                    next_functions,
                    cache,
                    visited,
                );
            }
        }
        Value::Pairs(pairs) => {
            for element in &mut pairs.data.values {
                remap_persisted_callable_value(
                    element,
                    prior_identities,
                    next_functions,
                    cache,
                    visited,
                );
            }
        }
        Value::Ref(inner) | Value::WeakRef(inner) if visited.insert(rc_id(inner)) => {
            remap_persisted_callable_value(
                &mut inner.borrow_mut(),
                prior_identities,
                next_functions,
                cache,
                visited,
            );
        }
        Value::Ref(_) | Value::WeakRef(_) => {}
        Value::Generator(generator) => {
            let replacement = match &mut generator.callable {
                GeneratorCallable::FunctionIndex(index) => {
                    let mapped = remap_persisted_candidate_indices(
                        *index,
                        prior_identities,
                        next_functions,
                        cache,
                    );
                    match mapped.as_slice() {
                        [only] => {
                            *index = *only;
                            None
                        }
                        _ => Some(GeneratorCallable::RuntimeValue(Box::new(
                            persisted_function_value_for_index(
                                *index,
                                prior_identities,
                                next_functions,
                                cache,
                            ),
                        ))),
                    }
                }
                GeneratorCallable::TupleSplatFunctionIndex(index) => {
                    let mapped = remap_persisted_candidate_indices(
                        *index,
                        prior_identities,
                        next_functions,
                        cache,
                    );
                    match mapped.as_slice() {
                        [only] => {
                            *index = *only;
                            None
                        }
                        _ => Some(GeneratorCallable::TupleSplatRuntimeValue(Box::new(
                            persisted_function_value_for_index(
                                *index,
                                prior_identities,
                                next_functions,
                                cache,
                            ),
                        ))),
                    }
                }
                GeneratorCallable::FilteredFunctionIndex {
                    map_func_index,
                    predicate_func_index,
                } => {
                    let old_map = *map_func_index;
                    let old_predicate = *predicate_func_index;
                    let mapped_map = remap_persisted_candidate_indices(
                        old_map,
                        prior_identities,
                        next_functions,
                        cache,
                    );
                    let mapped_predicate = remap_persisted_candidate_indices(
                        old_predicate,
                        prior_identities,
                        next_functions,
                        cache,
                    );
                    match (mapped_map.as_slice(), mapped_predicate.as_slice()) {
                        ([map], [predicate]) => {
                            *map_func_index = *map;
                            *predicate_func_index = *predicate;
                            None
                        }
                        _ => Some(GeneratorCallable::FilteredRuntimeValue {
                            map: Box::new(persisted_function_value_for_index(
                                old_map,
                                prior_identities,
                                next_functions,
                                cache,
                            )),
                            predicate: Box::new(persisted_function_value_for_index(
                                old_predicate,
                                prior_identities,
                                next_functions,
                                cache,
                            )),
                        }),
                    }
                }
                GeneratorCallable::RuntimeValue(callable)
                | GeneratorCallable::TupleSplatRuntimeValue(callable) => {
                    remap_persisted_callable_value(
                        callable,
                        prior_identities,
                        next_functions,
                        cache,
                        visited,
                    );
                    None
                }
                GeneratorCallable::FilteredRuntimeValue { map, predicate } => {
                    remap_persisted_callable_value(
                        map,
                        prior_identities,
                        next_functions,
                        cache,
                        visited,
                    );
                    remap_persisted_callable_value(
                        predicate,
                        prior_identities,
                        next_functions,
                        cache,
                        visited,
                    );
                    None
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                generator.callable = replacement;
            }
            remap_persisted_callable_value(
                &mut generator.iter,
                prior_identities,
                next_functions,
                cache,
                visited,
            );
        }
        Value::Expr(expr) => remap_persisted_callable_array_ref(
            &expr.args,
            prior_identities,
            next_functions,
            cache,
            visited,
        ),
        Value::QuoteNode(inner) => {
            remap_persisted_callable_value(inner, prior_identities, next_functions, cache, visited)
        }
        Value::ExprArgs(carrier) => {
            remap_persisted_callable_array_ref(
                carrier.as_array_ref(),
                prior_identities,
                next_functions,
                cache,
                visited,
            );
        }
        Value::Memory(memory) if visited.insert(rc_id(memory)) => {
            remap_persisted_callable_array_data(
                &mut memory.borrow_mut().data,
                prior_identities,
                next_functions,
                cache,
                visited,
            );
        }
        Value::Memory(_) => {}
        Value::MemoryRef(memory_ref) if visited.insert(rc_id(&memory_ref.memory)) => {
            remap_persisted_callable_array_data(
                &mut memory_ref.memory.borrow_mut().data,
                prior_identities,
                next_functions,
                cache,
                visited,
            );
        }
        Value::MemoryRef(_) => {}
        Value::Closure(closure) => {
            remap_persisted_callable_candidates(
                &mut closure.candidate_indices,
                prior_identities,
                next_functions,
                cache,
            );
            for (_, captured) in std::rc::Rc::make_mut(&mut closure.captures).iter_mut() {
                remap_persisted_callable_value(
                    captured,
                    prior_identities,
                    next_functions,
                    cache,
                    visited,
                );
            }
        }
        Value::ComposedFunction(composed) => {
            remap_persisted_callable_value(
                &mut composed.outer,
                prior_identities,
                next_functions,
                cache,
                visited,
            );
            remap_persisted_callable_value(
                &mut composed.inner,
                prior_identities,
                next_functions,
                cache,
                visited,
            );
        }
        _ => {}
    }
}

fn remap_persisted_callable_array_ref(
    array: &ArrayRef,
    prior_identities: &[super::repl_support::PersistedCallableIdentity],
    next_functions: &[Rc<FunctionInfo>],
    cache: &mut std::collections::HashMap<usize, Vec<usize>>,
    visited: &mut RemapVisited,
) {
    if !visited.insert(rc_id(array)) {
        return;
    }
    let parent = {
        let mut array = array.borrow_mut();
        remap_persisted_callable_array_data(
            &mut array.data,
            prior_identities,
            next_functions,
            cache,
            visited,
        );
        array.shared_parent.clone()
    };
    if let Some(parent) = parent {
        remap_persisted_callable_array_ref(
            &parent,
            prior_identities,
            next_functions,
            cache,
            visited,
        );
    }
}

fn remap_persisted_callable_array_data(
    data: &mut ArrayData,
    prior_identities: &[super::repl_support::PersistedCallableIdentity],
    next_functions: &[Rc<FunctionInfo>],
    cache: &mut std::collections::HashMap<usize, Vec<usize>>,
    visited: &mut RemapVisited,
) {
    if let ArrayData::Any(values) | ArrayData::String(values) = data {
        for value in values {
            remap_persisted_callable_value(value, prior_identities, next_functions, cache, visited);
        }
    }
}

/// Alias map used to detach a transplant candidate from session-owned mutable
/// `Rc` state before rewriting its `StructRef`s (Issue #9827). One map is shared
/// across every root and reachable heap field, preserving Julia object identity
/// within the detached graph while making the operation transactional.
#[derive(Default)]
struct DetachedRcAliases {
    arrays: std::collections::HashMap<usize, ArrayRef>,
    memories: std::collections::HashMap<usize, MemoryRef>,
    value_cells: std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<Value>>>,
}

fn detach_array_data_rc_graph(data: &mut ArrayData, aliases: &mut DetachedRcAliases) {
    match data {
        ArrayData::Any(values) | ArrayData::String(values) => {
            for value in values {
                detach_value_rc_graph(value, aliases);
            }
        }
        _ => {}
    }
}

fn detach_array_ref_rc_graph(source: &ArrayRef, aliases: &mut DetachedRcAliases) -> ArrayRef {
    let id = rc_id(source);
    if let Some(detached) = aliases.arrays.get(&id) {
        return detached.clone();
    }

    // Install the shallow placeholder before walking children so cycles and
    // shared-parent diamonds resolve back to this same detached allocation.
    let snapshot = source.borrow().clone();
    let detached = std::rc::Rc::new(std::cell::RefCell::new(snapshot));
    aliases.arrays.insert(id, detached.clone());
    {
        let mut array = detached.borrow_mut();
        detach_array_data_rc_graph(&mut array.data, aliases);
        if let Some(parent) = array.shared_parent.clone() {
            array.shared_parent = Some(detach_array_ref_rc_graph(&parent, aliases));
        }
    }
    detached
}

fn detach_memory_ref_rc_graph(source: &MemoryRef, aliases: &mut DetachedRcAliases) -> MemoryRef {
    let id = rc_id(source);
    if let Some(detached) = aliases.memories.get(&id) {
        return detached.clone();
    }

    let snapshot = source.borrow().clone();
    let detached = std::rc::Rc::new(std::cell::RefCell::new(snapshot));
    aliases.memories.insert(id, detached.clone());
    detach_array_data_rc_graph(&mut detached.borrow_mut().data, aliases);
    detached
}

fn detach_value_cell_rc_graph(
    source: &std::rc::Rc<std::cell::RefCell<Value>>,
    aliases: &mut DetachedRcAliases,
) -> std::rc::Rc<std::cell::RefCell<Value>> {
    let id = rc_id(source);
    if let Some(detached) = aliases.value_cells.get(&id) {
        return detached.clone();
    }

    let detached = std::rc::Rc::new(std::cell::RefCell::new(Value::Nothing));
    aliases.value_cells.insert(id, detached.clone());
    let mut target = source.borrow().clone();
    detach_value_rc_graph(&mut target, aliases);
    *detached.borrow_mut() = target;
    detached
}

fn detach_generator_callable_rc_graph(
    callable: &mut GeneratorCallable,
    aliases: &mut DetachedRcAliases,
) {
    match callable {
        GeneratorCallable::RuntimeValue(value)
        | GeneratorCallable::TupleSplatRuntimeValue(value) => {
            detach_value_rc_graph(value, aliases);
        }
        GeneratorCallable::FilteredRuntimeValue { map, predicate } => {
            detach_value_rc_graph(map, aliases);
            detach_value_rc_graph(predicate, aliases);
        }
        _ => {}
    }
}

fn detach_value_rc_graph(value: &mut Value, aliases: &mut DetachedRcAliases) {
    match value {
        Value::Struct(instance) => {
            for field in &mut instance.values {
                detach_value_rc_graph(field, aliases);
            }
        }
        Value::Tuple(tuple) | Value::SimpleVector(tuple) => {
            for element in &mut tuple.elements {
                detach_value_rc_graph(element, aliases);
            }
        }
        Value::NamedTuple(named) => {
            for element in &mut named.values {
                detach_value_rc_graph(element, aliases);
            }
        }
        Value::Pairs(pairs) => {
            for element in &mut pairs.data.values {
                detach_value_rc_graph(element, aliases);
            }
        }
        Value::Ref(cell) | Value::WeakRef(cell) => {
            *cell = detach_value_cell_rc_graph(cell, aliases);
        }
        Value::Generator(generator) => {
            detach_generator_callable_rc_graph(&mut generator.callable, aliases);
            detach_value_rc_graph(&mut generator.iter, aliases);
        }
        Value::Expr(expr) => {
            expr.args = detach_array_ref_rc_graph(&expr.args, aliases);
        }
        Value::QuoteNode(inner) => detach_value_rc_graph(inner, aliases),
        Value::ExprArgs(carrier) => {
            let detached = detach_array_ref_rc_graph(carrier.as_array_ref(), aliases);
            *value = native_array_ref_value(detached);
        }
        Value::Memory(memory) => {
            *memory = detach_memory_ref_rc_graph(memory, aliases);
        }
        Value::MemoryRef(memory_ref) => {
            memory_ref.memory = detach_memory_ref_rc_graph(&memory_ref.memory, aliases);
        }
        Value::Closure(closure) => {
            for (_, captured) in std::rc::Rc::make_mut(&mut closure.captures).iter_mut() {
                detach_value_rc_graph(captured, aliases);
            }
        }
        Value::ComposedFunction(composed) => {
            detach_value_rc_graph(&mut composed.outer, aliases);
            detach_value_rc_graph(&mut composed.inner, aliases);
        }
        _ => {}
    }
}

/// Reachable-only compaction of an externally held struct heap against a set of
/// root `Value`s (Issue #9787).
///
/// This is the cross-eval analogue of [`Vm::compact_struct_heap_at_safe_point`]:
/// that pass marks from the VM's own frame/stack roots, whereas here the roots
/// are values held OUTSIDE any VM — the REPL persistent-eval seed globals that
/// are about to be transplanted into a fresh VM. It marks every `StructRef`
/// transitively reachable from `roots` (following nested struct fields,
/// arrays/dicts/tuples/named-tuples, closure captures, `Ref`, generators, … via
/// the shared [`mark_value_struct_refs`] walker), builds a dense old→new index
/// remap for the reachable structs, and returns a compacted heap containing ONLY
/// those structs, with every `StructRef` in the retained structs' own fields AND
/// in `roots` rewritten to the new indices.
///
/// Rationale: the persistent transplant path used to copy the WHOLE prior heap
/// into each fresh VM regardless of what the carried globals actually reference,
/// so dead structs from every prior eval accumulated without bound (Issue #9787,
/// violating the #8625 boundedness guarantee). Only the seed globals reference
/// the transplanted region (reconstructed globals rebuild self-contained
/// literals into fresh indices), so `roots = seed globals` is a complete root
/// set: any struct not reachable from them is unreachable after the transplant
/// and is safely reclaimed. The work is O(reachable structs + roots), never
/// O(session length) — which keeps the heap bounded across a long session.
pub(crate) fn reachable_compacted_struct_heap(
    prior_heap: &[StructInstance],
    roots: &mut [(String, Value)],
) -> (Vec<StructInstance>, Vec<Option<usize>>) {
    if prior_heap.is_empty() {
        // `Value::clone` is shallow for mutable Rc-backed carriers even when no
        // StructRef exists. A subsequent callable-candidate rebase mutates those
        // roots before `vm.run`; detach them transactionally here so a failed
        // rebuild cannot corrupt the session-owned globals. One alias map across
        // all roots preserves identity between two globals sharing the same Rc
        // (Issue #9784 / #9827).
        let mut aliases = DetachedRcAliases::default();
        for (_, value) in roots.iter_mut() {
            detach_value_rc_graph(value, &mut aliases);
        }
        return (Vec::new(), Vec::new());
    }

    // Mark: every heap index transitively reachable from a root value.
    let mut live = std::collections::HashSet::new();
    let mut visited = MarkVisited::new();
    for (_, value) in roots.iter() {
        mark_value_struct_refs(value, prior_heap, &mut live, &mut visited);
    }

    // Build the dense old→new remap and the compacted heap in original order
    // (order preservation keeps the transplant deterministic across evals).
    let mut remap = vec![None; prior_heap.len()];
    let mut compacted = Vec::with_capacity(live.len());
    for (old_idx, instance) in prior_heap.iter().enumerate() {
        if live.contains(&old_idx) {
            remap[old_idx] = Some(compacted.len());
            compacted.push(instance.clone());
        }
    }

    // `Value::clone` is shallow for mutable Rc-backed carriers. Detach them as
    // one alias-preserving graph before remapping so the candidate transplant is
    // independent of `self.globals` / `last_struct_heap`. If `vm.run()` errors,
    // dropping this candidate leaves the session byte-for-byte unchanged.
    let mut aliases = DetachedRcAliases::default();
    for instance in &mut compacted {
        for value in &mut instance.values {
            detach_value_rc_graph(value, &mut aliases);
        }
    }
    for (_, value) in roots.iter_mut() {
        detach_value_rc_graph(value, &mut aliases);
    }

    // Rewrite every retained struct's own fields, then every root, to new indices.
    // One shared visited set across fields AND roots so an `Rc` array/memory/`Ref`
    // aliased between a root and a retained field (or between two roots) is
    // remapped exactly once (Issue #9787 — codex-flagged double-remap class).
    let mut visited = RemapVisited::new();
    for instance in &mut compacted {
        for value in &mut instance.values {
            remap_value_struct_refs(value, &remap, &mut visited);
        }
    }
    for (_, value) in roots.iter_mut() {
        remap_value_struct_refs(value, &remap, &mut visited);
    }

    (compacted, remap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn struct_instance(name: &str, value: i64) -> StructInstance {
        StructInstance::with_name(0, name.to_string(), vec![Value::I64(value)])
    }

    include!("../../tests/internal/empty_heap_transplant_9784_test.rs");

    #[test]
    fn transient_root_keeps_and_remaps_struct_graph_11372() -> Result<(), VmError> {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_heap = vec![
            struct_instance("Dead", 0),
            StructInstance::with_name(0, "Live".to_string(), vec![Value::StructRef(2)]),
            struct_instance("Child", 7),
        ];

        vm.with_transient_root_frame(|vm| -> Result<(), VmError> {
            let live = vm.push_transient_root(Value::StructRef(1))?;
            let stats = vm.compact_struct_heap_for_explicit_gc();
            assert_eq!(stats.reclaimed, 1);
            assert!(matches!(
                vm.clone_transient_root(live),
                Ok(Value::StructRef(0))
            ));
            assert!(matches!(vm.struct_heap[0].values[0], Value::StructRef(1)));
            Ok(())
        })?;

        assert!(vm.transient_roots.is_empty());
        Ok(())
    }

    #[test]
    fn transient_root_shares_one_remap_visited_set_with_stack_11372() -> Result<(), VmError> {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_heap = vec![
            struct_instance("Dead0", 0),
            struct_instance("Live1", 1),
            struct_instance("Dead2", 2),
            struct_instance("Live3", 3),
        ];
        let shared = Rc::new(RefCell::new(Value::StructRef(3)));
        vm.stack.push(Value::StructRef(1));
        vm.stack.push(Value::Ref(shared.clone()));

        vm.with_transient_root_frame(|vm| -> Result<(), VmError> {
            vm.push_transient_root(Value::Ref(shared.clone()))?;
            vm.compact_struct_heap_for_explicit_gc();
            assert!(matches!(*shared.borrow(), Value::StructRef(1)));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn transient_root_participates_in_weak_liveness_11372() -> Result<(), VmError> {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_heap = vec![struct_instance("Dead", 0), struct_instance("Live", 1)];
        let weak_target = Rc::new(RefCell::new(Value::StructRef(1)));
        vm.weak_refs.push(Rc::downgrade(&weak_target));

        vm.with_transient_root_frame(|vm| -> Result<(), VmError> {
            vm.push_transient_root(Value::StructRef(1))?;
            vm.compact_struct_heap_for_explicit_gc();
            vm.clear_weak_refs_without_stack_roots();
            assert!(matches!(*weak_target.borrow(), Value::StructRef(0)));
            Ok(())
        })?;

        vm.clear_weak_refs_without_stack_roots();
        assert!(matches!(*weak_target.borrow(), Value::Nothing));
        Ok(())
    }

    #[test]
    fn weak_registry_and_transient_alias_remap_once_11378() -> Result<(), VmError> {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_heap = vec![
            struct_instance("Dead0", 0),
            struct_instance("Live1", 1),
            struct_instance("Dead2", 2),
            struct_instance("Live3", 3),
        ];
        let shared = Rc::new(RefCell::new(Value::StructRef(3)));
        vm.weak_refs.push(Rc::downgrade(&shared));

        vm.with_transient_root_frame(|vm| -> Result<(), VmError> {
            vm.push_transient_root(Value::StructRef(1))?;
            vm.push_transient_root(Value::StructRef(3))?;
            vm.push_transient_root(Value::WeakRef(shared.clone()))?;
            vm.compact_struct_heap_for_explicit_gc();
            assert!(
                matches!(*shared.borrow(), Value::StructRef(1)),
                "weak cell was remapped twice through the non-idempotent 3→1→0 map"
            );
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn finalizer_snapshot_and_heap_alias_remap_once_11378() -> Result<(), VmError> {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let shared = Rc::new(RefCell::new(Value::StructRef(3)));
        vm.struct_heap = vec![
            struct_instance("Dead0", 0),
            struct_instance("Live1", 1),
            struct_instance("Dead2", 2),
            StructInstance::with_name(
                0,
                "Finalized3".to_string(),
                vec![Value::Ref(shared.clone())],
            ),
        ];
        let registered = vm.register_finalizer(
            Value::Function(FunctionValue::new("finalizer_11378")),
            Value::StructRef(3),
        );
        assert!(registered.is_ok());

        vm.with_transient_root_frame(|vm| -> Result<(), VmError> {
            vm.push_transient_root(Value::StructRef(1))?;
            vm.push_transient_root(Value::StructRef(3))?;
            vm.compact_struct_heap_for_explicit_gc();
            assert!(
                matches!(*shared.borrow(), Value::StructRef(1)),
                "finalizer snapshot remapped a shared Ref twice"
            );
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn nested_transient_root_frames_cleanup_ready_and_error_results_11372() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));

        let result: Result<(), VmError> = vm.with_transient_root_frame(|vm| {
            vm.push_transient_root(Value::I64(1))?;
            let outer_depth = vm.transient_roots.len();
            let inner: Result<(), VmError> = vm.with_transient_root_frame(|vm| {
                vm.push_transient_root(Value::I64(2))?;
                Err(VmError::TypeError("inner".to_string()))
            });
            assert!(inner.is_err());
            assert_eq!(vm.transient_roots.len(), outer_depth);
            Ok(())
        });

        assert!(result.is_ok());
        assert!(vm.transient_roots.is_empty());
    }

    #[test]
    fn stale_transient_root_id_fails_after_slot_reuse_11372() -> Result<(), VmError> {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let stale = vm.with_transient_root_frame(|vm| vm.push_transient_root(Value::I64(1)))?;
        vm.with_transient_root_frame(|vm| -> Result<(), VmError> {
            let current = vm.push_transient_root(Value::I64(2))?;
            assert_eq!(stale.index, current.index, "test must reuse the slot");
            assert_ne!(stale.generation, current.generation);
            assert!(matches!(
                vm.clone_transient_root(stale),
                Err(VmError::InternalError(message)) if message.contains("stale transient GC root")
            ));
            assert!(matches!(
                vm.clone_transient_root(current),
                Ok(Value::I64(2))
            ));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn exhausted_transient_root_generation_returns_internal_error_11372() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.next_transient_root_generation = u64::MAX;

        let result = vm.push_transient_root(Value::I64(1));

        assert!(
            matches!(result, Err(VmError::InternalError(message))
                if message.contains("transient GC root generation exhausted")),
            "generation exhaustion must be reported without panicking"
        );
        assert!(vm.transient_roots.is_empty());
    }

    /// Reachable-only transplant compaction (Issue #9787) must keep every live
    /// `StructRef` valid while densely dropping unreachable structs: it marks
    /// through inline-struct-field roots AND nested heap-struct fields, drops
    /// structs no root reaches (even a dead struct that itself points into the
    /// heap), and rewrites every surviving `StructRef` — in the roots and in the
    /// retained structs' own fields — to the new dense indices. A missed
    /// reference would be a dangling `StructRef` (heap corruption).
    #[test]
    fn reachable_compaction_remaps_live_refs_and_drops_unreachable_9787() {
        // heap: 0 "Live"→field StructRef(2); 1 "Dead"; 2 "Leaf"→I64(99);
        //       3 "Dead2"→field StructRef(1) (unreachable: nothing points to 3).
        let prior_heap = vec![
            StructInstance::with_name(0, "Live".to_string(), vec![Value::StructRef(2)]),
            StructInstance::with_name(0, "Dead".to_string(), vec![Value::I64(7)]),
            StructInstance::with_name(0, "Leaf".to_string(), vec![Value::I64(99)]),
            StructInstance::with_name(0, "Dead2".to_string(), vec![Value::StructRef(1)]),
        ];
        // Two roots that both reach struct 0: a direct StructRef and an inline
        // struct whose field is a StructRef. Neither reaches 1 or 3.
        let mut roots = vec![
            ("a".to_string(), Value::StructRef(0)),
            (
                "b".to_string(),
                Value::Struct(StructInstance::with_name(
                    0,
                    "Wrap".to_string(),
                    vec![Value::StructRef(0)],
                )),
            ),
        ];

        let (compacted, remap) = reachable_compacted_struct_heap(&prior_heap, &mut roots);

        // The remap must map the two live old indices densely and drop the rest.
        assert_eq!(remap[0], Some(0));
        assert_eq!(remap[2], Some(1));
        assert_eq!(remap[1], None);
        assert_eq!(remap[3], None);
        // Only the reachable set {0, 2} survives, densely: Live→0, Leaf→1.
        assert_eq!(compacted.len(), 2, "unreachable structs must be reclaimed");
        assert_eq!(&*compacted[0].struct_name, "Live");
        assert_eq!(&*compacted[1].struct_name, "Leaf");
        // Live's field pointed at old index 2 → must be remapped to new index 1.
        assert!(
            matches!(compacted[0].values[0], Value::StructRef(1)),
            "retained struct field must be remapped, got {:?}",
            compacted[0].values[0]
        );
        assert!(matches!(compacted[1].values[0], Value::I64(99)));
        // Roots must be remapped: both point at old index 0 → new index 0.
        assert!(matches!(roots[0].1, Value::StructRef(0)));
        match &roots[1].1 {
            Value::Struct(s) => {
                assert!(
                    matches!(s.values[0], Value::StructRef(0)),
                    "inline-struct root field must be remapped, got {:?}",
                    s.values[0]
                );
            }
            other => panic!("root b must stay an inline struct, got {other:?}"),
        }
    }

    /// A root that reaches nothing on the heap yields an empty compacted heap —
    /// the exact bug case (Issue #9787): the persistent seed globals were scalars
    /// (`s`, `ans`) that reference no struct, yet the whole accumulated heap was
    /// transplanted. Reachable-only compaction drops all of it.
    #[test]
    fn reachable_compaction_scalar_roots_reclaim_entire_heap_9787() {
        let prior_heap = vec![
            struct_instance("Counter", 0),
            struct_instance("Counter", 1),
            struct_instance("Counter", 2),
        ];
        let mut roots = vec![
            ("s".to_string(), Value::I64(210)),
            ("ans".to_string(), Value::I64(210)),
        ];
        let (compacted, remap) = reachable_compacted_struct_heap(&prior_heap, &mut roots);
        assert!(
            compacted.is_empty(),
            "scalar-only roots reference no struct; the whole heap is dead weight"
        );
        assert!(
            remap.iter().all(|r| r.is_none()),
            "every struct is reclaimed"
        );
    }

    /// `remap_value_struct_refs` must rewrite a SHARED `Rc` container exactly once
    /// across a whole pass (Issue #9787 — the codex-flagged double-remap class,
    /// which the in-VM safe-point GC hits when two frame slots alias one array).
    /// Two `Value::Ref`s alias the same `Rc<RefCell<Value>>` holding `StructRef(3)`;
    /// with `remap[3]=1`, `remap[1]=0`, a non-idempotent double visit would rewrite
    /// `3→1` then `1→0`. The `Rc::as_ptr` visited set keeps it at `StructRef(1)`.
    #[test]
    fn remap_value_struct_refs_dedups_shared_rc_9787() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let remap = vec![None, Some(0usize), None, Some(1usize)]; // 1→0, 3→1
        let shared = Rc::new(RefCell::new(Value::StructRef(3)));
        let mut a = Value::Ref(shared.clone());
        let mut b = Value::Ref(shared.clone());
        let mut visited = RemapVisited::new();
        remap_value_struct_refs(&mut a, &remap, &mut visited);
        // `b` aliases the SAME Rc; the shared visited set must skip it.
        remap_value_struct_refs(&mut b, &remap, &mut visited);
        assert!(
            matches!(*shared.borrow(), Value::StructRef(1)),
            "shared Rc double-remapped: {:?}",
            *shared.borrow()
        );
    }

    /// Cross-eval compaction must detach shared mutable containers from session
    /// state while preserving aliasing within the detached graph (Issue #9827).
    /// Both roots alias one `Ref`; the compacted roots must alias one NEW `Rc`,
    /// remapped exactly once, while the session-owned source stays untouched.
    #[test]
    fn reachable_compaction_detaches_and_preserves_shared_rc_aliases_9827() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let prior_heap = vec![
            struct_instance("A", 0),
            struct_instance("B", 1),
            StructInstance::with_name(0, "C".to_string(), vec![Value::StructRef(1)]), // 2
        ];
        let shared = Rc::new(RefCell::new(Value::StructRef(2)));
        let mut roots = vec![
            ("r".to_string(), Value::Ref(shared.clone())),
            ("alias".to_string(), Value::Ref(shared.clone())),
        ];

        let (compacted, remap) = reachable_compacted_struct_heap(&prior_heap, &mut roots);

        assert_eq!(remap, vec![None, Some(0), Some(1)]);
        assert_eq!(compacted.len(), 2, "dead struct A must be reclaimed");
        assert!(
            matches!(*shared.borrow(), Value::StructRef(2)),
            "the session-owned Rc must remain untouched: {:?}",
            *shared.borrow()
        );
        let (Value::Ref(detached), Value::Ref(detached_alias)) = (&roots[0].1, &roots[1].1) else {
            panic!("both roots must remain Ref values")
        };
        assert!(
            Rc::ptr_eq(detached, detached_alias),
            "the detached roots must preserve their shared identity"
        );
        assert!(
            !Rc::ptr_eq(detached, &shared),
            "the compacted graph must not alias session-owned state"
        );
        assert!(
            matches!(*detached.borrow(), Value::StructRef(1)),
            "the detached shared Rc must be remapped exactly once: {:?}",
            *detached.borrow()
        );
    }

    /// Marking runs before the alias-preserving detach, so it must terminate on
    /// cycles in the session-owned `Rc` graph while still discovering heap refs
    /// adjacent to the back-edge (Issue #9827).
    #[test]
    fn reachable_compaction_marks_cyclic_ref_carrier_9827() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let prior_heap = vec![struct_instance("Dead", 0), struct_instance("Live", 1)];
        let cycle = Rc::new(RefCell::new(Value::Nothing));
        *cycle.borrow_mut() = Value::Tuple(TupleValue::new(vec![
            Value::StructRef(1),
            Value::Ref(cycle.clone()),
        ]));
        let mut roots = vec![("cycle".to_string(), Value::Ref(cycle.clone()))];

        let (compacted, remap) = reachable_compacted_struct_heap(&prior_heap, &mut roots);

        assert_eq!(remap, vec![None, Some(0)]);
        assert_eq!(compacted.len(), 1, "the adjacent live ref must be marked");
        let Value::Ref(detached) = &roots[0].1 else {
            panic!("cyclic root must remain a Ref")
        };
        assert!(!Rc::ptr_eq(detached, &cycle));
        let target = detached.borrow();
        let Value::Tuple(tuple) = &*target else {
            panic!("cyclic Ref target must remain a tuple")
        };
        assert!(matches!(tuple.elements[0], Value::StructRef(0)));
        let Value::Ref(back_edge) = &tuple.elements[1] else {
            panic!("tuple must retain its Ref back-edge")
        };
        assert!(
            Rc::ptr_eq(detached, back_edge),
            "self-cycle identity was lost"
        );
    }

    #[test]
    fn reachable_compaction_does_not_mark_through_weak_ref_9827() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let prior_heap = vec![struct_instance("WeakOnly", 1)];
        let target = Rc::new(RefCell::new(Value::StructRef(0)));
        let mut roots = vec![("weak".to_string(), Value::WeakRef(target.clone()))];

        let (compacted, remap) = reachable_compacted_struct_heap(&prior_heap, &mut roots);

        assert!(
            compacted.is_empty(),
            "a weak edge must not retain its target"
        );
        assert_eq!(remap, vec![None]);
        assert!(matches!(*target.borrow(), Value::StructRef(0)));
        let Value::WeakRef(detached) = &roots[0].1 else {
            panic!("root must remain a WeakRef")
        };
        assert!(matches!(*detached.borrow(), Value::Nothing));
    }

    #[test]
    fn memory_stats_report_struct_heap_and_cache_growth_issue_8453() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_heap.push(struct_instance("Box", 1));
        vm.dispatch_cache
            .entry(3)
            .or_default()
            .insert(CallSiteArgIds::from_slice(&[ConcreteTypeId(11)]), 7);
        vm.binary_both_dispatch_cache
            .entry(4)
            .or_default()
            .insert((12, 13), Some(8));
        vm.method_dispatch_cache.insert(
            MethodDispatchKey {
                names: vec![1],
                arg_types: vec![2],
            },
            Some(9),
        );
        vm.specialization_cache.insert(
            SpecializationKey {
                func_index: 1,
                arg_types: vec![ValueType::I64],
            },
            SpecializedCode {
                entry: 42,
                return_type: ValueType::I64,
                code_len: 1,
                local_slot_count: 0,
            },
        );

        let stats = vm.memory_stats();

        assert_eq!(stats.struct_heap_len, 1);
        assert!(stats.struct_heap_capacity >= 1);
        assert_eq!(stats.dispatch_cache_entries, 1);
        assert_eq!(stats.binary_both_dispatch_cache_entries, 1);
        assert_eq!(stats.method_dispatch_cache_entries, 1);
        assert_eq!(stats.specialization_cache_entries, 1);
    }

    #[test]
    fn runtime_cache_limits_bound_memory_stats_issue_8610() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));

        for i in 0..=RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT {
            vm.dispatch_cache
                .entry(i)
                .or_default()
                .insert(CallSiteArgIds::from_slice(&[ConcreteTypeId(i as u32)]), i);
            vm.binary_both_dispatch_cache
                .entry(i)
                .or_default()
                .insert((i as u64, i as u64 + 1), Some(i));
            vm.method_dispatch_cache.insert(
                MethodDispatchKey {
                    names: vec![i as u64],
                    arg_types: vec![i as u64 + 1],
                },
                Some(i),
            );
        }
        for i in 0..=RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT {
            vm.specialization_cache.insert(
                SpecializationKey {
                    func_index: i,
                    arg_types: vec![ValueType::I64],
                },
                SpecializedCode {
                    entry: i,
                    return_type: ValueType::I64,
                    code_len: 1,
                    local_slot_count: 0,
                },
            );
            vm.specialization_i64_cache.insert(
                (i, 1),
                I64SpecDispatch {
                    entry: i,
                    code_end: i + 1,
                    fallback_index: i,
                    local_slot_count: 0,
                    param_slots: std::rc::Rc::from(Vec::<usize>::new()),
                },
            );
            vm.i64_function_cache.insert(i, None);
            vm.f64_function_cache.insert(i, None);
            vm.generated_expr_cache
                .insert((i, vec![format!("T{i}")]), Value::Nothing);
        }

        vm.enforce_runtime_cache_limits();
        let stats = vm.memory_stats();

        assert!(stats.dispatch_cache_entries <= RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT);
        assert!(stats.binary_both_dispatch_cache_entries <= RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT);
        assert!(stats.method_dispatch_cache_entries <= RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT);
        assert!(stats.specialization_cache_entries <= RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT);
        assert!(stats.specialization_i64_cache_entries <= RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT);
        assert!(stats.i64_function_cache_entries <= RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT);
        assert!(stats.f64_function_cache_entries <= RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT);
        assert!(stats.generated_expr_cache_entries <= RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT);

        // Issue #8625: the clears are now observable. Every cache overflowed
        // its cap by one, so all 8 capped caches fired exactly one clear.
        assert_eq!(stats.cache_clears, 8);
        assert!(stats.cache_cleared_entries >= 8);
        assert_eq!(
            stats.dispatch_cache_entry_limit,
            RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT
        );
        assert_eq!(
            stats.specialization_cache_entry_limit,
            RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT
        );
    }

    /// Issue #9197 S3: the L2 dispatch cache keys on interned `ConcreteTypeId`
    /// sequences, so distinct argument shapes — including a 1-arg vs 2-arg key
    /// at the *same* call site — are distinct entries and can never collide the
    /// way the pre-S3 unverified type-name hash could.
    #[test]
    fn l2_dispatch_cache_id_keys_do_not_collide_issue_9197_s3() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let ip = 7;
        let shape_a: [ConcreteTypeId; 1] = [ConcreteTypeId(10)];
        let shape_b: [ConcreteTypeId; 1] = [ConcreteTypeId(11)];
        let two_arg: [ConcreteTypeId; 2] = [ConcreteTypeId(10), ConcreteTypeId(11)];

        vm.store_call_site_dispatch_cache(ip, &shape_a, 100);
        vm.store_call_site_dispatch_cache(ip, &shape_b, 200);
        vm.store_call_site_dispatch_cache(ip, &two_arg, 300);

        assert_eq!(vm.lookup_call_site_dispatch_cache(ip, &shape_a), Some(100));
        assert_eq!(vm.lookup_call_site_dispatch_cache(ip, &shape_b), Some(200));
        assert_eq!(vm.lookup_call_site_dispatch_cache(ip, &two_arg), Some(300));
        // A shape never stored is a deterministic miss — no probabilistic hash hit.
        assert_eq!(
            vm.lookup_call_site_dispatch_cache(ip, &[ConcreteTypeId(99)]),
            None
        );
        // Re-storing the same key overwrites in place (one entry per shape).
        vm.store_call_site_dispatch_cache(ip, &shape_a, 101);
        assert_eq!(vm.lookup_call_site_dispatch_cache(ip, &shape_a), Some(101));
        assert_eq!(vm.dispatch_cache.get(&ip).map(HashMap::len), Some(3));
    }

    /// Issue #9197 S3: on overflow the L2 dispatch cache evicts only the excess
    /// (bounded eviction) instead of clearing every entry (the pre-S3 cliff).
    /// Survivors remain across an overflow, and the total returns to the cap.
    #[test]
    fn l2_dispatch_cache_overflow_evicts_bounded_not_wholesale_issue_9197_s3() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let limit = 8usize;
        vm.set_cache_entry_limits(Some(limit), None);

        // One entry per distinct call site, overflowing the cap several times
        // (each store enforces the limit, as the real insert sites do).
        for i in 0..(limit + 4) {
            vm.store_call_site_dispatch_cache(i, &[ConcreteTypeId(i as u32)], i);
        }

        let entries: usize = vm.dispatch_cache.values().map(HashMap::len).sum();
        // Bounded to the cap exactly — NOT a wholesale clear (which would be 0).
        assert_eq!(entries, limit);
        assert!(
            entries > 0,
            "bounded eviction must not wipe the whole cache"
        );
        // Eviction events were observed (Issue #8625 counters still fire).
        assert!(vm.memory_stats().cache_clears >= 1);
    }

    /// Issue #8603: the negative specialization cache is bounded by the same
    /// cap as the positive cache, and is retired on method-table mutations
    /// (a new method/struct definition can turn a failure into a success) and
    /// by `clear_runtime_caches`.
    #[test]
    fn specialization_failure_cache_is_bounded_and_invalidated_issue_8603() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));

        // Overflow by one -> enforce clears the set (same policy as the
        // positive specialization cache).
        for i in 0..=RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT {
            vm.specialization_failure_cache.insert(SpecializationKey {
                func_index: i,
                arg_types: vec![ValueType::BigFloat],
            });
        }
        vm.enforce_specialization_failure_cache_limit();
        assert!(vm.specialization_failure_cache.is_empty());

        let key = SpecializationKey {
            func_index: 1,
            arg_types: vec![ValueType::BigFloat],
        };
        vm.specialization_failure_cache.insert(key.clone());
        vm.note_method_table_mutation();
        assert!(
            vm.specialization_failure_cache.is_empty(),
            "method-table mutation must retire negative specialization entries"
        );

        vm.specialization_failure_cache.insert(key);
        vm.clear_runtime_caches();
        assert!(vm.specialization_failure_cache.is_empty());
    }

    /// Issue #8625: a host can lower the cache caps, and the low cap then
    /// bounds `memory_stats()` and fires observable clears far below the
    /// built-in default.
    #[test]
    fn configurable_cache_entry_limits_bound_memory_stats_issue_8625() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.set_cache_entry_limits(Some(8), Some(8));
        assert_eq!(vm.memory_stats().dispatch_cache_entry_limit, 8);
        assert_eq!(vm.memory_stats().specialization_cache_entry_limit, 8);

        // Simulate a long session: repeatedly add distinct entries and enforce
        // the caps after each batch, as the real insert sites do.
        for round in 0..100 {
            for k in 0..4 {
                let key = round * 4 + k;
                vm.dispatch_cache.entry(key).or_default().insert(
                    CallSiteArgIds::from_slice(&[ConcreteTypeId(key as u32)]),
                    key,
                );
                vm.method_dispatch_cache.insert(
                    MethodDispatchKey {
                        names: vec![key as u64],
                        arg_types: vec![key as u64],
                    },
                    Some(key),
                );
            }
            vm.enforce_dispatch_cache_limit();
            vm.enforce_method_dispatch_cache_limit();
        }

        let stats = vm.memory_stats();
        // Bounded well under the 4096 default by the injected cap of 8.
        assert!(stats.dispatch_cache_entries <= 8 + 4);
        assert!(stats.method_dispatch_cache_entries <= 8 + 4);
        // Clears actually fired and are counted.
        assert!(stats.cache_clears >= 2);
        assert!(stats.cache_cleared_entries >= 16);

        // Restoring defaults widens the cap again.
        vm.set_cache_entry_limits(None, None);
        assert_eq!(
            vm.memory_stats().dispatch_cache_entry_limit,
            RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT
        );
    }

    #[test]
    fn safe_point_compaction_reclaims_dead_struct_heap_entries_issue_8453() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.frames[0].locals_slots.resize(2, None);
        vm.struct_heap.extend([
            struct_instance("Box", 10),
            struct_instance("Box", 20),
            struct_instance("Box", 30),
        ]);
        vm.frames[0].locals_slots[0] = Some(Value::StructRef(2));
        vm.frames[0]
            .locals_any
            .insert("kept".to_string(), Value::StructRef(0));
        vm.frames[0]
            .var_types
            .insert("kept".to_string(), VarTypeTag::Struct);

        let result = vm.compact_struct_heap_at_safe_point();

        assert_eq!(result.before_len, 3);
        assert_eq!(result.after_len, 2);
        assert_eq!(result.reclaimed, 1);
        assert_eq!(vm.get_struct_heap().len(), 2);
        assert!(matches!(
            vm.frames[0].locals_slots[0],
            Some(Value::StructRef(1))
        ));
        assert!(matches!(
            vm.frames[0].locals_any.get("kept"),
            Some(Value::StructRef(0))
        ));
        assert!(matches!(
            vm.struct_heap[0].values.as_slice(),
            [Value::I64(10)]
        ));
        assert!(matches!(
            vm.struct_heap[1].values.as_slice(),
            [Value::I64(30)]
        ));
    }

    #[test]
    fn safe_point_compaction_skips_when_non_top_level_frames_are_live_issue_8453() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_heap
            .extend([struct_instance("Box", 10), struct_instance("Box", 20)]);
        vm.frames.push(Frame::new());

        let result = vm.compact_struct_heap_at_safe_point();

        assert_eq!(result.before_len, 2);
        assert_eq!(result.after_len, 2);
        assert_eq!(result.reclaimed, 0);
        assert!(!result.compacted);
    }
}
