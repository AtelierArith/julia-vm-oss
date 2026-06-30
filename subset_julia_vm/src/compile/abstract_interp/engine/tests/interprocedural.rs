use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

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
        is_runtime_eval: false,
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
        is_runtime_eval: false,
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
        is_runtime_eval: false,
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
        is_runtime_eval: false,
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
        is_runtime_eval: false,
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
        is_runtime_eval: false,
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

#[test]
fn test_recursive_call_returns_concrete_int64_not_top() {
    use crate::types::JuliaType;

    // function fact(n::Int64)
    //     if n <= 1
    //         return 1
    //     end
    //     return n * fact(n - 1)
    // end
    //
    // Issue #3527: the recursive edge previously poisoned inference to
    // Top. After the fix the function is inferred as an integer type.
    let fact_func = Function {
        name: "fact".to_string(),
        params: vec![TypedParam::new(
            "n".to_string(),
            Some(JuliaType::Int64),
            dummy_span(),
        )],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::If {
                    condition: Expr::BinaryOp {
                        op: BinaryOp::Le,
                        left: Box::new(Expr::Var("n".to_string(), dummy_span())),
                        right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::Literal(Literal::Int(1), dummy_span())),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(Expr::Var("n".to_string(), dummy_span())),
                        right: Box::new(Expr::Call {
                            function: "fact".to_string(),
                            args: vec![Expr::BinaryOp {
                                op: BinaryOp::Sub,
                                left: Box::new(Expr::Var("n".to_string(), dummy_span())),
                                right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                                span: dummy_span(),
                            }],
                            kwargs: vec![],
                            kwargs_splat_mask: vec![],
                            splat_mask: vec![false],
                            span: dummy_span(),
                        }),
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
    };

    let mut function_table = HashMap::new();
    function_table.insert("fact".to_string(), fact_func.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let result = engine.infer_function(&fact_func);
    assert_ne!(
        result,
        LatticeType::Top,
        "Recursive function must not poison inference to Top (Issue #3527)"
    );
    let is_integer = match &result {
        LatticeType::Concrete(ct) => ct.is_integer(),
        LatticeType::Const(ConstValue::Int64(_)) => true,
        LatticeType::Union(types) => types.iter().all(|t| t.is_integer()),
        _ => false,
    };
    assert!(
        is_integer,
        "Recursive Int64 factorial should infer to an integer type, got {:?}",
        result
    );
}

#[test]
fn test_varargs_call_packs_remaining_args_into_tuple() {
    // sum_varargs(xs...) returns xs[1].
    // The body returns the first vararg via xs[1], so we can verify the
    // parameter was bound to a Tuple containing all args (Issue #3526).
    // Returning xs[1] from `Tuple{Int64, Int64, Int64}` should infer Int64.
    let sum_func = Function {
        name: "sum_varargs".to_string(),
        params: vec![TypedParam::varargs("xs".to_string(), None, dummy_span())],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Index {
                    array: Box::new(Expr::Var("xs".to_string(), dummy_span())),
                    indices: vec![Expr::Literal(Literal::Int(1), dummy_span())],
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

    let mut function_table = HashMap::new();
    function_table.insert("sum_varargs".to_string(), sum_func);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    let env = TypeEnv::new();

    // sum_varargs(1, 2, 3) — the call site path packs remaining args into a Tuple.
    let call = Expr::Call {
        function: "sum_varargs".to_string(),
        args: vec![
            Expr::Literal(Literal::Int(1), dummy_span()),
            Expr::Literal(Literal::Int(2), dummy_span()),
            Expr::Literal(Literal::Int(3), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false, false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call, &env);
    let ok = matches!(
        &result,
        LatticeType::Const(ConstValue::Int64(_))
            | LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
    );
    assert!(
        ok,
        "Expected Int64 for xs[1] when xs is bound to Tuple{{Int64, Int64, Int64}}, got {:?}",
        result
    );
}

#[test]
fn test_call_inference_union_splits_method_matches() {
    let mut table = MethodTable::new("classify".to_string());
    table.add_method(MethodSig::for_tests(
        0,
        0,
        vec![("x".to_string(), JuliaType::Int64)],
        ValueType::I64,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    ));
    table.add_method(MethodSig::for_tests(
        1,
        1,
        vec![("x".to_string(), JuliaType::String)],
        ValueType::Str,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    ));

    let mut method_tables = HashMap::new();
    method_tables.insert("classify".to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    let mut union_types = BTreeSet::new();
    union_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )));
    union_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::String,
    )));

    let mut env = TypeEnv::new();
    env.set("x", LatticeType::Union(union_types.clone()));

    let call = Expr::Call {
        function: "classify".to_string(),
        args: vec![Expr::Var("x".to_string(), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call, &env);
    assert_eq!(result, LatticeType::Union(union_types));
}

#[test]
fn test_ternary_inference_joins_branch_types_4287() {
    let mut engine = InferenceEngine::new();
    let mut env = TypeEnv::new();
    env.set(
        "b",
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
    );

    let expr = Expr::Ternary {
        condition: Box::new(Expr::Var("b".to_string(), dummy_span())),
        then_expr: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
        else_expr: Box::new(Expr::Literal(Literal::Str("x".to_string()), dummy_span())),
        span: dummy_span(),
    };

    let mut expected = BTreeSet::new();
    expected.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )));
    expected.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::String,
    )));
    assert_eq!(engine.infer_expr(&expr, &env), LatticeType::Union(expected));
}

#[test]
fn test_call_inference_union_split_budget_matches_julia_4287() {
    let mut table = MethodTable::new("pair_classify".to_string());
    for (idx, (lhs, rhs, ret)) in [
        (JuliaType::Int8, JuliaType::Bool, ValueType::I8),
        (JuliaType::Int8, JuliaType::String, ValueType::I16),
        (JuliaType::Int16, JuliaType::Bool, ValueType::Bool),
        (JuliaType::Int16, JuliaType::String, ValueType::Str),
    ]
    .into_iter()
    .enumerate()
    {
        table.add_method(MethodSig::for_tests(
            idx,
            idx,
            vec![("x".to_string(), lhs), ("y".to_string(), rhs)],
            ret,
            None,
            false,
            vec![],
            crate::inference_core::CoreType::Bottom,
            None,
            None,
        ));
    }

    let mut method_tables = HashMap::new();
    method_tables.insert("pair_classify".to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    let mut lhs_types = BTreeSet::new();
    lhs_types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)));
    lhs_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int16,
    )));
    let mut rhs_types = BTreeSet::new();
    rhs_types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
    rhs_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::String,
    )));

    let mut env = TypeEnv::new();
    env.set("x", LatticeType::Union(lhs_types));
    env.set("y", LatticeType::Union(rhs_types));

    let call = Expr::Call {
        function: "pair_classify".to_string(),
        args: vec![
            Expr::Var("x".to_string(), dummy_span()),
            Expr::Var("y".to_string(), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call, &env);
    let LatticeType::Union(types) = result else {
        panic!("expected four-way union split result, got {result:?}");
    };
    assert_eq!(types.len(), 4);
    assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int8
    ))));
    assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int16
    ))));
    assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Bool
    ))));
    assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::String
    ))));
}

#[test]
fn test_call_inference_large_union_split_bails_with_diagnostic_4287() {
    DiagnosticsCollector::enable();
    DiagnosticsCollector::clear();

    let mut table = MethodTable::new("wide_classify".to_string());
    for (idx, (ty, ret)) in [
        (JuliaType::Int8, ValueType::I8),
        (JuliaType::Int16, ValueType::I16),
        (JuliaType::Int32, ValueType::I32),
        (JuliaType::Int64, ValueType::I64),
        (JuliaType::Float64, ValueType::F64),
    ]
    .into_iter()
    .enumerate()
    {
        table.add_method(MethodSig::for_tests(
            idx,
            idx,
            vec![("x".to_string(), ty)],
            ret,
            None,
            false,
            vec![],
            crate::inference_core::CoreType::Bottom,
            None,
            None,
        ));
    }

    let mut method_tables = HashMap::new();
    method_tables.insert("wide_classify".to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    let mut union_types = BTreeSet::new();
    union_types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)));
    union_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int16,
    )));
    union_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int32,
    )));
    union_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )));
    union_types.insert(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Float64,
    )));

    let mut env = TypeEnv::new();
    env.set("x", LatticeType::Union(union_types));

    let call = Expr::Call {
        function: "wide_classify".to_string(),
        args: vec![Expr::Var("x".to_string(), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call, &env);
    assert_eq!(result, LatticeType::Top);

    let diags = DiagnosticsCollector::take();
    assert!(diags.iter().any(|diag| {
        matches!(
            &diag.reason,
            DiagnosticReason::UnionSplitTooLarge {
                variants: 5,
                max: 4
            }
        )
    }));
    DiagnosticsCollector::disable();
}

#[test]
fn test_call_inference_skips_method_table_when_arg_is_top() {
    let mut table = MethodTable::new("maybe_struct".to_string());
    table.add_method(MethodSig::for_tests(
        0,
        0,
        vec![("x".to_string(), JuliaType::Any)],
        ValueType::Struct(42),
        Some(JuliaType::Struct("SomeStruct".to_string())),
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    ));

    let mut method_tables = HashMap::new();
    method_tables.insert("maybe_struct".to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    let mut env = TypeEnv::new();
    env.set("x", LatticeType::Top);

    let call = Expr::Call {
        function: "maybe_struct".to_string(),
        args: vec![Expr::Var("x".to_string(), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call, &env);
    assert_eq!(result, LatticeType::Top);
}

#[test]
fn test_issue_5603_ambiguous_method_table_call_infers_bottom() {
    let mut table = MethodTable::new("ambiguous_5603".to_string());
    table.add_method(MethodSig::for_tests(
        0,
        0,
        vec![
            ("x".to_string(), JuliaType::Integer),
            ("y".to_string(), JuliaType::Real),
        ],
        ValueType::I64,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    ));
    table.add_method(MethodSig::for_tests(
        1,
        1,
        vec![
            ("x".to_string(), JuliaType::Real),
            ("y".to_string(), JuliaType::Integer),
        ],
        ValueType::Str,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    ));

    let mut method_tables = HashMap::new();
    method_tables.insert("ambiguous_5603".to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    let mut env = TypeEnv::new();
    env.set(
        "x",
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    env.set(
        "y",
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );

    let call = Expr::Call {
        function: "ambiguous_5603".to_string(),
        args: vec![
            Expr::Var("x".to_string(), dummy_span()),
            Expr::Var("y".to_string(), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    assert_eq!(engine.infer_expr(&call, &env), LatticeType::Bottom);
}

/// Issue #7215: a call to a user function with a declared return type
/// (`f(...)::T`) must infer that declared type at the call site instead of
/// re-inferring the callee's body. Julia guarantees the result is
/// `convert(T, …)::T`, so `T` is the call-site return type — and
/// short-circuiting to it prevents the combinatorial body re-expansion that
/// made `using Symbolics` / `Differential(x)(cos(x))` take ~7–17 s to compile
/// (the mutually recursive `_deriv ⇄ _deriv_*` family was re-inferred at every
/// call site). Here the body returns its `Int64` argument, so without the
/// short-circuit the call would infer `Int64`; the declared `::Float64` must
/// win instead.
#[test]
fn test_issue_7215_declared_return_type_short_circuits_call_site() {
    use crate::ir::core::TypedParam;
    use crate::types::JuliaType;

    let declared = Function {
        name: "declared_ret".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, dummy_span())],
        kwparams: vec![],
        type_params: vec![],
        return_type: Some(JuliaType::Float64),
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };

    let mut function_table = HashMap::new();
    function_table.insert("declared_ret".to_string(), declared);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    let env = TypeEnv::new();

    // declared_ret(7): the body returns its `Int64` argument, but the declared
    // `::Float64` return annotation must short-circuit the call site.
    let call = Expr::Call {
        function: "declared_ret".to_string(),
        args: vec![Expr::Literal(Literal::Int(7), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&call, &env);
    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        ))),
        "declared `::Float64` return type must win at the call site, not the \
         body-inferred Int64 (Issue #7215)"
    );

    // The short-circuited result is committed to the long-lived cache so a
    // repeated identical call hits immediately instead of re-converting.
    assert_eq!(
        engine.get_cached_return_type("declared_ret", &[LatticeType::Const(ConstValue::Int64(7))]),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );
}
