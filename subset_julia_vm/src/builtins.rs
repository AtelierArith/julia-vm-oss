//! Compatibility re-export for bytecode builtin identifiers.
//!
//! `BuiltinId` is now owned by `subset_julia_vm_bytecode` (Issue #8656). Keep
//! the historical `crate::builtins::BuiltinId` path valid while compile/VM
//! crate extraction continues.

pub use subset_julia_vm_bytecode::builtins::*;
