use super::*;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, TypedParam};
use crate::span::Span;

fn dummy_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

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
        span: dummy_span(),
    };

    let result = engine.infer_function(&func);
    assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
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
        assert_eq!(ty, LatticeType::Concrete(ConcreteType::Int64));
    }
}

#[test]
fn test_cache_function_return_type() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "cached_fn".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Float(1.5), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        span: dummy_span(),
    };

    // First inference
    let result1 = engine.infer_function(&func);
    assert_eq!(result1, LatticeType::Const(ConstValue::Float64(1.5)));

    // Check cache (using empty arg types since function has no params)
    let cached = engine.get_cached_return_type("cached_fn", &[]);
    assert_eq!(cached, Some(&LatticeType::Const(ConstValue::Float64(1.5))));

    // Second inference should use cache
    let result2 = engine.infer_function(&func);
    assert_eq!(result2, result1);
}

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
        Some(&LatticeType::Concrete(ConcreteType::Int64))
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
        assert!(types.contains(&ConcreteType::Int64));
        assert!(types.contains(&ConcreteType::Float64));
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
        Some(&LatticeType::Concrete(ConcreteType::Int64))
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
        Some(&LatticeType::Concrete(ConcreteType::Char))
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
        Some(&LatticeType::Concrete(ConcreteType::Float64))
    );
}

#[test]
fn test_foreach_updates_accumulator_type() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();

    // sum starts as Int64
    env.set("sum", LatticeType::Concrete(ConcreteType::Int64));

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
            Some(LatticeType::Concrete(ConcreteType::Float64)) | Some(LatticeType::Union(_))
        ),
        "Expected sum to include Float64, got {:?}",
        env.get("sum")
    );
    if let Some(LatticeType::Union(types)) = env.get("sum") {
        assert!(types.contains(&ConcreteType::Float64));
    }
}

#[test]
fn test_field_access_inference() {
    // Create a struct table with a simple Point struct
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Concrete(ConcreteType::Int64));
    fields.insert(
        "y".to_string(),
        LatticeType::Concrete(ConcreteType::Float64),
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
        object: Box::new(Expr::Var("p".to_string(), dummy_span())),
        field: "x".to_string(),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&field_access, &env);
    assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));

    // Test field access: p.y
    let field_access_y = Expr::FieldAccess {
        object: Box::new(Expr::Var("p".to_string(), dummy_span())),
        field: "y".to_string(),
        span: dummy_span(),
    };

    let result_y = engine.infer_expr(&field_access_y, &env);
    assert_eq!(result_y, LatticeType::Concrete(ConcreteType::Float64));
}

#[test]
fn test_field_access_unknown_field() {
    // Create a struct table with a simple Point struct
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Concrete(ConcreteType::Int64));

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
        object: Box::new(Expr::Var("p".to_string(), dummy_span())),
        field: "z".to_string(),
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
        object: Box::new(Expr::Var("obj".to_string(), dummy_span())),
        field: "field".to_string(),
        span: dummy_span(),
    };

    let result = engine.infer_expr(&field_access, &env);
    assert_eq!(result, LatticeType::Top); // Unknown struct falls back to Top
}

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
            element: Box::new(ConcreteType::Int64),
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
            element: Box::new(ConcreteType::Int64),
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
            element: Box::new(ConcreteType::Float64),
        })
    );
}

#[test]
fn test_getfield_call_with_struct_table() {
    // Create struct table with Point struct
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Concrete(ConcreteType::Int64));
    fields.insert(
        "y".to_string(),
        LatticeType::Concrete(ConcreteType::Float64),
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
        function: "getfield".to_string(),
        args: vec![
            Expr::Var("p".to_string(), dummy_span()),
            Expr::Literal(Literal::Symbol("x".to_string()), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&getfield_call, &env);
    assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));

    // Test getfield(p, :y) call
    let getfield_call_y = Expr::Call {
        function: "getfield".to_string(),
        args: vec![
            Expr::Var("p".to_string(), dummy_span()),
            Expr::Literal(Literal::Symbol("y".to_string()), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result_y = engine.infer_expr(&getfield_call_y, &env);
    assert_eq!(result_y, LatticeType::Concrete(ConcreteType::Float64));
}

#[test]
fn test_getfield_call_unknown_field() {
    // Create struct table with Point struct
    let mut struct_table = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), LatticeType::Concrete(ConcreteType::Int64));
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
        function: "getfield".to_string(),
        args: vec![
            Expr::Var("p".to_string(), dummy_span()),
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

#[test]
fn test_interprocedural_analysis() {
    use crate::ir::core::TypedParam;

    // Create a helper function: add(x, y) = x + y
    let add_func = Function {
        name: "add".to_string(),
        params: vec![
            TypedParam::new("x".to_string(), None, dummy_span()),
            TypedParam::new("y".to_string(), None, dummy_span()),
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
        span: dummy_span(),
    };

    // Create function table with add function
    let mut function_table = HashMap::new();
    function_table.insert("add".to_string(), add_func);

    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    let env = TypeEnv::new();

    // Test: add(1, 2) should infer return type as Int64
    let call_expr = Expr::Call {
        function: "add".to_string(),
        args: vec![
            Expr::Literal(Literal::Int(1), dummy_span()),
            Expr::Literal(Literal::Int(2), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call_expr, &env);
    assert_eq!(result, LatticeType::Const(ConstValue::Int64(3)));
}

#[test]
fn test_interprocedural_analysis_float() {
    use crate::ir::core::TypedParam;

    // Create: double(x) = x * 2.0
    let double_func = Function {
        name: "double".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, dummy_span())],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::Var("x".to_string(), dummy_span())),
                    right: Box::new(Expr::Literal(Literal::Float(2.0), dummy_span())),
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        span: dummy_span(),
    };

    let mut function_table = HashMap::new();
    function_table.insert("double".to_string(), double_func);

    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    let env = TypeEnv::new();

    // Test: double(5) should infer return type as Float64 (due to * 2.0)
    let call_expr = Expr::Call {
        function: "double".to_string(),
        args: vec![Expr::Literal(Literal::Int(5), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call_expr, &env);
    assert_eq!(result, LatticeType::Const(ConstValue::Float64(10.0)));
}

#[test]
fn test_interprocedural_caches_result() {
    use crate::ir::core::TypedParam;

    // Create: identity(x) = x
    let identity_func = Function {
        name: "identity".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, dummy_span())],
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
        span: dummy_span(),
    };

    let mut function_table = HashMap::new();
    function_table.insert("identity".to_string(), identity_func);

    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    let env = TypeEnv::new();

    // First call
    let call_expr1 = Expr::Call {
        function: "identity".to_string(),
        args: vec![Expr::Literal(Literal::Int(42), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result1 = engine.infer_expr(&call_expr1, &env);
    assert_eq!(result1, LatticeType::Const(ConstValue::Int64(42)));

    // Result should be cached (using get_cached_return_type_by_name for simplicity)
    assert!(engine.get_cached_return_type_by_name("identity").is_some());
}

#[test]
fn test_interprocedural_polymorphic_function() {
    use crate::ir::core::TypedParam;

    // Create: identity(x) = x
    // This function should return the same type as its argument
    let identity_func = Function {
        name: "identity".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, dummy_span())],
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
        span: dummy_span(),
    };

    let mut function_table = HashMap::new();
    function_table.insert("identity".to_string(), identity_func);

    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    let env = TypeEnv::new();

    // Call with Int64 argument
    let call_int = Expr::Call {
        function: "identity".to_string(),
        args: vec![Expr::Literal(Literal::Int(42), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result_int = engine.infer_expr(&call_int, &env);
    assert_eq!(result_int, LatticeType::Const(ConstValue::Int64(42)));

    // Call with Float64 argument - should cache separately
    let call_float = Expr::Call {
        function: "identity".to_string(),
        args: vec![Expr::Literal(
            Literal::Float(std::f64::consts::PI),
            dummy_span(),
        )],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result_float = engine.infer_expr(&call_float, &env);
    assert_eq!(
        result_float,
        LatticeType::Const(ConstValue::Float64(std::f64::consts::PI))
    );

    // Verify both are cached separately
    assert_eq!(
        engine.get_cached_return_type("identity", &[LatticeType::Const(ConstValue::Int64(42))]),
        Some(&LatticeType::Const(ConstValue::Int64(42)))
    );
    assert_eq!(
        engine.get_cached_return_type(
            "identity",
            &[LatticeType::Const(ConstValue::Float64(
                std::f64::consts::PI
            ))]
        ),
        Some(&LatticeType::Const(ConstValue::Float64(
            std::f64::consts::PI
        )))
    );
}

#[test]
fn test_interprocedural_function_chain() {
    use crate::ir::core::TypedParam;

    // Create: helper(x, y) = x + y
    let helper_func = Function {
        name: "helper".to_string(),
        params: vec![
            TypedParam::new("x".to_string(), None, dummy_span()),
            TypedParam::new("y".to_string(), None, dummy_span()),
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
        span: dummy_span(),
    };

    // Create: caller() = helper(1, 2)
    let caller_func = Function {
        name: "caller".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "helper".to_string(),
                    args: vec![
                        Expr::Literal(Literal::Int(1), dummy_span()),
                        Expr::Literal(Literal::Int(2), dummy_span()),
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
        span: dummy_span(),
    };

    let mut function_table = HashMap::new();
    function_table.insert("helper".to_string(), helper_func);
    function_table.insert("caller".to_string(), caller_func.clone());

    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    // Infer caller() - should propagate through to helper(1, 2) and return Int64
    let result = engine.infer_function(&caller_func);
    assert_eq!(result, LatticeType::Const(ConstValue::Int64(3)));
}
