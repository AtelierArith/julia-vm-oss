//! Runtime-visible type metadata — partial extraction for `subset_julia_vm_types`.
//!
//! This module holds the parts of the `runtime_types` facade that have been
//! extracted into this crate: the type lattice definitions (`LatticeType`,
//! `ConcreteType`, `ConstValue`), parametric constructor inference, and the
//! runtime-visible effect / exception models (`Effects`, `EffectBit`,
//! `ExceptionType`, `BaseCalleeExceptionClassifier`), effect inference walkers,
//! plus the abstract interpretation type environment (`TypeEnv`). The
//! remaining parts of the facade (`MethodTable`, `MethodSig`, `StructInfo`,
//! `ParametricStructDef`, and the re-exports that bridge `compile` and `vm`)
//! stay in the main
//! `subset_julia_vm` crate because they depend on `vm::ValueType` which has not
//! yet been moved to a shared lower layer.
//!
//! See `docs/vm/CRATE_SPLIT.md` §4.2 and Issue #8655.

pub mod effect_inference;
pub mod effects;
pub mod exception;
pub mod function_effects;
pub(crate) mod lattice;
pub mod parametric;
pub mod type_env;

pub use effect_inference::{
    infer_binary_op_effects, infer_builtin_effects, infer_expr_effects,
    infer_expr_effects_with_callees, infer_unary_op_effects,
};
pub use effects::{EffectBit, Effects};
pub use exception::{BaseCalleeExceptionClassifier, ExceptionType};
pub use function_effects::{compute_function_effects, infer_function_effects, FuncId};
pub use lattice::{
    ConcreteType, ConstValue, LatticeType, MAX_INFERENCE_ITERATIONS, MAX_UNION_COMPLEXITY,
    MAX_UNION_LENGTH,
};
pub use parametric::infer_parametric_type_args;
pub use type_env::TypeEnv;

#[cfg(test)]
mod tests {
    use super::{
        infer_builtin_effects, infer_function_effects, infer_parametric_type_args,
        BaseCalleeExceptionClassifier, EffectBit, Effects, ExceptionType, LatticeType, TypeEnv,
    };
    use crate::ir::core::{Block, Expr, Function, Literal, Stmt, StructDef, StructField};
    use crate::types::{JuliaType, TypeExpr, TypeParam};
    use std::collections::HashMap;
    use subset_julia_vm_ir::Span;

    #[test]
    fn parametric_type_arg_inference_lives_in_types_crate_issue_9090() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let def = StructDef {
            name: "Box".to_string(),
            is_mutable: false,
            is_base_origin: false,
            type_params: vec![TypeParam::new("T".to_string())],
            parent_type: None,
            fields: vec![StructField {
                name: "value".to_string(),
                type_expr: Some(TypeExpr::TypeVar("T".to_string())),
                span,
            }],
            inner_constructors: vec![],
            span,
            global_new_helpers: Vec::new(),
        };
        let inferred = infer_parametric_type_args(&def, "Box", &[JuliaType::Int64]).unwrap();
        assert_eq!(inferred, vec![JuliaType::Int64]);
    }
    #[test]
    fn effects_model_lives_in_types_crate_issue_9090() {
        let pure = Effects::pure_arithmetic();
        assert!(pure.is_pure());
        assert!(pure.is_foldable());

        let arbitrary = Effects::arbitrary();
        assert!(!arbitrary.is_removable());
        assert_eq!(
            EffectBit::AlwaysTrue.merge(&EffectBit::AlwaysFalse),
            EffectBit::Conditional
        );
    }

    #[test]
    fn exception_model_lives_in_types_crate_issue_9090() {
        let merged =
            ExceptionType::Known("DomainError").merge(&ExceptionType::Known("InexactError"));
        let ExceptionType::Union(names) = merged else {
            panic!("expected distinct known exceptions to canonicalize as a union");
        };
        assert!(names.contains("DomainError"));
        assert!(names.contains("InexactError"));

        struct StaticClassifier;
        impl BaseCalleeExceptionClassifier for StaticClassifier {
            fn classify_base_callee(
                &mut self,
                name: &str,
                _arg_types: &[LatticeType],
            ) -> Option<ExceptionType> {
                (name == "sqrt").then_some(ExceptionType::Known("DomainError"))
            }
        }

        let mut classifier = StaticClassifier;
        assert_eq!(
            classifier.classify_base_callee("sqrt", &[LatticeType::Top]),
            Some(ExceptionType::Known("DomainError"))
        );
    }

    #[test]
    fn type_env_lives_in_types_crate_issue_9090() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Top);
        let snapshot = env.snapshot();

        env.set("x", LatticeType::Bottom);
        assert_eq!(env.get("x"), Some(&LatticeType::Bottom));

        env.restore(snapshot);
        assert_eq!(env.get("x"), Some(&LatticeType::Top));
    }

    #[test]
    fn effect_walkers_live_in_types_crate_issue_9090() {
        assert!(infer_builtin_effects("+", &[]).is_pure());

        let span = Span::new(0, 0, 1, 1, 1, 1);
        let func = Function {
            name: "const_one".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Literal(Literal::Int(1), span)),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        };

        let effects = infer_function_effects(&func, &HashMap::new());
        assert!(effects.is_pure());
        assert!(effects.nothrow);
        assert!(effects.terminates);
    }
}
