//! Core IR types co-located with the type system.
//!
//! `ir/core.rs` uses both `Span` (from `subset_julia_vm_ir`) and `JuliaType`
//! (from `crate::types`).  Because `JuliaType` and `CoreType` have a two-way
//! bridge, splitting `ir/` into a crate below `_types` would create a cycle.
//! It therefore lives here, in `subset_julia_vm_types`, where both `types/`
//! and `_ir` spans are directly accessible.
//!
//! Issue #8656 — moved from the integration crate during Phase 1 completion.

pub mod core;
pub mod free_vars;
pub mod wire_ids;

pub use core::*;
pub use wire_ids::{builtinop_from_wire_id, builtinop_to_wire_id};
