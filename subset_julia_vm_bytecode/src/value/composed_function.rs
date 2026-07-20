//! Composed function value (`f ∘ g`).
//!
//! Split out of `container.rs` by value kind (Issue #6835).

use super::Value;

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
