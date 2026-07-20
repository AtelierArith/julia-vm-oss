//! Compatibility re-export for the VM RNG module.
//!
//! `rng` is owned by `subset_julia_vm_bytecode` (Issue #8656): `Value::Rng`
//! holds `RngInstance`, so the RNG state types live with the value model.
//! The historical `crate::rng::*` paths remain valid.
pub use subset_julia_vm_bytecode::rng::*;
