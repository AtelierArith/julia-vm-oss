//! Bytecode-owned runtime-specialization metadata serialized with programs.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use subset_julia_vm_types::ir::core::Function;

/// A function that can be specialized at runtime.
///
/// The `ir` field is wrapped in `Arc` so that cloning a `SpecializableFunction`
/// (e.g. during cached-Base restore at startup) is O(1) instead of O(IR size).
/// Serde treats `Arc<T>` identically to `T`, so the serialized cache format is
/// unchanged (Issue #9104).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecializableFunction {
    /// The Core IR for this function, retained for specialization.
    pub ir: Arc<Function>,
    /// Function name, used for diagnostics.
    pub name: String,
    /// Fallback function index for the generic version.
    pub fallback_index: usize,
}
