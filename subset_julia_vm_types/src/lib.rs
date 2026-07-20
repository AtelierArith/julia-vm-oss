//! `subset_julia_vm_types` — Julia type system, inference primitives, and the
//! type lattice for SubsetJuliaVM.
//!
//! This crate bundles three tightly-coupled modules:
//! - [`types`]: Julia type hierarchy (`JuliaType`, `TypeExpr`, `TypeParam`, …)
//! - [`inference_core`]: shared inference primitives (`CoreType`, `LatticeType`,
//!   dispatch resolver, subtype engine)
//! - [`runtime_types`]: the runtime-visible type metadata layer, currently
//!   exposing [`runtime_types::LatticeType`] / [`runtime_types::ConcreteType`] /
//!   [`runtime_types::ConstValue`] / [`runtime_types::TypeEnv`] plus effect
//!   inference walkers (owned since Issue #8557 / #9090)
//!
//! The two-way `JuliaType ↔ CoreType` bridge means these modules must be
//! co-located (splitting them across crates would create a dependency cycle —
//! see `docs/vm/CRATE_SPLIT.md §4.3`).  Everything below this layer sits in
//! `subset_julia_vm_ir`; everything above sits in the main `subset_julia_vm`
//! integration crate.
//!
//! Issue #8655 — initial extraction from the 397k-line monolith.

pub mod inference_cache_key;
pub mod inference_core;
pub mod ir;
pub mod promotion;
pub mod runtime_types;
pub mod types;

// Top-level re-exports matching the public surface that downstream crates use
// via `crate::types::*`, `crate::inference_core::*`, `crate::runtime_types::*`.
// These are all pub so that `subset_julia_vm` can `pub use subset_julia_vm_types::*;`
// and maintain backward-compatible import paths during the migration window.
pub use inference_cache_key::{
    cache_fn_id_base_name, const_specialization, is_const_eligible, widen_argtype_for_cache_key,
    widen_argtypes_for_cache_key, CacheArgType, InferenceCacheKey, SpecializationConst,
    SMALL_INT_CONST_THRESHOLD,
};
pub use inference_core::{
    CoreAbstract, CorePrimitive, CoreSubtypeEngine, CoreType, CoreTypeSubstitution, CoreTypeVar,
    CoreTypeVarId, CoreValueParam, PrimitiveNumeric,
};
pub use runtime_types::{ConcreteType, ConstValue, LatticeType};
pub use types::{
    builtin_type_binding_authority, builtin_type_for_compiler, builtin_type_for_parser,
    builtin_type_for_reflection, BuiltinTypeBindingAuthority, DispatchError, JuliaType,
    StructHierarchy, StructHierarchyEntry, TypeExpr, TypeParam, Variance,
};
