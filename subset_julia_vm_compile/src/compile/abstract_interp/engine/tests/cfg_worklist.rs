use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_cfg_worklist_records_branch_and_loop_states_4267() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "cfg_states".to_string(),
        params: vec![TypedParam {
            name: "flag".to_string(),
            type_annotation: Some(JuliaType::Bool),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Assign {
                    var: "x".to_string(),
                    value: Expr::Literal(Literal::Int(1), dummy_span()),
                    span: dummy_span(),
                },
                Stmt::If {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    then_branch: Block {
                        stmts: vec![Stmt::Assign {
                            var: "y".to_string(),
                            value: Expr::Literal(Literal::Int(2), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![Stmt::Assign {
                            var: "y".to_string(),
                            value: Expr::Literal(Literal::Float(2.5), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
                Stmt::While {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    body: Block {
                        stmts: vec![Stmt::Assign {
                            var: "z".to_string(),
                            value: Expr::Literal(Literal::Bool(true), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                },
                Stmt::Expr {
                    expr: Expr::Var("y".to_string().into(), dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_ne!(
        result,
        LatticeType::Bottom,
        "CFG observation must not replace legacy return inference"
    );

    let join_input = engine
        .cfg_block_input("cfg_states", BlockId(2))
        .expect("if join / while header block should have CFG input");
    let y_ty = join_input
        .get("y")
        .expect("branch assignment should reach the CFG join input");
    assert_ne!(y_ty, &LatticeType::Const(ConstValue::Int64(2)));
    assert_ne!(y_ty, &LatticeType::Const(ConstValue::Float64(2.5)));

    let loop_body_output = engine
        .cfg_block_output("cfg_states", BlockId(4))
        .expect("while body block should have CFG output");
    assert_eq!(
        loop_body_output.get("z"),
        Some(&LatticeType::Const(ConstValue::Bool(true)))
    );

    let loop_exit_input = engine
        .cfg_block_input("cfg_states", BlockId(5))
        .expect("while exit block should have CFG input");
    assert_eq!(
        loop_exit_input.get("z"),
        Some(&LatticeType::Const(ConstValue::Bool(true)))
    );

    assert_eq!(
        engine.statement_type("cfg_states", 2),
        Some(&LatticeType::Const(ConstValue::Int64(2)))
    );
    assert_eq!(
        engine.statement_type("cfg_states", 3),
        Some(&LatticeType::Const(ConstValue::Float64(2.5)))
    );
}

#[test]
fn test_cfg_worklist_applies_if_edge_narrowing_5602() {
    let mut engine = InferenceEngine::new();

    let condition = Expr::Builtin {
        name: BuiltinOp::Isa,
        args: vec![
            Expr::Var("val".to_string().into(), dummy_span()),
            Expr::Var("Int64".to_string().into(), dummy_span()),
        ],
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_edge_narrowing".to_string(),
        params: vec![TypedParam {
            name: "val".to_string(),
            type_annotation: Some(JuliaType::Union(vec![JuliaType::Int64, JuliaType::String])),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::If {
                    condition,
                    then_branch: Block {
                        stmts: vec![Stmt::Expr {
                            expr: Expr::Var("val".to_string().into(), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![Stmt::Expr {
                            expr: Expr::Var("val".to_string().into(), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
                Stmt::Expr {
                    expr: Expr::Var("val".to_string().into(), dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_ne!(
        result,
        LatticeType::Bottom,
        "CFG edge narrowing must remain observe-only for return inference"
    );

    let then_input = engine
        .cfg_block_input("cfg_edge_narrowing", BlockId(1))
        .expect("then successor should have CFG input");
    assert_eq!(
        then_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );

    let else_input = engine
        .cfg_block_input("cfg_edge_narrowing", BlockId(3))
        .expect("else successor should have CFG input");
    assert_eq!(
        else_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );

    let join_input = engine
        .cfg_block_input("cfg_edge_narrowing", BlockId(2))
        .expect("join block should have CFG input");
    assert_eq!(
        join_input.get("val"),
        Some(&LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
            ]
            .into_iter()
            .collect()
        ))
    );
}

#[test]
fn test_cfg_worklist_records_loop_carried_backedge_state_5602() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "cfg_loop_carried".to_string(),
        params: vec![TypedParam {
            name: "flag".to_string(),
            type_annotation: Some(JuliaType::Bool),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Assign {
                    var: "x".to_string(),
                    value: Expr::Literal(Literal::Int(1), dummy_span()),
                    span: dummy_span(),
                },
                Stmt::While {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    body: Block {
                        stmts: vec![Stmt::Assign {
                            var: "x".to_string(),
                            value: Expr::Literal(Literal::Float(2.5), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                },
                Stmt::Expr {
                    expr: Expr::Var("x".to_string().into(), dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_ne!(
        result,
        LatticeType::Bottom,
        "CFG loop-carried state must remain observe-only for return inference"
    );

    let expected = LatticeType::Union(
        [
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ]
        .into_iter()
        .collect(),
    );

    let header_input = engine
        .cfg_block_input("cfg_loop_carried", BlockId(1))
        .expect("loop header should have CFG input");
    assert_eq!(header_input.get("x"), Some(&expected));

    let body_input = engine
        .cfg_block_input("cfg_loop_carried", BlockId(2))
        .expect("loop body should be revisited with carried CFG input");
    assert_eq!(body_input.get("x"), Some(&expected));

    let exit_input = engine
        .cfg_block_input("cfg_loop_carried", BlockId(3))
        .expect("loop exit should have joined CFG input");
    assert_eq!(exit_input.get("x"), Some(&expected));
}
