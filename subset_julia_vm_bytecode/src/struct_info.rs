//! Struct metadata shared by the compiler and the VM (Issue #8557).
//!
//! `StructInfo` (concrete struct layout for compilation) and
//! `ParametricStructDef` (the un-instantiated parametric definition) are
//! consumed on both sides of the compile/VM boundary: the compiler builds and
//! instantiates them, while the VM's runtime compile context, type objects,
//! and reflection builtins read them back. They are therefore owned by the
//! `runtime_types` layer; `compile::StructInfo` / `compile::ParametricStructDef`
//! remain as re-export shims (Issue #8449).

use crate::ValueType;
use subset_julia_vm_types::ir::core::StructDef;

/// Struct definition info for compilation.
///
/// `PartialEq` (Issue #11078): `StructRegistry::insert` recognizes an ALIAS (the
/// same declaration re-registered under a second name) by requiring an identical
/// layout, not by trusting the `type_id` alone — which contains Issue #11167,
/// where the dense allocator can hand one `type_id` to two different declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct StructInfo {
    pub type_id: usize,
    pub is_mutable: bool,
    pub fields: Vec<(String, ValueType)>,
    /// True if this struct has inner constructors defined
    pub has_inner_constructor: bool,
}

/// Parametric struct definition (stores the original definition before instantiation).
#[derive(Debug, Clone)]
pub struct ParametricStructDef {
    pub def: StructDef,
}
