//! Shared splat expansion helper for function call handlers.
//!
//! This module centralizes the logic for expanding splatted arguments
//! (`f(args...)`) from Julia's `_apply_iterate` fast-path carriers into flat
//! argument lists.
//! Used by `call.rs`, `call_dynamic.rs`, and `sync_exec.rs`.

use super::error::VmError;
use super::value::{native_array_value_ref, StructInstance, Value};
use std::collections::HashMap;

/// Result of synchronous splat preparation.
///
/// `Raised` means a nested Julia call already transferred control to an active
/// exception handler. Callers must return `DispatchAction::Continue` without
/// turning that handled exception back into a Rust `Err` (Issue #11372).
pub(in crate::vm) enum SplatPreparation<T> {
    Ready(T),
    Raised,
}

/// Insertion-ordered keyword-argument accumulator (Issue #11383).
///
/// Julia's keyword-container semantics preserve the position of each key's
/// *first* occurrence; a later duplicate replaces the value in that existing
/// slot rather than moving it to the end. A plain `HashMap` cannot express
/// this — its iteration order is seed-dependent per process — so every stage
/// of keyword-splat preparation and binding (accumulation, unknown-keyword
/// detection, `kwargs...` catch-all materialization) threads this ordered
/// container instead. It is a drop-in replacement for the `HashMap<String, V>`
/// this authority used to pass around: `get`/`keys`/`iter`/`len`/`is_empty`
/// behave the same, `insert` additionally preserves first-occurrence position
/// on a duplicate key, and iteration order is the insertion order rather than
/// hash order.
#[derive(Debug, Clone)]
pub(crate) struct KwargsMap<V> {
    entries: Vec<(String, V)>,
    index: HashMap<String, usize>,
}

impl<V> Default for KwargsMap<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<V> KwargsMap<V> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            index: HashMap::with_capacity(capacity),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    /// Insert `value` under `key`. If `key` was already present, its value is
    /// replaced in place — the key keeps its original (first-occurrence)
    /// position, matching Julia's keyword-merge semantics (Issue #11383).
    pub(crate) fn insert(&mut self, key: String, value: V) {
        if let Some(&i) = self.index.get(&key) {
            self.entries[i].1 = value;
        } else {
            self.index.insert(key.clone(), self.entries.len());
            self.entries.push((key, value));
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(k, _)| k)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
}

impl<V> IntoIterator for KwargsMap<V> {
    type Item = (String, V);
    type IntoIter = std::vec::IntoIter<(String, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a KwargsMap<V> {
    type Item = (&'a String, &'a V);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (String, V)>,
        fn(&'a (String, V)) -> (&'a String, &'a V),
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(|(k, v)| (k, v))
    }
}

impl<V> FromIterator<(String, V)> for KwargsMap<V> {
    fn from_iter<T: IntoIterator<Item = (String, V)>>(iter: T) -> Self {
        let mut map = Self::new();
        for (key, value) in iter {
            map.insert(key, value);
        }
        map
    }
}

/// Expand one value through the structural `_apply_iterate` fast paths.
///
/// `Ok(Some(values))` means the value is a recognized carrier and has been
/// fully expanded. `Ok(None)` delegates to the VM's generic `iterate`
/// protocol; it must never be treated as a singleton merely because its
/// carrier is unknown (Issue #11372).
pub(in crate::vm) fn try_expand_splat_value_with_heap(
    arg: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<Vec<Value>>, VmError> {
    if let Value::StructRef(index) = arg {
        if struct_heap.get(*index).is_none() {
            return Err(VmError::InternalError(format!(
                "invalid StructRef during splat preparation: index {} out of bounds",
                index
            )));
        }
    }

    if let Some(arr) = native_array_value_ref(arg) {
        let borrowed = arr.borrow();
        let mut expanded = Vec::with_capacity(borrowed.len());
        for i in 0..borrowed.len() {
            expanded.push(borrowed.get(&[(i + 1) as i64])?);
        }
        return Ok(Some(expanded));
    }

    if let Value::Memory(memory) = arg {
        let memory = memory.borrow();
        let mut expanded = Vec::with_capacity(memory.len());
        for index in 0..memory.len() {
            expanded.push(memory.get(index + 1)?);
        }
        return Ok(Some(expanded));
    }

    let expanded = match arg {
        // Tuple and Core.SimpleVector splat their elements (Issue #4722).
        Value::Tuple(tuple) | Value::SimpleVector(tuple) => tuple.elements.clone(),
        // A positional NamedTuple splat yields its field values in order
        // (Issue #9786); the keyword form `f(; nt...)` is a separate path.
        Value::NamedTuple(nt) => nt.values.clone(),
        _ => return Ok(None),
    };
    Ok(Some(expanded))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::vm::value::{new_memory_ref, ArrayData, ArrayElementType, MemoryValue, TupleValue};

    fn i64_val(v: i64) -> Value {
        Value::I64(v)
    }

    #[test]
    fn structural_tuple_expands_elements() {
        // f((1, 2, 3)...) → f(1, 2, 3)
        let tuple = Value::Tuple(TupleValue::new(vec![i64_val(10), i64_val(20)]));
        let result = try_expand_splat_value_with_heap(&tuple, &[])
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], Value::I64(10)));
        assert!(matches!(result[1], Value::I64(20)));
    }

    #[test]
    fn range_delegates_to_iterate_protocol() {
        let range = Value::Range(crate::vm::value::RangeValue::unit_range(1.0, 3.0));
        assert!(try_expand_splat_value_with_heap(&range, &[])
            .unwrap()
            .is_none());
    }

    #[test]
    fn pair_delegates_to_iterate_protocol() {
        let pair = Value::Struct(StructInstance::with_name(
            0,
            "Pair".to_string(),
            vec![i64_val(1), i64_val(2)],
        ));
        assert!(try_expand_splat_value_with_heap(&pair, &[])
            .unwrap()
            .is_none());
    }

    #[test]
    fn user_array_name_delegates_to_iterate_protocol_11388() {
        let faux = Value::Struct(StructInstance::with_name(
            0,
            "Faux11388.Array".to_string(),
            vec![Value::Nothing, Value::Nothing],
        ));
        assert!(try_expand_splat_value_with_heap(&faux, &[])
            .unwrap()
            .is_none());
    }

    #[test]
    fn generic_memory_expands_directly() {
        let memory = Value::Memory(new_memory_ref(MemoryValue::new(
            ArrayData::I64(vec![1, 2]),
            ArrayElementType::I64,
            2,
        )));
        let result = try_expand_splat_value_with_heap(&memory, &[])
            .unwrap()
            .unwrap();
        assert!(matches!(result.as_slice(), [Value::I64(1), Value::I64(2)]));
    }

    #[test]
    fn structural_empty_tuple_produces_no_values() {
        // f(()...) → f()
        let tuple = Value::Tuple(TupleValue::new(vec![]));
        let result = try_expand_splat_value_with_heap(&tuple, &[])
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn kwargs_map_preserves_insertion_order_11383() {
        let mut map: KwargsMap<i64> = KwargsMap::new();
        map.insert("z".to_string(), 1);
        map.insert("a".to_string(), 2);
        let collected: Vec<_> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(collected, vec![("z".to_string(), 1), ("a".to_string(), 2)]);
    }

    #[test]
    fn kwargs_map_duplicate_key_overwrites_value_in_place_11383() {
        let mut map: KwargsMap<i64> = KwargsMap::new();
        map.insert("b".to_string(), 1);
        map.insert("a".to_string(), 2);
        map.insert("b".to_string(), 3);
        let collected: Vec<_> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(collected, vec![("b".to_string(), 3), ("a".to_string(), 2)]);
    }

    #[test]
    fn kwargs_map_from_iterator_upserts_in_place() {
        let map: KwargsMap<i64> = vec![
            ("b".to_string(), 1),
            ("a".to_string(), 2),
            ("b".to_string(), 3),
        ]
        .into_iter()
        .collect();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("b"), Some(&3));
        let collected: Vec<_> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(collected, vec![("b".to_string(), 3), ("a".to_string(), 2)]);
    }
}
