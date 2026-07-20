use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_cfg_authoritative_all_return_if_kwarg_call_payload_5602() {
    let callee = Function {
        name: "cfg_kwarg_return".to_string(),
        params: vec![],
        kwparams: vec![KwParam::new(
            "value".to_string(),
            Expr::Literal(Literal::Float(1.5), dummy_span()),
            None,
            dummy_span(),
        )],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("value".to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };
    let mut function_table = HashMap::new();
    function_table.insert("cfg_kwarg_return".to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let call_with_value = |value: Expr| Expr::Call {
        function: "cfg_kwarg_return".to_string().into(),
        args: vec![],
        kwargs: vec![("value".to_string().into(), value)],
        splat_mask: vec![],
        kwargs_splat_mask: vec![false],
        span: dummy_span(),
    };

    let func = Function {
        name: "cfg_all_return_kwarg_call_if".to_string(),
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
            TypedParam {
                name: "y".to_string(),
                type_annotation: Some(JuliaType::Float64),
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
                        value: Some(call_with_value(Expr::Var(
                            "x".to_string().into(),
                            dummy_span(),
                        ))),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(call_with_value(Expr::Var(
                            "y".to_string().into(),
                            dummy_span(),
                        ))),
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
        engine.statement_type("cfg_all_return_kwarg_call_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_kwarg_call_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_kwarg_call_if", BlockId(2))
            .is_none(),
        "all-returning kwarg call if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_positional_splat_call_payload_5602() {
    let callee = Function {
        name: "cfg_splat_first".to_string(),
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
                type_annotation: Some(JuliaType::Float64),
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
                value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };
    let mut function_table = HashMap::new();
    function_table.insert("cfg_splat_first".to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let direct_call = Expr::Call {
        function: "cfg_splat_first".to_string().into(),
        args: vec![
            Expr::Var("x".to_string().into(), dummy_span()),
            Expr::Var("y".to_string().into(), dummy_span()),
        ],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span: dummy_span(),
    };
    let splat_call = Expr::Call {
        function: "cfg_splat_first".to_string().into(),
        args: vec![Expr::TupleLiteral {
            elements: vec![
                Expr::Var("x".to_string().into(), dummy_span()),
                Expr::Var("y".to_string().into(), dummy_span()),
            ],
            span: dummy_span(),
        }],
        kwargs: vec![],
        splat_mask: vec![true],
        kwargs_splat_mask: vec![],
        span: dummy_span(),
    };

    let func = Function {
        name: "cfg_all_return_positional_splat_call_if".to_string(),
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
            TypedParam {
                name: "y".to_string(),
                type_annotation: Some(JuliaType::Float64),
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
                        value: Some(splat_call),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(direct_call),
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
        engine.statement_type("cfg_all_return_positional_splat_call_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_positional_splat_call_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_positional_splat_call_if", BlockId(2))
            .is_none(),
        "all-returning positional splat call if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_keyword_splat_call_payload_5602() {
    let callee = Function {
        name: "cfg_kw_splat_return".to_string(),
        params: vec![],
        kwparams: vec![KwParam::new(
            "value".to_string(),
            Expr::Literal(Literal::Float(1.5), dummy_span()),
            None,
            dummy_span(),
        )],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("value".to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };
    let mut function_table = HashMap::new();
    function_table.insert("cfg_kw_splat_return".to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let kw_splat_call = Expr::Call {
        function: "cfg_kw_splat_return".to_string().into(),
        args: vec![],
        kwargs: vec![(
            String::new().into(),
            Expr::NamedTupleLiteral {
                fields: vec![(
                    "value".to_string().into(),
                    Expr::Var("x".to_string().into(), dummy_span()),
                )],
                span: dummy_span(),
            },
        )],
        splat_mask: vec![],
        kwargs_splat_mask: vec![true],
        span: dummy_span(),
    };
    let named_kw_call = Expr::Call {
        function: "cfg_kw_splat_return".to_string().into(),
        args: vec![],
        kwargs: vec![(
            "value".to_string().into(),
            Expr::Var("y".to_string().into(), dummy_span()),
        )],
        splat_mask: vec![],
        kwargs_splat_mask: vec![false],
        span: dummy_span(),
    };

    let func = Function {
        name: "cfg_all_return_keyword_splat_call_if".to_string(),
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
            TypedParam {
                name: "y".to_string(),
                type_annotation: Some(JuliaType::Float64),
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
                        value: Some(kw_splat_call),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(named_kw_call),
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
        engine.statement_type("cfg_all_return_keyword_splat_call_if", 1),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_keyword_splat_call_if", 2),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_keyword_splat_call_if", BlockId(2))
            .is_none(),
        "all-returning keyword splat call if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_array_literal_payload_5602() {
    let mut engine = InferenceEngine::new();

    let array_of = |expr: Expr| Expr::ArrayLiteral {
        elements: vec![expr],
        shape: vec![1],
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_array_literal_if".to_string(),
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
                        value: Some(array_of(Expr::Var("x".to_string().into(), dummy_span()))),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(array_of(Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
                            right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                            span: dummy_span(),
                        })),
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
        engine.statement_type("cfg_all_return_array_literal_if", 1),
        Some(&int_array)
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_array_literal_if", 2),
        Some(&int_array)
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_array_literal_if", BlockId(2))
            .is_none(),
        "all-returning array literal if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_tuple_literal_payload_5602() {
    let mut engine = InferenceEngine::new();

    let tuple_payload = |first: Expr| Expr::TupleLiteral {
        elements: vec![first, Expr::Var("y".to_string().into(), dummy_span())],
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_tuple_literal_if".to_string(),
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
            TypedParam {
                name: "y".to_string(),
                type_annotation: Some(JuliaType::Float64),
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
                        value: Some(tuple_payload(Expr::Var(
                            "x".to_string().into(),
                            dummy_span(),
                        ))),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(tuple_payload(Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
                            right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                            span: dummy_span(),
                        })),
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

    let tuple_ty = LatticeType::Concrete(ConcreteType::Tuple {
        elements: vec![
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ],
    });
    let result = engine.infer_function(&func);
    assert_eq!(result, tuple_ty);
    assert_eq!(
        engine.statement_type("cfg_all_return_tuple_literal_if", 1),
        Some(&tuple_ty)
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_tuple_literal_if", 2),
        Some(&tuple_ty)
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_namedtuple_literal_payload_5602() {
    let mut engine = InferenceEngine::new();

    let namedtuple_payload = |value: Expr| Expr::NamedTupleLiteral {
        fields: vec![("value".to_string().into(), value)],
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_namedtuple_literal_if".to_string(),
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
                        value: Some(namedtuple_payload(Expr::Var(
                            "x".to_string().into(),
                            dummy_span(),
                        ))),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(namedtuple_payload(Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
                            right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                            span: dummy_span(),
                        })),
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

    let namedtuple_ty = LatticeType::Concrete(ConcreteType::NamedTuple {
        fields: vec![(
            "value".to_string(),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        )],
    });
    let result = engine.infer_function(&func);
    assert_eq!(result, namedtuple_ty);
    assert_eq!(
        lattice_type_to_julia(&result),
        JuliaType::Struct("@NamedTuple{value::Int64}".to_string())
    );
    assert_eq!(
        crate::runtime_types::bridge::lattice_to_julia_type(&result),
        JuliaType::Struct("@NamedTuple{value::Int64}".to_string())
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_namedtuple_literal_if", 1),
        Some(&namedtuple_ty)
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_namedtuple_literal_if", 2),
        Some(&namedtuple_ty)
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_pair_dict_payload_5602() {
    let mut engine = InferenceEngine::new();

    let pair_payload = |first: Expr| Expr::Pair {
        key: Box::new(first),
        value: Box::new(Expr::Var("y".to_string().into(), dummy_span())),
        span: dummy_span(),
    };
    let dict_payload = |first: Expr| Expr::Call {
        function: "Dict".to_string().into(),
        args: vec![pair_payload(first)],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: dummy_span(),
    };
    let increment_x = || Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
        right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_pair_dict_if".to_string(),
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
            TypedParam {
                name: "y".to_string(),
                type_annotation: Some(JuliaType::Float64),
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
                        value: Some(pair_payload(Expr::Var(
                            "x".to_string().into(),
                            dummy_span(),
                        ))),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(dict_payload(increment_x())),
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

    let pair_ty = LatticeType::Concrete(ConcreteType::Struct {
        name: "Pair{Int64,Float64}".to_string(),
        type_id: 0,
    });
    let dict_ty = LatticeType::Concrete(ConcreteType::Dict {
        key: Box::new(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        value: Box::new(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    });
    let result = engine.infer_function(&func);
    assert_eq!(result, pair_ty.join(&dict_ty));
    assert_eq!(
        crate::runtime_types::bridge::lattice_to_julia_type(&dict_ty),
        JuliaType::Struct("Dict{Int64,Float64}".to_string())
    );
    assert_eq!(
        crate::runtime_types::bridge::lattice_to_parametric_julia_type(&dict_ty),
        Some(JuliaType::Struct("Dict{Int64,Float64}".to_string()))
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_pair_dict_if", 1),
        Some(&pair_ty)
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_pair_dict_if", 2),
        Some(&dict_ty)
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_pair_dict_if", BlockId(2))
            .is_none(),
        "all-returning Pair/Dict if join block should remain unreachable"
    );
}

#[test]
fn test_cfg_authoritative_all_return_if_letblock_payload_5602() {
    let mut engine = InferenceEngine::new();

    let let_payload = |value: Expr| Expr::LetBlock {
        bindings: vec![("y".to_string().into(), value)],
        body: Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Var("y".to_string().into(), dummy_span()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        span: dummy_span(),
    };
    let increment_x = |amount: i64| Expr::BinaryOp {
        op: BinaryOp::Add,
        left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
        right: Box::new(Expr::Literal(Literal::Int(amount), dummy_span())),
        span: dummy_span(),
    };
    let func = Function {
        name: "cfg_all_return_letblock_if".to_string(),
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
                        value: Some(let_payload(increment_x(1))),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(let_payload(increment_x(2))),
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

    let int_ty = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )));
    let result = engine.infer_function(&func);
    assert_eq!(result, int_ty);
    assert_eq!(
        engine.statement_type("cfg_all_return_letblock_if", 1),
        Some(&int_ty)
    );
    assert_eq!(
        engine.statement_type("cfg_all_return_letblock_if", 2),
        Some(&int_ty)
    );
    assert!(
        engine
            .cfg_block_input("cfg_all_return_letblock_if", BlockId(2))
            .is_none(),
        "all-returning LetBlock if join block should remain unreachable"
    );
}
