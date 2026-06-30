//! Shared inference primitives — the seed of a unified VM/AoT lattice.
//!
//! Issue #3508 — until now the VM-side abstract-interpretation lattice
//! (`compile::lattice`, `compile::promotion`, `compile::tfuncs`) and the
//! AoT-side static inference engine (`aot::inference`) maintained
//! independent-but-overlapping type machinery. Each side reimplemented its
//! own answer to "is this primitive numeric?", "what is its rank?", "which
//! types promote to which?" — and the two answers periodically drifted
//! apart as one side learned about a new primitive and the other forgot.
//!
//! This module is the neutral ground both sides will eventually share. It
//! starts intentionally small: just a canonical primitive numeric taxonomy
//! ([`PrimitiveNumeric`]) plus the predicates both pipelines were already
//! computing inline. Follow-up PRs can extend it with rank, promotion
//! rules, and lattice ops; the structure stays additive.
//!
//! The module is deliberately *not* gated behind any feature flag — the
//! VM-side is always compiled, and the AoT-side (`cfg(feature = "aot")`) is
//! consumed only behind that feature. Keeping the canonical module always
//! available means VM-only builds still benefit from the deduplication.
//!
//! # Compile-time vs runtime dispatch contract (Issue #6836)
//!
//! Method selection happens on two sides of the pipeline, and this module is the
//! neutral ground that keeps them in agreement:
//!
//! - **Compile time** (`compile/`): when a call site's argument types are known,
//!   the compiler resolves the target statically and emits `CallResolved` /
//!   `CallTypedDispatch`. It selects via [`selection`] + [`specificity`].
//! - **Run time** (`vm/`): when an argument's type is only known at run time
//!   (e.g. it flows through an `Any` container), the VM resolves the target with
//!   `Vm::find_best_method_index`, selecting via the *same* [`selection`] +
//!   [`specificity`] + [`dispatch_resolver`] core.
//!
//! Because both sides route through these shared utilities, **for identical
//! inputs they must select the same method** — the compiler decides *when types
//! are known*, the VM decides *when runtime types differ*, and the two never
//! disagree. That contract is pinned by the dispatch-parity fixture
//! `tests/fixtures/dispatch/compile_runtime_dispatch_parity_6836.jl` (and the
//! `base_method_core_*_parity_issue_6495` cache tests).
//!
//! Type values cross the compile/runtime boundary only through the documented
//! `compile::bridge` conversions (lattice <-> `ValueType` / `JuliaType`); the VM
//! does not otherwise reach into compile-time-only representations.

pub mod dispatch_resolver;
pub mod primitive_numeric;
pub(crate) mod selection;
pub(crate) mod specificity;
pub mod subtype;
pub mod type_core;

pub use primitive_numeric::PrimitiveNumeric;
pub use subtype::CoreSubtypeEngine;
pub(crate) use type_core::parse_parametric_type_name;
pub(crate) use type_core::{core_type_to_julia_type, core_type_var_to_type_param};
pub use type_core::{
    registered_instantiated_struct_supertype_in, registered_nominal_subtype_decision_in,
    registered_struct_parent_existential_in, registered_struct_parent_family_decision_in,
    CoreAbstract, CorePrimitive, CoreType, CoreTypeVar, CoreValueParam,
};
