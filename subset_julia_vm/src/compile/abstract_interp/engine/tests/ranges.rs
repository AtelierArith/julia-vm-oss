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
