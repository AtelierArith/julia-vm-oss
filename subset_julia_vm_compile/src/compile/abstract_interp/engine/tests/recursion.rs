use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_self_recursion_with_refined_arg_key_reuses_active_estimate_7357() {
    // `recurse_poly` is entered with an untyped parameter (`Top`) but its body
    // recursively calls itself with a literal Float64. That produces a different
    // inference cache key for the same method identity. Issue #7357: the exact
    // key cycle guard missed that case, recursively analyzed the same function
    // under each refined call-site key, and made WASM compile time explode for
    // the Apollonian Gasket `recurse!` sample.
    let recurse_poly = Function {
        name: "recurse_poly_7357".to_string(),
        params: vec![TypedParam::new("x".to_string(), None, dummy_span())],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![
                Stmt::If {
                    condition: Expr::BinaryOp {
                        op: BinaryOp::Eq,
                        left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
                        right: Box::new(Expr::Literal(Literal::Int(0), dummy_span())),
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![Stmt::Return {
                            value: None,
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                },
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "recurse_poly_7357".to_string().into(),
                        args: vec![Expr::Literal(Literal::Float(1.0), dummy_span())],
                        kwargs: vec![],
                        kwargs_splat_mask: vec![],
                        splat_mask: vec![false],
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: None,
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

    let mut function_table = HashMap::new();
    function_table.insert("recurse_poly_7357".to_string(), recurse_poly.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let result = engine.infer_function(&recurse_poly);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing
        )))
    );
    assert!(
        engine
            .get_cached_return_type(
                "recurse_poly_7357",
                &[LatticeType::Const(ConstValue::Float64(1.0))]
            )
            .is_none(),
        "recursive call with a refined arg key should reuse the active method estimate, not start a separate analysis"
    );
}

#[test]
fn test_mutual_recursion_caches_both_sides_with_full_type_3505() {
    // function f_mix(n::Int64)
    //     if n == 0; return 1.0; end       # Float64 base case
    //     return g_mix(n - 1)               # delegates to g
    // end
    //
    // function g_mix(n::Int64)
    //     if n == 0; return 2; end          # Int64 base case
    //     return f_mix(n - 1)               # delegates back to f
    // end
    //
    // function caller_mix(n::Int64)
    //     return f_mix(n)                   # outer driver of the cycle
    // end
    //
    // Both functions return values from the *other* base case via the
    // recursive edge, so the precise inferred return for either function is
    // `Union{Int64, Float64}`.
    //
    // Issue #3505: when the cycle is entered via `caller_mix` (so the
    // top-level entry point does NOT itself participate in the cycle), the
    // previous implementation cached `g_mix`'s result as `Int64` only. That
    // happened because `g_mix`'s analysis ran inside `f_mix`'s outer
    // fixpoint while `f_mix`'s in-progress estimate was still `Bottom`, the
    // result was committed to the long-lived cache, and `f_mix`'s
    // subsequent outer iterations short-circuited on the (poisoned) cache
    // hit instead of re-evaluating `g_mix` against the refined estimate.
    // The fix defers the commit to the cache until the outermost cycle
    // frame unwinds, then promotes the tentative entries together.
    //
    // (Inferring `f_mix` directly at the top level happens to mask the bug
    // because the top-level cache write overwrites the poisoned interior
    // entry — this is why we drive the cycle through `caller_mix`.)
    let f_mix = cycle_branch_function("f_mix", "g_mix", Literal::Float(1.0));
    let g_mix = cycle_branch_function("g_mix", "f_mix", Literal::Int(2));

    // caller_mix(n) = f_mix(n)
    let caller_mix = Function {
        name: "caller_mix".to_string(),
        params: vec![TypedParam::new(
            "n".to_string(),
            Some(JuliaType::Int64),
            dummy_span(),
        )],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "f_mix".to_string().into(),
                    args: vec![Expr::Var("n".to_string().into(), dummy_span())],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![false],
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

    let mut function_table = HashMap::new();
    function_table.insert("f_mix".to_string(), f_mix.clone());
    function_table.insert("g_mix".to_string(), g_mix.clone());
    function_table.insert("caller_mix".to_string(), caller_mix.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    // Drive the cycle from outside both participants.
    let caller_result = engine.infer_function(&caller_mix);

    let f_cached = engine
        .get_cached_return_type(
            "f_mix",
            &[LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))],
        )
        .cloned()
        .expect("f_mix should be cached after caller_mix's analysis");
    let g_cached = engine
        .get_cached_return_type(
            "g_mix",
            &[LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))],
        )
        .cloned()
        .expect("g_mix should be cached after caller_mix's analysis");

    // Sanity: nothing should poison to Top.
    assert_ne!(
        caller_result,
        LatticeType::Top,
        "caller_mix must not poison to Top (Issue #3505)"
    );

    // Both cached sides must contain BOTH base-case types — the previous
    // behaviour cached the inner side (g_mix) as a single branch only.
    let contains_int_and_float = |ty: &LatticeType| -> bool {
        match ty {
            LatticeType::Union(types) => {
                types.iter().any(|t| {
                    matches!(
                        t,
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
                    )
                }) && types.iter().any(|t| {
                    matches!(
                        t,
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))
                    )
                })
            }
            _ => false,
        }
    };
    assert!(
        contains_int_and_float(&f_cached),
        "f_mix should be cached as Union{{Int64, Float64}} after the cycle converges, got {:?}",
        f_cached
    );
    assert!(
        contains_int_and_float(&g_cached),
        "g_mix's cache must NOT be poisoned to a single branch (Issue #3505) — got {:?}",
        g_cached
    );
}

#[test]
fn test_depth_limit_records_limited_accuracy_3505() {
    DiagnosticsCollector::enable();
    DiagnosticsCollector::clear();

    let deep = Function {
        name: "deep".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Int(1), dummy_span())),
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
    function_table.insert("deep".to_string(), deep);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    engine.analysis_depth = MAX_INTERPROCEDURAL_ANALYSIS_DEPTH;

    let result = engine.infer_expr(
        &Expr::Call {
            function: "deep".to_string().into(),
            args: vec![],
            kwargs: vec![],
            kwargs_splat_mask: vec![],
            splat_mask: vec![],
            span: dummy_span(),
        },
        &TypeEnv::new(),
    );

    assert_eq!(result, LatticeType::Top);
    assert!(engine.is_limited_return_type("deep", &[]));

    engine.add_method(
        "unrelated_limited_5603".to_string(),
        int_identity_method_sig(),
    );
    assert!(
        engine.is_limited_return_type("deep", &[]),
        "unrelated method mutation should not clear limited-accuracy markers"
    );

    engine.add_method("deep".to_string(), zero_arg_i64_method_sig());
    assert!(
        !engine.is_limited_return_type("deep", &[]),
        "matching method mutation should retire the limited-accuracy marker"
    );

    let diags = DiagnosticsCollector::take();
    assert!(diags.iter().any(|diag| {
        matches!(
            &diag.reason,
            DiagnosticReason::LimitedAccuracy { function, .. } if function == "deep"
        )
    }));

    DiagnosticsCollector::disable();
}

#[test]
fn test_recursive_constructor_walker_partial_struct_terminates_7186() {
    // A self-recursive walker whose recursive branch builds a struct from its
    // own recursive result:
    //
    //   function walk(n::Int64)
    //       if n == 0; return PartialBox5603(0); end   # base: a clean partial
    //       return PartialBox5603(walk(n - 1))         # recurse INTO a ctor arg
    //   end
    //
    // The returned constructor argument is itself a `walk(...)` call, so
    // inferring `walk` re-enters `(walk, [Int64])`. Before the Issue #7186
    // fix the dedicated PartialStruct-return side walk had no in-flight guard
    // and no negative caching, so the re-analysis fanned out without bound
    // and `using`-time inference of a recursive constructor walker (the
    // Symbolics `_deriv`/`_mk*` family) hung. Since Issue #8739 constructor
    // facts ride the REGULAR return path, whose `analyzing_functions` cycle
    // guard + tentative-estimate fixpoint bound the recursion structurally;
    // this test reaching its assertions at all remains the regression guard
    // against the hang.
    let walk = Function {
        name: "walk_7186".to_string(),
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
                        op: BinaryOp::Eq,
                        left: Box::new(Expr::Var("n".to_string().into(), dummy_span())),
                        right: Box::new(Expr::Literal(Literal::Int(0), dummy_span())),
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![Stmt::Return {
                            value: Some(Expr::Call {
                                function: "PartialBox5603".to_string().into(),
                                args: vec![Expr::Literal(Literal::Int(0), dummy_span())],
                                kwargs: vec![],
                                kwargs_splat_mask: vec![],
                                splat_mask: vec![false],
                                span: dummy_span(),
                            }),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                },
                Stmt::Return {
                    value: Some(Expr::Call {
                        function: "PartialBox5603".to_string().into(),
                        args: vec![Expr::Call {
                            function: "walk_7186".to_string().into(),
                            args: vec![Expr::BinaryOp {
                                op: BinaryOp::Sub,
                                left: Box::new(Expr::Var("n".to_string().into(), dummy_span())),
                                right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                                span: dummy_span(),
                            }],
                            kwargs: vec![],
                            kwargs_splat_mask: vec![],
                            splat_mask: vec![false],
                            span: dummy_span(),
                        }],
                        kwargs: vec![],
                        kwargs_splat_mask: vec![],
                        splat_mask: vec![false],
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

    let mut function_table = HashMap::new();
    function_table.insert("walk_7186".to_string(), walk.clone());
    let mut engine = InferenceEngine::with_tables(partial_box_struct_table_5603(), function_table);

    // Must terminate (no unbounded re-analysis). The walker returns the struct
    // on every path, so the result is the concrete struct type, not `Top`.
    let result = engine.infer_function(&walk);
    assert_ne!(
        result,
        LatticeType::Top,
        "recursive constructor walker must not poison to Top (Issue #7186)"
    );
    // Issue #8544: the constructor site now surfaces a first-class
    // `PartialStruct` fact whose widened type is the struct — accept either
    // the widened `Concrete(Struct)` or the refined `PartialStruct` shape.
    assert!(
        matches!(
            result.widen_partial_struct(),
            LatticeType::Concrete(ConcreteType::Struct { .. })
        ),
        "walk_7186 returns PartialBox5603 on every path, got {:?}",
        result
    );
}

#[test]
fn test_statement_type_table_records_top_level_statements_3506() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "typed_stmts".to_string(),
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
                Stmt::Expr {
                    expr: Expr::Literal(Literal::Float(1.5), dummy_span()),
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

    let _ = engine.infer_function(&func);

    assert_eq!(
        engine.statement_type("typed_stmts", 0),
        Some(&LatticeType::Const(ConstValue::Int64(1)))
    );
    assert_eq!(
        engine.statement_type("typed_stmts", 1),
        Some(&LatticeType::Const(ConstValue::Float64(1.5)))
    );
}

#[test]
fn test_mutual_recursion_is_even_is_odd_terminates_3505() {
    // is_even / is_odd: classic mutual recursion that should terminate
    // without poisoning to Top, even though both branches return `Bool`.
    // This guards against regressions in the bounded-fixpoint loop where
    // tentative-result clearing causes infinite re-analysis.
    let is_even = cycle_branch_function("is_even", "is_odd", Literal::Bool(true));
    let is_odd = cycle_branch_function("is_odd", "is_even", Literal::Bool(false));

    let mut function_table = HashMap::new();
    function_table.insert("is_even".to_string(), is_even.clone());
    function_table.insert("is_odd".to_string(), is_odd.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let result = engine.infer_function(&is_even);
    assert_ne!(
        result,
        LatticeType::Top,
        "Mutual recursion is_even/is_odd must not poison to Top (Issue #3505)"
    );

    // Bool may be returned either as `Const(Bool(true))`, `Concrete(Bool)`,
    // or as a `Union{Bool}`; accept any of these shapes — the contract is
    // that the result is meaningful (not Top).
    let is_bool_like = match &result {
        LatticeType::Const(ConstValue::Bool(_)) => true,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))) => true,
        LatticeType::Union(types) => types.iter().all(|t| {
            matches!(
                t,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))
            )
        }),
        _ => false,
    };
    assert!(
        is_bool_like,
        "is_even must infer to Bool (or a Bool-valued union), got {:?}",
        result
    );
}
