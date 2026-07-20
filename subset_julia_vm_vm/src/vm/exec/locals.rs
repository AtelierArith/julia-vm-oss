//! Local variable load/store operations.
//!
//! This module handles Load*/Store* instructions for local variables,
//! as well as fused load+arithmetic operations.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;
use crate::types::JuliaType;

use super::super::error::VmError;
use super::super::frame::{Frame, VarTypeTag};
use super::super::instr::Instr;
use super::super::repl_support::PersistedCallableSnapshot;
use super::super::stack_ops::StackOps;
use super::super::value::{native_array_ref_from_value, native_array_ref_value, Value};
use super::super::Vm;
use super::DispatchAction;

/// Return the destination slot for every local-slot store opcode.
///
/// Keyword-default entry assertions must run before the opcode-specific typed
/// pop/conversion. Keeping the inventory here prevents slotization from
/// bypassing the assertion when a generic `StoreSlot` becomes `StoreSlotI64`,
/// `StoreSlotStr`, or another specialized store (Issue #11135).
fn stored_slot(instr: &Instr) -> Option<usize> {
    match instr {
        Instr::StoreSlot(slot)
        | Instr::StoreSlotI64(slot)
        | Instr::StoreSlotF64(slot)
        | Instr::StoreSlotBool(slot)
        | Instr::StoreSlotF32(slot)
        | Instr::StoreSlotF16(slot)
        | Instr::StoreSlotStr(slot)
        | Instr::StoreSlotChar(slot)
        | Instr::StoreSlotNarrowInt(slot)
        | Instr::StoreSlotNothing(slot)
        | Instr::StoreSlotArray(slot)
        | Instr::StoreSlotTuple(slot)
        | Instr::StoreSlotNamedTuple(slot)
        | Instr::StoreSlotDict(slot)
        | Instr::StoreSlotSet(slot)
        | Instr::StoreSlotStruct(slot)
        | Instr::StoreSlotRange(slot)
        | Instr::StoreSlotRng(slot)
        | Instr::StoreSlotGenerator(slot)
        | Instr::StoreSlotSymbol(slot) => Some(*slot),
        _ => None,
    }
}

fn value_as_i128(value: &Value) -> Option<i128> {
    match value {
        Value::I8(v) => Some(*v as i128),
        Value::I16(v) => Some(*v as i128),
        Value::I32(v) => Some(*v as i128),
        Value::I64(v) => Some(*v as i128),
        Value::I128(v) => Some(*v),
        Value::U8(v) => Some(*v as i128),
        Value::U16(v) => Some(*v as i128),
        Value::U32(v) => Some(*v as i128),
        Value::U64(v) => Some(*v as i128),
        Value::U128(v) => i128::try_from(*v).ok(),
        Value::Bool(v) => Some(if *v { 1 } else { 0 }),
        _ => None,
    }
}

fn value_as_u128(value: &Value) -> Option<u128> {
    match value {
        Value::I8(v) => u128::try_from(*v).ok(),
        Value::I16(v) => u128::try_from(*v).ok(),
        Value::I32(v) => u128::try_from(*v).ok(),
        Value::I64(v) => u128::try_from(*v).ok(),
        Value::I128(v) => u128::try_from(*v).ok(),
        Value::U8(v) => Some(*v as u128),
        Value::U16(v) => Some(*v as u128),
        Value::U32(v) => Some(*v as u128),
        Value::U64(v) => Some(*v as u128),
        Value::U128(v) => Some(*v),
        Value::Bool(v) => Some(if *v { 1 } else { 0 }),
        _ => None,
    }
}

fn is_narrow_int_value(value: &Value) -> bool {
    matches!(
        value,
        Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
    )
}

fn slot_value_as_f64(value: &Value, op_name: &str) -> Result<f64, VmError> {
    match value {
        Value::F64(v) => Ok(*v),
        Value::F32(v) => Ok(*v as f64),
        Value::F16(v) => Ok(v.to_f64()),
        Value::I64(v) => Ok(*v as f64),
        Value::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        Value::I8(v) => Ok(*v as f64),
        Value::I16(v) => Ok(*v as f64),
        Value::I32(v) => Ok(*v as f64),
        Value::I128(v) => Ok(*v as f64),
        Value::U8(v) => Ok(*v as f64),
        Value::U16(v) => Ok(*v as f64),
        Value::U32(v) => Ok(*v as f64),
        Value::U64(v) => Ok(*v as f64),
        Value::U128(v) => Ok(*v as f64),
        _ => Err(VmError::InternalError(format!(
            "{op_name}: expected F64-compatible value"
        ))),
    }
}

fn slot_f64_for_op(frame: &Frame, slot: usize, op_name: &str) -> Result<Option<f64>, VmError> {
    if let Some(v) = frame.slot_f64(slot) {
        return Ok(Some(v));
    }
    match frame.locals_slots.get(slot) {
        Some(Some(value)) => slot_value_as_f64(value, op_name).map(Some),
        Some(None) => Ok(None),
        None => Err(super::slot_out_of_bounds(op_name, slot)),
    }
}

fn slot_exact_f64(frame: &Frame, slot: usize) -> Option<f64> {
    if let Some(v) = frame.slot_f64(slot) {
        return Some(v);
    }
    match frame.locals_slots.get(slot) {
        Some(Some(Value::F64(v))) => Some(*v),
        _ => None,
    }
}

fn local_i64(frame: &Frame, name: &str) -> Option<i64> {
    match frame.locals_any.get(name) {
        Some(Value::I64(v)) => Some(*v),
        _ => None,
    }
}

fn local_i64_mut<'a>(frame: &'a mut Frame, name: &str) -> Option<&'a mut i64> {
    match frame.locals_any.get_mut(name) {
        Some(Value::I64(v)) => Some(v),
        _ => None,
    }
}

fn fused_integer_slot_op(slot_value: &Value, stack_value: &Value, op: Instr) -> Option<Value> {
    macro_rules! signed {
        ($variant:ident, $ty:ty, $value:expr) => {{
            let rhs = value_as_i128(stack_value)? as $ty;
            Some(Value::$variant(match op {
                Instr::LoadAddI64Slot(_) => ($value).wrapping_add(rhs),
                Instr::LoadSubI64Slot(_) => rhs.wrapping_sub($value),
                Instr::LoadMulI64Slot(_) => ($value).wrapping_mul(rhs),
                Instr::LoadModI64Slot(_) => rhs.wrapping_rem($value),
                _ => return None,
            }))
        }};
    }
    macro_rules! unsigned {
        ($variant:ident, $ty:ty, $value:expr) => {{
            let rhs = value_as_u128(stack_value)? as $ty;
            Some(Value::$variant(match op {
                Instr::LoadAddI64Slot(_) => ($value).wrapping_add(rhs),
                Instr::LoadSubI64Slot(_) => rhs.wrapping_sub($value),
                Instr::LoadMulI64Slot(_) => ($value).wrapping_mul(rhs),
                Instr::LoadModI64Slot(_) => rhs.wrapping_rem($value),
                _ => return None,
            }))
        }};
    }

    match slot_value {
        Value::I8(value) => signed!(I8, i8, *value),
        Value::I16(value) => signed!(I16, i16, *value),
        Value::I32(value) => signed!(I32, i32, *value),
        Value::I64(value) => signed!(I64, i64, *value),
        Value::I128(value) => signed!(I128, i128, *value),
        Value::U8(value) => unsigned!(U8, u8, *value),
        Value::U16(value) => unsigned!(U16, u16, *value),
        Value::U32(value) => unsigned!(U32, u32, *value),
        Value::U64(value) => unsigned!(U64, u64, *value),
        Value::U128(value) => unsigned!(U128, u128, *value),
        _ => None,
    }
}

impl<R: RngLike> Vm<R> {
    fn frame_i64_value_by_name(&self, frame: &Frame, name: &str) -> Option<i64> {
        local_i64(frame, name).or_else(|| match self.load_slot_value_by_name(frame, name) {
            Some(Value::I64(v)) => Some(v),
            _ => None,
        })
    }

    fn i64_value_from_current_or_global_frame(&self, name: &str) -> Option<i64> {
        self.frames
            .last()
            .and_then(|frame| self.frame_i64_value_by_name(frame, name))
            .or_else(|| {
                if self.frames.len() > 1 {
                    self.frames
                        .first()
                        .and_then(|frame| self.frame_i64_value_by_name(frame, name))
                } else {
                    None
                }
            })
    }

    /// Classify a value for a dynamic store: move heap-allocated structs onto
    /// `struct_heap` (returning a `StructRef`) and compute the `VarTypeTag` used
    /// by the named typed-map lookups, mirroring the legacy `StoreAny` handler.
    fn classify_value_for_store(&mut self, val: Value) -> (Value, VarTypeTag) {
        // The legacy native-array carrier is stored as-is with the `Any` tag.
        if super::super::value::is_native_array_value(&val) {
            return (val, VarTypeTag::Any);
        }
        // Structs are heap-allocated; move onto the heap and store a reference.
        if matches!(val, Value::Struct(_)) {
            let Value::Struct(data) = val else {
                unreachable!("matches!(val, Value::Struct(_)) guarantees this arm")
            };
            let idx = self.struct_heap.len();
            self.struct_heap.push(data);
            return (Value::StructRef(idx), VarTypeTag::Struct);
        }
        let tag = match &val {
            Value::I64(_) => VarTypeTag::I64,
            Value::F64(_) => VarTypeTag::F64,
            Value::Str(_) => VarTypeTag::Str,
            Value::Char(_) => VarTypeTag::Char,
            Value::Tuple(_) => VarTypeTag::Tuple,
            Value::NamedTuple(_) => VarTypeTag::NamedTuple,
            Value::F32(_) => VarTypeTag::F32,
            Value::F16(_) => VarTypeTag::F16,
            Value::Bool(_) => VarTypeTag::Bool,
            Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_) => VarTypeTag::NarrowInt,
            Value::StructRef(_) => VarTypeTag::Struct,
            Value::Range(_) => VarTypeTag::Range,
            Value::Rng(_) => VarTypeTag::Rng,
            Value::Generator(_) => VarTypeTag::Generator,
            Value::Nothing => VarTypeTag::Nothing,
            Value::Symbol(_) => VarTypeTag::Symbol,
            // Missing / BigInt / DataType / Function / ... and any future
            // variant default to the dynamic `Any` tag.
            _ => VarTypeTag::Any,
        };
        (val, tag)
    }

    /// Store `val` into the named typed maps of frame `frame_idx`, tagging it by
    /// runtime type exactly like the legacy `StoreAny` handler did.
    fn store_any_value_in_frame(&mut self, frame_idx: usize, name: &str, val: Value) {
        if frame_idx >= self.frames.len() {
            return;
        }
        let (stored, tag) = self.classify_value_for_store(val);
        if frame_idx == 0 {
            crate::vm::main_scope_visibility::note_main_scope_binding(name, &stored);
        }
        if let Some(frame) = self.frames.get_mut(frame_idx) {
            // O(1) removal via tag instead of clearing all per-type maps.
            frame.remove_var(name);
            frame.locals_any.insert(name.to_string(), stored);
            frame.var_types.insert(name.to_string(), tag);
        }
    }

    /// (Re)seed the Main-scope type-visibility registry (Issue #11365) from the
    /// program's struct families and the CURRENT frame-0 bindings. Called at
    /// `Vm::run()` entry so REPL-persisted and cache-restored `using`-imported
    /// type bindings are visible before the first instruction executes; the
    /// store choke points below keep the registry current during execution.
    pub(in crate::vm) fn seed_main_scope_visibility(&self) {
        use crate::vm::main_scope_visibility as visibility;
        visibility::reset_main_scope_visibility();
        visibility::set_user_module_roots(self.struct_defs.iter().filter_map(|def| {
            let head = def.name.split('{').next().unwrap_or(&def.name);
            let root = head.split('.').next().filter(|_| head.contains('.'))?;
            // Builtin scopes never take the Main. display prefix, so the
            // registry holds only genuine user roots by construction.
            (!crate::vm::util::is_top_level_module_binding_scope(root)).then(|| root.to_string())
        }));
        let Some(frame) = self.frames.first() else {
            return;
        };
        for (name, value) in frame.locals_any.iter() {
            if matches!(value, Value::DataType(_)) {
                visibility::note_main_scope_binding(name, value);
            }
        }
        for (slot, value) in frame.locals_slots.iter().enumerate() {
            if let Some(value @ Value::DataType(_)) = value {
                let name = self.slot_name_for_frame(frame, slot);
                visibility::note_main_scope_binding(&name, value);
            }
        }
    }

    /// Store `val` into the module-level (frame 0) binding for `name`, used by
    /// `StoreGlobalAny` for `global x` assignments inside a function (Issues
    /// #5548, #5549). Top-level globals live in `locals_slots`, which reads
    /// consult before the named maps, so write to the slot when one exists.
    pub fn store_global_value(&mut self, name: &str, val: Value) {
        if self.frames.is_empty() {
            return;
        }
        // Resolve the global slot (if any) for `name`, confirming it is in range
        // so the slot write below cannot silently drop the value.
        let slot = {
            let frame = &self.frames[0];
            self.slot_index_for_frame(frame, name)
                .filter(|&s| s < frame.locals_slots.len())
        };
        let (stored, tag) = self.classify_value_for_store(val);
        crate::vm::main_scope_visibility::note_main_scope_binding(name, &stored);
        if let Some(frame) = self.frames.get_mut(0) {
            // Clear any stale named-map binding so the slot stays authoritative
            // (or, in the slot-less path, the named map is unambiguous).
            frame.remove_var(name);
            match slot {
                Some(slot) => match stored {
                    Value::StructRef(idx) => {
                        frame.set_slot_struct_ref(slot, idx);
                    }
                    other => {
                        frame.set_slot_value(slot, other);
                    }
                },
                None => {
                    frame.locals_any.insert(name.to_string(), stored);
                    frame.var_types.insert(name.to_string(), tag);
                }
            }
        }
    }

    /// Seed REPL-persisted globals whose runtime `Value` has no init-expr form and
    /// therefore could not be reconstructed by the session as an init statement
    /// (e.g. an OrdinaryDiffEq `ODEProblem`, whose `kwargs::Base.Pairs` field has no
    /// source representation — Issue #8260). Rather than rebuild from source, the
    /// real `Value` is carried across the eval: the prior eval's struct heap is
    /// transplanted verbatim so every carried `StructRef` index stays valid, each
    /// transplanted instance's `type_id` is remapped to this program's struct table
    /// by name, then each global is bound by name into module (frame 0) scope.
    ///
    /// Must run on a freshly constructed VM (empty struct heap) and before `run()`,
    /// so the injected program body can read the seeded globals. Bails out (leaving
    /// the old drop behavior) if the heap is already populated, to avoid corrupting
    /// the carried indices.
    ///
    /// `transplant_heap` is the struct heap to install (the whole prior heap under
    /// the pre-#9787 behavior, or its reachable-only compaction under #9787). The
    /// caller is responsible for making the `StructRef` indices in `globals` — and
    /// in every OTHER session-held `Value` (`self.globals` / `ans` / module globals)
    /// — agree with `transplant_heap`, because the Legacy path does NOT re-read
    /// carried globals after the run and so relies on those cached indices staying
    /// valid in the post-run heap (Issue #9787).
    pub fn seed_persisted_globals(
        &mut self,
        globals: Vec<(String, Value)>,
        transplant_heap: Vec<super::super::value::StructInstance>,
    ) {
        if globals.is_empty() {
            return;
        }
        // Seeding runs immediately after `new_program`, whose heap is empty, so the
        // transplant is installed at index 0 and every carried `StructRef` (already
        // remapped by the caller to match) stays valid. Structs built during this
        // run append after it.
        if !self.struct_heap.is_empty() {
            debug_assert!(
                false,
                "seed_persisted_globals must run on a fresh VM heap (len {})",
                self.struct_heap.len()
            );
            return;
        }
        self.struct_heap.extend(transplant_heap);
        self.remap_seeded_struct_type_ids(0);
        for (name, value) in globals {
            self.store_global_value(&name, value);
        }
    }

    /// Rebase frozen callable candidate indices in values carried from a prior
    /// VM onto this freshly compiled VM. Function indices are positional and a
    /// full REPL rebuild may move a retained lowering helper even though its
    /// semantic identity is unchanged. Each old candidate is matched by the
    /// complete callable signature and accepted only when the new program has
    /// exactly one match; missing or ambiguous identities become an explicit
    /// empty candidate set rather than calling an unrelated function that now
    /// occupies the stale index (Issue #9784).
    pub fn remap_persisted_callable_candidates_from(
        &self,
        prior: &PersistedCallableSnapshot,
        globals: &mut [(String, Value)],
        transplant_heap: &mut [super::super::value::StructInstance],
    ) {
        // Build the reverse identity index once. The old implementation scanned
        // every new FunctionInfo for every referenced old index (O(K*N)); a full
        // rebuild with many carried callables therefore scaled quadratically.
        let mut next_by_identity = std::collections::HashMap::new();
        for (index, function) in self.functions.iter().enumerate() {
            next_by_identity
                .entry(
                    super::super::repl_support::PersistedCallableIdentity::from_function(function),
                )
                .or_insert_with(Vec::new)
                .push(index);
        }
        let mut cache = prior
            .identities
            .iter()
            .enumerate()
            .map(|(index, identity)| {
                (
                    index,
                    next_by_identity.get(identity).cloned().unwrap_or_default(),
                )
            })
            .collect();
        let mut visited = super::super::state::RemapVisited::new();
        for instance in transplant_heap {
            for value in &mut instance.values {
                super::super::state::remap_persisted_callable_value(
                    value,
                    &prior.identities,
                    &self.functions,
                    &mut cache,
                    &mut visited,
                );
            }
        }
        for (_, value) in globals {
            super::super::state::remap_persisted_callable_value(
                value,
                &prior.identities,
                &self.functions,
                &mut cache,
                &mut visited,
            );
        }
    }

    /// Capture the callable identity table independently of this VM's runtime
    /// state. The REPL retains it when an unrecoverable eval must drop the live
    /// VM but keeps globals whose frozen candidate indices still belong to the
    /// last successful program (Issue #9784).
    pub fn persisted_callable_snapshot(&self) -> PersistedCallableSnapshot {
        PersistedCallableSnapshot {
            identities: self
                .functions
                .iter()
                .map(|function| {
                    super::super::repl_support::PersistedCallableIdentity::from_function(function)
                })
                .collect(),
        }
    }

    /// Extend a snapshot after a verified append-only live delta without
    /// rescanning the stable prefix. Returns false if the snapshot cannot be a
    /// prefix of this VM, so the caller can conservatively rebuild it once.
    pub fn extend_persisted_callable_snapshot(
        &self,
        snapshot: &mut PersistedCallableSnapshot,
    ) -> bool {
        let start = snapshot.len();
        if start > self.functions.len() {
            return false;
        }
        snapshot
            .identities
            .extend(self.functions[start..].iter().map(|function| {
                super::super::repl_support::PersistedCallableIdentity::from_function(function)
            }));
        true
    }

    /// Remap the `type_id` of every transplanted struct instance (from `start` to the
    /// end of the heap) to this program's struct table by name. Field-by-name access
    /// already falls back to a name scan and dispatch keys off `struct_name`, so a
    /// stale `type_id` is tolerated, but remapping keeps the seeded instances
    /// first-class for reflection/printing (Issue #8260).
    fn remap_seeded_struct_type_ids(&mut self, start: usize) {
        for i in start..self.struct_heap.len() {
            let name = self.struct_heap[i].struct_name.clone();
            if let Some(&tid) = self.struct_def_name_index.get(&*name) {
                self.struct_heap[i].type_id = tid;
                continue;
            }
            // Parametric instances (`Foo{Int64}`) are registered under the base name.
            let base = name.split('{').next().unwrap_or(&name);
            if let Some(&tid) = self.struct_def_name_index.get(base) {
                self.struct_heap[i].type_id = tid;
            }
        }
    }

    fn infer_type_binding_from_frame_args(
        &self,
        name: &str,
        frame_idx: usize,
    ) -> Option<JuliaType> {
        let frame = self.frames.get(frame_idx)?;
        let func_index = frame.func_index?;
        let func = self.functions.get(func_index)?;
        if !func.type_params.iter().any(|tp| tp.name == name) {
            return None;
        }

        let mut binding_env = frame.type_bindings.clone();
        let mut candidate = binding_env.get(name).cloned();

        for (idx, param_jtype) in func.param_julia_types.iter().enumerate() {
            let slot = *func.param_slots.get(idx)?;
            let arg = frame.locals_slots.get(slot)?.as_ref()?;
            // An empty vararg collector does not constrain `xs::T...`; reading
            // T must stay unbound instead of inferring `Tuple{}` (Issue #6212).
            if func.vararg_param_index == Some(idx)
                && matches!(arg, Value::Tuple(tuple) if tuple.elements.is_empty())
            {
                continue;
            }
            if let (Value::DataType(jt), JuliaType::TypeOf(inner)) = (arg, param_jtype) {
                if let Some(bindings) =
                    jt.extract_type_bindings_in(inner, &func.type_params, &self.struct_hierarchy)
                {
                    if let Some(bound_type) = bindings.get(name) {
                        candidate = Some(bound_type.clone());
                    }
                    binding_env.extend(bindings);
                }
            }

            let arg_jtype = match arg {
                Value::DataType(jt) => jt.clone(),
                _ => Box::new(self.get_value_julia_type(arg)),
            };
            if let Some(bindings) = arg_jtype.extract_type_bindings_in(
                param_jtype,
                &func.type_params,
                &self.struct_hierarchy,
            ) {
                if let Some(bound_type) = bindings.get(name) {
                    candidate = Some(bound_type.clone());
                }
                binding_env.extend(bindings);
            }

            let matches_param = match param_jtype {
                JuliaType::TypeOf(inner) => match inner.as_ref() {
                    JuliaType::TypeVar(param_name, _) | JuliaType::Struct(param_name) => {
                        param_name == name
                    }
                    _ => false,
                },
                JuliaType::TypeVar(param_name, _) | JuliaType::Struct(param_name) => {
                    param_name == name
                }
                _ => false,
            };
            if !matches_param {
                continue;
            }
            let bound_type = if let Value::DataType(jt) = arg {
                *jt.clone()
            } else {
                *arg_jtype
            };
            binding_env.insert(name.to_string(), bound_type.clone());
            candidate = Some(bound_type);
        }

        let bound_type = candidate?;
        self.static_type_binding_satisfies_declared_bounds(
            name,
            &bound_type,
            &binding_env,
            &func.type_params,
        )
        .then_some(bound_type)
    }

    /// Execute local variable load/store instructions.
    /// Returns the execution result.
    // Hot dispatch handler: front-loaded in `dispatch_instr` (Issue #5175).
    #[inline(always)]
    pub(super) fn execute_locals(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        if let Some(slot) = stored_slot(instr) {
            let validate = self.frames.last_mut().is_some_and(|frame| {
                let Some(stores_to_skip) = frame.pending_kw_default_type_checks.get_mut(&slot)
                else {
                    return false;
                };
                if *stores_to_skip > 0 {
                    *stores_to_skip -= 1;
                    false
                } else {
                    true
                }
            });
            if validate {
                let value = self.stack.last().cloned().ok_or(VmError::StackUnderflow)?;
                if !self.validate_pending_kw_default_store(slot, &value)? {
                    return Ok(DispatchAction::Continue);
                }
            }
            if self.frames.len() == 1 {
                if let Some(name) = self.global_slot_names.get(slot) {
                    self.repl_written_globals.insert(name.clone());
                }
            }
        }

        match instr {
            Instr::LoadStr(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        self.load_slot_value_by_name(frame, name)
                            .and_then(|val| match val {
                                v if v.string_bytes().is_some() => Some(v),
                                _ => None,
                            })
                            .or_else(|| match frame.locals_any.get(name) {
                                Some(v) if v.string_bytes().is_some() => Some(v.clone()),
                                _ => None,
                            })
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                self.load_slot_value_by_name(frame, name)
                                    .and_then(|val| match val {
                                        v if v.string_bytes().is_some() => Some(v),
                                        _ => None,
                                    })
                                    .or_else(|| match frame.locals_any.get(name) {
                                        Some(v) if v.string_bytes().is_some() => Some(v.clone()),
                                        _ => None,
                                    })
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| Value::str_new(String::new()));
                self.stack.push(v);
                Ok(DispatchAction::Continue)
            }
            Instr::StoreStr(name) => {
                let v = self.stack.pop_value()?;
                if v.string_bytes().is_none() {
                    return Err(VmError::TypeError(format!(
                        "StoreStr: expected String, got {:?}",
                        v.value_type()
                    )));
                }
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), v);
                    frame.var_types.insert(name.clone(), VarTypeTag::Str);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadI64(name) => {
                let v = self.i64_value_from_current_or_global_frame(name);
                match v {
                    Some(val) => {
                        self.stack.push(Value::I64(val));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::StoreI64(name) => {
                let v = self.stack.pop_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), Value::I64(v));
                    frame.var_types.insert(name.clone(), VarTypeTag::I64);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadF64(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        frame
                            .locals_any
                            .get(name)
                            .and_then(|value| match value {
                                Value::F64(v) => Some(*v),
                                _ => None,
                            })
                            .or_else(|| match self.load_slot_value_by_name(frame, name) {
                                Some(Value::F64(v)) => Some(v),
                                _ => None,
                            })
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                frame
                                    .locals_any
                                    .get(name)
                                    .and_then(|value| match value {
                                        Value::F64(v) => Some(*v),
                                        _ => None,
                                    })
                                    .or_else(|| match self.load_slot_value_by_name(frame, name) {
                                        Some(Value::F64(v)) => Some(v),
                                        _ => None,
                                    })
                            })
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::F64(val));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::StoreF64(name) => {
                let v = self.pop_f64_or_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), Value::F64(v));
                    frame.var_types.insert(name.clone(), VarTypeTag::F64);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadF32(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| match frame.locals_any.get(name) {
                        Some(Value::F32(v)) => Some(*v),
                        _ => None,
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| match frame.locals_any.get(name) {
                                    Some(Value::F32(v)) => Some(*v),
                                    _ => None,
                                })
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::F32(val));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::StoreF32(name) => {
                let val = self.stack.pop_value()?;
                let v = match val {
                    Value::F32(f) => f,
                    Value::F64(f) => f as f32,
                    Value::I64(i) => i as f32,
                    _ => 0.0f32,
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), Value::F32(v));
                    frame.var_types.insert(name.clone(), VarTypeTag::F32);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadF16(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| match frame.locals_any.get(name) {
                        Some(Value::F16(v)) => Some(*v),
                        _ => None,
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| match frame.locals_any.get(name) {
                                    Some(Value::F16(v)) => Some(*v),
                                    _ => None,
                                })
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::F16(val));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::StoreF16(name) => {
                let val = self.stack.pop_value()?;
                let v = match val {
                    Value::F16(f) => f,
                    Value::F32(f) => half::f16::from_f32(f),
                    Value::F64(f) => half::f16::from_f64(f),
                    Value::I64(i) => half::f16::from_f64(i as f64),
                    _ => half::f16::from_f64(0.0),
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), Value::F16(v));
                    frame.var_types.insert(name.clone(), VarTypeTag::F16);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadBool(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        frame
                            .locals_any
                            .get(name)
                            .and_then(|value| match value {
                                Value::Bool(v) => Some(*v),
                                _ => None,
                            })
                            .or_else(|| match self.load_slot_value_by_name(frame, name) {
                                Some(Value::Bool(v)) => Some(v),
                                _ => None,
                            })
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                frame
                                    .locals_any
                                    .get(name)
                                    .and_then(|value| match value {
                                        Value::Bool(v) => Some(*v),
                                        _ => None,
                                    })
                                    .or_else(|| match self.load_slot_value_by_name(frame, name) {
                                        Some(Value::Bool(v)) => Some(v),
                                        _ => None,
                                    })
                            })
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::Bool(val));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::StoreBool(name) => {
                let v = self.stack.pop_bool()?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), Value::Bool(v));
                    frame.var_types.insert(name.clone(), VarTypeTag::Bool);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadSlot(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(slot_value) = slot_exact_f64(frame, *slot) {
                        if matches!(self.code.get(self.ip), Some(Instr::DupF64))
                            && matches!(self.code.get(self.ip + 1), Some(Instr::MulF64))
                        {
                            self.stack.push(Value::F64(slot_value * slot_value));
                            self.ip += 2;
                            return Ok(DispatchAction::Continue);
                        }
                        if matches!(self.code.get(self.ip), Some(Instr::MulF64)) {
                            if let Some(Value::F64(stack_value)) = self.stack.last().cloned() {
                                let _ = self.stack.pop();
                                self.stack.push(Value::F64(stack_value * slot_value));
                                self.ip += 1;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    }
                    let val = frame.locals_slots.get(*slot).and_then(|v| v.clone());
                    match val {
                        Some(v) => {
                            self.stack.push(v);
                            Ok(DispatchAction::Continue)
                        }
                        None => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::TakeSlot(slot) => {
                // Destructive slot load (Issue #10107): MOVE the value out of
                // the slot (leaving `None`) instead of cloning it. Emitted by
                // the peephole optimizer only for a `LoadSlot*; Return*` pair in
                // a function-body frame with no active exception handler, so the
                // emptied slot is discarded with the frame and never observed.
                let taken = self
                    .frames
                    .last_mut()
                    .and_then(|frame| frame.locals_slots.get_mut(*slot))
                    .and_then(Option::take);
                match taken {
                    Some(v) => {
                        self.stack.push(v);
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        // Undefined variable: mirror `LoadSlot`'s error exactly.
                        let name = match self.frames.last() {
                            Some(frame) => self.slot_name_for_frame(frame, *slot),
                            None => format!("slot {}", slot),
                        };
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::StoreSlot(slot) => {
                let val = self.stack.pop_value()?;
                let val = self.value_for_slot_storage(val);
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_value(*slot, val) {
                        // INTERNAL: slot index is compiler-generated; out-of-bounds means compiler produced an invalid slot
                        return Err(super::slot_out_of_bounds("StoreSlot", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotArray(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_array(*slot) {
                        self.stack.push(native_array_ref_value(v.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(v)) => {
                            // Cloning the slot value verbatim preserves the
                            // native-array carrier (a cheap `Rc` bump) without an
                            // explicit carrier-variant match (Issue #6806).
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotArray", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotArray(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match native_array_ref_from_value(val) {
                        Ok(arr) => frame.set_slot_array(*slot, arr),
                        Err(other) => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotArray", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotTuple(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_tuple(*slot) {
                        self.stack.push(Value::Tuple(v.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Tuple(v))) => {
                            self.stack.push(Value::Tuple(v.clone()));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotTuple", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotTuple(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match val {
                        Value::Tuple(tuple) => frame.set_slot_tuple(*slot, tuple),
                        other => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotTuple", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotNamedTuple(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_named_tuple(*slot) {
                        self.stack.push(Value::NamedTuple(v.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::NamedTuple(v))) => {
                            self.stack.push(Value::NamedTuple(v.clone()));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotNamedTuple", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotNamedTuple(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match val {
                        Value::NamedTuple(named_tuple) => {
                            frame.set_slot_named_tuple(*slot, named_tuple)
                        }
                        other => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotNamedTuple", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotDict(slot) => {
                // `Value::Dict` was retired (Issue #6731); a Dict local is now an
                // ordinary StructRef value, loaded through the generic slot arm.
                if let Some(frame) = self.frames.last() {
                    match frame.locals_slots.get(*slot) {
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotDict", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotDict(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_value(*slot, val) {
                        return Err(super::slot_out_of_bounds("StoreSlotDict", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotSet(slot) => {
                // `Value::Set` was retired (Issue #6732); a Set local is an ordinary
                // StructRef value, loaded through the generic slot arm.
                if let Some(frame) = self.frames.last() {
                    match frame.locals_slots.get(*slot) {
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotSet", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotSet(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_value(*slot, val) {
                        return Err(super::slot_out_of_bounds("StoreSlotSet", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotStruct(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_struct(*slot) {
                        self.stack.push(Value::StructRef(v));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::StructRef(v))) => {
                            self.stack.push(Value::StructRef(*v));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::Struct(v))) => {
                            self.stack.push(Value::Struct(v.clone()));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotStruct", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotStruct(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match val {
                        Value::StructRef(idx) => frame.set_slot_struct_ref(*slot, idx),
                        other => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotStruct", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotRange(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_range(*slot) {
                        self.stack.push(Value::Range(v.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Range(v))) => {
                            self.stack.push(Value::Range(v.clone()));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotRange", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotRange(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match val {
                        Value::Range(range) => frame.set_slot_range(*slot, range),
                        other => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotRange", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotRng(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_rng(*slot) {
                        self.stack.push(Value::Rng(v.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Rng(v))) => {
                            self.stack.push(Value::Rng(v.clone()));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotRng", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotRng(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match val {
                        Value::Rng(rng) => frame.set_slot_rng(*slot, rng),
                        other => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotRng", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotGenerator(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_generator(*slot) {
                        self.stack.push(Value::Generator(Box::new(v.clone())));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Generator(v))) => {
                            self.stack.push(Value::Generator(v.clone()));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(v)) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotGenerator", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotGenerator(slot) => {
                let val = self.stack.pop_value()?;
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match val {
                        Value::Generator(generator) => frame.set_slot_generator(*slot, generator),
                        other => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotGenerator", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotI64(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_i64(*slot) {
                        self.stack.push(Value::I64(v));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(
                            value @ (Value::I64(_)
                            | Value::Bool(_)
                            | Value::I32(_)
                            | Value::I16(_)
                            | Value::I8(_)
                            | Value::I128(_)
                            | Value::U8(_)
                            | Value::U16(_)
                            | Value::U32(_)
                            | Value::U64(_)
                            | Value::U128(_)
                            | Value::F16(_)
                            | Value::F32(_)
                            | Value::F64(_)),
                        )) => {
                            self.stack.push(value.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(value)) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            let ctx = self.slot_debug_context_for_frame(frame, *slot);
                            Err(VmError::InternalError(format!(
                                "LoadSlotI64: expected numeric in {}, got {:?} [{}]",
                                name, value, ctx
                            )))
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotI64", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::LoadSlotI64ToF64(slot) => {
                let loaded = if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_i64(*slot) {
                        Some(Value::I64(v))
                    } else {
                        match frame.locals_slots.get(*slot) {
                            Some(Some(
                                value @ (Value::I64(_)
                                | Value::Bool(_)
                                | Value::I32(_)
                                | Value::I16(_)
                                | Value::I8(_)
                                | Value::I128(_)
                                | Value::U8(_)
                                | Value::U16(_)
                                | Value::U32(_)
                                | Value::U64(_)
                                | Value::U128(_)
                                | Value::F16(_)
                                | Value::F32(_)
                                | Value::F64(_)),
                            )) => Some(value.clone()),
                            Some(Some(value)) => {
                                let name = self.slot_name_for_frame(frame, *slot);
                                let ctx = self.slot_debug_context_for_frame(frame, *slot);
                                return Err(VmError::InternalError(format!(
                                    "LoadSlotI64: expected numeric in {}, got {:?} [{}]",
                                    name, value, ctx
                                )));
                            }
                            Some(None) => {
                                let name = self.slot_name_for_frame(frame, *slot);
                                self.raise(VmError::UndefVarError(name))?;
                                return Ok(DispatchAction::Continue);
                            }
                            None => {
                                return Err(super::slot_out_of_bounds("LoadSlotI64", slot));
                            }
                        }
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    return Ok(DispatchAction::Continue);
                };

                let Some(value) = loaded else {
                    return Ok(DispatchAction::Continue);
                };
                self.stack.push(Value::F64(self.convert_to_f64(&value)?));
                Ok(DispatchAction::Continue)
            }
            Instr::StoreSlotI64(slot) => {
                let val = self.stack.pop_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_i64(*slot, val) {
                        return Err(super::slot_out_of_bounds("StoreSlotI64", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotF64(slot) => {
                if let Some(frame) = self.frames.last() {
                    if matches!(
                        self.code.get(self.ip),
                        Some(Instr::CallDynamicBinaryBoth(
                            crate::intrinsics::Intrinsic::DynamicAdd,
                            _
                        ))
                    ) {
                        if let (Some(slot_value), Some(Value::F64(stack_value))) = (
                            slot_f64_for_op(frame, *slot, "LoadSlotF64 add fast path")?,
                            self.stack.last().cloned(),
                        ) {
                            let _ = self.stack.pop();
                            self.stack.push(Value::F64(stack_value + slot_value));
                            self.ip += 1;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                    // If a protected jump target prevents static fusion, the
                    // fall-through path can still execute x*x without changing
                    // what a separate jump into DupF64/MulF64 would do.
                    if matches!(self.code.get(self.ip), Some(Instr::DupF64))
                        && matches!(self.code.get(self.ip + 1), Some(Instr::MulF64))
                    {
                        match slot_f64_for_op(frame, *slot, "LoadSlotF64 square fast path")? {
                            Some(value) => {
                                self.stack.push(Value::F64(value * value));
                                self.ip += 2;
                                return Ok(DispatchAction::Continue);
                            }
                            None => {
                                let name = self.slot_name_for_frame(frame, *slot);
                                self.raise(VmError::UndefVarError(name))?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    }
                    if let Some(v) = frame.slot_f64(*slot) {
                        self.stack.push(Value::F64(v));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::F64(v))) => {
                            self.stack.push(Value::F64(*v));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(value @ (Value::F16(_) | Value::F32(_)))) => {
                            self.stack.push(value.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::I64(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::Bool(v))) => {
                            self.stack.push(Value::F64(if *v { 1.0 } else { 0.0 }));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::I8(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::I16(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::I32(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::I128(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::U8(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::U16(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::U32(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::U64(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::U128(v))) => {
                            self.stack.push(Value::F64(*v as f64));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotF64: expected F64-compatible value".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotF64", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotF64(slot) => {
                let val = self.pop_f64_or_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_f64(*slot, val) {
                        return Err(super::slot_out_of_bounds("StoreSlotF64", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSquareF64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                match slot_f64_for_op(frame, *slot, "LoadSquareF64Slot")? {
                    Some(value) => {
                        self.stack.push(Value::F64(value * value));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        let name = self.slot_name_for_frame(frame, *slot);
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadAddF64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                match slot_f64_for_op(frame, *slot, "LoadAddF64Slot")? {
                    Some(value) => {
                        let stack_value = self.pop_f64_or_i64()?;
                        self.stack.push(Value::F64(stack_value + value));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        let name = self.slot_name_for_frame(frame, *slot);
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            // Issue #9126: fused F64 slot add — `slot[dst] = slot[lhs] + slot[rhs]`
            // (AddF64Slots) or `slot[dst] = slot[lhs] + F64(slot[rhs_i64])`
            // (AddF64I64Slots). Replaces the 4-instruction
            // `LoadSlotF64; (LoadSlotF64|LoadSlotI64ToF64); AddF64; StoreSlotF64`
            // body without touching the stack. Operand loads mirror the
            // originals: `slot_f64_for_op` accepts exactly the numeric slot
            // values `LoadSlotF64`/`LoadSlotI64ToF64` accept, in lhs-then-rhs
            // order, raising UndefVarError for unset slots like the originals.
            Instr::AddF64Slots(dst, lhs, rhs) | Instr::AddF64I64Slots(dst, lhs, rhs) => {
                let op_name = if matches!(instr, Instr::AddF64Slots(..)) {
                    "AddF64Slots"
                } else {
                    "AddF64I64Slots"
                };
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", dst)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let lhs_value = match slot_f64_for_op(frame, *lhs, op_name)? {
                    Some(value) => value,
                    None => {
                        let name = self.slot_name_for_frame(frame, *lhs);
                        self.raise(VmError::UndefVarError(name))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let rhs_value = match slot_f64_for_op(frame, *rhs, op_name)? {
                    Some(value) => value,
                    None => {
                        let name = self.slot_name_for_frame(frame, *rhs);
                        self.raise(VmError::UndefVarError(name))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let sum = lhs_value + rhs_value;
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_f64(*dst, sum) {
                        return Err(super::slot_out_of_bounds(op_name, *dst));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSubF64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                match slot_f64_for_op(frame, *slot, "LoadSubF64Slot")? {
                    Some(value) => {
                        let stack_value = self.pop_f64_or_i64()?;
                        self.stack.push(Value::F64(stack_value - value));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        let name = self.slot_name_for_frame(frame, *slot);
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadMulF64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                match slot_f64_for_op(frame, *slot, "LoadMulF64Slot")? {
                    Some(value) => {
                        let stack_value = self.pop_f64_or_i64()?;
                        self.stack.push(Value::F64(stack_value * value));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        let name = self.slot_name_for_frame(frame, *slot);
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadDivF64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                match slot_f64_for_op(frame, *slot, "LoadDivF64Slot")? {
                    Some(value) => {
                        let stack_value = self.pop_f64_or_i64()?;
                        self.stack.push(Value::F64(stack_value / value));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        let name = self.slot_name_for_frame(frame, *slot);
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadSlotBool(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_bool(*slot) {
                        self.stack.push(Value::Bool(v));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Bool(v))) => {
                            self.stack.push(Value::Bool(*v));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotBool: expected Bool".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotBool", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotBool(slot) => {
                let val = self.stack.pop_bool()?;
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_bool(*slot, val) {
                        return Err(super::slot_out_of_bounds("StoreSlotBool", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotF32(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_f32(*slot) {
                        self.stack.push(Value::F32(v));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::F32(v))) => {
                            self.stack.push(Value::F32(*v));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotF32: expected Float32".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotF32", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotF32(slot) => {
                let val = self.stack.pop_value()?;
                let v = match val {
                    Value::F32(f) => f,
                    Value::F64(f) => f as f32,
                    Value::I64(i) => i as f32,
                    _ => 0.0f32,
                };
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_f32(*slot, v) {
                        return Err(super::slot_out_of_bounds("StoreSlotF32", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotF16(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_f16(*slot) {
                        self.stack.push(Value::F16(v));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::F16(v))) => {
                            self.stack.push(Value::F16(*v));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotF16: expected Float16".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotF16", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotF16(slot) => {
                let val = self.stack.pop_value()?;
                let v = match val {
                    Value::F16(f) => f,
                    Value::F32(f) => half::f16::from_f32(f),
                    Value::F64(f) => half::f16::from_f64(f),
                    Value::I64(i) => half::f16::from_f64(i as f64),
                    _ => half::f16::from_f64(0.0),
                };
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_f16(*slot, v) {
                        return Err(super::slot_out_of_bounds("StoreSlotF16", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotStr(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_string_value(*slot) {
                        self.stack.push(v.clone());
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(v)) if v.string_bytes().is_some() => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotStr: expected String".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotStr", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotStr(slot) => {
                let val = self.stack.pop_value()?;
                if val.string_bytes().is_none() {
                    return Err(VmError::TypeError(format!(
                        "StoreSlotStr: expected String, got {:?}",
                        val.value_type()
                    )));
                }
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_string_value(*slot, val) {
                        return Err(super::slot_out_of_bounds("StoreSlotStr", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotChar(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_char(*slot) {
                        self.stack.push(Value::Char(v));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Char(v))) => {
                            self.stack.push(Value::Char(*v));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(Value::CharMalformed(v))) => {
                            self.stack.push(Value::CharMalformed(*v));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotChar: expected Char".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotChar", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotChar(slot) => {
                // Accept both valid and malformed Chars (Issue #8995); the
                // slot storage is a boxed Value either way.
                let val = self.stack.pop_value()?;
                if !matches!(val, Value::Char(_) | Value::CharMalformed(_)) {
                    return Err(VmError::InternalError(
                        "StoreSlotChar: expected Char".to_string(),
                    ));
                }
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_char_value(*slot, val) {
                        return Err(super::slot_out_of_bounds("StoreSlotChar", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotSymbol(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_symbol(*slot) {
                        self.stack.push(Value::Symbol(v.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Symbol(v))) => {
                            self.stack.push(Value::Symbol(v.clone()));
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotSymbol: expected Symbol".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotSymbol", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotSymbol(slot) => {
                let val = self.stack.pop_value()?;
                if let Some(frame) = self.frames.last_mut() {
                    let ok = match val {
                        Value::Symbol(symbol) => frame.set_slot_symbol(*slot, symbol),
                        other => frame.set_slot_value(*slot, other),
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotSymbol", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotNarrowInt(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_narrow_int(*slot) {
                        self.stack.push(v.clone());
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(v)) if is_narrow_int_value(v) => {
                            self.stack.push(v.clone());
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotNarrowInt: expected narrow integer".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotNarrowInt", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotNarrowInt(slot) => {
                let val = self.stack.pop_value()?;
                if let Some(frame) = self.frames.last_mut() {
                    let ok = if is_narrow_int_value(&val) {
                        frame.set_slot_narrow_int(*slot, val)
                    } else {
                        frame.set_slot_value(*slot, val)
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotNarrowInt", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotNothing(slot) => {
                if let Some(frame) = self.frames.last() {
                    if frame.slot_nothing(*slot) {
                        self.stack.push(Value::Nothing);
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Nothing)) => {
                            self.stack.push(Value::Nothing);
                            Ok(DispatchAction::Continue)
                        }
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotNothing: expected Nothing".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(super::slot_out_of_bounds("LoadSlotNothing", slot)),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotNothing(slot) => {
                let val = self.stack.pop_value()?;
                if let Some(frame) = self.frames.last_mut() {
                    let ok = if matches!(val, Value::Nothing) {
                        frame.set_slot_nothing(*slot)
                    } else {
                        frame.set_slot_value(*slot, val)
                    };
                    if !ok {
                        return Err(super::slot_out_of_bounds("StoreSlotNothing", slot));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadAny(name) => {
                let current_frame_idx = self.frames.len().saturating_sub(1);
                let found = self.try_load_from_frame(name, current_frame_idx);
                if !found {
                    // Infer a missing binder only from this function's own
                    // arguments. Walking caller frames turns lexical `where`
                    // binders into dynamic scope (Issue #11069); nested
                    // closures receive legitimate outer binders explicitly in
                    // their capture namespace (Issue #11031).
                    let inferred = self.infer_type_binding_from_frame_args(name, current_frame_idx);
                    let type_binding_found = if let Some(julia_type) = inferred {
                        if let Some(frame) = self.frames.get_mut(current_frame_idx) {
                            frame.type_bindings.insert(name.clone(), julia_type.clone());
                        }
                        self.stack.push(Value::DataType(Box::new(julia_type)));
                        true
                    } else {
                        false
                    };
                    if !type_binding_found {
                        let global_found =
                            self.frames.len() > 1 && self.try_load_from_frame(name, 0);
                        if !global_found {
                            if let Some(value) = self.get_global_definition_value(name) {
                                self.stack.push(value);
                            } else {
                                self.raise(VmError::UndefVarError(name.clone()))?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::ProbeRuntimeBinding(name) => {
                let current_frame_idx = self.frames.len().saturating_sub(1);
                let found = self.try_load_from_frame(name, current_frame_idx);
                if !found {
                    let inferred = self.infer_type_binding_from_frame_args(name, current_frame_idx);
                    let type_binding_found = if let Some(julia_type) = inferred {
                        if let Some(frame) = self.frames.get_mut(current_frame_idx) {
                            frame.type_bindings.insert(name.clone(), julia_type.clone());
                        }
                        self.stack.push(Value::DataType(Box::new(julia_type)));
                        true
                    } else {
                        false
                    };
                    if !type_binding_found {
                        let global_found =
                            self.frames.len() > 1 && self.try_load_from_frame(name, 0);
                        if !global_found {
                            if let Some(value) = self.get_published_eval_nominal_type_value(name) {
                                self.stack.push(value);
                                return Ok(DispatchAction::Continue);
                            }
                            self.raise(VmError::UndefVarError(name.clone()))?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadGlobalAny(name) => {
                let loaded = !self.frames.is_empty() && self.try_load_from_frame(name, 0);
                if !loaded {
                    if let Some(value) = self.get_global_definition_value(name) {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                    self.raise(VmError::UndefVarError(name.clone()))?;
                    return Ok(DispatchAction::Continue);
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadTypeBinding(name) => {
                // Value type parameters (e.g. `N` in `Arr{T,N}`, `sym` in
                // `TaggedFoo{:hello}`, `v` in `VP{1.5}` / `VP{Int8(5)}`) are
                // stored as value locals by bind_type_params. Prefer that so
                // the method body sees the raw value, not a `DataType`
                // wrapper, for EVERY kind bind_type_params can store —
                // including constructor-form narrow numerics (Issues #6625,
                // #8869, #10599).
                if let Some(frame) = self.frames.last() {
                    if let Some(
                        v @ (Value::I64(_)
                        | Value::I8(_)
                        | Value::I16(_)
                        | Value::I32(_)
                        | Value::I128(_)
                        | Value::U8(_)
                        | Value::U16(_)
                        | Value::U32(_)
                        | Value::U64(_)
                        | Value::U128(_)
                        | Value::F64(_)
                        | Value::F32(_)
                        | Value::F16(_)
                        | Value::Bool(_)
                        | Value::Char(_)
                        | Value::Symbol(_)
                        | Value::Tuple(_)),
                    ) = frame.locals_any.get(name)
                    {
                        let v = v.clone();
                        self.stack.push(v);
                        return Ok(DispatchAction::Continue);
                    }
                }
                let current_frame_idx = self.frames.len().saturating_sub(1);
                let inferred = if self
                    .frames
                    .last()
                    .is_some_and(|frame| !frame.type_bindings.contains_key(name))
                {
                    self.infer_type_binding_from_frame_args(name, current_frame_idx)
                } else {
                    None
                };
                if let Some(julia_type) = inferred {
                    if let Some(frame) = self.frames.get_mut(current_frame_idx) {
                        frame.type_bindings.insert(name.clone(), julia_type.clone());
                    }
                    self.stack.push(Value::DataType(Box::new(julia_type)));
                    return Ok(DispatchAction::Continue);
                }

                if let Some(frame) = self.frames.last() {
                    if let Some(julia_type) = frame.type_bindings.get(name) {
                        self.stack
                            .push(Value::DataType(Box::new(julia_type.clone())));
                        Ok(DispatchAction::Continue)
                    } else {
                        self.raise(VmError::UndefVarError(format!(
                            "Unbound type parameter: {}",
                            name
                        )))?;
                        Ok(DispatchAction::Continue)
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!(
                        "No frame for type binding: {}",
                        name
                    )))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::LoadValBool(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Bool(val)) = frame.locals_any.get(name) {
                        self.stack.push(Value::Bool(*val));
                        Ok(DispatchAction::Continue)
                    } else {
                        self.raise(VmError::UndefVarError(format!(
                            "Unbound Val boolean parameter: {}",
                            name
                        )))?;
                        Ok(DispatchAction::Continue)
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!(
                        "No frame for Val boolean: {}",
                        name
                    )))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::LoadValSymbol(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Symbol(symbol)) = frame.locals_any.get(name).cloned() {
                        self.stack.push(Value::Symbol(symbol));
                        Ok(DispatchAction::Continue)
                    } else {
                        self.raise(VmError::UndefVarError(format!(
                            "Unbound Val symbol parameter: {}",
                            name
                        )))?;
                        Ok(DispatchAction::Continue)
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!(
                        "No frame for Val symbol: {}",
                        name
                    )))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreAny(name) => {
                let val = self.stack.pop_value()?;
                let frame_idx = self.frames.len().saturating_sub(1);
                self.store_any_value_in_frame(frame_idx, name, val);
                if frame_idx == 0 {
                    self.repl_written_globals.insert(name.clone());
                }
                Ok(DispatchAction::Continue)
            }
            Instr::StoreGlobalAny(name) => {
                // Route the write to the module-level frame (frame 0) so a
                // `global x` assignment inside a function updates the top-level
                // binding (Issues #5548, #5549). Top-level globals live in
                // slots, which reads consult first, so this is slot-aware.
                let val = self.stack.pop_value()?;
                self.store_global_value(name, val);
                self.repl_written_globals.insert(name.clone());
                self.repl_explicit_global_writes.insert(name.clone());
                Ok(DispatchAction::Continue)
            }

            // === Fused load+arithmetic instructions ===
            Instr::LoadAddI64(name) => {
                let var_val = self.i64_value_from_current_or_global_frame(name);
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(var.wrapping_add(stack_val)));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadAddI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let slot_value = frame.locals_slots.get(*slot).and_then(|v| v.clone());
                match slot_value {
                    Some(value) => {
                        let stack_value = self.stack.pop_value()?;
                        if let Some(result) = fused_integer_slot_op(
                            &value,
                            &stack_value,
                            Instr::LoadAddI64Slot(*slot),
                        ) {
                            self.stack.push(result);
                        } else {
                            return Err(VmError::TypeError(format!(
                                "LoadAddI64Slot: expected integer in {}, got {:?}",
                                name, value
                            )));
                        }
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadAddConstI64Slot(slot, delta) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let slot_value = frame.locals_slots.get(*slot).and_then(|v| v.clone());
                match slot_value {
                    Some(value) => {
                        if let Some(result) = fused_integer_slot_op(
                            &value,
                            &Value::I64(*delta),
                            Instr::LoadAddI64Slot(*slot),
                        ) {
                            self.stack.push(result);
                        } else {
                            return Err(VmError::TypeError(format!(
                                "LoadAddConstI64Slot: expected integer in {}, got {:?}",
                                name, value
                            )));
                        }
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }

            Instr::LoadSubI64(name) => {
                let var_val = self.i64_value_from_current_or_global_frame(name);
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(stack_val.wrapping_sub(var)));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadSubI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let slot_value = frame.locals_slots.get(*slot).and_then(|v| v.clone());
                match slot_value {
                    Some(value) => {
                        let stack_value = self.stack.pop_value()?;
                        if let Some(result) = fused_integer_slot_op(
                            &value,
                            &stack_value,
                            Instr::LoadSubI64Slot(*slot),
                        ) {
                            self.stack.push(result);
                        } else {
                            return Err(VmError::TypeError(format!(
                                "LoadSubI64Slot: expected integer in {}, got {:?}",
                                name, value
                            )));
                        }
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }

            Instr::LoadMulI64(name) => {
                let var_val = self.i64_value_from_current_or_global_frame(name);
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(var.wrapping_mul(stack_val)));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadMulI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let slot_value = frame.locals_slots.get(*slot).and_then(|v| v.clone());
                match slot_value {
                    Some(value) => {
                        let stack_value = self.stack.pop_value()?;
                        if let Some(result) = fused_integer_slot_op(
                            &value,
                            &stack_value,
                            Instr::LoadMulI64Slot(*slot),
                        ) {
                            self.stack.push(result);
                        } else {
                            return Err(VmError::TypeError(format!(
                                "LoadMulI64Slot: expected integer in {}, got {:?}",
                                name, value
                            )));
                        }
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }

            Instr::LoadModI64(name) => {
                let var_val = self.i64_value_from_current_or_global_frame(name);
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        if var == 0 {
                            self.raise(VmError::DivisionByZero)?;
                            return Ok(DispatchAction::Continue);
                        }
                        // wrapping_rem: rem(typemin(Int64), -1) == 0 in Julia; a
                        // plain `%` panics on the i64::MIN % -1 overflow (Issue #9429).
                        self.stack.push(Value::I64(stack_val.wrapping_rem(var)));
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }
            Instr::LoadModI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let slot_value = frame.locals_slots.get(*slot).and_then(|v| v.clone());
                match slot_value {
                    Some(value) => {
                        let is_zero = matches!(
                            value,
                            Value::I8(0)
                                | Value::I16(0)
                                | Value::I32(0)
                                | Value::I64(0)
                                | Value::I128(0)
                                | Value::U8(0)
                                | Value::U16(0)
                                | Value::U32(0)
                                | Value::U64(0)
                                | Value::U128(0)
                        );
                        if is_zero {
                            self.raise(VmError::DivisionByZero)?;
                            return Ok(DispatchAction::Continue);
                        }
                        let stack_value = self.stack.pop_value()?;
                        if let Some(result) = fused_integer_slot_op(
                            &value,
                            &stack_value,
                            Instr::LoadModI64Slot(*slot),
                        ) {
                            self.stack.push(result);
                        } else {
                            return Err(VmError::TypeError(format!(
                                "LoadModI64Slot: expected integer in {}, got {:?}",
                                name, value
                            )));
                        }
                        Ok(DispatchAction::Continue)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(DispatchAction::Continue)
                    }
                }
            }

            Instr::IncVarI64(name) => {
                let increment = self.stack.pop_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    if let Some(val) = local_i64_mut(frame, name) {
                        *val = (*val).wrapping_add(increment);
                        return Ok(DispatchAction::Continue);
                    }
                    // Try global frame
                    if self.frames.len() > 1 {
                        if let Some(val) =
                            self.frames.first_mut().and_then(|f| local_i64_mut(f, name))
                        {
                            *val = (*val).wrapping_add(increment);
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                self.raise(VmError::UndefVarError(name.clone()))?;
                Ok(DispatchAction::Continue)
            }
            Instr::IncVarI64Slot(slot) => {
                let increment = self.stack.pop_i64()?;
                let name = self
                    .frames
                    .last()
                    .map(|frame| self.slot_name_for_frame(frame, *slot))
                    .unwrap_or_else(|| format!("slot {}", slot));
                if let Some(frame) = self.frames.last_mut() {
                    match frame.locals_slots.get_mut(*slot) {
                        Some(Some(Value::I64(val))) => {
                            *val = (*val).wrapping_add(increment);
                            return Ok(DispatchAction::Continue);
                        }
                        Some(Some(_)) => {
                            // INTERNAL: IncVarI64Slot is emitted only for I64-typed slots; wrong type is a compiler bug
                            return Err(VmError::InternalError(
                                "IncVarI64Slot: expected I64".to_string(),
                            ));
                        }
                        Some(None) => {
                            self.raise(VmError::UndefVarError(name))?;
                            return Ok(DispatchAction::Continue);
                        }
                        None => {
                            // INTERNAL: slot index is compiler-generated; out-of-bounds means compiler produced an invalid slot
                            return Err(super::slot_out_of_bounds("IncVarI64Slot", slot));
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::AddConstI64Slot(slot, delta) => {
                let name = self
                    .frames
                    .last()
                    .map(|frame| self.slot_name_for_frame(frame, *slot))
                    .unwrap_or_else(|| format!("slot {}", slot));
                if let Some(frame) = self.frames.last_mut() {
                    match frame.locals_slots.get_mut(*slot) {
                        Some(Some(Value::I64(val))) => {
                            *val = (*val).wrapping_add(*delta);
                            return Ok(DispatchAction::Continue);
                        }
                        Some(Some(_)) => {
                            return Err(VmError::InternalError(
                                "AddConstI64Slot: expected I64".to_string(),
                            ));
                        }
                        Some(None) => {
                            self.raise(VmError::UndefVarError(name))?;
                            return Ok(DispatchAction::Continue);
                        }
                        None => {
                            return Err(super::slot_out_of_bounds("AddConstI64Slot", slot));
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DecVarI64(name) => {
                let decrement = self.stack.pop_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    if let Some(val) = local_i64_mut(frame, name) {
                        *val = (*val).wrapping_sub(decrement);
                        return Ok(DispatchAction::Continue);
                    }
                    // Try global frame
                    if self.frames.len() > 1 {
                        if let Some(val) =
                            self.frames.first_mut().and_then(|f| local_i64_mut(f, name))
                        {
                            *val = (*val).wrapping_sub(decrement);
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                self.raise(VmError::UndefVarError(name.clone()))?;
                Ok(DispatchAction::Continue)
            }
            Instr::DecVarI64Slot(slot) => {
                let decrement = self.stack.pop_i64()?;
                let name = self
                    .frames
                    .last()
                    .map(|frame| self.slot_name_for_frame(frame, *slot))
                    .unwrap_or_else(|| format!("slot {}", slot));
                if let Some(frame) = self.frames.last_mut() {
                    match frame.locals_slots.get_mut(*slot) {
                        Some(Some(Value::I64(val))) => {
                            *val = (*val).wrapping_sub(decrement);
                            return Ok(DispatchAction::Continue);
                        }
                        Some(Some(_)) => {
                            // INTERNAL: DecVarI64Slot is emitted only for I64-typed slots; wrong type is a compiler bug
                            return Err(VmError::InternalError(
                                "DecVarI64Slot: expected I64".to_string(),
                            ));
                        }
                        Some(None) => {
                            self.raise(VmError::UndefVarError(name))?;
                            return Ok(DispatchAction::Continue);
                        }
                        None => {
                            // INTERNAL: slot index is compiler-generated; out-of-bounds means compiler produced an invalid slot
                            return Err(super::slot_out_of_bounds("DecVarI64Slot", slot));
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }

            // Variable reflection: check if a variable is defined
            Instr::IsDefined(name) => {
                // Check current frame first
                let current_frame_idx = self.frames.len().saturating_sub(1);
                let defined_in_current = self.is_var_defined_in_frame(name, current_frame_idx);

                if defined_in_current {
                    self.stack.push(Value::Bool(true));
                    return Ok(DispatchAction::Continue);
                }

                // Check global frame
                let defined_in_global =
                    self.frames.len() > 1 && self.is_var_defined_in_frame(name, 0);
                let definition_owned =
                    !defined_in_global && self.get_global_definition_value(name).is_some();
                self.stack
                    .push(Value::Bool(defined_in_global || definition_owned));

                Ok(DispatchAction::Continue)
            }

            Instr::EnterLexicalScope(names) => {
                self.enter_root_lexical_scope(names)?;
                Ok(DispatchAction::Continue)
            }

            Instr::LoadLexical(name) => {
                if self.frames.len() != 1 {
                    return Err(VmError::InternalError(
                        "LoadLexical executed outside module/main frame".to_string(),
                    ));
                }
                match self.root_lexical_binding(name).cloned() {
                    Some(Some(value)) => self.stack.push(value),
                    Some(None) | None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::StoreLexical(name) => {
                if self.frames.len() != 1 {
                    return Err(VmError::InternalError(
                        "StoreLexical executed outside module/main frame".to_string(),
                    ));
                }
                let value = self.stack.pop_value()?;
                self.store_root_lexical(name, value)?;
                Ok(DispatchAction::Continue)
            }

            Instr::IsLexicalDefined(name) => {
                if self.frames.len() != 1 {
                    return Err(VmError::InternalError(
                        "IsLexicalDefined executed outside module/main frame".to_string(),
                    ));
                }
                let is_defined = matches!(self.root_lexical_binding(name), Some(Some(_)));
                self.stack.push(Value::Bool(is_defined));
                Ok(DispatchAction::Continue)
            }

            Instr::ExitLexicalScope => {
                self.exit_root_lexical_scope()?;
                Ok(DispatchAction::Continue)
            }

            Instr::ForgetLetLocals(names) => {
                // Discard hard-scope `let`-block locals at block exit (Issue
                // #9313), so an outer `@isdefined`/read after the `let` sees them
                // as undefined. sjulia runs a `let` body in the enclosing frame
                // (frame 0 at module scope) with global slots, so a let-local is
                // stored either in a slot or a named map; clear both. Names that
                // shadow an initialized outer binding are restored by the caller
                // and never appear here, and explicit `global` names are excluded
                // by the compiler, so this only forgets genuine let-locals. The
                // let's result value on the stack is untouched.
                let frame_idx = self.frames.len().saturating_sub(1);
                // Resolve slot indices under an immutable borrow first, then
                // clear them under a mutable borrow (`slot_index_for_frame`
                // reads `self`'s slot maps).
                let mut slots: Vec<usize> = Vec::new();
                if let Some(frame) = self.frames.get(frame_idx) {
                    for name in names {
                        if let Some(slot) = self.slot_index_for_frame(frame, name) {
                            slots.push(slot);
                        }
                    }
                }
                if let Some(frame) = self.frames.get_mut(frame_idx) {
                    for slot in slots {
                        if let Some(cell) = frame.locals_slots.get_mut(slot) {
                            *cell = None;
                        }
                    }
                    for name in names {
                        frame.remove_var(name);
                    }
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
