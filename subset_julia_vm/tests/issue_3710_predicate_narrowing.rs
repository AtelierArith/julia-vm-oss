use std::collections::{BTreeSet, HashMap};
use subset_julia_vm::inference_core::{CorePrimitive, CoreType};

use subset_julia_vm::compile::abstract_interp::InferenceEngine;
use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
use subset_julia_vm::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, TypedParam};
use subset_julia_vm::span::Span;

fn dummy_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

fn call(function: &str, args: Vec<Expr>) -> Expr {
    let splat_mask = vec![false; args.len()];
    Expr::Call {
        function: function.to_string(),
        args,
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask,
        span: dummy_span(),
    }
}

fn ret(value: Expr) -> Stmt {
    Stmt::Return {
        value: Some(value),
        span: dummy_span(),
    }
}

fn unannotated_param(name: &str) -> TypedParam {
    TypedParam::new(name.to_string(), None, dummy_span())
}

#[test]
fn predicate_call_refines_nullable_actual_argument() {
    let is_present = Function {
        name: "is_present_3710".to_string(),
        params: vec![unannotated_param("x")],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![ret(Expr::BinaryOp {
                op: BinaryOp::NotEgal,
                left: Box::new(Expr::Var("x".to_string(), dummy_span())),
                right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
                span: dummy_span(),
            })],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };

    let caller = Function {
        name: "nullable_caller_3710".to_string(),
        params: vec![unannotated_param("x")],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition: call(
                    "is_present_3710",
                    vec![Expr::Var("x".to_string(), dummy_span())],
                ),
                then_branch: Block {
                    stmts: vec![ret(Expr::Var("x".to_string(), dummy_span()))],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![ret(Expr::Literal(Literal::Int(0), dummy_span()))],
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

    let mut nullable = BTreeSet::new();
    nullable.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )));
    nullable.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Nothing,
    )));

    let mut function_table = HashMap::new();
    function_table.insert("is_present_3710".to_string(), is_present);
    function_table.insert("nullable_caller_3710".to_string(), caller.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let result = engine.infer_function_with_arg_types(&caller, &[LatticeType::Union(nullable)]);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
}

#[test]
fn predicate_call_refines_isa_actual_argument() {
    let is_int = Function {
        name: "is_int_3710".to_string(),
        params: vec![unannotated_param("x")],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![ret(call(
                "isa",
                vec![
                    Expr::Var("x".to_string(), dummy_span()),
                    Expr::Var("Int64".to_string(), dummy_span()),
                ],
            ))],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };

    let caller = Function {
        name: "isa_caller_3710".to_string(),
        params: vec![unannotated_param("x")],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition: call(
                    "is_int_3710",
                    vec![Expr::Var("x".to_string(), dummy_span())],
                ),
                then_branch: Block {
                    stmts: vec![ret(Expr::Var("x".to_string(), dummy_span()))],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![ret(Expr::Literal(Literal::Int(0), dummy_span()))],
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

    let mut int_or_string = BTreeSet::new();
    int_or_string.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )));
    int_or_string.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::String,
    )));

    let mut function_table = HashMap::new();
    function_table.insert("is_int_3710".to_string(), is_int);
    function_table.insert("isa_caller_3710".to_string(), caller.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let result =
        engine.infer_function_with_arg_types(&caller, &[LatticeType::Union(int_or_string)]);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
}
