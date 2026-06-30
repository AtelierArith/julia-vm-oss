//! CodeInstance-like specialization units for AoT compilation.
//!
//! AoT lowering, inference, and backend codegen need a shared unit that names
//! one method specialization and carries its dependency edges. This is smaller
//! than Julia's full `CodeInstance`, but it gives the native AoT pipeline a
//! first-class compilation unit instead of passing ad-hoc `(name, arg_types)`
//! tuples between phases.

use crate::aot::inference::FunctionSignature;
use crate::aot::types::StaticType;
use crate::compile::abstract_interp::engine::InferenceCacheKey;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::CoreType;
use crate::ir::core::Function;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// Stable key for one AoT method specialization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeInstanceKey {
    /// Julia-level function name before Rust identifier mangling.
    pub function: String,
    /// Concrete argument tuple that selects this specialization.
    pub arg_types: Vec<StaticType>,
    /// Shared compile/AoT cache identity used for specialization de-duplication.
    pub inference_key: InferenceCacheKey,
}

impl CodeInstanceKey {
    pub fn new(function: impl Into<String>, arg_types: Vec<StaticType>) -> Self {
        let function = function.into();
        let lattice_arg_types = arg_types
            .iter()
            .map(lattice_type_for_static_type)
            .collect::<Vec<_>>();
        let inference_key = InferenceCacheKey::new(&function, &lattice_arg_types);
        Self::new_with_inference_key(function, arg_types, inference_key)
    }

    pub fn new_with_inference_key(
        function: impl Into<String>,
        arg_types: Vec<StaticType>,
        inference_key: InferenceCacheKey,
    ) -> Self {
        let function = function.into();
        debug_assert_eq!(
            function, inference_key.fn_id,
            "AoT specialization function name must match inference cache key"
        );
        Self {
            function,
            arg_types,
            inference_key,
        }
    }

    pub fn arity(&self) -> usize {
        self.arg_types.len()
    }

    fn sort_key(&self) -> String {
        format!("{:?}", self)
    }
}

pub(crate) fn lattice_type_for_static_type(ty: &StaticType) -> LatticeType {
    LatticeType::Concrete(ConcreteType::from(&CoreType::from(ty)))
}

impl fmt::Display for CodeInstanceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args = self
            .arg_types
            .iter()
            .map(StaticType::julia_type_name)
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "{}({})", self.function, args)
    }
}

/// Coarse lifecycle state for an AoT specialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeInstanceState {
    /// Discovered by a root call site or dependency edge.
    Enqueued,
    /// Inference has attached a signature and return type.
    Inferred,
    /// A backend has emitted code for this instance.
    Emitted,
}

/// AoT-owned specialization unit.
#[derive(Debug, Clone)]
pub struct CodeInstance {
    pub key: CodeInstanceKey,
    /// Source method IR for user-defined methods. Builtins and future runtime
    /// helpers may have no source function.
    pub source: Option<Function>,
    pub signature: Option<FunctionSignature>,
    pub return_type: Option<StaticType>,
    pub dependencies: Vec<CodeInstanceKey>,
    /// Backend-specific symbol or artifact name once emitted.
    pub emitted_artifact: Option<String>,
    pub state: CodeInstanceState,
}

impl CodeInstance {
    pub fn enqueued(key: CodeInstanceKey) -> Self {
        Self {
            key,
            source: None,
            signature: None,
            return_type: None,
            dependencies: Vec::new(),
            emitted_artifact: None,
            state: CodeInstanceState::Enqueued,
        }
    }

    pub fn attach_inference(&mut self, source: Function, signature: FunctionSignature) {
        self.return_type = Some(signature.return_type.clone());
        self.signature = Some(signature);
        self.source = Some(source);
        self.state = CodeInstanceState::Inferred;
    }

    pub fn add_dependency(&mut self, dependency: CodeInstanceKey) -> bool {
        if self.dependencies.contains(&dependency) {
            return false;
        }
        self.dependencies.push(dependency);
        true
    }
}

/// Work queue and registry for AoT specializations.
#[derive(Debug, Clone, Default)]
pub struct SpecializationQueue {
    instances: HashMap<CodeInstanceKey, CodeInstance>,
    pending: VecDeque<CodeInstanceKey>,
    pending_set: HashSet<CodeInstanceKey>,
}

impl SpecializationQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a specialization if it has not been seen before.
    pub fn enqueue(&mut self, key: CodeInstanceKey) -> bool {
        if self.instances.contains_key(&key) {
            return false;
        }

        self.pending_set.insert(key.clone());
        self.pending.push_back(key.clone());
        self.instances
            .insert(key.clone(), CodeInstance::enqueued(key));
        true
    }

    /// Record that `owner` depends on `dependency`, enqueueing the dependency.
    pub fn add_dependency(&mut self, owner: &CodeInstanceKey, dependency: CodeInstanceKey) -> bool {
        self.enqueue(owner.clone());
        self.enqueue(dependency.clone());
        self.instances
            .get_mut(owner)
            .expect("owner instance was enqueued")
            .add_dependency(dependency)
    }

    pub fn pop_next(&mut self) -> Option<CodeInstanceKey> {
        let next = self.pending.pop_front()?;
        self.pending_set.remove(&next);
        Some(next)
    }

    pub fn attach_inference(&mut self, source: &Function, signature: FunctionSignature) {
        let matching_keys = self
            .instances
            .keys()
            .filter(|key| key.function == source.name && key.arg_types == signature.param_types)
            .cloned()
            .collect::<Vec<_>>();

        if matching_keys.is_empty() {
            let key = CodeInstanceKey::new(source.name.clone(), signature.param_types.clone());
            self.enqueue(key.clone());
            self.instances
                .get_mut(&key)
                .expect("instance was enqueued")
                .attach_inference(source.clone(), signature);
            return;
        }

        for key in matching_keys {
            self.instances
                .get_mut(&key)
                .expect("matching instance exists")
                .attach_inference(source.clone(), signature.clone());
        }
    }

    pub fn get(&self, key: &CodeInstanceKey) -> Option<&CodeInstance> {
        self.instances.get(key)
    }

    pub fn instances(&self) -> impl Iterator<Item = &CodeInstance> {
        self.instances.values()
    }

    pub fn keys_snapshot(&self) -> Vec<CodeInstanceKey> {
        let mut keys = self.instances.keys().cloned().collect::<Vec<_>>();
        keys.sort_by_key(CodeInstanceKey::sort_key);
        keys
    }

    pub fn observed_args_for(&self, function: &str) -> Vec<&[StaticType]> {
        self.instances
            .keys()
            .filter(|key| key.function == function)
            .map(|key| key.arg_types.as_slice())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::abstract_interp::engine::{widen_argtype_for_cache_key, CacheArgType};
    use crate::compile::lattice::types::ConstValue;

    fn const_arg(cv: ConstValue) -> CacheArgType {
        widen_argtype_for_cache_key(&LatticeType::Const(cv))
    }

    fn type_arg(ty: &StaticType) -> CacheArgType {
        CacheArgType::Type(lattice_type_for_static_type(ty))
    }

    #[test]
    fn enqueue_deduplicates_code_instance_keys() {
        let mut queue = SpecializationQueue::new();
        let key = CodeInstanceKey::new("f", vec![StaticType::I64]);

        assert!(queue.enqueue(key.clone()));
        assert!(!queue.enqueue(key.clone()));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop_next(), Some(key));
        assert_eq!(queue.pop_next(), None);
    }

    #[test]
    fn dependency_edges_enqueue_callee_once() {
        let mut queue = SpecializationQueue::new();
        let owner = CodeInstanceKey::new("outer", vec![StaticType::I64]);
        let callee = CodeInstanceKey::new("inner", vec![StaticType::F64]);

        assert!(queue.add_dependency(&owner, callee.clone()));
        assert!(!queue.add_dependency(&owner, callee.clone()));

        let owner_instance = queue.get(&owner).expect("owner instance");
        assert_eq!(owner_instance.dependencies, vec![callee]);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn display_uses_julia_type_names() {
        let key = CodeInstanceKey::new("mix", vec![StaticType::I64, StaticType::F64]);
        assert_eq!(key.to_string(), "mix(Int64, Float64)");
    }

    #[test]
    fn issue_4272_const_bool_arg_keys_remain_distinct() {
        let true_key = CodeInstanceKey::new_with_inference_key(
            "f4272",
            vec![StaticType::Bool],
            InferenceCacheKey::from_argtypes(
                "f4272",
                vec![CacheArgType::Const(ConstValue::Bool(true))],
            ),
        );
        let false_key = CodeInstanceKey::new_with_inference_key(
            "f4272",
            vec![StaticType::Bool],
            InferenceCacheKey::from_argtypes(
                "f4272",
                vec![CacheArgType::Const(ConstValue::Bool(false))],
            ),
        );

        assert_ne!(true_key, false_key);
    }

    #[test]
    fn issue_8372_code_instance_key_stores_inference_cache_key_directly() {
        use crate::compile::abstract_interp::engine::{CacheArgType, InferenceCacheKey};
        use crate::compile::lattice::types::{ConcreteType, LatticeType};
        use crate::inference_core::{CorePrimitive, CoreType};

        let expected = InferenceCacheKey::from_argtypes(
            "f8372",
            vec![
                CacheArgType::Const(ConstValue::Bool(true)),
                CacheArgType::Type(LatticeType::Concrete(ConcreteType::Core(
                    CoreType::Primitive(CorePrimitive::Int64),
                ))),
            ],
        );
        let key = CodeInstanceKey::new_with_inference_key(
            "f8372",
            vec![StaticType::Bool, StaticType::I64],
            expected.clone(),
        );

        assert_eq!(key.inference_key, expected);
    }

    #[test]
    fn issue_4272_from_const_value_preserves_eligible_constants() {
        // Eligible constants map to shared cache-key const slots, NOT the ABI
        // fallback type.
        assert_eq!(
            const_arg(ConstValue::Bool(true)),
            CacheArgType::Const(ConstValue::Bool(true))
        );
        assert_eq!(
            const_arg(ConstValue::Nothing),
            CacheArgType::Const(ConstValue::Nothing)
        );
        assert_eq!(
            const_arg(ConstValue::Symbol("a".to_string())),
            CacheArgType::Const(ConstValue::Symbol("a".to_string()))
        );
        assert_eq!(
            const_arg(ConstValue::Int64(3)),
            CacheArgType::Const(ConstValue::Int64(3))
        );
    }

    #[test]
    fn issue_4272_from_const_value_widens_unprofitable_constants() {
        // Large ints, floats, and strings widen to the shared lattice type, so
        // distinct values reuse one specialization.
        assert_eq!(
            const_arg(ConstValue::Int64(1_000_000)),
            type_arg(&StaticType::I64)
        );
        assert_eq!(
            const_arg(ConstValue::Float64(2.5)),
            type_arg(&StaticType::F64)
        );
        assert_eq!(
            const_arg(ConstValue::String("x".to_string())),
            type_arg(&StaticType::Str)
        );
    }

    #[test]
    fn issue_4272_aot_and_compile_paths_agree_on_const_specialization() {
        use crate::compile::lattice::types::LatticeType;

        // AoT now constructs the same `InferenceCacheKey` type as the compile
        // path, so representative ConstValues must produce identical keys.
        let cases = [
            ConstValue::Bool(true),
            ConstValue::Bool(false),
            ConstValue::Nothing,
            ConstValue::Symbol("s".to_string()),
            ConstValue::Int64(0),
            ConstValue::Int64(8),
            ConstValue::Int64(9),
            ConstValue::Int64(1_000_000),
            ConstValue::Float64(2.5),
            ConstValue::String("hi".to_string()),
        ];
        for cv in cases {
            let compile = widen_argtype_for_cache_key(&LatticeType::Const(cv.clone()));
            let aot = InferenceCacheKey::from_argtypes("f4272", vec![compile.clone()]);
            let compile = InferenceCacheKey::new("f4272", &[LatticeType::Const(cv.clone())]);

            assert_eq!(
                aot, compile,
                "AoT and compile disagree on cache key construction for {cv:?}"
            );
        }
    }

    #[test]
    fn issue_4272_attach_inference_updates_const_specializations() {
        let mut queue = SpecializationQueue::new();
        let true_key = CodeInstanceKey::new_with_inference_key(
            "f4272",
            vec![StaticType::Bool],
            InferenceCacheKey::from_argtypes(
                "f4272",
                vec![CacheArgType::Const(ConstValue::Bool(true))],
            ),
        );
        let false_key = CodeInstanceKey::new_with_inference_key(
            "f4272",
            vec![StaticType::Bool],
            InferenceCacheKey::from_argtypes(
                "f4272",
                vec![CacheArgType::Const(ConstValue::Bool(false))],
            ),
        );
        queue.enqueue(true_key.clone());
        queue.enqueue(false_key.clone());

        let source = Function {
            name: "f4272".to_string(),
            params: vec![crate::ir::core::TypedParam::new(
                "flag".to_string(),
                Some(crate::types::JuliaType::Bool),
                crate::span::Span::new(0, 0, 1, 1, 0, 0),
            )],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: crate::ir::core::Block {
                stmts: vec![],
                span: crate::span::Span::new(0, 0, 1, 1, 0, 0),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: crate::span::Span::new(0, 0, 1, 1, 0, 0),
        };
        let signature = FunctionSignature::new(
            "f4272".to_string(),
            vec!["flag".to_string()],
            vec![StaticType::Bool],
            StaticType::I64,
        );

        queue.attach_inference(&source, signature);

        assert_eq!(queue.len(), 2);
        assert_eq!(
            queue.get(&true_key).and_then(|i| i.return_type.clone()),
            Some(StaticType::I64)
        );
        assert_eq!(
            queue.get(&false_key).and_then(|i| i.return_type.clone()),
            Some(StaticType::I64)
        );
    }
}
