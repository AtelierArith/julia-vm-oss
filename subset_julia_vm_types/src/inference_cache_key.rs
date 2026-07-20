//! Typed inference cache keys with controlled const specialization (Issue #3510).
//!
//! Mirrors Julia's `MethodInstance` / `WidenedArgtypes` / `most_general_argtypes`
//! treatment from `julia/Compiler/src/inferenceresult.jl`: most argtypes are
//! widened to their concrete (non-`Const`) form for the cache key, while a
//! small, controlled set of argtypes are preserved as `Const` to enable
//! constant propagation that materially affects dispatch / branching.
//!
//! # Policy
//!
//! [`widen_argtypes_for_cache_key`] keeps `Const` only for the cases that
//! are most likely to influence inference (singleton-like values whose
//! type is essentially equivalent to the value):
//!
//! - `Bool`     - branch elimination (`if flag then ... end`).
//! - `Symbol`   - field access / `Val`-like dispatch.
//! - `Nothing`  - singleton type.
//! - small `Int64` (|n| <= 8) - `Val{N}`-style and tuple-length dispatch.
//!
//! Everything else is widened to the corresponding [`LatticeType::Concrete`]
//! (e.g. `Const(42_000_000)` -> `Concrete(Int64)`) so that calls with the same
//! widened argtypes hit the same cache entry, matching Julia's default
//! behavior of `widenconst`-ing arguments before inference.
//!
//! # Shared const-specialization policy (Issue #4272)
//!
//! The AoT path keeps ABI/codegen layout in its `CodeInstanceKey`, but the
//! specialization identity is an embedded [`InferenceCacheKey`]. Literal
//! call-site arguments are normalized to [`CacheArgType`] slots by this module's
//! policy, so compile and AoT cache construction share both the key type and the
//! preserve-vs-widen decision.

use crate::runtime_types::{ConcreteType, ConstValue, LatticeType};
use serde::{Deserialize, Serialize};

/// Maximum absolute integer value that is preserved as `Const` in the
/// cache key. Larger integers are widened so that
/// `f(1_000_000)` and `f(2_000_000)` reuse the same inference result.
pub const SMALL_INT_CONST_THRESHOLD: i64 = 8;

/// One slot in an [`InferenceCacheKey`]. Either a widened type, or a
/// preserved `Const` value when const specialization is allowed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CacheArgType {
    /// Widened (non-`Const`) form - the default for most argtypes.
    Type(LatticeType),
    /// Const value preserved for specialization.
    ///
    /// Only produced for values that pass [`is_const_eligible`]; constructing
    /// other variants directly is allowed but not produced by
    /// [`widen_argtypes_for_cache_key`].
    Const(ConstValue),
}

impl CacheArgType {
    /// Widened lattice form, regardless of whether this slot kept the
    /// const. Used for diagnostics and equality checks against an
    /// already-widened lattice slice.
    pub fn widened(&self) -> LatticeType {
        match self {
            CacheArgType::Type(t) => t.clone(),
            CacheArgType::Const(c) => LatticeType::Concrete(c.to_concrete_type()),
        }
    }
}

/// Typed cache key for interprocedural inference results.
///
/// Replaces the ad-hoc `(String, Vec<LatticeType>)` previously used by
/// the VM-side inference engine. The key carries the function identity
/// plus a per-argument [`CacheArgType`] that explicitly records whether
/// the slot was widened or preserved as `Const`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InferenceCacheKey {
    /// Function identifier (currently the function name).
    pub fn_id: String,
    /// One [`CacheArgType`] per argument, in declaration order.
    pub argtypes: Vec<CacheArgType>,
}

impl InferenceCacheKey {
    /// Construct a cache key by widening `arg_types` according to the
    /// default const-specialization policy
    /// ([`widen_argtypes_for_cache_key`]).
    pub fn new(fn_id: &str, arg_types: &[LatticeType]) -> Self {
        Self {
            fn_id: fn_id.to_string(),
            argtypes: widen_argtypes_for_cache_key(arg_types),
        }
    }

    /// The bare function name represented by this cache key's function id.
    ///
    /// Current #5939 groundwork still encodes method identity as
    /// `name(declared_param_types)`, while method-table mutations and legacy
    /// backedges are keyed by bare `name`. Keeping the projection on the key
    /// type limits how much code knows about the string encoding.
    pub fn base_fn_id(&self) -> &str {
        cache_fn_id_base_name(&self.fn_id)
    }

    /// Construct a cache key without applying any widening when the caller
    /// already supplies normalized [`CacheArgType`] slots.
    pub fn from_argtypes(fn_id: &str, argtypes: Vec<CacheArgType>) -> Self {
        Self {
            fn_id: fn_id.to_string(),
            argtypes,
        }
    }
}

/// Extracts the bare function name from a possibly specialized inference
/// cache function id.
///
/// `inference_cache_function_id` currently produces `name` for nullary
/// functions and `name(types)` otherwise. This helper is intentionally small
/// and centralized so #5939 can later replace the string id with a structured
/// MethodInstance key without auditing ad hoc `find('(')` call sites.
pub fn cache_fn_id_base_name(fn_id: &str) -> &str {
    match fn_id.find('(') {
        Some(idx) => &fn_id[..idx],
        None => fn_id,
    }
}

/// Returns whether `value` is small/singleton enough to keep as `Const`
/// in a cache key (mirrors Julia's `most_general_argtypes` treatment of
/// singleton types and forwardable consts).
pub fn is_const_eligible(value: &ConstValue) -> bool {
    match value {
        // Boolean and Nothing are singleton-like; preserving them lets
        // inference eliminate dead branches.
        ConstValue::Bool(_) | ConstValue::Nothing => true,
        // Symbols often act as field selectors / `Val`-like dispatch
        // tags; preserving them is cheap (small key population).
        ConstValue::Symbol(_) => true,
        // Small integers correspond to common `Val{N}` / tuple-length
        // dispatch. Bound the absolute value to avoid blowup from
        // arbitrary user inputs. Use `checked_abs` so `i64::MIN` (whose
        // absolute value is unrepresentable as i64) is correctly widened
        // rather than wrap-around-aliased to `i64::MIN`.
        ConstValue::Int64(n) => n
            .checked_abs()
            .map(|v| v <= SMALL_INT_CONST_THRESHOLD)
            .unwrap_or(false),
        // Floats and arbitrary strings rarely change dispatch and would
        // explode the cache key population - widen them.
        ConstValue::Float64(_) | ConstValue::String(_) => false,
    }
}

/// Apply the default cache-key policy to `arg_types`.
///
/// Mirrors Julia's `most_general_argtypes` (`julia/Compiler/src/inferenceresult.jl`):
/// most argtypes are widened, with `Const` preserved only for the small
/// controlled set described in [`is_const_eligible`].
pub fn widen_argtypes_for_cache_key(arg_types: &[LatticeType]) -> Vec<CacheArgType> {
    arg_types.iter().map(widen_argtype_for_cache_key).collect()
}

/// Per-slot version of [`widen_argtypes_for_cache_key`].
pub fn widen_argtype_for_cache_key(arg: &LatticeType) -> CacheArgType {
    match arg {
        LatticeType::Const(cv) => match const_specialization(cv) {
            SpecializationConst::Preserve(cv) => CacheArgType::Const(cv),
            SpecializationConst::Widen(ty) => CacheArgType::Type(LatticeType::Concrete(ty)),
        },
        // All other lattice forms (Concrete, Union, Conditional, Top, Bottom)
        // pass through as-is. We deliberately do NOT collapse Conditional /
        // Union here because doing so would lose information that the
        // inference engine may want to read back from the cache key for
        // diagnostics.
        other => CacheArgType::Type(other.clone()),
    }
}

/// The canonical, path-agnostic outcome of applying the const-specialization
/// policy to a single constant argument.
///
/// This is the **single source of truth** for "given a `Const` argument, does
/// the inference cache key / AoT specialization key preserve it (enabling
/// constprop) or widen it (for cache reuse)?". The compile-side cache and AoT
/// `CodeInstanceKey` both store [`InferenceCacheKey`] values, so the two paths
/// cannot carry different key types or const-specialization decisions.
///
/// Mirrors upstream's single `is_forwardable_argtype` predicate
/// (`julia/Compiler/src/abstractlattice.jl`), which gates both cache-key
/// construction (`inferenceresult.jl`) and constprop forwarding
/// (`abstractinterpretation.jl`) from one definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecializationConst {
    /// The constant materially affects inference and is kept verbatim.
    Preserve(ConstValue),
    /// The constant is not profitable to specialize on; widen to this concrete
    /// type so calls with different values of the same type share a key.
    Widen(ConcreteType),
}

/// Apply the shared const-specialization policy to one constant value.
///
/// This routes the decision through [`is_const_eligible`] so every consumer
/// (compile cache key, AoT specialization key) makes an identical
/// preserve-vs-widen choice for the same value. See [`SpecializationConst`].
pub fn const_specialization(cv: &ConstValue) -> SpecializationConst {
    if is_const_eligible(cv) {
        SpecializationConst::Preserve(cv.clone())
    } else {
        SpecializationConst::Widen(cv.to_concrete_type())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_core::{CorePrimitive, CoreType};

    fn concrete(primitive: CorePrimitive) -> LatticeType {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(primitive)))
    }

    fn const_int(n: i64) -> LatticeType {
        LatticeType::Const(ConstValue::Int64(n))
    }

    #[test]
    fn cache_key_policy_preserves_small_singleton_consts() {
        for cv in [
            ConstValue::Bool(true),
            ConstValue::Bool(false),
            ConstValue::Nothing,
            ConstValue::Symbol("field".to_string()),
            ConstValue::Int64(SMALL_INT_CONST_THRESHOLD),
            ConstValue::Int64(-SMALL_INT_CONST_THRESHOLD),
        ] {
            assert_eq!(
                widen_argtype_for_cache_key(&LatticeType::Const(cv.clone())),
                CacheArgType::Const(cv)
            );
        }
    }

    #[test]
    fn cache_key_policy_widens_unprofitable_consts() {
        for (cv, expected) in [
            (
                ConstValue::Int64(SMALL_INT_CONST_THRESHOLD + 1),
                CorePrimitive::Int64,
            ),
            (ConstValue::Float64(2.5), CorePrimitive::Float64),
            (ConstValue::String("hi".to_string()), CorePrimitive::String),
        ] {
            assert_eq!(
                widen_argtype_for_cache_key(&LatticeType::Const(cv)),
                CacheArgType::Type(concrete(expected))
            );
        }
    }

    #[test]
    fn inference_cache_key_collapses_large_consts_but_keeps_small_consts() {
        let big_a = InferenceCacheKey::new("f", &[const_int(1_000_000)]);
        let big_b = InferenceCacheKey::new("f", &[const_int(2_000_000)]);
        assert_eq!(big_a, big_b);

        let small_a = InferenceCacheKey::new("f", &[const_int(1)]);
        let small_b = InferenceCacheKey::new("f", &[const_int(2)]);
        assert_ne!(small_a, small_b);
    }

    #[test]
    fn cache_key_base_name_strips_specialized_signature() {
        assert_eq!(cache_fn_id_base_name("Base.:+(Int64,Int64)"), "Base.:+");
        assert_eq!(
            InferenceCacheKey::new("f(Any)", &[concrete(CorePrimitive::Int64)]).base_fn_id(),
            "f"
        );
    }
}
