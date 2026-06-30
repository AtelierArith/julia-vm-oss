use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_infer_literal_int() {
    let engine = InferenceEngine::new();
    let lit = Literal::Int(42);
    let result = engine.infer_literal(&lit);
    assert_eq!(result, LatticeType::Const(ConstValue::Int64(42)));
}

#[test]
fn test_infer_literal_float() {
    let engine = InferenceEngine::new();
    let lit = Literal::Float(std::f64::consts::PI);
    let result = engine.infer_literal(&lit);
    assert_eq!(
        result,
        LatticeType::Const(ConstValue::Float64(std::f64::consts::PI))
    );
}

#[test]
fn test_infer_simple_function() {
    let mut engine = InferenceEngine::new();

    // function f() return 42 end
    let func = Function {
        name: "f".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Int(42), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };

    let result = engine.infer_function(&func);
    assert_eq!(result, LatticeType::Const(ConstValue::Int64(42)));
}

#[test]
fn test_infer_function_with_addition() {
    let mut engine = InferenceEngine::new();

    // function add(x::Int64, y::Int64) return x + y end
    let func = Function {
        name: "add".to_string(),
        params: vec![
            TypedParam {
                name: "x".to_string(),
                type_annotation: Some(JuliaType::Int64),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "y".to_string(),
                type_annotation: Some(JuliaType::Int64),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Var("x".to_string(), dummy_span())),
                    right: Box::new(Expr::Var("y".to_string(), dummy_span())),
                    span: dummy_span(),
                }),
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
            CorePrimitive::Int64
        )))
    );
}

#[test]
fn test_infer_if_statement() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // if true then return 1 else return 2 end
    let stmt = Stmt::If {
        condition: Expr::Literal(Literal::Bool(true), dummy_span()),
        then_branch: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Int(1), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        else_branch: Some(Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Int(2), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        }),
        span: dummy_span(),
    };

    let result = engine.infer_stmt(&stmt, &mut env);
    assert!(
        matches!(&result, StmtResult::Return(_)),
        "Expected Return, got {:?}",
        result
    );
    if let StmtResult::Return(ty) = result {
        assert_eq!(
            ty,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }
}

#[test]
fn test_issue_6258_empty_while_true_dead_tail_infers_bottom() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "while_true_dead_tail_6258".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::While {
                    condition: Expr::Literal(Literal::Bool(true), dummy_span()),
                    body: Block {
                        stmts: vec![],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                },
                Stmt::Expr {
                    expr: Expr::Literal(Literal::Str("dead".to_string()), dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };

    assert_eq!(engine.infer_function(&func), LatticeType::Bottom);
}
