//! Compile-side compatibility shim for inference cache keys.
//!
//! The cache-key type and const-specialization policy are owned by
//! `subset_julia_vm_types::inference_cache_key` so AoT and compile do not
//! depend on each other's module internals (Issue #9090). This module keeps the
//! historical `compile::abstract_interp::engine::*` re-export path valid and
//! retains the compile-local `MethodInstanceKey`, which still depends on Core IR
//! function syntax.

pub use subset_julia_vm_types::inference_cache_key::{
    cache_fn_id_base_name, const_specialization, is_const_eligible, widen_argtype_for_cache_key,
    widen_argtypes_for_cache_key, CacheArgType, InferenceCacheKey, SpecializationConst,
    SMALL_INT_CONST_THRESHOLD,
};

use crate::ir::core::Function;
use crate::types::{JuliaType, TypeParam};
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
    use crate::inference_core::{CorePrimitive, CoreType};

    fn concrete(primitive: CorePrimitive) -> LatticeType {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(primitive)))
    }

    #[test]
    fn inference_cache_key_reexport_serializes_roundtrip_issue_5093() {
        let key = InferenceCacheKey::from_argtypes(
            "f",
            vec![
                CacheArgType::Type(concrete(CorePrimitive::Int64)),
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
        let bare = InferenceCacheKey::new("f", &[concrete(CorePrimitive::Int64)]);
        let specialized = InferenceCacheKey::new("f(Any)", &[concrete(CorePrimitive::Int64)]);

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
}
