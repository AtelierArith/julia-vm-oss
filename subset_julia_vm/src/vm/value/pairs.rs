//! `Base.Pairs` value type (kwargs wrapper).
//!
//! Split out of `container.rs` by value kind (Issue #6835).

use super::super::error::VmError;
use super::macro_::SymbolValue;
use super::named_tuple::NamedTupleValue;
use super::Value;

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
