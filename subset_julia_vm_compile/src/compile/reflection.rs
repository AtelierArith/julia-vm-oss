//! Reflection-inference adapter owned with the concrete inference engine.

use super::abstract_interp::InferenceEngine;
use crate::ir::core::Function;
use crate::runtime_types::{
    BaseCalleeExceptionClassifier, ExceptionType, LatticeType, MethodSig, ParametricStructDef,
    ReflectionInferenceSession, TypeEnv,
};
use std::collections::{HashMap, HashSet};

impl ReflectionInferenceSession for InferenceEngine {
    fn set_parametric_structs(&mut self, structs: HashMap<String, ParametricStructDef>) {
        InferenceEngine::set_parametric_structs(self, structs);
    }

    fn set_base_function_names(&mut self, names: HashSet<String>) {
        InferenceEngine::set_base_function_names(self, names);
    }

    fn add_initial_method(&mut self, table_name: String, sig: MethodSig) {
        InferenceEngine::add_initial_method(self, table_name, sig);
    }

    fn infer_function_with_arg_types(
        &mut self,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> LatticeType {
        InferenceEngine::infer_function_with_arg_types(self, func, arg_types)
    }

    fn infer_function_with_arg_types_and_base_env(
        &mut self,
        func: &Function,
        arg_types: &[LatticeType],
        base_env: &TypeEnv,
    ) -> LatticeType {
        InferenceEngine::infer_function_with_arg_types_and_base_env(self, func, arg_types, base_env)
    }

    fn infer_function_exception_type(
        &mut self,
        classifier: &mut dyn BaseCalleeExceptionClassifier,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> ExceptionType {
        InferenceEngine::infer_function_exception_type(self, classifier, func, arg_types)
    }
}
