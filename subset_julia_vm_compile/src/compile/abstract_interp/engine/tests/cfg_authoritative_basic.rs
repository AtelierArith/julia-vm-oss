use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_cfg_authoritative_return_for_straightline_block_5602() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "cfg_straightline_return".to_string(),
        params: vec![],
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
                Stmt::Return {
                    value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                    span: dummy_span(),
                },
                Stmt::Assign {
                    var: "unreachable".to_string(),
                    value: Expr::Literal(Literal::Float(2.5), dummy_span()),
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
    assert_eq!(result, LatticeType::Const(ConstValue::Int64(1)));

    let input = engine
        .cfg_block_input("cfg_straightline_return", BlockId(0))
        .expect("straight-line CFG authoritative path should record block input");
    assert!(input.is_empty());

    let output = engine
        .cfg_block_output("cfg_straightline_return", BlockId(0))
        .expect("straight-line CFG authoritative path should record block output");
    assert_eq!(
        output.get("x"),
        Some(&LatticeType::Const(ConstValue::Int64(1)))
    );
    assert!(
        output.get("unreachable").is_none(),
        "CFG authoritative return path must stop at explicit return"
    );

    assert_eq!(
        engine.statement_type("cfg_straightline_return", 0),
        Some(&LatticeType::Const(ConstValue::Int64(1)))
    );
    assert_eq!(
        engine.statement_type("cfg_straightline_return", 1),
        Some(&LatticeType::Const(ConstValue::Int64(1)))
    );
    assert_eq!(engine.statement_type("cfg_straightline_return", 2), None);
}

#[test]
fn test_cfg_authoritative_all_return_if_5602() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "cfg_all_return_if".to_string(),
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
                Stmt::If {
                    condition: Expr::Var("flag".to_string().into(), dummy_span()),
                    then_branch: Block {
                        stmts: vec![
                            Stmt::Assign {
                                var: "x".to_string(),
                                value: Expr::Literal(Literal::Int(1), dummy_span()),
                                span: dummy_span(),
                            },
                            Stmt::Return {
                                value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                                span: dummy_span(),
                            },
                        ],
                        span: dummy_span(),
                    },
                    else_branch: Some(Block {
                        stmts: vec![
                            Stmt::Assign {
                                var: "x".to_string(),
                                value: Expr::Literal(Literal::Int(2), dummy_span()),
                                span: dummy_span(),
                            },
                            Stmt::Return {
                                value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                                span: dummy_span(),
                            },
                        ],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
                Stmt::Assign {
                    var: "unreachable_tail".to_string(),
                    value: Expr::Literal(Literal::Float(2.5), dummy_span()),
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
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );

    let then_output = engine
        .cfg_block_output("cfg_all_return_if", BlockId(1))
        .expect("then return block should record CFG output");
    assert_eq!(
        then_output.get("x"),
        Some(&LatticeType::Const(ConstValue::Int64(1)))
    );
    let else_output = engine
        .cfg_block_output("cfg_all_return_if", BlockId(3))
        .expect("else return block should record CFG output");
    assert_eq!(
        else_output.get("x"),
        Some(&LatticeType::Const(ConstValue::Int64(2)))
    );

    assert!(
        engine
            .cfg_block_input("cfg_all_return_if", BlockId(2))
            .is_none(),
        "all-returning if join block should remain unreachable"
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_if", 2),
        Some(&LatticeType::Const(ConstValue::Int64(1)))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_if", 4),
        Some(&LatticeType::Const(ConstValue::Int64(2)))
    );
    assert_eq!(engine.statement_type("cfg_all_return_if", 5), None);
}

#[test]
fn test_cfg_authoritative_all_return_if_call_payload_5602() {
    let callee = int_identity_function("cfg_call_id");
    let mut function_table = HashMap::new();
    function_table.insert("cfg_call_id".to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let call_id = |arg: Expr| Expr::Call {
        function: "cfg_call_id".to_string().into(),
        args: vec![arg],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span: dummy_span(),
    };

    let func = Function {
        name: "cfg_all_return_call_if".to_string(),
        params: vec![
            TypedParam {
                name: "flag".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "x".to_string(),
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
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(call_id(Expr::Var("x".to_string().into(), dummy_span()))),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![
                        Stmt::Assign {
                            var: "y".to_string(),
                            value: call_id(Expr::Var("x".to_string().into(), dummy_span())),
                            span: dummy_span(),
                        },
                        Stmt::Return {
                            value: Some(Expr::Var("y".to_string().into(), dummy_span())),
                            span: dummy_span(),
                        },
                    ],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_call_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_call_if", 3),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );

    let else_output = engine
        .cfg_block_output("cfg_all_return_call_if", BlockId(3))
        .expect("else return block should record CFG output");
    assert_eq!(
        else_output.get("y"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_call_if", BlockId(2))
            .is_none(),
        "all-returning call if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_binary_payload_5602() {
    let mut engine = InferenceEngine::new();

    let var_x = || Expr::Var("x".to_string().into(), dummy_span());
    let one = || Expr::Literal(Literal::Int(1), dummy_span());
    let func = Function {
        name: "cfg_all_return_binary_if".to_string(),
        params: vec![
            TypedParam {
                name: "flag".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "x".to_string(),
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
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(var_x()),
                            right: Box::new(one()),
                            span: dummy_span(),
                        }),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::BinaryOp {
                            op: BinaryOp::Sub,
                            left: Box::new(var_x()),
                            right: Box::new(one()),
                            span: dummy_span(),
                        }),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_binary_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_binary_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_binary_if", BlockId(2))
            .is_none(),
        "all-returning binary if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_unary_payload_5602() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "cfg_all_return_unary_if".to_string(),
        params: vec![
            TypedParam {
                name: "flag".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "x".to_string(),
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
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::UnaryOp {
                            op: UnaryOp::Neg,
                            operand: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
                            span: dummy_span(),
                        }),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::UnaryOp {
                            op: UnaryOp::Not,
                            operand: Box::new(Expr::Var("flag".to_string().into(), dummy_span())),
                            span: dummy_span(),
                        }),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            ]
            .into_iter()
            .collect()
        )
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_unary_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_unary_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Bool)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_unary_if", BlockId(2))
            .is_none(),
        "all-returning unary if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_ternary_payload_5602() {
    let mut engine = InferenceEngine::new();

    let var_x = || Expr::Var("x".to_string().into(), dummy_span());
    let choose = || Expr::Var("choose".to_string().into(), dummy_span());
    let one = || Expr::Literal(Literal::Int(1), dummy_span());
    let func = Function {
        name: "cfg_all_return_ternary_if".to_string(),
        params: vec![
            TypedParam {
                name: "flag".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "choose".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "x".to_string(),
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
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Ternary {
                            condition: Box::new(choose()),
                            then_expr: Box::new(Expr::BinaryOp {
                                op: BinaryOp::Add,
                                left: Box::new(var_x()),
                                right: Box::new(one()),
                                span: dummy_span(),
                            }),
                            else_expr: Box::new(Expr::BinaryOp {
                                op: BinaryOp::Sub,
                                left: Box::new(var_x()),
                                right: Box::new(one()),
                                span: dummy_span(),
                            }),
                            span: dummy_span(),
                        }),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Ternary {
                            condition: Box::new(choose()),
                            then_expr: Box::new(Expr::UnaryOp {
                                op: UnaryOp::Neg,
                                operand: Box::new(var_x()),
                                span: dummy_span(),
                            }),
                            else_expr: Box::new(var_x()),
                            span: dummy_span(),
                        }),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_ternary_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_ternary_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_ternary_if", BlockId(2))
            .is_none(),
        "all-returning ternary if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_field_payload_5602() {
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert(
        "x".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    fields.insert(
        "y".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );
    struct_table.insert(
        "Point".to_string(),
        StructTypeInfo::new(1, false, fields, false),
    );
    let mut engine = InferenceEngine::with_struct_table(struct_table);

    let point_field = |field: &str| Expr::FieldAccess {
        object: Box::new(Expr::Var("p".to_string().into(), dummy_span())),
        field: field.to_string().into(),
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_field_if".to_string(),
        params: vec![
            TypedParam {
                name: "flag".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "p".to_string(),
                type_annotation: Some(JuliaType::Struct("Point".to_string())),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(point_field("x")),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(point_field("y")),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            ]
            .into_iter()
            .collect()
        )
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_field_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_field_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_field_if", BlockId(2))
            .is_none(),
        "all-returning field if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_index_payload_5602() {
    let mut engine = InferenceEngine::new();

    let vector_param = |name: &str, element: JuliaType| TypedParam {
        name: name.to_string(),
        type_annotation: Some(JuliaType::VectorOf(Box::new(element))),
        is_varargs: false,
        vararg_count: None,
        span: dummy_span(),
    };
    let first_element = |name: &str| Expr::Index {
        array: Box::new(Expr::Var(name.to_string().into(), dummy_span())),
        indices: vec![Expr::Literal(Literal::Int(1), dummy_span())],
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_index_if".to_string(),
        params: vec![
            TypedParam {
                name: "flag".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            vector_param("ints", JuliaType::Int64),
            vector_param("floats", JuliaType::Float64),
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(first_element("ints")),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(first_element("floats")),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            ]
            .into_iter()
            .collect()
        )
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_index_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_index_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_index_if", BlockId(2))
            .is_none(),
        "all-returning index if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_slice_range_index_payload_5602() {
    let mut engine = InferenceEngine::new();

    let int_lit = |value| Expr::Literal(Literal::Int(value), dummy_span());
    let matrix_slice_all = Expr::Index {
        array: Box::new(Expr::Var("m".to_string().into(), dummy_span())),
        indices: vec![int_lit(1), Expr::SliceAll { span: dummy_span() }],
        span: dummy_span(),
    };
    let matrix_range_index = Expr::Index {
        array: Box::new(Expr::Var("m".to_string().into(), dummy_span())),
        indices: vec![
            Expr::Range {
                start: Box::new(int_lit(1)),
                step: None,
                stop: Box::new(int_lit(2)),
                span: dummy_span(),
            },
            int_lit(1),
        ],
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_slice_range_index_if".to_string(),
        params: vec![
            TypedParam {
                name: "flag".to_string(),
                type_annotation: Some(JuliaType::Bool),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "m".to_string(),
                type_annotation: Some(JuliaType::MatrixOf(Box::new(JuliaType::Int64))),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition: Expr::Var("flag".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(matrix_slice_all),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(matrix_range_index),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let int_array = LatticeType::Concrete(ConcreteType::Array {
        element: Box::new(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        ndims: None,
    });
    let result = engine.infer_function(&func);
    assert_eq!(result, int_array);
    assert_eq!(
        engine.statement_type("cfg_all_return_slice_range_index_if", 1),
        Some(&int_array)
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_slice_range_index_if", 2),
        Some(&int_array)
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_slice_range_index_if", BlockId(2))
            .is_none(),
        "all-returning slice/range index if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_isa_edge_narrowing_5602() {
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
        name: "cfg_all_return_isa_if".to_string(),
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
            stmts: vec![Stmt::If {
                condition,
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Var("val".to_string().into(), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Var("val".to_string().into(), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
            ]
            .into_iter()
            .collect()
        )
    );

    let then_input = engine
        .cfg_block_input("cfg_all_return_isa_if", BlockId(1))
        .expect("then return block should get narrowed CFG input");
    assert_eq!(
        then_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    let else_input = engine
        .cfg_block_input("cfg_all_return_isa_if", BlockId(3))
        .expect("else return block should get narrowed CFG input");
    assert_eq!(
        else_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );

    assert_eq!(
        engine.statement_type("cfg_all_return_isa_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_isa_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_isa_if", BlockId(2))
            .is_none(),
        "all-returning isa if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_nothing_edge_narrowing_5602() {
    let mut engine = InferenceEngine::new();

    let condition = Expr::BinaryOp {
        op: BinaryOp::NotEgal,
        left: Box::new(Expr::Var("val".to_string().into(), dummy_span())),
        right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_nothing_if".to_string(),
        params: vec![TypedParam {
            name: "val".to_string(),
            type_annotation: Some(JuliaType::Union(vec![
                JuliaType::Nothing,
                JuliaType::String,
            ])),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition,
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Var("val".to_string().into(), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Var("val".to_string().into(), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
            ]
            .into_iter()
            .collect()
        )
    );

    let then_input = engine
        .cfg_block_input("cfg_all_return_nothing_if", BlockId(1))
        .expect("then return block should get non-nothing CFG input");
    assert_eq!(
        then_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );
    let else_input = engine
        .cfg_block_input("cfg_all_return_nothing_if", BlockId(3))
        .expect("else return block should get nothing CFG input");
    assert_eq!(
        else_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Nothing)
        )))
    );

    assert_eq!(
        engine.statement_type("cfg_all_return_nothing_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_nothing_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Nothing)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_nothing_if", BlockId(2))
            .is_none(),
        "all-returning nothing if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_typeof_edge_narrowing_5602() {
    let mut engine = InferenceEngine::new();

    let condition = Expr::BinaryOp {
        op: BinaryOp::Egal,
        left: Box::new(Expr::Call {
            function: "typeof".to_string().into(),
            args: vec![Expr::Var("val".to_string().into(), dummy_span())],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        }),
        right: Box::new(Expr::Var("Int64".to_string().into(), dummy_span())),
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_typeof_if".to_string(),
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
            stmts: vec![Stmt::If {
                condition,
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Var("val".to_string().into(), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Var("val".to_string().into(), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let result = engine.infer_function(&func);
    assert_eq!(
        result,
        LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
            ]
            .into_iter()
            .collect()
        )
    );

    let then_input = engine
        .cfg_block_input("cfg_all_return_typeof_if", BlockId(1))
        .expect("then return block should get typeof-narrowed CFG input");
    assert_eq!(
        then_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    let else_input = engine
        .cfg_block_input("cfg_all_return_typeof_if", BlockId(3))
        .expect("else return block should get typeof-narrowed CFG input");
    assert_eq!(
        else_input.get("val"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );

    assert_eq!(
        engine.statement_type("cfg_all_return_typeof_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_typeof_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_typeof_if", BlockId(2))
            .is_none(),
        "all-returning typeof if join block should remain unreachable"
    );
}
