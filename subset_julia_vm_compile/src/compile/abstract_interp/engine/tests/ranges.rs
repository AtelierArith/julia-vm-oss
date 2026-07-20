use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_infer_range_int() {
    let mut engine = InferenceEngine::new();
    let env = TypeEnv::new();

    // 1:10 -> Range{Int64}
    let range_expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
        step: None,
        stop: Box::new(Expr::Literal(Literal::Int(10), dummy_span())),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&range_expr, &env);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))),
        })
    );
}

#[test]
fn test_infer_range_with_step() {
    let mut engine = InferenceEngine::new();
    let env = TypeEnv::new();

    // 1:2:10 -> Range{Int64}
    let range_expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
        step: Some(Box::new(Expr::Literal(Literal::Int(2), dummy_span()))),
        stop: Box::new(Expr::Literal(Literal::Int(10), dummy_span())),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&range_expr, &env);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))),
        })
    );
}

#[test]
fn test_infer_range_float() {
    let mut engine = InferenceEngine::new();
    let env = TypeEnv::new();

    // 1.0:10.0 -> Range{Float64}
    let range_expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Float(1.0), dummy_span())),
        step: None,
        stop: Box::new(Expr::Literal(Literal::Float(10.0), dummy_span())),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&range_expr, &env);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))),
        })
    );
}

#[test]
fn array_getindex_with_unknown_index_has_unknown_cardinality_issue_10970() {
    let mut engine = InferenceEngine::new();
    let array_type = LatticeType::Concrete(ConcreteType::Array {
        element: Box::new(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        ndims: Some(1),
    });
    let mut env = TypeEnv::new();
    env.set("a", array_type.clone());
    env.set("k", LatticeType::Top);

    let dynamic_index = Expr::Index {
        array: Box::new(Expr::Var("a".to_string().into(), dummy_span())),
        indices: vec![Expr::Var("k".to_string().into(), dummy_span())],
        span: dummy_span(),
    };
    assert_eq!(
        engine.infer_expr(&dynamic_index, &env),
        LatticeType::Top,
        "an Any index may select either one element or an array"
    );

    let scalar_index = Expr::Index {
        array: Box::new(Expr::Var("a".to_string().into(), dummy_span())),
        indices: vec![Expr::Literal(Literal::Int(2), dummy_span())],
        span: dummy_span(),
    };
    assert_eq!(
        engine.infer_expr(&scalar_index, &env),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );

    env.set(
        "ci",
        LatticeType::Concrete(ConcreteType::Struct {
            name: "M.CartesianIndex{2}".to_string(),
            type_id: 0,
        }),
    );
    let cartesian_index = Expr::Index {
        array: Box::new(Expr::Var("a".to_string().into(), dummy_span())),
        indices: vec![Expr::Var("ci".to_string().into(), dummy_span())],
        span: dummy_span(),
    };
    assert_eq!(
        engine.infer_expr(&cartesian_index, &env),
        LatticeType::Top,
        "an untrusted nominal spelling must not prove CartesianIndex cardinality",
    );

    env.set(
        "ci",
        LatticeType::Concrete(ConcreteType::Core(CoreType::Struct {
            name: "ArbitraryIndex".to_string(),
            params: Vec::new(),
        })),
    );
    assert_eq!(
        engine.infer_expr(&cartesian_index, &env),
        LatticeType::Top,
        "arbitrary Core Struct indices also have unknown cardinality",
    );

    let range_index = Expr::Index {
        array: Box::new(Expr::Var("a".to_string().into(), dummy_span())),
        indices: vec![Expr::Range {
            start: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
            step: Some(Box::new(Expr::Literal(Literal::Int(2), dummy_span()))),
            stop: Box::new(Expr::Literal(Literal::Int(4), dummy_span())),
            span: dummy_span(),
        }],
        span: dummy_span(),
    };
    assert_eq!(
        engine.infer_expr(&range_index, &env),
        LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        }),
    );
}
