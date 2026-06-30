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
use super::super::stack_ops::StackOps;
use super::super::value::{native_array_ref_from_value, native_array_ref_value, Value};
use super::super::Vm;
use super::DispatchAction;

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
        None => Err(VmError::InternalError(format!(
            "{op_name}: slot out of bounds: {slot}"
        ))),
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
        if let Some(frame) = self.frames.get_mut(frame_idx) {
            // O(1) removal via tag instead of clearing all per-type maps.
            frame.remove_var(name);
            frame.locals_any.insert(name.to_string(), stored);
            frame.var_types.insert(name.to_string(), tag);
        }
    }

    /// Store `val` into the module-level (frame 0) binding for `name`, used by
    /// `StoreGlobalAny` for `global x` assignments inside a function (Issues
    /// #5548, #5549). Top-level globals live in `locals_slots`, which reads
    /// consult before the named maps, so write to the slot when one exists.
    pub(crate) fn store_global_value(&mut self, name: &str, val: Value) {
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
    pub fn seed_persisted_globals(
        &mut self,
        globals: Vec<(String, Value)>,
        prior_heap: &[super::super::value::StructInstance],
    ) {
        if globals.is_empty() {
            return;
        }
        // Carried `StructRef` indices are relative to `prior_heap`, which started at
        // index 0. Seeding runs immediately after `new_program`, whose heap is empty,
        // so prepend the prior heap at index 0 to keep every carried index valid.
        // Structs built during this run append after it.
        if !self.struct_heap.is_empty() {
            debug_assert!(
                false,
                "seed_persisted_globals must run on a fresh VM heap (len {})",
                self.struct_heap.len()
            );
            return;
        }
        self.struct_heap.extend_from_slice(prior_heap);
        self.remap_seeded_struct_type_ids(0);
        for (name, value) in globals {
            self.store_global_value(&name, value);
        }
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
                if let Some(bindings) = jt.extract_type_bindings(inner, &func.type_params) {
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
            if let Some(bindings) = arg_jtype.extract_type_bindings(param_jtype, &func.type_params)
            {
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
        match instr {
            Instr::LoadStr(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        self.load_slot_value_by_name(frame, name)
                            .and_then(|val| match val {
                                Value::Str(v) => Some(v),
                                _ => None,
                            })
                            .or_else(|| match frame.locals_any.get(name) {
                                Some(Value::Str(v)) => Some(v.clone()),
                                _ => None,
                            })
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                self.load_slot_value_by_name(frame, name)
                                    .and_then(|val| match val {
                                        Value::Str(v) => Some(v),
                                        _ => None,
                                    })
                                    .or_else(|| match frame.locals_any.get(name) {
                                        Some(Value::Str(v)) => Some(v.clone()),
                                        _ => None,
                                    })
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                self.stack.push(Value::Str(v));
                Ok(DispatchAction::Continue)
            }
            Instr::StoreStr(name) => {
                let v = self.stack.pop_str()?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), Value::Str(v));
                    frame.var_types.insert(name.clone(), VarTypeTag::Str);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadI64(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        local_i64(frame, name).or_else(|| {
                            match self.load_slot_value_by_name(frame, name) {
                                Some(Value::I64(v)) => Some(v),
                                _ => None,
                            }
                        })
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                local_i64(frame, name).or_else(|| {
                                    match self.load_slot_value_by_name(frame, name) {
                                        Some(Value::I64(v)) => Some(v),
                                        _ => None,
                                    }
                                })
                            })
                        } else {
                            None
                        }
                    });
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
            Instr::StoreSlot(slot) => {
                let val = self.stack.pop_value()?;
                let val = self.value_for_slot_storage(val);
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_value(*slot, val) {
                        // INTERNAL: slot index is compiler-generated; out-of-bounds means compiler produced an invalid slot
                        return Err(VmError::InternalError(format!(
                            "StoreSlot: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotArray: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotArray: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotTuple: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotTuple: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotNamedTuple: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotNamedTuple: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotDict: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotDict: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotSet: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotSet: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotStruct: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotStruct: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotRange: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotRange: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotRng: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotRng: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotGenerator: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotGenerator: slot out of bounds: {}",
                            slot
                        )));
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
                            Err(VmError::InternalError(format!(
                                "LoadSlotI64: expected numeric in {}, got {:?}",
                                name, value
                            )))
                        }
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotI64: slot out of bounds: {}",
                            slot
                        ))),
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
                                return Err(VmError::InternalError(format!(
                                    "LoadSlotI64: expected numeric in {}, got {:?}",
                                    name, value
                                )));
                            }
                            Some(None) => {
                                let name = self.slot_name_for_frame(frame, *slot);
                                self.raise(VmError::UndefVarError(name))?;
                                return Ok(DispatchAction::Continue);
                            }
                            None => {
                                return Err(VmError::InternalError(format!(
                                    "LoadSlotI64: slot out of bounds: {}",
                                    slot
                                )));
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotI64: slot out of bounds: {}",
                            slot
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotF64(slot) => {
                if let Some(frame) = self.frames.last() {
                    if matches!(
                        self.code.get(self.ip),
                        Some(Instr::CallDynamicBinaryBoth(
                            crate::intrinsics::Intrinsic::AddFloat,
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotF64: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotF64: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotBool: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotBool: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotF32: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotF32: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotF16: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotF16: slot out of bounds: {}",
                            slot
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadSlotStr(slot) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.slot_str(*slot) {
                        self.stack.push(Value::Str(v.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                    match frame.locals_slots.get(*slot) {
                        Some(Some(Value::Str(v))) => {
                            self.stack.push(Value::Str(v.clone()));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotStr: slot out of bounds: {}",
                            slot
                        ))),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotStr(slot) => {
                let val = self.stack.pop_str()?;
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_str(*slot, val) {
                        return Err(VmError::InternalError(format!(
                            "StoreSlotStr: slot out of bounds: {}",
                            slot
                        )));
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
                        Some(Some(_)) => Err(VmError::InternalError(
                            "LoadSlotChar: expected Char".to_string(),
                        )),
                        Some(None) => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(DispatchAction::Continue)
                        }
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotChar: slot out of bounds: {}",
                            slot
                        ))),
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(DispatchAction::Continue)
                }
            }
            Instr::StoreSlotChar(slot) => {
                let val = self.stack.pop_char()?;
                if let Some(frame) = self.frames.last_mut() {
                    if !frame.set_slot_char(*slot, val) {
                        return Err(VmError::InternalError(format!(
                            "StoreSlotChar: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotSymbol: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotSymbol: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotNarrowInt: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotNarrowInt: slot out of bounds: {}",
                            slot
                        )));
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
                        None => Err(VmError::InternalError(format!(
                            "LoadSlotNothing: slot out of bounds: {}",
                            slot
                        ))),
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
                        return Err(VmError::InternalError(format!(
                            "StoreSlotNothing: slot out of bounds: {}",
                            slot
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::LoadAny(name) => {
                let found = self.try_load_from_frame(name, self.frames.len().saturating_sub(1));
                if !found {
                    // Check type_bindings from all frames (for where clause type parameters)
                    // Type parameters like T from `where T` should be accessible from nested function calls
                    let mut type_binding_found = false;
                    for frame_idx in (0..self.frames.len()).rev() {
                        if let Some(frame) = self.frames.get(frame_idx) {
                            if let Some(julia_type) = frame.type_bindings.get(name) {
                                self.stack
                                    .push(Value::DataType(Box::new(julia_type.clone())));
                                type_binding_found = true;
                                break;
                            }
                        }
                        let inferred = self.infer_type_binding_from_frame_args(name, frame_idx);
                        if let Some(julia_type) = inferred {
                            if let Some(frame) = self.frames.get_mut(frame_idx) {
                                frame.type_bindings.insert(name.clone(), julia_type.clone());
                            }
                            self.stack.push(Value::DataType(Box::new(julia_type)));
                            type_binding_found = true;
                            break;
                        }
                    }
                    if !type_binding_found {
                        if self.frames.len() > 1 {
                            let global_found = self.try_load_from_frame(name, 0);
                            if !global_found {
                                self.raise(VmError::UndefVarError(name.clone()))?;
                                return Ok(DispatchAction::Continue);
                            }
                        } else {
                            self.raise(VmError::UndefVarError(name.clone()))?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadGlobalAny(name) => {
                if self.frames.is_empty() || !self.try_load_from_frame(name, 0) {
                    self.raise(VmError::UndefVarError(name.clone()))?;
                    return Ok(DispatchAction::Continue);
                }
                Ok(DispatchAction::Continue)
            }
            Instr::LoadTypeBinding(name) => {
                // Integer value type parameters (e.g. `N` in `Arr{T,N}` bound to
                // `2` from `Arr{Int,2}`) are stored as a value local by
                // bind_type_params. Prefer that so the method body sees the `Int`
                // value, not a `DataType` (Issue #6625). Regular type parameters
                // (`T`) are never placed in `locals_any`, so they are unaffected.
                if let Some(frame) = self.frames.last() {
                    if let Some(v @ (Value::I64(_) | Value::Bool(_) | Value::Char(_))) =
                        frame.locals_any.get(name)
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
                Ok(DispatchAction::Continue)
            }
            Instr::StoreGlobalAny(name) => {
                // Route the write to the module-level frame (frame 0) so a
                // `global x` assignment inside a function updates the top-level
                // binding (Issues #5548, #5549). Top-level globals live in
                // slots, which reads consult first, so this is slot-aware.
                let val = self.stack.pop_value()?;
                self.store_global_value(name, val);
                Ok(DispatchAction::Continue)
            }

            // === Fused load+arithmetic instructions ===
            Instr::LoadAddI64(name) => {
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| local_i64(frame, name))
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| local_i64(frame, name))
                        } else {
                            None
                        }
                    });
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

            Instr::LoadSubI64(name) => {
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| local_i64(frame, name))
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| local_i64(frame, name))
                        } else {
                            None
                        }
                    });
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
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| local_i64(frame, name))
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| local_i64(frame, name))
                        } else {
                            None
                        }
                    });
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
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| local_i64(frame, name))
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| local_i64(frame, name))
                        } else {
                            None
                        }
                    });
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        if var == 0 {
                            self.raise(VmError::DivisionByZero)?;
                            return Ok(DispatchAction::Continue);
                        }
                        self.stack.push(Value::I64(stack_val % var));
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
                            return Err(VmError::InternalError(format!(
                                "IncVarI64Slot: slot out of bounds: {}",
                                slot
                            )));
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
                            return Err(VmError::InternalError(format!(
                                "AddConstI64Slot: slot out of bounds: {}",
                                slot
                            )));
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
                            return Err(VmError::InternalError(format!(
                                "DecVarI64Slot: slot out of bounds: {}",
                                slot
                            )));
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

                // Check type bindings from all frames (for where clause type parameters)
                for frame_idx in (0..self.frames.len()).rev() {
                    if let Some(frame) = self.frames.get(frame_idx) {
                        if frame.type_bindings.contains_key(name) {
                            self.stack.push(Value::Bool(true));
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }

                // Check global frame
                if self.frames.len() > 1 {
                    let defined_in_global = self.is_var_defined_in_frame(name, 0);
                    self.stack.push(Value::Bool(defined_in_global));
                } else {
                    self.stack.push(Value::Bool(false));
                }

                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
