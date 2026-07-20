//! Re-export shim: `MethodSig`/`MethodTable` and their dispatch helpers are
//! owned by the shared bytecode crate
//! (`subset_julia_vm_bytecode::method_table`) since Issue #9090. The
//! `compile::method_table::*` paths stay valid for existing compile-side
//! users.

pub(crate) use subset_julia_vm_bytecode::method_table::*;
