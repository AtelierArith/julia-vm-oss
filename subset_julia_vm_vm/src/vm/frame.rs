use super::value::{
    native_array_ref_value as array_value, native_array_value_ref, ArrayRef, GeneratorValue,
    NamedTupleValue, RangeValue, StrRef, SymbolValue, TupleValue, Value,
};
use crate::rng::RngInstance;
use crate::types::JuliaType;
use half::f16;
use std::collections::HashMap;
pub use subset_julia_vm_bytecode::VarTypeTag;

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

#[derive(Debug, Clone)]
pub(crate) struct LazyLocalMap<V> {
    inner: Option<HashMap<String, V>>,
}

impl<V> LazyLocalMap<V> {
    fn new() -> Self {
        Self { inner: None }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&V> {
        self.inner.as_ref()?.get(name)
    }

    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut V> {
        self.inner.as_mut()?.get_mut(name)
    }

    pub(crate) fn insert(&mut self, name: String, value: V) -> Option<V> {
        self.inner
            .get_or_insert_with(HashMap::new)
            .insert(name, value)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.iter().flat_map(|inner| inner.values())
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &String> {
        self.inner.iter().flat_map(|inner| inner.keys())
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.inner.iter().flat_map(|inner| inner.iter())
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.inner.iter_mut().flat_map(|inner| inner.values_mut())
    }

    pub(crate) fn remove(&mut self, name: &str) -> Option<V> {
        self.inner.as_mut()?.remove(name)
    }

    pub(crate) fn clear(&mut self) {
        if let Some(inner) = self.inner.as_mut() {
            inner.clear();
        }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.inner.as_ref().is_none_or(HashMap::is_empty)
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.inner.as_ref().map_or(0, HashMap::capacity)
    }

    #[cfg(test)]
    pub(crate) fn contains_key(&self, name: &str) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.contains_key(name))
    }
}

/// Runtime storage for one lexical call frame.
///
/// Namespace-consumer checklist (Issue #11051): `Vm::lookup_frame_binding` in
/// `vm/state.rs` is the authoritative general projection used by `LoadAny`,
/// closure snapshots, `isdefined`, and eval. When adding a readable namespace
/// here, add it there once and extend the namespace-consumer matrix in
/// `vm::tests`. Precedence is slot local, typed/generic local, prior
/// lexical type binding, then prior capture. The type binding must precede an
/// inherited same-named capture so an inner `where T` shadows an outer `T`
/// (Issue #11070).
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    /// Single contiguous slot store (Issue #6344). The former 19 typed
    /// "sidecar" `Vec`s were pure mirrors of this store — every write kept
    /// them in sync with the `Value` here — so typed access is now served by
    /// the `slot_*` accessor methods matching directly on this vector,
    /// eliminating 19 heap buffers per frame and the 19 sidecar writes that
    /// every slot store previously performed.
    pub locals_slots: Vec<Option<Value>>,
    pub locals_any: LazyLocalMap<Value>,
    /// Type parameter bindings from where clauses (e.g., T -> Float64)
    pub type_bindings: HashMap<String, JuliaType>,
    pub func_index: Option<usize>,
    /// Annotated optional-keyword slots whose final entry value has not yet
    /// been asserted, mapped to the number of default-materialization stores
    /// that must run before the validation self-store. A body-evaluated omitted
    /// default skips its guard store; supplied/literal defaults skip none
    /// (Issue #11135).
    pub pending_kw_default_type_checks: HashMap<usize, usize>,
    pub world_age: u64,
    /// Operand stack height at function entry, after call arguments have been
    /// consumed. Internal returns truncate back to this boundary before pushing
    /// the return value.
    pub stack_base: usize,
    /// True when this frame was entered from an `@inbounds` call context.
    pub inbounds_context: bool,
    /// Captured variables from closure environment.
    /// When calling a closure, these values are populated from the ClosureValue's captures.
    pub captured_vars: HashMap<String, Value>,
    /// Type tag cache: tracks which typed map each variable is stored in.
    /// Enables O(1) lookup dispatch and O(1) removal in StoreAny.
    pub var_types: HashMap<String, VarTypeTag>,
}

impl Frame {
    pub fn new() -> Self {
        Self::new_with_slots(0, None)
    }

    pub fn new_with_slots(slot_count: usize, func_index: Option<usize>) -> Self {
        Self {
            locals_slots: vec![None; slot_count],
            locals_any: LazyLocalMap::new(),
            type_bindings: HashMap::new(),
            func_index,
            pending_kw_default_type_checks: HashMap::new(),
            world_age: 1,
            stack_base: 0,
            inbounds_context: false,
            captured_vars: HashMap::new(),
            var_types: HashMap::new(),
        }
    }

    #[inline]
    fn store_slot(&mut self, slot: usize, value: Value) -> bool {
        if let Some(slot_ref) = self.locals_slots.get_mut(slot) {
            *slot_ref = Some(value);
            true
        } else {
            false
        }
    }

    pub(crate) fn set_slot_i64(&mut self, slot: usize, value: i64) -> bool {
        self.store_slot(slot, Value::I64(value))
    }

    pub(crate) fn set_slot_f64(&mut self, slot: usize, value: f64) -> bool {
        self.store_slot(slot, Value::F64(value))
    }

    pub(crate) fn set_slot_f32(&mut self, slot: usize, value: f32) -> bool {
        self.store_slot(slot, Value::F32(value))
    }

    pub(crate) fn set_slot_f16(&mut self, slot: usize, value: f16) -> bool {
        self.store_slot(slot, Value::F16(value))
    }

    pub(crate) fn set_slot_bool(&mut self, slot: usize, value: bool) -> bool {
        self.store_slot(slot, Value::Bool(value))
    }

    #[cfg(test)]
    pub(crate) fn set_slot_str(&mut self, slot: usize, value: impl Into<StrRef>) -> bool {
        self.store_slot(slot, Value::str_new(value))
    }

    pub(crate) fn set_slot_string_value(&mut self, slot: usize, value: Value) -> bool {
        if value.string_bytes().is_none() {
            return false;
        }
        self.store_slot(slot, value)
    }

    /// Store a Char-typed slot from a runtime value: accepts both valid
    /// `Char` and malformed `CharMalformed` carriers (Issue #8995).
    pub(crate) fn set_slot_char_value(&mut self, slot: usize, value: Value) -> bool {
        if !matches!(value, Value::Char(_) | Value::CharMalformed(_)) {
            return false;
        }
        self.store_slot(slot, value)
    }

    pub(crate) fn set_slot_symbol(&mut self, slot: usize, value: SymbolValue) -> bool {
        self.store_slot(slot, Value::Symbol(value))
    }

    pub(crate) fn set_slot_narrow_int(&mut self, slot: usize, value: Value) -> bool {
        self.store_slot(slot, value)
    }

    pub(crate) fn set_slot_nothing(&mut self, slot: usize) -> bool {
        self.store_slot(slot, Value::Nothing)
    }

    pub(crate) fn set_slot_array(&mut self, slot: usize, value: ArrayRef) -> bool {
        self.store_slot(slot, array_value(value))
    }

    pub(crate) fn set_slot_tuple(&mut self, slot: usize, value: TupleValue) -> bool {
        self.store_slot(slot, Value::Tuple(value))
    }

    pub(crate) fn set_slot_named_tuple(&mut self, slot: usize, value: NamedTupleValue) -> bool {
        self.store_slot(slot, Value::NamedTuple(value))
    }

    pub(crate) fn set_slot_struct_ref(&mut self, slot: usize, heap_idx: usize) -> bool {
        self.store_slot(slot, Value::StructRef(heap_idx))
    }

    pub(crate) fn set_slot_range(&mut self, slot: usize, value: RangeValue) -> bool {
        self.store_slot(slot, Value::Range(value))
    }

    pub(crate) fn set_slot_rng(&mut self, slot: usize, value: RngInstance) -> bool {
        self.store_slot(slot, Value::Rng(value))
    }

    pub(crate) fn set_slot_generator(&mut self, slot: usize, value: Box<GeneratorValue>) -> bool {
        self.store_slot(slot, Value::Generator(value))
    }

    pub(crate) fn set_slot_value(&mut self, slot: usize, value: Value) -> bool {
        self.store_slot(slot, value)
    }

    /// Typed slot accessors (Issue #6344). These replace the former typed
    /// sidecar vectors: each reads `locals_slots` and yields the payload only
    /// when the slot currently holds a value of the requested type.
    #[inline]
    pub(crate) fn slot_i64(&self, slot: usize) -> Option<i64> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::I64(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_f64(&self, slot: usize) -> Option<f64> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::F64(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_f32(&self, slot: usize) -> Option<f32> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::F32(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_f16(&self, slot: usize) -> Option<f16> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::F16(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_bool(&self, slot: usize) -> Option<bool> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    // Issue #10559: no longer test-only — the typed-loop String slot class
    // (`load_str_slot` in executable.rs) reads live-in `String` locals through
    // this accessor.
    #[inline]
    pub(crate) fn slot_str(&self, slot: usize) -> Option<&StrRef> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Str(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_string_value(&self, slot: usize) -> Option<&Value> {
        match self.locals_slots.get(slot)?.as_ref()? {
            v if v.string_bytes().is_some() => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_char(&self, slot: usize) -> Option<char> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Char(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_symbol(&self, slot: usize) -> Option<&SymbolValue> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Symbol(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_narrow_int(&self, slot: usize) -> Option<&Value> {
        self.locals_slots
            .get(slot)?
            .as_ref()
            .filter(|v| is_narrow_int_value(v))
    }

    #[inline]
    pub(crate) fn slot_nothing(&self, slot: usize) -> bool {
        matches!(self.locals_slots.get(slot), Some(Some(Value::Nothing)))
    }

    #[inline]
    pub(crate) fn slot_array(&self, slot: usize) -> Option<&ArrayRef> {
        native_array_value_ref(self.locals_slots.get(slot)?.as_ref()?)
    }

    #[inline]
    pub(crate) fn slot_tuple(&self, slot: usize) -> Option<&TupleValue> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Tuple(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_named_tuple(&self, slot: usize) -> Option<&NamedTupleValue> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::NamedTuple(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_struct(&self, slot: usize) -> Option<usize> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::StructRef(v) => Some(*v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_range(&self, slot: usize) -> Option<&RangeValue> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Range(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_rng(&self, slot: usize) -> Option<&RngInstance> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Rng(v) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn slot_generator(&self, slot: usize) -> Option<&GeneratorValue> {
        match self.locals_slots.get(slot)?.as_ref()? {
            Value::Generator(v) => Some(v),
            _ => None,
        }
    }

    /// O(1) variable lookup: check tag first, fall back to cascade for untagged vars.
    pub fn get_local(&self, name: &str) -> Option<Value> {
        if let Some(tag) = self.var_types.get(name) {
            self.get_by_tag(name, *tag)
        } else {
            self.get_by_cascade(name)
        }
    }

    /// Direct lookup using the type tag -- O(1) dispatch to the correct map.
    fn get_by_tag(&self, name: &str, tag: VarTypeTag) -> Option<Value> {
        match tag {
            VarTypeTag::I64 => self.locals_any.get(name).and_then(|v| match v {
                Value::I64(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::F64 => self.locals_any.get(name).and_then(|v| match v {
                Value::F64(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::F32 => self.locals_any.get(name).and_then(|v| match v {
                Value::F32(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::F16 => self.locals_any.get(name).and_then(|v| match v {
                Value::F16(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Str => self.locals_any.get(name).and_then(|v| match v {
                Value::Str(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Char => self.locals_any.get(name).and_then(|v| match v {
                Value::Char(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Array => self
                .locals_any
                .get(name)
                .and_then(|v| native_array_value_ref(v).map(|arr| array_value(arr.clone()))),
            VarTypeTag::Tuple => self.locals_any.get(name).and_then(|v| match v {
                Value::Tuple(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::NamedTuple => self.locals_any.get(name).and_then(|v| match v {
                Value::NamedTuple(_) => Some(v.clone()),
                _ => None,
            }),
            // `Value::Dict`/`Value::Set` were retired (Issues #6731/#6732); a
            // Dict/Set local is a StructRef resolved via the Struct/Any tags.
            VarTypeTag::Dict | VarTypeTag::Set => None,
            VarTypeTag::Struct => self.locals_any.get(name).and_then(|v| match v {
                Value::StructRef(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Range => self.locals_any.get(name).and_then(|v| match v {
                Value::Range(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Rng => self.locals_any.get(name).and_then(|v| match v {
                Value::Rng(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Generator => self.locals_any.get(name).and_then(|v| match v {
                Value::Generator(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Any => self.locals_any.get(name).cloned(),
            VarTypeTag::NarrowInt => self
                .locals_any
                .get(name)
                .filter(|v| is_narrow_int_value(v))
                .cloned(),
            VarTypeTag::Nothing => self.locals_any.get(name).and_then(|v| match v {
                Value::Nothing => Some(Value::Nothing),
                _ => None,
            }),
            VarTypeTag::Bool => self.locals_any.get(name).and_then(|v| match v {
                Value::Bool(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::ValSymbol => self.locals_any.get(name).and_then(|v| match v {
                Value::Symbol(_) => Some(v.clone()),
                _ => None,
            }),
            VarTypeTag::Symbol => self.locals_any.get(name).and_then(|v| match v {
                Value::Symbol(_) => Some(v.clone()),
                _ => None,
            }),
        }
    }

    /// Fallback linear search for variables without a tag (safety net).
    fn get_by_cascade(&self, name: &str) -> Option<Value> {
        self.locals_any.get(name).cloned()
    }

    /// O(1) removal: remove variable from its tagged map, then clear the tag.
    pub fn remove_var(&mut self, name: &str) {
        if let Some(tag) = self.var_types.remove(name) {
            self.remove_by_tag(name, tag);
        } else {
            self.remove_from_all(name);
        }
    }

    /// Targeted removal from a specific typed map.
    fn remove_by_tag(&mut self, name: &str, tag: VarTypeTag) {
        match tag {
            VarTypeTag::I64 => {
                self.locals_any.remove(name);
            }
            VarTypeTag::F64 => {
                self.locals_any.remove(name);
            }
            VarTypeTag::F32 => {
                self.locals_any.remove(name);
            }
            VarTypeTag::F16 => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Str => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Char => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Array => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Tuple => {
                self.locals_any.remove(name);
            }
            VarTypeTag::NamedTuple => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Dict => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Set => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Struct => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Range => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Rng => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Generator => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Any => {
                self.locals_any.remove(name);
            }
            VarTypeTag::NarrowInt => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Nothing => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Bool => {
                self.locals_any.remove(name);
            }
            VarTypeTag::ValSymbol => {
                self.locals_any.remove(name);
            }
            VarTypeTag::Symbol => {
                self.locals_any.remove(name);
            }
        }
    }

    /// Fallback: remove from all typed maps (for untagged variables).
    fn remove_from_all(&mut self, name: &str) {
        self.locals_any.remove(name);
    }

    /// Empty a retired frame before it is returned to the VM's frame pool
    /// (Issue #5172).
    ///
    /// `HashMap::clear`/`Vec::clear` drop all entries but
    /// *retain* the allocated table/buffer, so a frame pulled back out of the
    /// pool skips the heap allocations that `new_with_slots` would otherwise
    /// perform for every map on every call. Clearing at retirement (rather than
    /// at reuse) also releases the contained `Value`s — e.g. refcounted array
    /// handles and struct-heap indices — promptly, instead of pinning them in
    /// the pool until the slot is next reused.
    pub fn clear_for_pool(&mut self) {
        self.locals_slots.clear();
        self.locals_any.clear();
        self.type_bindings.clear();
        self.pending_kw_default_type_checks.clear();
        self.captured_vars.clear();
        self.var_types.clear();
    }

    /// Prepare a pooled frame (already emptied by [`clear_for_pool`]) for a fresh
    /// call: size `locals_slots` to `slot_count` and reset the scalar bookkeeping
    /// fields to their fresh-frame defaults (Issue #5172).
    ///
    /// The maps are assumed already empty; only `locals_slots` is (re)filled with
    /// `None` and the scalars are set. Keeping this separate from
    /// [`clear_for_pool`] lets the VM release a frame's values at retirement while
    /// deferring the (slot-count-dependent) re-initialization to reuse time.
    pub fn prepare_for_reuse(&mut self, slot_count: usize, func_index: Option<usize>) {
        debug_assert!(
            self.locals_slots.is_empty(),
            "prepare_for_reuse expects a frame cleared via clear_for_pool"
        );
        self.locals_slots.resize(slot_count, None);
        self.func_index = func_index;
        self.pending_kw_default_type_checks.clear();
        self.stack_base = 0;
        self.inbounds_context = false;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Handler {
    pub catch_ip: Option<usize>,
    pub finally_ip: Option<usize>,
    pub stack_len: usize,
    pub frame_len: usize,
    pub return_ip_len: usize,
    /// Root lexical-environment depth when this handler was installed. Error
    /// unwind truncates task-local module/main scopes back to this boundary.
    pub lexical_scope_len: usize,
    pub caught_exception_len: usize,
    /// Depth of `Vm::pending_finally_rethrows` when this handler was pushed
    /// (Issue #11306). `handle_error` truncates the stack back to this depth
    /// whenever the handler is popped, so a nested try/catch entered *inside*
    /// a `finally` body can never see, let alone clear, the marker an
    /// *enclosing* finally pushed for the exception whose unwind entered it.
    pub finally_pending_len: usize,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn clear_then_prepare_resets_all_state() {
        let mut frame = Frame::new_with_slots(3, Some(7));
        // Populate a representative sample of the typed maps + bookkeeping.
        frame.locals_any.insert("a".to_string(), Value::I64(1));
        frame
            .locals_any
            .insert("b".to_string(), Value::str_new("x".to_string()));
        frame.locals_any.insert("c".to_string(), Value::Nothing);
        frame.var_types.insert("a".to_string(), VarTypeTag::I64);
        frame.var_types.insert("c".to_string(), VarTypeTag::Nothing);
        frame.captured_vars.insert("d".to_string(), Value::I64(2));
        frame.locals_slots[0] = Some(Value::I64(9));
        frame.pending_kw_default_type_checks.insert(1, 1);
        frame.stack_base = 42;
        frame.inbounds_context = true;

        // Pool retirement empties everything (releasing values).
        frame.clear_for_pool();
        assert!(frame.locals_any.is_empty());
        assert!(frame.var_types.is_empty());
        assert!(frame.captured_vars.is_empty());
        assert!(frame.locals_slots.is_empty() && frame.pending_kw_default_type_checks.is_empty());

        // Reuse re-sizes the slots and resets the scalar fields.
        frame.prepare_for_reuse(2, Some(11));
        assert_eq!(frame.locals_slots.len(), 2);
        assert!(frame.locals_slots.iter().all(|s| s.is_none()));
        assert_eq!(frame.func_index, Some(11));
        assert_eq!(frame.stack_base, 0);
        assert!(!frame.inbounds_context);
    }

    #[test]
    fn typed_slot_accessors_read_primitive_values_issue_6344() {
        let mut frame = Frame::new_with_slots(9, None);

        assert!(frame.set_slot_i64(0, 11));
        assert!(frame.set_slot_f64(1, 2.5));
        assert!(frame.set_slot_f32(2, 3.5));
        assert!(frame.set_slot_f16(3, f16::from_f32(4.5)));
        assert!(frame.set_slot_bool(4, true));
        assert!(frame.set_slot_str(5, "value".to_string()));
        assert!(frame.set_slot_char_value(6, Value::Char('x')));
        assert!(frame.set_slot_narrow_int(7, Value::U64(7)));
        assert!(frame.set_slot_nothing(8));

        assert!(matches!(frame.locals_slots[0], Some(Value::I64(11))));
        assert_eq!(frame.slot_i64(0), Some(11));
        assert!(matches!(frame.locals_slots[1], Some(Value::F64(v)) if v == 2.5));
        assert_eq!(frame.slot_f64(1), Some(2.5));
        assert!(matches!(frame.locals_slots[2], Some(Value::F32(v)) if v == 3.5));
        assert_eq!(frame.slot_f32(2), Some(3.5));
        assert!(matches!(frame.locals_slots[3], Some(Value::F16(v)) if v == f16::from_f32(4.5)));
        assert_eq!(frame.slot_f16(3), Some(f16::from_f32(4.5)));
        assert!(matches!(frame.locals_slots[4], Some(Value::Bool(true))));
        assert_eq!(frame.slot_bool(4), Some(true));
        assert!(matches!(frame.locals_slots[5], Some(Value::Str(ref v)) if v.as_ref() == "value"));
        assert_eq!(frame.slot_str(5).map(|s| s.as_ref()), Some("value"));
        assert!(matches!(frame.locals_slots[6], Some(Value::Char('x'))));
        assert_eq!(frame.slot_char(6), Some('x'));
        assert!(matches!(frame.locals_slots[7], Some(Value::U64(7))));
        assert!(matches!(frame.slot_narrow_int(7), Some(Value::U64(7))));
        assert!(matches!(frame.locals_slots[8], Some(Value::Nothing)));
        assert!(frame.slot_nothing(8));

        // A typed accessor only matches the type currently stored.
        assert_eq!(frame.slot_str(0), None);
        assert_eq!(frame.slot_i64(1), None);
        // Out-of-bounds slots read as empty rather than panicking.
        assert_eq!(frame.slot_i64(99), None);
        assert!(!frame.slot_nothing(99));

        assert!(frame.set_slot_value(0, Value::str_new("changed".to_string())));
        assert_eq!(frame.slot_i64(0), None);
        assert_eq!(frame.slot_str(0).map(|s| s.as_ref()), Some("changed"));
        assert!(
            matches!(frame.locals_slots[0], Some(Value::Str(ref v)) if v.as_ref() == "changed")
        );
    }

    #[test]
    fn typed_slot_accessors_read_container_values_issue_6344() {
        use crate::vm::value::{new_array_ref, ArrayValue};

        let mut frame = Frame::new_with_slots(9, None);
        let array = new_array_ref(ArrayValue::ones_i64(vec![3]));
        let tuple = TupleValue::new(vec![Value::I64(1), Value::Bool(true)]);
        let named_tuple = NamedTupleValue::new(
            vec!["a".to_string(), "b".to_string()],
            vec![Value::I64(1), Value::Bool(true)],
        )
        .expect("valid named tuple");
        let range = RangeValue::unit_range(1.0, 3.0);
        let rng = RngInstance::stable(1);
        let generator = Box::new(GeneratorValue::eager(Value::Tuple(tuple.clone())));

        assert!(frame.set_slot_array(0, array.clone()));
        assert!(frame.set_slot_tuple(1, tuple.clone()));
        assert!(frame.set_slot_named_tuple(2, named_tuple.clone()));
        assert!(frame.set_slot_struct_ref(5, 17));
        assert!(frame.set_slot_range(6, range.clone()));
        assert!(frame.set_slot_rng(7, rng.clone()));
        assert!(frame.set_slot_generator(8, generator.clone()));

        assert!(frame.locals_slots[0]
            .as_ref()
            .is_some_and(|v| native_array_value_ref(v).is_some()));
        assert!(frame.slot_array(0).is_some());
        assert!(
            matches!(frame.locals_slots[1], Some(Value::Tuple(ref v)) if v.elements.len() == 2)
        );
        assert_eq!(frame.slot_tuple(1).map(|v| v.elements.len()), Some(2));
        assert!(matches!(frame.locals_slots[2], Some(Value::NamedTuple(_))));
        assert!(frame.slot_named_tuple(2).is_some());
        assert!(matches!(frame.locals_slots[5], Some(Value::StructRef(17))));
        assert_eq!(frame.slot_struct(5), Some(17));
        assert!(matches!(frame.locals_slots[6], Some(Value::Range(_))));
        assert!(frame.slot_range(6).is_some());
        assert!(matches!(frame.locals_slots[7], Some(Value::Rng(_))));
        assert!(frame.slot_rng(7).is_some());
        assert!(matches!(frame.locals_slots[8], Some(Value::Generator(_))));
        assert!(frame.slot_generator(8).is_some());

        assert!(frame.set_slot_value(5, Value::I64(99)));
        assert_eq!(frame.slot_struct(5), None);
        assert_eq!(frame.slot_i64(5), Some(99));
    }

    #[test]
    fn clear_for_pool_retains_map_capacity() {
        let mut frame = Frame::new_with_slots(0, None);
        for i in 0..32 {
            frame.locals_any.insert(format!("v{i}"), Value::I64(i));
        }
        let cap_before = frame.locals_any.capacity();
        assert!(cap_before > 0);

        frame.clear_for_pool();

        // clear() retains the allocated table -- this is the whole point of the
        // pool: a recycled frame avoids re-allocating its backing maps.
        assert!(frame.locals_any.is_empty());
        assert_eq!(frame.locals_any.capacity(), cap_before);
    }

    #[test]
    fn empty_frames_do_not_allocate_legacy_local_maps_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);

        assert_eq!(frame.locals_any.capacity(), 0);

        frame.locals_any.insert("x".to_string(), Value::I64(1));
        assert!(frame.locals_any.capacity() > 0);
    }

    #[test]
    fn nothing_locals_use_any_carrier_with_nothing_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert("x".to_string(), Value::Nothing);
        frame.var_types.insert("x".to_string(), VarTypeTag::Nothing);

        assert!(matches!(frame.get_local("x"), Some(Value::Nothing)));
        frame.remove_var("x");
        assert!(frame.get_local("x").is_none());
        assert!(!frame.locals_any.contains_key("x"));
    }

    #[test]
    fn rng_locals_use_any_carrier_with_rng_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        let rng = RngInstance::stable(1);
        frame.locals_any.insert("r".to_string(), Value::Rng(rng));
        frame.var_types.insert("r".to_string(), VarTypeTag::Rng);

        assert!(matches!(frame.get_local("r"), Some(Value::Rng(_))));
        frame.remove_var("r");
        assert!(frame.get_local("r").is_none());
        assert!(!frame.locals_any.contains_key("r"));
    }

    #[test]
    fn val_symbol_locals_use_any_carrier_with_val_symbol_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame
            .locals_any
            .insert("mode".to_string(), Value::Symbol(SymbolValue::new("fast")));
        frame
            .var_types
            .insert("mode".to_string(), VarTypeTag::ValSymbol);

        assert!(
            matches!(frame.get_local("mode"), Some(Value::Symbol(ref s)) if s.as_str() == "fast")
        );
        frame.remove_var("mode");
        assert!(frame.get_local("mode").is_none());
        assert!(!frame.locals_any.contains_key("mode"));
    }

    #[test]
    fn narrow_int_locals_use_any_carrier_with_narrow_int_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert("x".to_string(), Value::I8(7));
        frame
            .var_types
            .insert("x".to_string(), VarTypeTag::NarrowInt);

        assert!(matches!(frame.get_local("x"), Some(Value::I8(7))));
        frame.remove_var("x");
        assert!(frame.get_local("x").is_none());
        assert!(!frame.locals_any.contains_key("x"));
    }

    #[test]
    fn f16_locals_use_any_carrier_with_f16_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        let value = f16::from_f32(1.5);
        frame.locals_any.insert("x".to_string(), Value::F16(value));
        frame.var_types.insert("x".to_string(), VarTypeTag::F16);

        assert!(matches!(frame.get_local("x"), Some(Value::F16(v)) if v == value));
        frame.remove_var("x");
        assert!(frame.get_local("x").is_none());
        assert!(!frame.locals_any.contains_key("x"));
    }

    #[test]
    fn f32_locals_use_any_carrier_with_f32_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert("x".to_string(), Value::F32(1.5));
        frame.var_types.insert("x".to_string(), VarTypeTag::F32);

        assert!(matches!(frame.get_local("x"), Some(Value::F32(v)) if v == 1.5));
        frame.remove_var("x");
        assert!(frame.get_local("x").is_none());
        assert!(!frame.locals_any.contains_key("x"));
    }

    #[test]
    fn f64_locals_use_any_carrier_with_f64_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert("x".to_string(), Value::F64(1.5));
        frame.var_types.insert("x".to_string(), VarTypeTag::F64);

        assert!(matches!(frame.get_local("x"), Some(Value::F64(v)) if v == 1.5));
        frame.remove_var("x");
        assert!(frame.get_local("x").is_none());
        assert!(!frame.locals_any.contains_key("x"));
    }

    #[test]
    fn i64_locals_use_any_carrier_with_i64_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert("x".to_string(), Value::I64(42));
        frame.var_types.insert("x".to_string(), VarTypeTag::I64);

        assert!(matches!(frame.get_local("x"), Some(Value::I64(42))));
        frame.remove_var("x");
        assert!(frame.get_local("x").is_none());
        assert!(!frame.locals_any.contains_key("x"));
    }

    #[test]
    fn bool_locals_use_any_carrier_with_bool_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame
            .locals_any
            .insert("flag".to_string(), Value::Bool(true));
        frame.var_types.insert("flag".to_string(), VarTypeTag::Bool);

        assert!(matches!(frame.get_local("flag"), Some(Value::Bool(true))));
        frame.remove_var("flag");
        assert!(frame.get_local("flag").is_none());
        assert!(!frame.locals_any.contains_key("flag"));
    }

    #[test]
    fn char_locals_use_any_carrier_with_char_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert("ch".to_string(), Value::Char('x'));
        frame.var_types.insert("ch".to_string(), VarTypeTag::Char);

        assert!(matches!(frame.get_local("ch"), Some(Value::Char('x'))));
        frame.remove_var("ch");
        assert!(frame.get_local("ch").is_none());
        assert!(!frame.locals_any.contains_key("ch"));
    }

    #[test]
    fn str_locals_use_any_carrier_with_str_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame
            .locals_any
            .insert("s".to_string(), Value::str_new("abc".to_string()));
        frame.var_types.insert("s".to_string(), VarTypeTag::Str);

        assert!(matches!(frame.get_local("s"), Some(Value::Str(ref v)) if v.as_ref() == "abc"));
        frame.remove_var("s");
        assert!(frame.get_local("s").is_none());
        assert!(!frame.locals_any.contains_key("s"));
    }

    #[test]
    fn range_locals_use_any_carrier_with_range_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert(
            "r".to_string(),
            Value::Range(RangeValue::unit_range(1.0, 3.0)),
        );
        frame.var_types.insert("r".to_string(), VarTypeTag::Range);

        assert!(matches!(frame.get_local("r"), Some(Value::Range(_))));
        frame.remove_var("r");
        assert!(frame.get_local("r").is_none());
        assert!(!frame.locals_any.contains_key("r"));
    }

    #[test]
    fn tuple_locals_use_any_carrier_with_tuple_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame.locals_any.insert(
            "t".to_string(),
            Value::Tuple(TupleValue::new(vec![Value::I64(1), Value::Bool(true)])),
        );
        frame.var_types.insert("t".to_string(), VarTypeTag::Tuple);

        assert!(matches!(frame.get_local("t"), Some(Value::Tuple(ref v)) if v.elements.len() == 2));
        frame.remove_var("t");
        assert!(frame.get_local("t").is_none());
        assert!(!frame.locals_any.contains_key("t"));
    }

    #[test]
    fn named_tuple_locals_use_any_carrier_with_named_tuple_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        let named = NamedTupleValue::new(
            vec!["a".to_string(), "b".to_string()],
            vec![Value::I64(1), Value::str_new("x".to_string())],
        )
        .expect("valid named tuple");
        frame
            .locals_any
            .insert("nt".to_string(), Value::NamedTuple(named));
        frame
            .var_types
            .insert("nt".to_string(), VarTypeTag::NamedTuple);

        assert!(
            matches!(frame.get_local("nt"), Some(Value::NamedTuple(ref v)) if v.names.len() == 2)
        );
        frame.remove_var("nt");
        assert!(frame.get_local("nt").is_none());
        assert!(!frame.locals_any.contains_key("nt"));
    }

    #[test]
    fn array_locals_use_any_carrier_with_array_tag_issue_5081() {
        use crate::vm::value::{new_array_ref, ArrayValue};

        let mut frame = Frame::new_with_slots(0, None);
        let array = new_array_ref(ArrayValue::ones_i64(vec![3]));
        frame
            .locals_any
            .insert("a".to_string(), array_value(array.clone()));
        frame.var_types.insert("a".to_string(), VarTypeTag::Array);

        assert!(frame
            .get_local("a")
            .as_ref()
            .is_some_and(|v| native_array_value_ref(v).is_some()));
        frame.remove_var("a");
        assert!(frame.get_local("a").is_none());
        assert!(!frame.locals_any.contains_key("a"));
    }

    #[test]
    fn struct_locals_use_any_carrier_with_struct_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        frame
            .locals_any
            .insert("s".to_string(), Value::StructRef(17));
        frame.var_types.insert("s".to_string(), VarTypeTag::Struct);

        assert!(matches!(frame.get_local("s"), Some(Value::StructRef(17))));
        frame.remove_var("s");
        assert!(frame.get_local("s").is_none());
        assert!(!frame.locals_any.contains_key("s"));
    }

    #[test]
    fn generator_locals_use_any_carrier_with_generator_tag_issue_5081() {
        let mut frame = Frame::new_with_slots(0, None);
        let generator = Box::new(GeneratorValue::eager(Value::Tuple(TupleValue::new(vec![
            Value::I64(1),
        ]))));
        frame
            .locals_any
            .insert("g".to_string(), Value::Generator(generator));
        frame
            .var_types
            .insert("g".to_string(), VarTypeTag::Generator);

        assert!(matches!(frame.get_local("g"), Some(Value::Generator(_))));
        frame.remove_var("g");
        assert!(frame.get_local("g").is_none());
        assert!(!frame.locals_any.contains_key("g"));
    }
}
