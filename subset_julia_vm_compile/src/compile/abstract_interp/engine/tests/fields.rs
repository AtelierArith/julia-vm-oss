use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_getfield_preserves_union_field_type_issue_4270() {
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        LatticeType::Union(BTreeSet::from([
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ])),
    );
    let mut struct_table = HashMap::new();
    struct_table.insert(
        "Box4270".to_string(),
        StructTypeInfo::new(1, false, fields, false),
    );
    let mut engine = InferenceEngine::with_struct_table(struct_table);

    let func = Function {
        name: "field_get_4270".to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Struct("Box4270".to_string())),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "getfield".to_string().into(),
                    args: vec![
                        Expr::Var("x".to_string().into(), dummy_span()),
                        Expr::QuoteLiteral {
                            constructor: Box::new(Expr::Builtin {
                                name: crate::ir::core::BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(
                                    Literal::Str("value".to_string()),
                                    dummy_span(),
                                )],
                                span: dummy_span(),
                            }),
                            span: dummy_span(),
                        },
                    ],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![false, false],
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
    let LatticeType::Union(types) = result else {
        panic!("expected union field type");
    };
    assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64
    ))));
    assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Nothing
    ))));
}

#[test]
fn test_field_access_inference() {
    // Create a struct table with a simple Point struct
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

    let point_info = StructTypeInfo::new(1, false, fields, false);
    struct_table.insert("Point".to_string(), point_info);

    let mut engine = InferenceEngine::with_struct_table(struct_table);
    let mut env = TypeEnv::new();

    // Set p as a Point struct
    env.set(
        "p",
        LatticeType::Concrete(ConcreteType::Struct {
            name: "Point".to_string(),
            type_id: 1,
        }),
    );

    // Test field access: p.x
    let field_access = Expr::FieldAccess {
        object: Box::new(Expr::Var("p".to_string().into(), dummy_span())),
        field: "x".to_string().into(),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&field_access, &env);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );

    // Test field access: p.y
    let field_access_y = Expr::FieldAccess {
        object: Box::new(Expr::Var("p".to_string().into(), dummy_span())),
        field: "y".to_string().into(),
        span: dummy_span(),
    };

    let result_y = engine.infer_expr(&field_access_y, &env);
    assert_eq!(
        result_y,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        )))
    );
}

#[test]
fn test_immutable_constructor_preserves_argument_field_type() {
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Top);
    struct_table.insert(
        "BoxAny".to_string(),
        StructTypeInfo::new(1, false, fields, false),
    );

    let mut engine = InferenceEngine::with_struct_table(struct_table);

    let func = Function {
        name: "read_box".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Assign {
                    var: "b".to_string(),
                    value: Expr::Call {
                        function: "BoxAny".to_string().into(),
                        args: vec![Expr::Literal(Literal::Int(1), dummy_span())],
                        kwargs: vec![],
                        kwargs_splat_mask: vec![],
                        splat_mask: vec![false],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: Some(Expr::FieldAccess {
                        object: Box::new(Expr::Var("b".to_string().into(), dummy_span())),
                        field: "x".to_string().into(),
                        span: dummy_span(),
                    }),
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
}

#[test]
fn test_immutable_constructor_preserves_argument_field_type_for_getfield() {
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Top);
    struct_table.insert(
        "BoxAny".to_string(),
        StructTypeInfo::new(1, false, fields, false),
    );

    let mut engine = InferenceEngine::with_struct_table(struct_table);

    let func = Function {
        name: "read_box_getfield".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Assign {
                    var: "b".to_string(),
                    value: Expr::Call {
                        function: "BoxAny".to_string().into(),
                        args: vec![Expr::Literal(Literal::Str("s".to_string()), dummy_span())],
                        kwargs: vec![],
                        kwargs_splat_mask: vec![],
                        splat_mask: vec![false],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: Some(Expr::Call {
                        function: "getfield".to_string().into(),
                        args: vec![
                            Expr::Var("b".to_string().into(), dummy_span()),
                            Expr::Literal(Literal::Symbol("x".to_string()), dummy_span()),
                        ],
                        kwargs: vec![],
                        kwargs_splat_mask: vec![],
                        splat_mask: vec![false, false],
                        span: dummy_span(),
                    }),
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
        LatticeType::Const(ConstValue::String("s".to_string()))
    );
}

#[test]
fn test_immutable_constructor_partial_struct_integer_index_field() {
    // Issue #4269: `getfield(s, i::Int)` on a freshly constructed immutable
    // struct must resolve the field type positionally, matching upstream
    // `getfield_tfunc`'s `_getfield_fieldindex` handling of a PartialStruct.
    //
    // `StructTypeInfo::new` sorts the field names, so the declaration order is
    // `["x", "y"]`: index 1 -> `x` (Const(1)), index 2 -> `y` (Const(2.0)).
    fn read_index(index_1based: i64) -> LatticeType {
        let mut struct_table = HashMap::new();
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), LatticeType::Top);
        fields.insert("y".to_string(), LatticeType::Top);
        struct_table.insert(
            "Pair2".to_string(),
            StructTypeInfo::new(1, false, fields, false),
        );

        let mut engine = InferenceEngine::with_struct_table(struct_table);
        let func = Function {
            name: "read_index".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![
                    Stmt::Assign {
                        var: "b".to_string(),
                        value: Expr::Call {
                            function: "Pair2".to_string().into(),
                            args: vec![
                                Expr::Literal(Literal::Int(1), dummy_span()),
                                Expr::Literal(Literal::Float(2.0), dummy_span()),
                            ],
                            kwargs: vec![],
                            kwargs_splat_mask: vec![],
                            splat_mask: vec![false, false],
                            span: dummy_span(),
                        },
                        span: dummy_span(),
                    },
                    Stmt::Return {
                        value: Some(Expr::Call {
                            function: "getfield".to_string().into(),
                            args: vec![
                                Expr::Var("b".to_string().into(), dummy_span()),
                                Expr::Literal(Literal::Int(index_1based), dummy_span()),
                            ],
                            kwargs: vec![],
                            kwargs_splat_mask: vec![],
                            splat_mask: vec![false, false],
                            span: dummy_span(),
                        }),
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
        engine.infer_function(&func)
    }

    // `getfield(b, 1)` -> field `x` -> Const(1); `getfield(b, 2)` -> field `y`
    // -> Const(2.0). Each is at least as precise as the declared field type.
    assert_eq!(read_index(1), LatticeType::Const(ConstValue::Int64(1)));
    assert_eq!(read_index(2), LatticeType::Const(ConstValue::Float64(2.0)));

    // Out-of-range and zero indices must not panic or return a wrong field;
    // they fall back to the conservative declared field type (`Top`).
    assert_eq!(read_index(0), LatticeType::Top);
    assert_eq!(read_index(3), LatticeType::Top);
}

#[test]
fn test_immutable_constructor_rebind_replaces_partial_field_info() {
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Top);
    struct_table.insert(
        "BoxAny".to_string(),
        StructTypeInfo::new(1, false, fields, false),
    );

    let mut engine = InferenceEngine::with_struct_table(struct_table);

    let make_box = |value| Expr::Call {
        function: "BoxAny".to_string().into(),
        args: vec![value],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let func = Function {
        name: "read_rebound_box".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::Assign {
                    var: "b".to_string(),
                    value: make_box(Expr::Literal(Literal::Int(1), dummy_span())),
                    span: dummy_span(),
                },
                Stmt::Assign {
                    var: "b".to_string(),
                    value: make_box(Expr::Literal(Literal::Float(2.0), dummy_span())),
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: Some(Expr::FieldAccess {
                        object: Box::new(Expr::Var("b".to_string().into(), dummy_span())),
                        field: "x".to_string().into(),
                        span: dummy_span(),
                    }),
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
    assert_eq!(result, LatticeType::Const(ConstValue::Float64(2.0)));
}

#[test]
fn test_field_access_unknown_field() {
    // Create a struct table with a simple Point struct
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert(
        "x".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );

    let point_info = StructTypeInfo::new(1, false, fields, false);
    struct_table.insert("Point".to_string(), point_info);

    let mut engine = InferenceEngine::with_struct_table(struct_table);
    let mut env = TypeEnv::new();

    env.set(
        "p",
        LatticeType::Concrete(ConcreteType::Struct {
            name: "Point".to_string(),
            type_id: 1,
        }),
    );

    // Test field access to unknown field: p.z
    let field_access = Expr::FieldAccess {
        object: Box::new(Expr::Var("p".to_string().into(), dummy_span())),
        field: "z".to_string().into(),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&field_access, &env);
    assert_eq!(result, LatticeType::Top); // Unknown field falls back to Top
}

#[test]
fn test_field_access_unknown_struct() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    env.set(
        "obj",
        LatticeType::Concrete(ConcreteType::Struct {
            name: "UnknownStruct".to_string(),
            type_id: 99,
        }),
    );

    // Test field access on unknown struct: obj.field
    let field_access = Expr::FieldAccess {
        object: Box::new(Expr::Var("obj".to_string().into(), dummy_span())),
        field: "field".to_string().into(),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&field_access, &env);
    assert_eq!(result, LatticeType::Top); // Unknown struct falls back to Top
}

#[test]
fn test_getfield_call_with_struct_table() {
    // Create struct table with Point struct
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
    let mut env = TypeEnv::new();

    env.set(
        "p",
        LatticeType::Concrete(ConcreteType::Struct {
            name: "Point".to_string(),
            type_id: 1,
        }),
    );

    // Test getfield(p, :x) call
    let getfield_call = Expr::Call {
        function: "getfield".to_string().into(),
        args: vec![
            Expr::Var("p".to_string().into(), dummy_span()),
            Expr::Literal(Literal::Symbol("x".to_string()), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&getfield_call, &env);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );

    // Test getfield(p, :y) call
    let getfield_call_y = Expr::Call {
        function: "getfield".to_string().into(),
        args: vec![
            Expr::Var("p".to_string().into(), dummy_span()),
            Expr::Literal(Literal::Symbol("y".to_string()), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result_y = engine.infer_expr(&getfield_call_y, &env);
    assert_eq!(
        result_y,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        )))
    );
}

#[test]
fn test_getfield_call_unknown_field() {
    // Create struct table with Point struct
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert(
        "x".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    struct_table.insert(
        "Point".to_string(),
        StructTypeInfo::new(1, false, fields, false),
    );

    let mut engine = InferenceEngine::with_struct_table(struct_table);
    let mut env = TypeEnv::new();

    env.set(
        "p",
        LatticeType::Concrete(ConcreteType::Struct {
            name: "Point".to_string(),
            type_id: 1,
        }),
    );

    // Test getfield(p, :z) - unknown field
    let getfield_call = Expr::Call {
        function: "getfield".to_string().into(),
        args: vec![
            Expr::Var("p".to_string().into(), dummy_span()),
            Expr::Literal(Literal::Symbol("z".to_string()), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&getfield_call, &env);
    // Unknown field falls back to tfunc which returns Top
    assert_eq!(result, LatticeType::Top);
}
