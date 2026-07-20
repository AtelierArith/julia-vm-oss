//! Compatibility re-export for Core IR types.
//!
//! `ir::core` is now owned by `subset_julia_vm_types::ir` (Issue #8656 Phase 1
//! completion). The historical `crate::ir::*` / `crate::ir::core::*` paths
//! remain valid for all callers in the integration crate during the migration
//! window.

// This is an *inline* module (`pub mod core { ... }`), not a `mod core;`
// file declaration — there is no `subset_julia_vm/src/ir/core.rs` (removed as
// an uncompiled orphan, Issue #10739). The sole production Core IR lives in
// `subset_julia_vm_types/src/ir/core.rs`; this block only keeps the path
// `crate::ir::core::BuiltinOp` used throughout the codebase resolving there.
pub mod core {
    pub use subset_julia_vm_types::ir::core::*;
}

pub use subset_julia_vm_types::ir::*;
