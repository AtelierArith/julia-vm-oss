//! Runtime type metadata shared by the compiler and interpreter.

pub mod bridge;

pub use bridge::{
    julia_type_to_lattice, lattice_to_julia_type, lattice_to_parametric_julia_type,
    lattice_to_value_type,
};

pub use crate::struct_info;
pub use crate::type_intern;
pub use crate::{
    ArrayElementType, MethodSig, MethodTable, ParametricStructDef, StructInfo, ValueType,
};
pub use subset_julia_vm_types::runtime_types::parametric::infer_parametric_type_args;
pub use subset_julia_vm_types::runtime_types::{
    infer_function_effects, parametric, BaseCalleeExceptionClassifier, ConcreteType, ConstValue,
    EffectBit, Effects, ExceptionType, LatticeType, TypeEnv, MAX_INFERENCE_ITERATIONS,
    MAX_UNION_COMPLEXITY, MAX_UNION_LENGTH,
};
pub use subset_julia_vm_types::types::{JuliaType, StructHierarchy};

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use subset_julia_vm_types::ir::core::Function;

pub trait ReflectionInferenceSession {
    fn set_parametric_structs(&mut self, parametric_structs: HashMap<String, ParametricStructDef>);
    fn set_base_function_names(&mut self, base_function_names: HashSet<String>);
    fn add_initial_method(&mut self, table_name: String, sig: MethodSig);
    fn infer_function_with_arg_types(
        &mut self,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> LatticeType;
    fn infer_function_with_arg_types_and_base_env(
        &mut self,
        func: &Function,
        arg_types: &[LatticeType],
        base_env: &TypeEnv,
    ) -> LatticeType;
    fn infer_function_exception_type(
        &mut self,
        classifier: &mut dyn BaseCalleeExceptionClassifier,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> ExceptionType;
}

pub trait ReflectionInferenceFactory: Sync {
    fn build(
        &self,
        struct_table: &crate::StructRegistry,
        global_types: &HashMap<String, ValueType>,
        all_functions: Vec<Function>,
    ) -> Box<dyn ReflectionInferenceSession>;
}

static REFLECTION_INFERENCE_FACTORY: OnceLock<&'static dyn ReflectionInferenceFactory> =
    OnceLock::new();

pub fn install_reflection_inference_factory(factory: &'static dyn ReflectionInferenceFactory) {
    let _ = REFLECTION_INFERENCE_FACTORY.set(factory);
}

pub fn build_reflection_inference_session<'a>(
    struct_table: &crate::StructRegistry,
    global_types: &HashMap<String, ValueType>,
    all_functions: impl IntoIterator<Item = &'a Function>,
) -> Option<Box<dyn ReflectionInferenceSession>> {
    let all_functions = all_functions.into_iter().cloned().collect::<Vec<_>>();
    REFLECTION_INFERENCE_FACTORY
        .get()
        .copied()
        .map(|factory| factory.build(struct_table, global_types, all_functions))
}
