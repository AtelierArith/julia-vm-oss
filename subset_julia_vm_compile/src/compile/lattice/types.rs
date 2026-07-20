//! Re-export shim: the lattice type definitions (`LatticeType`, `ConstValue`,
//! `ConcreteType`) are owned by the shared runtime type layer
//! (`crate::runtime_types::lattice`) since Issue #8557. The
//! `compile::lattice::types::*` paths stay valid for existing users, including
//! integration tests and benches outside the crate.

// `lattice` types moved to `subset_julia_vm_types` (Issue #8655); re-routed
// through `crate::runtime_types` which now re-exports them from there.
pub use crate::runtime_types::{ConcreteType, ConstValue, LatticeType};
