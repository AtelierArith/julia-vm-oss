use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

/// Issue #3531: ::BigInt / ::BigFloat parameter annotations must be
/// inferred as their corresponding `ConcreteType`, not collapsed to `Top`.
#[test]
fn test_issue_3531_bigint_annotation_preserved() {
    let engine = InferenceEngine::new();
    assert_eq!(
        engine.julia_type_to_lattice(&JuliaType::BigInt),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt
        ))),
    );
}

#[test]
fn test_issue_3531_bigfloat_annotation_preserved() {
    let engine = InferenceEngine::new();
    assert_eq!(
        engine.julia_type_to_lattice(&JuliaType::BigFloat),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat
        ))),
    );
}

/// Issue #3531: Sanity check for the other annotations called out in the
/// issue text. These already mapped correctly; the test pins the contract.
#[test]
fn test_issue_3531_symbol_nothing_missing_annotations_preserved() {
    let engine = InferenceEngine::new();
    assert_eq!(
        engine.julia_type_to_lattice(&JuliaType::Symbol),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Symbol
        ))),
    );
    assert_eq!(
        engine.julia_type_to_lattice(&JuliaType::Nothing),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing
        ))),
    );
    assert_eq!(
        engine.julia_type_to_lattice(&JuliaType::Missing),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Missing
        ))),
    );
}

/// Issue #3531: A function with a ::BigInt-annotated parameter should
/// see its parameter inferred as `Concrete(BigInt)` in the type env,
/// so a trivial identity returns `Concrete(BigInt)` rather than `Top`.
#[test]
fn test_issue_3531_bigint_param_propagates_through_identity() {
    let mut engine = InferenceEngine::new();
    let func = Function {
        name: "id_bigint".to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::BigInt),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };
    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt
        )))
    );
}

#[test]
fn test_issue_3531_bigfloat_param_propagates_through_identity() {
    let mut engine = InferenceEngine::new();
    let func = Function {
        name: "id_bigfloat".to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::BigFloat),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };
    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat
        )))
    );
}
