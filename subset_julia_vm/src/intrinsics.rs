//! Compatibility re-export for bytecode intrinsic identifiers.
//!
//! `Intrinsic` is now owned by `subset_julia_vm_bytecode` (Issue #8656). Keep
//! the historical `crate::intrinsics::Intrinsic` path valid while compile/VM
//! crate extraction continues.

pub use subset_julia_vm_bytecode::intrinsics::*;
