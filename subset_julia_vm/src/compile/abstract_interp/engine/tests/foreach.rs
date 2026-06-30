use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_foreach_array_int() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // for x in [1, 2, 3]
    //     # x should be Int64
    // end
    let stmt = Stmt::ForEach {
        var: "x".to_string(),
        iterable: Expr::ArrayLiteral {
            elements: vec![
                Expr::Literal(Literal::Int(1), dummy_span()),
                Expr::Literal(Literal::Int(2), dummy_span()),
                Expr::Literal(Literal::Int(3), dummy_span()),
            ],
            shape: vec![3],
            span: dummy_span(),
        },
        body: Block {
            stmts: vec![],
            span: dummy_span(),
        },
        span: dummy_span(),
    };

    engine.infer_stmt(&stmt, &mut env);

    // Check that x was inferred as Int64
    assert_eq!(
        env.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
}

#[test]
fn test_foreach_tuple_heterogeneous() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // for x in (1, 2.0)
    //     # x should be Union{Int64, Float64}
    // end
    let stmt = Stmt::ForEach {
        var: "x".to_string(),
        iterable: Expr::TupleLiteral {
            elements: vec![
                Expr::Literal(Literal::Int(1), dummy_span()),
                Expr::Literal(Literal::Float(2.0), dummy_span()),
            ],
            span: dummy_span(),
        },
        body: Block {
            stmts: vec![],
            span: dummy_span(),
        },
        span: dummy_span(),
    };

    engine.infer_stmt(&stmt, &mut env);

    // Check that x was inferred as Union{Int64, Float64}
    assert!(
        matches!(env.get("x"), Some(LatticeType::Union(_))),
        "Expected Union type, got {:?}",
        env.get("x")
    );
    if let Some(LatticeType::Union(types)) = env.get("x") {
        assert_eq!(types.len(), 2);
        assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        ))));
        assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        ))));
    }
}

#[test]
fn test_foreach_tuple_homogeneous() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // for x in (1, 2, 3)
    //     # x should be Int64 (all elements same type)
    // end
    let stmt = Stmt::ForEach {
        var: "x".to_string(),
        iterable: Expr::TupleLiteral {
            elements: vec![
                Expr::Literal(Literal::Int(1), dummy_span()),
                Expr::Literal(Literal::Int(2), dummy_span()),
                Expr::Literal(Literal::Int(3), dummy_span()),
            ],
            span: dummy_span(),
        },
        body: Block {
            stmts: vec![],
            span: dummy_span(),
        },
        span: dummy_span(),
    };

    engine.infer_stmt(&stmt, &mut env);

    // Check that x was inferred as Int64
    assert_eq!(
        env.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
}

#[test]
fn test_foreach_string() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // for c in "hello"
    //     # c should be Char
    // end
    let stmt = Stmt::ForEach {
        var: "c".to_string(),
        iterable: Expr::Literal(Literal::Str("hello".to_string()), dummy_span()),
        body: Block {
            stmts: vec![],
            span: dummy_span(),
        },
        span: dummy_span(),
    };

    engine.infer_stmt(&stmt, &mut env);

    // Check that c was inferred as Char
    assert_eq!(
        env.get("c"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Char)
        )))
    );
}

#[test]
fn test_foreach_array_float() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // for x in [1.0, 2.0, 3.0]
    //     # x should be Float64
    // end
    let stmt = Stmt::ForEach {
        var: "x".to_string(),
        iterable: Expr::ArrayLiteral {
            elements: vec![
                Expr::Literal(Literal::Float(1.0), dummy_span()),
                Expr::Literal(Literal::Float(2.0), dummy_span()),
                Expr::Literal(Literal::Float(3.0), dummy_span()),
            ],
            shape: vec![3],
            span: dummy_span(),
        },
        body: Block {
            stmts: vec![],
            span: dummy_span(),
        },
        span: dummy_span(),
    };

    engine.infer_stmt(&stmt, &mut env);

    // Check that x was inferred as Float64
    assert_eq!(
        env.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );
}

#[test]
fn test_foreach_updates_accumulator_type() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // sum starts as Int64
    env.set(
        "sum",
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );

    // for x in [1.0, 2.0]
    //     sum = sum + x
    // end
    let stmt = Stmt::ForEach {
        var: "x".to_string(),
        iterable: Expr::ArrayLiteral {
            elements: vec![
                Expr::Literal(Literal::Float(1.0), dummy_span()),
                Expr::Literal(Literal::Float(2.0), dummy_span()),
            ],
            shape: vec![2],
            span: dummy_span(),
        },
        body: Block {
            stmts: vec![Stmt::Assign {
                var: "sum".to_string(),
                value: Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Var("sum".to_string(), dummy_span())),
                    right: Box::new(Expr::Var("x".to_string(), dummy_span())),
                    span: dummy_span(),
                },
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        span: dummy_span(),
    };

    engine.infer_stmt(&stmt, &mut env);

    assert!(
        matches!(
            env.get("sum"),
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            ))) | Some(LatticeType::Union(_))
        ),
        "Expected sum to include Float64, got {:?}",
        env.get("sum")
    );
    if let Some(LatticeType::Union(types)) = env.get("sum") {
        assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        ))));
    }
}
