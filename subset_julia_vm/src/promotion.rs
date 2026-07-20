//! Compatibility re-export for the type promotion module.
//!
//! `promotion` is now owned by `subset_julia_vm_types` (Issue #8656 Phase 1
//! completion). The historical `crate::promotion::*` path remains valid for
//! all callers in the integration crate during the migration window.

pub use subset_julia_vm_types::promotion::*;
