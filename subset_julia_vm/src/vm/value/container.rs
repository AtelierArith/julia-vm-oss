//! Container types - Various small container types for Julia values.
//!
//! This module contains:
//! - `GeneratorValue`: Lazy generator for map operations
//! - `NamedTupleValue`: Tuple with named fields
//! - `PairsValue`: Base.Pairs for kwargs

// SAFETY: i64→usize casts for NamedTupleValue::get_by_index are guarded by
// `index < 1 || index as usize > len`; isize→usize casts in DictValue::insert
// are guarded by `if pos >= 0` and `(-avail - 1)` patterns that ensure non-negative values.
#![allow(clippy::cast_sign_loss)]
//! - `DictKey`, `DictValue`: Dictionary types
//! - `SetValue`: Set type
//! - `ComposedFunctionValue`: Composed functions (f ∘ g)
//! - `ExprValue`: Julia Expr for metaprogramming

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::super::error::VmError;
use super::macro_::SymbolValue;
use super::new_array_ref;
use super::ArrayValue;
use super::Value;

/// Generator value: lazy iterator that applies a function to each element.
///
/// Julia's Generator is defined as:
/// ```julia
/// struct Generator{I, F}
///     f::F
///     iter::I
/// end
/// ```
///
/// When iterated, it yields `f(x)` for each `x` in `iter`.
#[derive(Debug, Clone)]
pub struct GeneratorValue {
    /// Index of the function to apply (in the VM's function table)
    pub func_index: usize,
    /// The underlying iterator (Array, Range, Tuple, etc.)
    pub iter: Box<Value>,
}

impl GeneratorValue {
    pub fn new(func_index: usize, iter: Value) -> Self {
        Self {
            func_index,
            iter: Box::new(iter),
        }
    }
}

/// Named tuple value: tuple with named fields
#[derive(Debug, Clone)]
pub struct NamedTupleValue {
    pub names: Vec<String>,
    pub values: Vec<Value>,
}

impl NamedTupleValue {
    pub fn new(names: Vec<String>, values: Vec<Value>) -> Result<Self, VmError> {
        if names.len() != values.len() {
            return Err(VmError::NamedTupleLengthMismatch {
                names_count: names.len(),
                values_count: values.len(),
            });
        }
        Ok(Self { names, values })
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get_by_name(&self, name: &str) -> Result<&Value, VmError> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|idx| &self.values[idx])
            .ok_or_else(|| VmError::NamedTupleFieldNotFound(name.to_string()))
    }

    pub fn get_by_index(&self, index: i64) -> Result<&Value, VmError> {
        if index < 1 || index as usize > self.values.len() {
            return Err(VmError::TupleIndexOutOfBounds {
                index,
                length: self.values.len(),
            });
        }
        Ok(&self.values[(index - 1) as usize])
    }

    pub fn field_names(&self) -> &[String] {
        &self.names
    }
}

/// Base.Pairs value: wrapper for kwargs that matches Julia's Base.Pairs type
/// In Julia, kwargs... collects keyword arguments as Base.Pairs, not NamedTuple.
/// Base.Pairs supports: length, keys, values, getindex with Symbol
/// Base.Pairs does NOT support: dot notation (kwargs.a is an error)
#[derive(Debug, Clone)]
pub struct PairsValue {
    /// The underlying data as a NamedTuple
    pub data: NamedTupleValue,
}

impl PairsValue {
    pub fn new(names: Vec<String>, values: Vec<Value>) -> Result<Self, VmError> {
        Ok(Self {
            data: NamedTupleValue::new(names, values)?,
        })
    }

    pub fn from_named_tuple(nt: NamedTupleValue) -> Self {
        Self { data: nt }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get value by symbol name (kwargs[:key])
    pub fn get_by_symbol(&self, name: &str) -> Result<&Value, VmError> {
        self.data.get_by_name(name)
    }

    /// Get keys as a tuple of symbols
    pub fn keys(&self) -> Vec<SymbolValue> {
        self.data.names.iter().map(SymbolValue::new).collect()
    }

    /// Get values as a NamedTuple (Julia compatibility)
    pub fn values(&self) -> &NamedTupleValue {
        &self.data
    }
}

/// Dictionary key: supports String, I64, or Symbol
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum DictKey {
    Str(String),
    I64(i64),
    Symbol(String),
}

impl DictKey {
    pub fn from_value(v: &Value) -> Result<Self, VmError> {
        match v {
            Value::Str(s) => Ok(DictKey::Str(s.clone())),
            Value::I64(i) => Ok(DictKey::I64(*i)),
            Value::Symbol(sym) => Ok(DictKey::Symbol(sym.as_str().to_string())),
            _ => Err(VmError::InvalidDictKey(format!("{:?}", v))),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            DictKey::Str(s) => Value::Str(s.clone()),
            DictKey::I64(i) => Value::I64(*i),
            DictKey::Symbol(s) => Value::Symbol(super::macro_::SymbolValue::new(s.clone())),
        }
    }
}

impl std::fmt::Display for DictKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DictKey::Str(s) => write!(f, "\"{}\"", s),
            DictKey::I64(i) => write!(f, "{}", i),
            DictKey::Symbol(s) => write!(f, ":{}", s),
        }
    }
}

// Hash table constants matching Julia's dict.jl
const SLOT_EMPTY: u8 = 0x00;
const SLOT_DELETED: u8 = 0x7f;
const MIN_DICT_TABLE_SIZE: usize = 16;
const MAX_ALLOWED_PROBE: usize = 16;
const MAX_PROBE_SHIFT: usize = 6;

/// Get the 7 most significant bits of the hash, with high bit set.
#[inline]
fn shorthash7(hsh: u64) -> u8 {
    ((hsh >> 57) as u8) | 0x80
}

/// Compute hash of a DictKey.
#[inline]
fn hash_dict_key(key: &DictKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Compute the optimal slot position and short hash for a key.
/// Returns (0-based index, shorthash7).
#[inline]
fn hashindex(key: &DictKey, sz: usize) -> (usize, u8) {
    let hsh = hash_dict_key(key);
    let idx = (hsh as usize) & (sz - 1);
    (idx, shorthash7(hsh))
}

/// Round up to next power of 2, with a minimum table size.
/// Matches Julia's _tablesz function.
fn table_size(x: usize) -> usize {
    if x == 0 {
        return 0;
    }
    let min_sz = if x < MIN_DICT_TABLE_SIZE {
        MIN_DICT_TABLE_SIZE
    } else {
        x
    };
    min_sz.next_power_of_two()
}

#[inline]
fn is_slot_empty(slot: u8) -> bool {
    slot == SLOT_EMPTY
}

#[inline]
fn is_slot_filled(slot: u8) -> bool {
    (slot & 0x80) != 0
}

#[inline]
fn is_slot_deleted(slot: u8) -> bool {
    slot == SLOT_DELETED
}

/// Dictionary value: key-value mapping (open-addressing hash table).
///
/// Internal storage matches Julia's Dict struct layout:
/// - `slots`: Memory{UInt8} — slot metadata (0x00=empty, 0x7f=deleted, 0x80|sh=filled)
/// - `keys`: Memory{K} — key storage
/// - `vals`: Memory{V} — value storage
///
/// Uses open addressing with linear probing, matching Julia's hash table algorithm.
#[derive(Debug, Clone)]
pub struct DictValue {
    /// Slot metadata: 0x00=empty, 0x7f=deleted, 0x80|shorthash7=filled
    slots: Vec<u8>,
    /// Key storage (valid only when corresponding slot is filled)
    keys: Vec<DictKey>,
    /// Value storage (valid only when corresponding slot is filled)
    vals: Vec<Value>,
    /// Number of deleted entries
    ndel: usize,
    /// Number of live entries
    count: usize,
    /// Maximum probe distance
    maxprobe: usize,
    /// Type parameter for keys (e.g., "Int64" for Dict{Int64,V})
    pub key_type: Option<String>,
    /// Type parameter for values (e.g., "String" for Dict{K,String})
    pub value_type: Option<String>,
}

/// Iterator over filled entries of a DictValue.
pub struct DictIter<'a> {
    dict: &'a DictValue,
    index: usize,
}

impl<'a> Iterator for DictIter<'a> {
    type Item = (&'a DictKey, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.dict.slots.len() {
            let i = self.index;
            self.index += 1;
            if is_slot_filled(self.dict.slots[i]) {
                return Some((&self.dict.keys[i], &self.dict.vals[i]));
            }
        }
        None
    }
}

impl DictValue {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            keys: Vec::new(),
            vals: Vec::new(),
            ndel: 0,
            count: 0,
            maxprobe: 0,
            key_type: None,
            value_type: None,
        }
    }

    pub fn with_entries(entries: Vec<(DictKey, Value)>) -> Self {
        let mut dict = Self::new();
        for (k, v) in entries {
            dict.insert(k, v);
        }
        dict
    }

    pub fn with_type_params(key_type: String, value_type: String) -> Self {
        Self {
            slots: Vec::new(),
            keys: Vec::new(),
            vals: Vec::new(),
            ndel: 0,
            count: 0,
            maxprobe: 0,
            key_type: Some(key_type),
            value_type: Some(value_type),
        }
    }

    /// Create a DictValue with optional type params.
    pub fn with_type_params_opt(key_type: Option<String>, value_type: Option<String>) -> Self {
        Self {
            slots: Vec::new(),
            keys: Vec::new(),
            vals: Vec::new(),
            ndel: 0,
            count: 0,
            maxprobe: 0,
            key_type,
            value_type,
        }
    }

    /// Get value by key. Returns None if key not found.
    pub fn get(&self, key: &DictKey) -> Option<&Value> {
        self.ht_keyindex(key).map(|idx| &self.vals[idx])
    }

    /// Insert a key-value pair. Updates the value if key already exists.
    pub fn insert(&mut self, key: DictKey, value: Value) {
        let (pos, sh) = self.ht_keyindex2(&key);
        if pos >= 0 {
            // Key exists, update value
            self.vals[pos as usize] = value;
        } else {
            // Key not found, insert at (-pos - 1)
            let idx = (-(pos + 1)) as usize;
            if is_slot_deleted(self.slots[idx]) {
                self.ndel -= 1;
            }
            self.slots[idx] = sh;
            self.keys[idx] = key;
            self.vals[idx] = value;
            self.count += 1;

            // Rehash now if necessary (matching Julia's _setindex! logic):
            // > 3/4 deleted or > 2/3 full
            let sz = self.slots.len();
            if self.ndel >= (3 * sz / 4) || self.count * 3 > sz * 2 {
                let new_cnt = self.count;
                let new_sz = if new_cnt > 64000 {
                    new_cnt * 2
                } else {
                    new_cnt * 4
                };
                self.rehash(new_sz);
            }
        }
    }

    /// Remove a key and return its value, or None if key not found.
    pub fn remove(&mut self, key: &DictKey) -> Option<Value> {
        if let Some(idx) = self.ht_keyindex(key) {
            let val = std::mem::replace(&mut self.vals[idx], Value::Nothing);
            self.slots[idx] = SLOT_DELETED;
            self.ndel += 1;
            self.count -= 1;
            Some(val)
        } else {
            None
        }
    }

    /// Check if the dict contains the given key.
    pub fn contains_key(&self, key: &DictKey) -> bool {
        self.ht_keyindex(key).is_some()
    }

    /// Get the number of live entries.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if the dict is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get all keys as a Vec.
    pub fn keys(&self) -> Vec<DictKey> {
        self.iter().map(|(k, _)| k.clone()).collect()
    }

    /// Get all values as a Vec.
    pub fn values(&self) -> Vec<Value> {
        self.iter().map(|(_, v)| v.clone()).collect()
    }

    /// Iterate over filled entries as (&DictKey, &Value) pairs.
    pub fn iter(&self) -> DictIter<'_> {
        DictIter {
            dict: self,
            index: 0,
        }
    }

    /// Find the next filled slot starting from `from_index` (0-based).
    /// Returns (slot_index, &key, &value) or None if no more filled slots.
    pub fn next_filled_slot(&self, from_index: usize) -> Option<(usize, &DictKey, &Value)> {
        let sz = self.slots.len();
        let mut i = from_index;
        while i < sz {
            if is_slot_filled(self.slots[i]) {
                return Some((i, &self.keys[i], &self.vals[i]));
            }
            i += 1;
        }
        None
    }

    /// Merge another dict into this one (other's values override).
    pub fn merge(&mut self, other: &DictValue) {
        for (k, v) in other.iter() {
            self.insert(k.clone(), v.clone());
        }
    }

    /// Clear all entries (keeps allocated capacity).
    pub fn clear(&mut self) {
        let sz = self.slots.len();
        self.slots.fill(SLOT_EMPTY);
        for i in 0..sz {
            self.keys[i] = DictKey::I64(0);
            self.vals[i] = Value::Nothing;
        }
        self.ndel = 0;
        self.count = 0;
        self.maxprobe = 0;
    }

    /// Get a stable identity value for hashing (used by objectid).
    pub fn identity_ptr(&self) -> usize {
        self.slots.as_ptr() as usize
    }

    // =========================================================================
    // Internal hash table operations
    // =========================================================================

    /// Find the slot index where a key is stored, or None if not present.
    fn ht_keyindex(&self, key: &DictKey) -> Option<usize> {
        let sz = self.slots.len();
        if sz == 0 {
            return None;
        }
        let (mut index, sh) = hashindex(key, sz);
        let mut iter = 0;
        loop {
            if is_slot_empty(self.slots[index]) {
                return None;
            }
            if self.slots[index] == sh && self.keys[index] == *key {
                return Some(index);
            }
            index = (index + 1) & (sz - 1);
            iter += 1;
            if iter > self.maxprobe {
                return None;
            }
        }
    }

    /// Find the position for inserting a key.
    /// Returns (pos, shorthash) where:
    /// - pos >= 0: key exists at this position
    /// - pos < 0: key not found, insert at (-pos - 1)
    fn ht_keyindex2(&mut self, key: &DictKey) -> (isize, u8) {
        let sz = self.slots.len();
        if sz == 0 {
            self.rehash(4);
            let (idx, sh) = hashindex(key, self.slots.len());
            return (-(idx as isize) - 1, sh);
        }
        let (start_index, sh) = hashindex(key, sz);
        let mut index = start_index;
        let mut avail: isize = 0;
        let mut iter = 0;

        loop {
            if is_slot_empty(self.slots[index]) {
                let pos = if avail < 0 {
                    (-avail - 1) as usize
                } else {
                    index
                };
                return (-(pos as isize) - 1, sh);
            }
            if is_slot_deleted(self.slots[index]) {
                if avail == 0 {
                    avail = -(index as isize) - 1;
                }
            } else if self.slots[index] == sh && self.keys[index] == *key {
                return (index as isize, sh);
            }
            index = (index + 1) & (sz - 1);
            iter += 1;
            if iter > self.maxprobe {
                break;
            }
        }

        if avail < 0 {
            return (avail, sh);
        }

        let max_allowed = std::cmp::max(MAX_ALLOWED_PROBE, sz >> MAX_PROBE_SHIFT);
        while iter < max_allowed {
            if !is_slot_filled(self.slots[index]) {
                self.maxprobe = iter;
                return (-(index as isize) - 1, sh);
            }
            index = (index + 1) & (sz - 1);
            iter += 1;
        }

        // Need to rehash and retry
        let new_sz = if self.count > 64000 { sz * 2 } else { sz * 4 };
        self.rehash(new_sz);
        self.ht_keyindex2(key)
    }

    /// Rehash the hash table to a new size.
    fn rehash(&mut self, newsz: usize) {
        let newsz = table_size(newsz);
        if self.count == 0 {
            self.slots = vec![SLOT_EMPTY; newsz];
            self.keys = vec![DictKey::I64(0); newsz];
            self.vals = vec![Value::Nothing; newsz];
            self.ndel = 0;
            self.maxprobe = 0;
            return;
        }

        let old_slots = std::mem::replace(&mut self.slots, vec![SLOT_EMPTY; newsz]);
        let mut old_keys = std::mem::replace(&mut self.keys, vec![DictKey::I64(0); newsz]);
        let mut old_vals = std::mem::replace(&mut self.vals, vec![Value::Nothing; newsz]);

        let mut count = 0;
        let mut maxprobe = 0;

        for i in 0..old_slots.len() {
            if is_slot_filled(old_slots[i]) {
                let key = std::mem::replace(&mut old_keys[i], DictKey::I64(0));
                let val = std::mem::replace(&mut old_vals[i], Value::Nothing);

                let (mut index, _) = hashindex(&key, newsz);
                let index0 = index;
                while self.slots[index] != SLOT_EMPTY {
                    index = (index + 1) & (newsz - 1);
                }
                let probe = index.wrapping_sub(index0) & (newsz - 1);
                if probe > maxprobe {
                    maxprobe = probe;
                }
                self.slots[index] = old_slots[i];
                self.keys[index] = key;
                self.vals[index] = val;
                count += 1;
            }
        }

        self.count = count;
        self.ndel = 0;
        self.maxprobe = maxprobe;
    }
}

impl Default for DictValue {
    fn default() -> Self {
        Self::new()
    }
}

/// Set value: unordered collection of unique elements
#[derive(Debug, Clone)]
pub struct SetValue {
    /// Storage as Vec to maintain insertion order (like Julia's Set)
    pub elements: Vec<DictKey>,
}

impl SetValue {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
        }
    }

    pub fn with_elements(elements: Vec<DictKey>) -> Self {
        let mut set = Self::new();
        for elem in elements {
            set.insert(elem);
        }
        set
    }

    pub fn insert(&mut self, elem: DictKey) -> bool {
        if self.contains(&elem) {
            false
        } else {
            self.elements.push(elem);
            true
        }
    }

    pub fn contains(&self, elem: &DictKey) -> bool {
        self.elements.iter().any(|e| e == elem)
    }

    pub fn remove(&mut self, elem: &DictKey) -> bool {
        if let Some(pos) = self.elements.iter().position(|e| e == elem) {
            self.elements.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &DictKey> {
        self.elements.iter()
    }

    /// Union: self ∪ other
    pub fn union(&self, other: &SetValue) -> SetValue {
        let mut result = self.clone();
        for elem in &other.elements {
            result.insert(elem.clone());
        }
        result
    }

    /// Intersection: self ∩ other
    pub fn intersect(&self, other: &SetValue) -> SetValue {
        let mut result = SetValue::new();
        for elem in &self.elements {
            if other.contains(elem) {
                result.elements.push(elem.clone());
            }
        }
        result
    }

    /// Difference: self \ other
    pub fn setdiff(&self, other: &SetValue) -> SetValue {
        let mut result = SetValue::new();
        for elem in &self.elements {
            if !other.contains(elem) {
                result.elements.push(elem.clone());
            }
        }
        result
    }
}

impl Default for SetValue {
    fn default() -> Self {
        Self::new()
    }
}

/// Composed function value - represents f ∘ g
///
/// Stores function references that can be either simple function names
/// or nested composed functions for chaining (f ∘ g ∘ h).
#[derive(Debug, Clone)]
pub struct ComposedFunctionValue {
    /// Outer function (f in f ∘ g)
    pub outer: Box<Value>,
    /// Inner function (g in f ∘ g)
    pub inner: Box<Value>,
}

impl ComposedFunctionValue {
    pub fn new(outer: Value, inner: Value) -> Self {
        Self {
            outer: Box::new(outer),
            inner: Box::new(inner),
        }
    }
}

/// Julia Expr - an AST node for metaprogramming
///
/// In Julia: `Expr(:call, :+, 1, 2)` represents `1 + 2`
///
/// Structure:
/// - `head`: Symbol indicating the node type (:call, :block, :if, etc.)
/// - `args`: Vector of child nodes (Expr, Symbol, literals)
#[derive(Debug, Clone)]
pub struct ExprValue {
    /// The expression head (e.g., :call, :block, :if, :quote)
    pub head: SymbolValue,
    /// Child arguments (can be Expr, Symbol, or literal values)
    pub args: Vec<Value>,
}

impl ExprValue {
    pub fn new(head: SymbolValue, args: Vec<Value>) -> Self {
        Self { head, args }
    }

    /// Create an Expr from a head string and args
    pub fn from_head(head: impl Into<String>, args: Vec<Value>) -> Self {
        Self {
            head: SymbolValue::new(head),
            args,
        }
    }

    /// Check if this expression has the given head
    pub fn is_head(&self, head: &str) -> bool {
        self.head.as_str() == head
    }

    /// Get the head as a Symbol value
    pub fn get_head(&self) -> Value {
        Value::Symbol(self.head.clone())
    }

    /// Get args as an array value
    pub fn get_args(&self) -> Value {
        Value::Array(new_array_ref(ArrayValue::any_vector(self.args.clone())))
    }

    /// Get argument at 1-based index (Julia convention)
    pub fn get_arg(&self, index: usize) -> Option<&Value> {
        if index >= 1 && index <= self.args.len() {
            Some(&self.args[index - 1])
        } else {
            None
        }
    }

    /// Get the number of arguments
    pub fn nargs(&self) -> usize {
        self.args.len()
    }
}

impl std::fmt::Display for ExprValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expr(:{}", self.head.as_str())?;
        for arg in &self.args {
            write!(f, ", ")?;
            match arg {
                Value::Symbol(s) => write!(f, ":{}", s.as_str())?,
                Value::I64(n) => write!(f, "{}", n)?,
                Value::F64(n) => write!(f, "{}", n)?,
                Value::Str(s) => write!(f, "\"{}\"", s)?,
                Value::Expr(e) => write!(f, "{}", e)?,
                _ => write!(f, "{:?}", arg)?,
            }
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dict_new_empty() {
        let d = DictValue::new();
        assert_eq!(d.len(), 0);
        assert!(d.is_empty());
        assert!(d.get(&DictKey::Str("x".into())).is_none());
    }

    #[test]
    fn test_dict_insert_and_get() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));
        d.insert(DictKey::I64(42), Value::Str("hello".into()));

        assert_eq!(d.len(), 3);
        assert!(!d.is_empty());
        assert!(matches!(
            d.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));
        assert!(matches!(
            d.get(&DictKey::Str("b".into())),
            Some(Value::I64(2))
        ));
        assert!(matches!(d.get(&DictKey::I64(42)), Some(Value::Str(s)) if s == "hello"));
        assert!(d.get(&DictKey::Str("c".into())).is_none());
    }

    #[test]
    fn test_dict_update_existing_key() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        assert!(matches!(
            d.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));

        d.insert(DictKey::Str("a".into()), Value::I64(100));
        assert!(matches!(
            d.get(&DictKey::Str("a".into())),
            Some(Value::I64(100))
        ));
        assert_eq!(d.len(), 1); // Length unchanged
    }

    #[test]
    fn test_dict_remove() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));
        d.insert(DictKey::Str("c".into()), Value::I64(3));

        let removed = d.remove(&DictKey::Str("b".into()));
        assert!(matches!(removed, Some(Value::I64(2))));
        assert_eq!(d.len(), 2);
        assert!(!d.contains_key(&DictKey::Str("b".into())));
        assert!(d.contains_key(&DictKey::Str("a".into())));
        assert!(d.contains_key(&DictKey::Str("c".into())));

        // Remove non-existent key
        let removed2 = d.remove(&DictKey::Str("x".into()));
        assert!(removed2.is_none());
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn test_dict_contains_key() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::Str("one".into()));
        assert!(d.contains_key(&DictKey::I64(1)));
        assert!(!d.contains_key(&DictKey::I64(2)));
    }

    #[test]
    fn test_dict_with_entries() {
        let entries = vec![
            (DictKey::Str("x".into()), Value::I64(10)),
            (DictKey::Str("y".into()), Value::I64(20)),
        ];
        let d = DictValue::with_entries(entries);
        assert_eq!(d.len(), 2);
        assert!(matches!(
            d.get(&DictKey::Str("x".into())),
            Some(Value::I64(10))
        ));
        assert!(matches!(
            d.get(&DictKey::Str("y".into())),
            Some(Value::I64(20))
        ));
    }

    #[test]
    fn test_dict_with_type_params() {
        let d = DictValue::with_type_params("String".into(), "Int64".into());
        assert_eq!(d.key_type.as_deref(), Some("String"));
        assert_eq!(d.value_type.as_deref(), Some("Int64"));
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn test_dict_keys_values() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::Str("a".into()));
        d.insert(DictKey::I64(2), Value::Str("b".into()));

        let keys = d.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&DictKey::I64(1)));
        assert!(keys.contains(&DictKey::I64(2)));

        let vals = d.values();
        assert_eq!(vals.len(), 2);
    }

    #[test]
    fn test_dict_iter() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));

        let pairs: Vec<_> = d.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_dict_merge() {
        let mut d1 = DictValue::new();
        d1.insert(DictKey::Str("a".into()), Value::I64(1));
        d1.insert(DictKey::Str("b".into()), Value::I64(2));

        let mut d2 = DictValue::new();
        d2.insert(DictKey::Str("b".into()), Value::I64(20));
        d2.insert(DictKey::Str("c".into()), Value::I64(30));

        d1.merge(&d2);
        assert_eq!(d1.len(), 3);
        assert!(matches!(
            d1.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));
        assert!(matches!(
            d1.get(&DictKey::Str("b".into())),
            Some(Value::I64(20))
        ));
        assert!(matches!(
            d1.get(&DictKey::Str("c".into())),
            Some(Value::I64(30))
        ));
    }

    #[test]
    fn test_dict_clear() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));
        assert_eq!(d.len(), 2);

        d.clear();
        assert_eq!(d.len(), 0);
        assert!(d.is_empty());
        assert!(d.get(&DictKey::Str("a".into())).is_none());

        // Can insert after clear
        d.insert(DictKey::Str("c".into()), Value::I64(3));
        assert_eq!(d.len(), 1);
        assert!(matches!(
            d.get(&DictKey::Str("c".into())),
            Some(Value::I64(3))
        ));
    }

    #[test]
    fn test_dict_rehash_with_many_entries() {
        let mut d = DictValue::new();
        // Insert enough entries to trigger rehash (initial table size is 16)
        for i in 0..50 {
            d.insert(DictKey::I64(i), Value::I64(i * 10));
        }
        assert_eq!(d.len(), 50);

        // Verify all entries are still accessible
        for i in 0..50 {
            assert!(
                matches!(d.get(&DictKey::I64(i)), Some(Value::I64(v)) if *v == i * 10),
                "Failed to get key {} after rehash",
                i
            );
        }
    }

    #[test]
    fn test_dict_delete_and_reinsert() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::I64(10));
        d.insert(DictKey::I64(2), Value::I64(20));
        d.insert(DictKey::I64(3), Value::I64(30));

        d.remove(&DictKey::I64(2));
        assert!(!d.contains_key(&DictKey::I64(2)));
        assert_eq!(d.len(), 2);

        // Reinsert at the same key
        d.insert(DictKey::I64(2), Value::I64(200));
        assert!(matches!(d.get(&DictKey::I64(2)), Some(Value::I64(200))));
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn test_dict_next_filled_slot() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::I64(10));
        d.insert(DictKey::I64(2), Value::I64(20));

        // First filled slot from 0
        let first = d.next_filled_slot(0);
        assert!(first.is_some());

        // Scan all entries
        let mut count = 0;
        let mut idx = 0;
        while let Some((slot_idx, _, _)) = d.next_filled_slot(idx) {
            count += 1;
            idx = slot_idx + 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn test_table_size_function() {
        assert_eq!(table_size(0), 0);
        assert_eq!(table_size(1), MIN_DICT_TABLE_SIZE); // minimum table size
        assert_eq!(table_size(4), MIN_DICT_TABLE_SIZE);
        assert_eq!(table_size(MIN_DICT_TABLE_SIZE), MIN_DICT_TABLE_SIZE);
        assert_eq!(table_size(17), 32);
        assert_eq!(table_size(32), 32);
        assert_eq!(table_size(33), 64);
    }

    #[test]
    fn test_shorthash7_has_high_bit_set() {
        // shorthash7 should always have the high bit (0x80) set
        for i in 0..100u64 {
            let sh = shorthash7(i * 1234567890);
            assert!((sh & 0x80) != 0, "shorthash7 must have high bit set");
        }
    }

    #[test]
    fn test_slot_metadata_constants() {
        assert!(is_slot_empty(SLOT_EMPTY));
        assert!(!is_slot_filled(SLOT_EMPTY));
        assert!(!is_slot_deleted(SLOT_EMPTY));

        assert!(is_slot_deleted(SLOT_DELETED));
        assert!(!is_slot_filled(SLOT_DELETED));
        assert!(!is_slot_empty(SLOT_DELETED));

        // A filled slot has high bit set
        let filled = 0x83u8; // 0x80 | 0x03
        assert!(is_slot_filled(filled));
        assert!(!is_slot_empty(filled));
        assert!(!is_slot_deleted(filled));
    }

    #[test]
    fn test_dict_symbol_keys() {
        let mut d = DictValue::new();
        d.insert(DictKey::Symbol("x".into()), Value::I64(1));
        d.insert(DictKey::Symbol("y".into()), Value::I64(2));

        assert!(d.contains_key(&DictKey::Symbol("x".into())));
        assert!(matches!(
            d.get(&DictKey::Symbol("y".into())),
            Some(Value::I64(2))
        ));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn test_dict_mixed_key_types() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("hello".into()), Value::I64(1));
        d.insert(DictKey::I64(42), Value::I64(2));
        d.insert(DictKey::Symbol("sym".into()), Value::I64(3));

        assert_eq!(d.len(), 3);
        assert!(matches!(
            d.get(&DictKey::Str("hello".into())),
            Some(Value::I64(1))
        ));
        assert!(matches!(d.get(&DictKey::I64(42)), Some(Value::I64(2))));
        assert!(matches!(
            d.get(&DictKey::Symbol("sym".into())),
            Some(Value::I64(3))
        ));
    }

    #[test]
    fn test_dict_clone() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));

        let d2 = d.clone();
        assert_eq!(d2.len(), 2);
        assert!(matches!(
            d2.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));

        // Modifying original doesn't affect clone
        d.insert(DictKey::Str("c".into()), Value::I64(3));
        assert_eq!(d.len(), 3);
        assert_eq!(d2.len(), 2);
    }
}
