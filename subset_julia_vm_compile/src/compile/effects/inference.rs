//! Compatibility re-export for expression-level effect inference.
//!
//! The implementation now lives below `compile` in
//! `subset_julia_vm_types::runtime_types::effect_inference` so VM-facing effect
//! summaries can compose method bodies without depending on `compile::effects`
//! ownership (Issue #9090).

pub use subset_julia_vm_types::runtime_types::effect_inference::*;
