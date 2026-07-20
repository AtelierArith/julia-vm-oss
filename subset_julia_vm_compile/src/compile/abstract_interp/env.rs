//! Compatibility re-export for the abstract-interpretation type environment.
//!
//! The `TypeEnv` implementation now lives below `compile` in
//! `crate::runtime_types` so VM-facing runtime type surfaces do not
//! depend on `compile::abstract_interp` ownership (Issue #9090).

pub use crate::runtime_types::TypeEnv;
