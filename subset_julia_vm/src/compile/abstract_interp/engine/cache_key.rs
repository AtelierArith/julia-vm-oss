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
//! - `Bool`     — branch elimination (`if flag then ... end`).
//! - `Symbol`   — field access / `Val`-like dispatch.
//! - `Nothing`  — singleton type.
//! - small `Int64` (|n| ≤ 8) — `Val{N}`-style and tuple-length dispatch.
//!
//! Everything else is widened to the corresponding [`LatticeType::Concrete`]
//! (e.g. `Const(42_000_000)` → `Concrete(Int64)`) so that calls with the same
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

use crate::compile::lattice::types::{ConstValue, LatticeType};
#[cfg(test)]
use crate::inference_core::{CorePrimitive, CoreType};
use crate::ir::core::Function;
use crate::types::{JuliaType, TypeParam};
use serde::{Deserialize, Serialize};

/// Maximum absolute integer value that is preserved as `Const` in the
/// cache key. Larger integers are widened so that
/// `f(1_000_000)` and `f(2_000_000)` reuse the same inference result.
pub const SMALL_INT_CONST_THRESHOLD: i64 = 8;

/// One slot in an [`InferenceCacheKey`]. Either a widened type, or a
/// preserved `Const` value when const specialization is allowed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CacheArgType {
    /// Widened (non-`Const`) form — the default for most argtypes.
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

/// Structured identity for a resolved method instance (Issue #5939).
///
/// The persisted inference cache still uses [`InferenceCacheKey`] with a legacy
/// string `fn_id`, but #5939 needs to move that identity away from ad hoc
/// `name(declared_param_types)` parsing. This type carries the same method
/// identity as structured fields so future cache/backedge maps can replace the
/// string id without changing the widened argument policy.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct MethodInstanceKey {
    function: String,
    declared_arg_types: Vec<JuliaType>,
    type_params: Vec<TypeParam>,
    vararg_param_index: Option<usize>,
    vararg_fixed_count: Option<usize>,
}

impl MethodInstanceKey {
    pub(crate) fn new(
        function: impl Into<String>,
        declared_arg_types: Vec<JuliaType>,
        type_params: Vec<TypeParam>,
        vararg_param_index: Option<usize>,
        vararg_fixed_count: Option<usize>,
    ) -> Self {
        Self {
            function: function.into(),
            declared_arg_types,
            type_params,
            vararg_param_index,
            vararg_fixed_count,
        }
    }

    pub(crate) fn from_function(func: &Function) -> Self {
        Self::new(
            func.name.clone(),
            func.params
                .iter()
                .map(|param| param.effective_type())
                .collect(),
            func.type_params.clone(),
            func.params.iter().position(|param| param.is_varargs),
            func.params.iter().find_map(|param| param.vararg_count),
        )
    }

    pub(crate) fn base_fn_id(&self) -> &str {
        &self.function
    }

    /// Legacy `InferenceCacheKey.fn_id` projection.
    ///
    /// This keeps the current persisted key format stable while routing all
    /// string construction through the structured method identity.
    pub(crate) fn legacy_fn_id(&self) -> String {
        if self.declared_arg_types.is_empty() {
            return self.base_fn_id().to_string();
        }

        let params = self
            .declared_arg_types
            .iter()
            .enumerate()
            .map(|(idx, ty)| {
                if self.vararg_param_index == Some(idx) {
                    format!("{ty}...")
                } else {
                    ty.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{}({})", self.base_fn_id(), params)
    }
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
    pub(crate) fn base_fn_id(&self) -> &str {
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
pub(crate) fn cache_fn_id_base_name(fn_id: &str) -> &str {
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
        // explode the cache key population — widen them.
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
    Widen(crate::compile::lattice::types::ConcreteType),
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
    use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
    use crate::types::TypeParam;

    fn t(ty: ConcreteType) -> LatticeType {
        LatticeType::Concrete(ty)
    }

    fn k_int(n: i64) -> LatticeType {
        LatticeType::Const(ConstValue::Int64(n))
    }

    fn k_bool(b: bool) -> LatticeType {
        LatticeType::Const(ConstValue::Bool(b))
    }

    fn k_sym(s: &str) -> LatticeType {
        LatticeType::Const(ConstValue::Symbol(s.to_string()))
    }

    fn k_str(s: &str) -> LatticeType {
        LatticeType::Const(ConstValue::String(s.to_string()))
    }

    fn k_float(x: f64) -> LatticeType {
        LatticeType::Const(ConstValue::Float64(x))
    }

    #[test]
    fn inference_cache_key_serializes_roundtrip_issue_5093() {
        let key = InferenceCacheKey::from_argtypes(
            "f",
            vec![
                CacheArgType::Type(LatticeType::Concrete(ConcreteType::Core(
                    CoreType::Primitive(CorePrimitive::Int64),
                ))),
                CacheArgType::Const(ConstValue::Bool(true)),
                CacheArgType::Const(ConstValue::Symbol("field".to_string())),
            ],
        );

        let encoded = bincode::serialize(&key).expect("serialize inference cache key");
        let decoded: InferenceCacheKey =
            bincode::deserialize(&encoded).expect("deserialize inference cache key");

        assert_eq!(decoded, key);
    }

    #[test]
    fn issue_5939_cache_key_exposes_base_function_id() {
        let bare = InferenceCacheKey::new(
            "f",
            &[t(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))],
        );
        let specialized = InferenceCacheKey::new(
            "f(Any)",
            &[t(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))],
        );

        assert_eq!(bare.base_fn_id(), "f");
        assert_eq!(specialized.base_fn_id(), "f");
        assert_eq!(cache_fn_id_base_name("Base.:+(Int64,Int64)"), "Base.:+");
    }

    #[test]
    fn issue_5939_method_instance_key_preserves_declared_signature() {
        let any_method = MethodInstanceKey::new("f", vec![JuliaType::Any], vec![], None, None);
        let int_method = MethodInstanceKey::new("f", vec![JuliaType::Int64], vec![], None, None);
        let bounded_method = MethodInstanceKey::new(
            "f",
            vec![JuliaType::TypeVar(
                "T".to_string(),
                Some("Number".to_string()),
            )],
            vec![TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            None,
            None,
        );
        let vararg_method =
            MethodInstanceKey::new("g", vec![JuliaType::Int64], vec![], Some(0), Some(2));

        assert_ne!(
            any_method, int_method,
            "declared method signature is part of the structured method identity"
        );
        assert_ne!(
            int_method, bounded_method,
            "where-bound method signatures stay distinct from concrete methods"
        );
        assert_eq!(int_method.base_fn_id(), "f");
        assert_eq!(int_method.legacy_fn_id(), "f(Int64)");
        assert_eq!(any_method.legacy_fn_id(), "f(Any)");
        assert_eq!(vararg_method.legacy_fn_id(), "g(Int64...)");
    }

    #[test]
    fn small_int_consts_are_preserved() {
        for n in [-8, -1, 0, 1, 7, 8] {
            let arg = k_int(n);
            let widened = widen_argtype_for_cache_key(&arg);
            assert_eq!(
                widened,
                CacheArgType::Const(ConstValue::Int64(n)),
                "small int {n} should be preserved as Const"
            );
        }
    }

    #[test]
    fn large_int_consts_are_widened_to_int64() {
        for n in [9_i64, -9, 1_000, i64::MAX, i64::MIN] {
            let arg = k_int(n);
            let widened = widen_argtype_for_cache_key(&arg);
            assert_eq!(
                widened,
                CacheArgType::Type(t(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                )))),
                "large int {n} should be widened"
            );
        }
    }

    #[test]
    fn bool_const_is_preserved() {
        assert_eq!(
            widen_argtype_for_cache_key(&k_bool(true)),
            CacheArgType::Const(ConstValue::Bool(true))
        );
        assert_eq!(
            widen_argtype_for_cache_key(&k_bool(false)),
            CacheArgType::Const(ConstValue::Bool(false))
        );
    }

    #[test]
    fn symbol_const_is_preserved() {
        assert_eq!(
            widen_argtype_for_cache_key(&k_sym("x")),
            CacheArgType::Const(ConstValue::Symbol("x".to_string()))
        );
    }

    #[test]
    fn nothing_const_is_preserved() {
        assert_eq!(
            widen_argtype_for_cache_key(&LatticeType::Const(ConstValue::Nothing)),
            CacheArgType::Const(ConstValue::Nothing)
        );
    }

    #[test]
    fn float_and_string_consts_are_widened() {
        // Use a non-PI-like value to dodge clippy's `approx_constant` lint.
        assert_eq!(
            widen_argtype_for_cache_key(&k_float(2.5)),
            CacheArgType::Type(t(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))))
        );
        assert_eq!(
            widen_argtype_for_cache_key(&k_str("hello")),
            CacheArgType::Type(t(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            ))))
        );
    }

    #[test]
    fn concrete_types_pass_through() {
        let arg = t(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            widen_argtype_for_cache_key(&arg),
            CacheArgType::Type(arg.clone())
        );
        let arg2 = t(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        assert_eq!(
            widen_argtype_for_cache_key(&arg2),
            CacheArgType::Type(arg2.clone())
        );
    }

    #[test]
    fn top_and_bottom_pass_through() {
        assert_eq!(
            widen_argtype_for_cache_key(&LatticeType::Top),
            CacheArgType::Type(LatticeType::Top)
        );
        assert_eq!(
            widen_argtype_for_cache_key(&LatticeType::Bottom),
            CacheArgType::Type(LatticeType::Bottom)
        );
    }

    #[test]
    fn cache_key_collapses_distinct_large_ints() {
        // The whole point: f(1_000_000) and f(2_000_000) should hit the
        // same cache entry.
        let k1 = InferenceCacheKey::new("f", &[k_int(1_000_000)]);
        let k2 = InferenceCacheKey::new("f", &[k_int(2_000_000)]);
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_keeps_small_ints_distinct() {
        let k1 = InferenceCacheKey::new("f", &[k_int(0)]);
        let k2 = InferenceCacheKey::new("f", &[k_int(1)]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_keeps_bool_distinct() {
        let k1 = InferenceCacheKey::new("f", &[k_bool(true)]);
        let k2 = InferenceCacheKey::new("f", &[k_bool(false)]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_distinct_function_ids() {
        let k1 = InferenceCacheKey::new(
            "f",
            &[t(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))],
        );
        let k2 = InferenceCacheKey::new(
            "g",
            &[t(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))],
        );
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_collapses_concrete_and_widened_const() {
        // Const(big) widens to Concrete(Int64), should match Concrete(Int64).
        let k1 = InferenceCacheKey::new("f", &[k_int(1_000_000)]);
        let k2 = InferenceCacheKey::new(
            "f",
            &[t(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))],
        );
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_mixed_args_partial_const() {
        // (Bool const, large int): bool kept const, int widened.
        let k1 = InferenceCacheKey::new("f", &[k_bool(true), k_int(1_000_000)]);
        let k2 = InferenceCacheKey::new("f", &[k_bool(true), k_int(2_000_000)]);
        let k3 = InferenceCacheKey::new("f", &[k_bool(false), k_int(1_000_000)]);
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn widened_helper_returns_concrete_for_const_slot() {
        let slot = CacheArgType::Const(ConstValue::Bool(true));
        assert_eq!(
            slot.widened(),
            t(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    // --- Issue #4272: shared const-specialization policy ---------------------

    #[test]
    fn const_specialization_preserves_eligible_values() {
        for cv in [
            ConstValue::Bool(true),
            ConstValue::Bool(false),
            ConstValue::Nothing,
            ConstValue::Symbol("field".to_string()),
            ConstValue::Int64(0),
            ConstValue::Int64(SMALL_INT_CONST_THRESHOLD),
            ConstValue::Int64(-SMALL_INT_CONST_THRESHOLD),
        ] {
            assert_eq!(
                const_specialization(&cv),
                SpecializationConst::Preserve(cv.clone()),
                "{cv:?} should be preserved by the shared policy"
            );
        }
    }

    #[test]
    fn const_specialization_widens_unprofitable_values() {
        let cases = [
            (
                ConstValue::Int64(SMALL_INT_CONST_THRESHOLD + 1),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ),
            (
                ConstValue::Int64(1_000_000),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ),
            (
                ConstValue::Float64(2.5),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ),
            (
                ConstValue::String("hi".to_string()),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ),
        ];
        for (cv, expected) in cases {
            assert_eq!(
                const_specialization(&cv),
                SpecializationConst::Widen(expected),
                "{cv:?} should be widened by the shared policy"
            );
        }
    }

    #[test]
    fn const_specialization_is_the_decision_behind_widen_argtype() {
        // widen_argtype_for_cache_key must agree, slot-for-slot, with the
        // standalone const_specialization decision for every ConstValue. This
        // is what lets the AoT path consult const_specialization directly and
        // stay consistent with the compile cache key.
        let values = [
            ConstValue::Bool(true),
            ConstValue::Nothing,
            ConstValue::Symbol("s".to_string()),
            ConstValue::Int64(3),
            ConstValue::Int64(1_000_000),
            ConstValue::Float64(2.5),
            ConstValue::String("x".to_string()),
        ];
        for cv in values {
            let via_widen = widen_argtype_for_cache_key(&LatticeType::Const(cv.clone()));
            let via_policy = match const_specialization(&cv) {
                SpecializationConst::Preserve(c) => CacheArgType::Const(c),
                SpecializationConst::Widen(ty) => CacheArgType::Type(LatticeType::Concrete(ty)),
            };
            assert_eq!(via_widen, via_policy, "mismatch for {cv:?}");
        }
    }

    #[test]
    fn const_specialization_widen_carries_concrete_type() {
        // Widened ints keep Int64 so f(big1)/f(big2) collapse, and the carried
        // concrete type matches the value's natural type.
        assert_eq!(
            const_specialization(&ConstValue::Int64(i64::MAX)),
            SpecializationConst::Widen(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        // i64::MIN has no representable abs; must widen, not alias to a small int.
        assert_eq!(
            const_specialization(&ConstValue::Int64(i64::MIN)),
            SpecializationConst::Widen(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }
}
