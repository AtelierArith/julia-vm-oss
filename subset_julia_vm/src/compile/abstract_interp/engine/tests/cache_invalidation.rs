use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn test_issue_4271_method_replacement_invalidates_return_cache() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "world_cached".to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Int64),
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
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

    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let inferred = engine.infer_function_with_arg_types(&func, &arg_types);
    assert_eq!(
        inferred,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.get_cached_return_type("world_cached", &arg_types),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );

    let world_before = engine.method_world_for_tests();
    engine.add_method(
        "world_cached".to_string(),
        MethodSig::for_tests(
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
        ),
    );

    assert!(
        engine.method_world_for_tests() > world_before,
        "method-table mutation should advance the conservative inference world"
    );
    assert_eq!(
        engine.get_cached_return_type("world_cached", &arg_types),
        None,
        "method-table mutation must not leave stale return inference cached"
    );
}

/// Issue #4271: a method mutation to function `f` must NOT invalidate an
/// unrelated function `g`'s cached return result. This is the precision win
/// over the previous table-wide `.clear()`: only entries that are (or depend
/// on) the mutated function are retired, mirroring upstream targeted backedge
/// invalidation in `julia/src/gf.c`.
#[test]
fn test_issue_4271_unrelated_method_mutation_preserves_other_cache() {
    let mut engine = InferenceEngine::new();
    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let f = int_identity_function("f_target");
    let g = int_identity_function("g_unrelated");

    // Prime both caches.
    assert_eq!(
        engine.infer_function_with_arg_types(&f, &arg_types),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.infer_function_with_arg_types(&g, &arg_types),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert!(engine
        .get_cached_return_type("f_target", &arg_types)
        .is_some());
    assert!(engine
        .get_cached_return_type("g_unrelated", &arg_types)
        .is_some());

    // Redefine only `f_target`.
    engine.add_method("f_target".to_string(), int_identity_method_sig());

    // `f_target`'s own entry is retired (world-gated miss)...
    assert_eq!(
        engine.get_cached_return_type("f_target", &arg_types),
        None,
        "the mutated function's own cached result must be invalidated"
    );
    // ...but `g_unrelated`, which has no dependency on `f_target`, survives.
    assert_eq!(
        engine.get_cached_return_type("g_unrelated", &arg_types),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "an unrelated function's cached result must NOT be invalidated by a \
         mutation to a different method (targeted invalidation, Issue #4271)"
    );
}

/// Issue #4271: a method mutation to a callee `f` must invalidate any caller
/// `h` whose cached inference depended on `f` (conservative backedge
/// approximation). This is the soundness counterpart to targeted invalidation:
/// we never keep a stale caller result when one of its callees changes.
#[test]
fn test_issue_4271_caller_invalidated_when_callee_method_mutates() {
    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    // callee(x::Int64) = x
    let callee = int_identity_function("callee_fn");
    // caller(x::Int64) = callee_fn(x)
    let caller = Function {
        name: "caller_fn".to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: Some(JuliaType::Int64),
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
                    function: "callee_fn".to_string(),
                    args: vec![Expr::Var("x".to_string(), dummy_span())],
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
    };

    let mut function_table = HashMap::new();
    function_table.insert("callee_fn".to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    // Infer the caller; this records caller_fn -> {callee_fn} as a dependency
    // edge and caches caller_fn's result.
    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &arg_types),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert!(
        engine
            .get_cached_return_type("caller_fn", &arg_types)
            .is_some(),
        "caller result should be cached after inference"
    );

    // Redefine the callee. The caller depends on it via a backedge, so the
    // caller's cached result must be invalidated even though `caller_fn`
    // itself was not the mutated method.
    engine.add_method("callee_fn".to_string(), int_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type("caller_fn", &arg_types),
        None,
        "a caller that depends on a mutated callee must be invalidated \
         (backedge approximation, Issue #4271)"
    );
}

/// Issue #4271: the world-range model means an entry retired by one mutation
/// stays retired across later, unrelated mutations (the world counter is
/// monotonic and `cap_before` never widens), and the live-entry count shrinks
/// rather than the whole table being wiped.
#[test]
fn test_issue_4271_targeted_invalidation_retains_unaffected_entries() {
    let mut engine = InferenceEngine::new();
    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let a = int_identity_function("a_fn");
    let b = int_identity_function("b_fn");
    engine.infer_function_with_arg_types(&a, &arg_types);
    engine.infer_function_with_arg_types(&b, &arg_types);

    // Two distinct functions inferred. (Legacy + specialized keys may coexist,
    // so assert a lower bound rather than an exact count.)
    let before = engine.return_cache_len_for_tests();
    assert!(
        before >= 2,
        "expected at least two live entries, got {before}"
    );

    let world_before = engine.method_world_for_tests();
    engine.add_method("a_fn".to_string(), int_identity_method_sig());
    assert!(engine.method_world_for_tests() > world_before);

    // `b_fn` remains a live hit after mutating `a_fn`.
    assert_eq!(
        engine.get_cached_return_type("b_fn", &arg_types),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    // `a_fn` is gone.
    assert_eq!(engine.get_cached_return_type("a_fn", &arg_types), None);
    // The live-entry count strictly decreased (a's entries were retired),
    // i.e. we did NOT wipe the whole table.
    let after = engine.return_cache_len_for_tests();
    assert!(
        after < before,
        "targeted invalidation should retire only affected entries: \
         before={before} after={after}"
    );
    assert!(after >= 1, "b_fn's entry should still be present");
}

/// Issue #5603: method-table mutations retire same-name cache entries only
/// when their widened argtypes could dispatch to the changed signature. This
/// is a narrow MethodInstance-key precision slice: unrelated same-name
/// specializations stay live.
#[test]
fn test_issue_5603_method_mutation_preserves_unmatched_same_name_cache() {
    let mut engine = InferenceEngine::new();
    let func = any_identity_function("poly_5603");
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let float_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Float64),
    ))];

    assert_eq!(
        engine.infer_function_with_arg_types(&func, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.infer_function_with_arg_types(&func, &float_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        )))
    );
    assert_eq!(
        engine.get_cached_return_type("poly_5603", &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.get_cached_return_type("poly_5603", &float_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );

    engine.add_method("poly_5603".to_string(), int_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type("poly_5603", &int_args),
        None,
        "the cache entry matching the changed Int64 method must retire"
    );
    assert_eq!(
        engine.get_cached_return_type("poly_5603", &float_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        ))),
        "same-name Float64 specialization should remain valid"
    );
}

/// Issue #5939: a same-name mutation whose signature is a supertype of an
/// already-more-specific cached method must not retire that more-specific
/// cache entry. The mutation matches the call argtypes, but it is not the
/// post-mutation dispatch winner for those argtypes.
#[test]
fn test_issue_5939_same_name_supertype_mutation_preserves_more_specific_cache() {
    let fn_name = "same_name_supertype_5939";
    let int_func = int_identity_function(fn_name);
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let mut engine = InferenceEngine::new();
    engine.add_initial_method(fn_name.to_string(), int_identity_method_sig());

    assert_eq!(
        engine.infer_function_with_arg_types(&int_func, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.get_cached_return_type_for_function(&int_func, &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );

    engine.add_method(fn_name.to_string(), any_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type_for_function(&int_func, &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "adding/replacing a less-specific Any method must not retire the \
         Int64 method cache when Int64 remains the dispatch winner"
    );
}

/// Issue #6538: on the cached-Base compile path, multi-method Base callees'
/// `MethodSig`s live only in the deserialized cached `MethodTable`s — the
/// per-function loop short-circuits them without `add_initial_method`, and
/// `add_function` drops multi-signature names as ambiguous. Seeding the cached
/// tables wholesale via `seed_initial_method_tables` must make calls to such
/// callees resolve through the method-table snapshot channel (instead of
/// falling through to the tfunc registry and inferring `Any`).
#[test]
fn test_issue_6538_seeded_method_tables_resolve_multi_method_callee() {
    let callee_name = "cached_multi_6538";
    let mut table = MethodTable::new(callee_name.to_string());
    table.add_method(int_identity_method_sig());
    table.add_method(float_identity_method_sig());
    let mut cached_tables = HashMap::new();
    cached_tables.insert(callee_name.to_string(), table);

    // Fresh engine, exactly like phase 2 on the cached path: the callee is
    // NOT in the function table and gets no add_initial_method calls.
    let mut engine = InferenceEngine::new();
    engine.seed_initial_method_tables(cached_tables.iter());

    let int_caller =
        typed_forwarder_function_5603("caller_int_6538", JuliaType::Int64, callee_name);
    let float_caller =
        typed_forwarder_function_5603("caller_float_6538", JuliaType::Float64, callee_name);
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let float_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Float64),
    ))];

    assert_eq!(
        engine.infer_function_with_arg_types(&int_caller, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        ))),
        "seeded cached method tables must drive caller inference (Int64 method)"
    );
    assert_eq!(
        engine.infer_function_with_arg_types(&float_caller, &float_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        ))),
        "seeded cached method tables must drive caller inference (Float64 method)"
    );
}

/// Issue #6538: seeding must never clobber a table the engine already holds
/// (it runs during the initial pipeline build, before user registrations, but
/// guard the invariant anyway).
#[test]
fn test_issue_6538_seeding_keeps_existing_engine_table() {
    let callee_name = "existing_table_6538";
    let mut engine = InferenceEngine::new();
    engine.add_initial_method(callee_name.to_string(), int_identity_method_sig());

    let mut cached_table = MethodTable::new(callee_name.to_string());
    cached_table.add_method(float_identity_method_sig());
    let mut cached_tables = HashMap::new();
    cached_tables.insert(callee_name.to_string(), cached_table);

    engine.seed_initial_method_tables(cached_tables.iter());

    let int_caller =
        typed_forwarder_function_5603("caller_existing_6538", JuliaType::Int64, callee_name);
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    assert_eq!(
        engine.infer_function_with_arg_types(&int_caller, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        ))),
        "an existing engine table must win over a seeded table of the same name"
    );
}

/// Issue #5603: successful method-table calls record dispatched method edges
/// with observed argument types. Mutating `callee(::Float64)` must not retire a
/// caller that only dispatched to `callee(::Int64)`.
#[test]
fn test_issue_5603_callee_method_mutation_preserves_unmatched_dispatched_caller_cache() {
    let callee_name = "callee_precise_5603";
    let int_caller =
        typed_forwarder_function_5603("caller_int_precise_5603", JuliaType::Int64, callee_name);
    let float_caller =
        typed_forwarder_function_5603("caller_float_precise_5603", JuliaType::Float64, callee_name);

    let mut table = MethodTable::new(callee_name.to_string());
    table.add_method(int_identity_method_sig());
    table.add_method(float_identity_method_sig());
    let mut method_tables = HashMap::new();
    method_tables.insert(callee_name.to_string(), table);

    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let float_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Float64),
    ))];

    assert_eq!(
        engine.infer_function_with_arg_types(&int_caller, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.infer_function_with_arg_types(&float_caller, &float_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        )))
    );
    assert_eq!(
        engine.get_cached_return_type("caller_int_precise_5603", &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.get_cached_return_type("caller_float_precise_5603", &float_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        )))
    );
    let int_cache_key =
        InferenceCacheKey::new(&inference_cache_function_id(&int_caller), &int_args);
    let int_cached = engine
        .return_type_cache
        .get(&int_cache_key)
        .expect("precise Int64 caller cache");
    assert!(
        !int_cached.edges.contains(callee_name),
        "precise method-table dependencies must not stamp a legacy bare \
         callee edge; otherwise an unrelated callee method mutation would \
         still over-invalidate through CachedReturn.edges"
    );
    assert_eq!(
        int_cached.method_edges,
        vec![DispatchedMethodEdge {
            callee: callee_name.to_string(),
            arg_types: vec![JuliaType::Int64],
        }],
        "the dispatched method edge carries the observed callee argument types"
    );

    engine.add_method(callee_name.to_string(), any_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type("caller_int_precise_5603", &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "Int64 caller should survive an Any callee mutation because Int64 still \
         dispatches to the Int64 callee method"
    );
    assert_eq!(
        engine.get_cached_return_type("caller_float_precise_5603", &float_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64)
        ))),
        "Float64 caller should survive an Any callee mutation because Float64 \
         still dispatches to the Float64 callee method"
    );

    engine.add_method(callee_name.to_string(), float_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type("caller_int_precise_5603", &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "Int64 caller should survive a Float64-only callee method mutation"
    );
    assert_eq!(
        engine.get_cached_return_type("caller_float_precise_5603", &float_args),
        None,
        "Float64 caller should retire because its dispatched method edge matches"
    );
}

/// Issue #5939: precise function-table callees should stamp the same method
/// edge shape as method-table dispatch. A caller that invoked
/// `callee(::Int64)` through the function table must survive a later
/// `callee(::Float64)` mutation instead of being retired by a bare callee edge.
#[test]
fn test_issue_5939_function_table_callee_method_mutation_preserves_unmatched_caller_cache() {
    let callee_name = "callee_function_edge_5939";
    let caller =
        typed_forwarder_function_5603("caller_function_edge_5939", JuliaType::Int64, callee_name);
    let callee = int_identity_function(callee_name);
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let mut function_table = HashMap::new();
    function_table.insert(callee_name.to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    let caller_key = InferenceCacheKey::new(&inference_cache_function_id(&caller), &int_args);
    let caller_cached = engine
        .return_type_cache
        .get(&caller_key)
        .expect("caller cache");
    assert!(
        !caller_cached.edges.contains(callee_name),
        "precise function-table dependency must not stamp a legacy bare callee edge"
    );
    assert_eq!(
        caller_cached.method_edges,
        vec![DispatchedMethodEdge {
            callee: callee_name.to_string(),
            arg_types: vec![JuliaType::Int64],
        }],
        "function-table dependency carries the observed callee argument types"
    );

    engine.add_method(callee_name.to_string(), float_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type_for_function(&caller, &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "Int64 caller should survive an unmatched Float64 callee method mutation"
    );
}

/// Issue #5939: function-table precise edges must still honor existing Julia
/// signature binding rules. If a same-name `TypeVar` cannot bind consistently,
/// keep the conservative bare callee edge instead of stamping an unsound
/// method edge.
#[test]
fn test_issue_5939_function_table_typevar_mismatch_keeps_bare_edge_fallback() {
    let callee_name = "callee_function_typevar_edge_5939";
    let caller_key = "caller_function_typevar_edge_5939";
    let typevar_t = JuliaType::TypeVar("T".to_string(), None);
    let callee = Function {
        name: callee_name.to_string(),
        params: vec![
            TypedParam {
                name: "x".to_string(),
                type_annotation: Some(typevar_t.clone()),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "y".to_string(),
                type_annotation: Some(typevar_t),
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
        ],
        kwparams: vec![],
        type_params: vec![crate::types::TypeParam::new("T".to_string())],
        return_type: None,
        body: Block {
            stmts: vec![],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
    };
    let mixed_args = [
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    ];
    let mut function_table = HashMap::new();
    function_table.insert(callee_name.to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let previous = engine.replace_active_context(caller_key.to_string(), caller_key.to_string());
    assert!(
        !engine.record_function_table_dependency_if_precise(callee_name, &mixed_args),
        "mixed argument types cannot consistently bind the same TypeVar"
    );
    engine.record_call_dependency(callee_name);
    engine.restore_active_context(previous);

    assert!(
        !engine.method_dependencies.contains_key(caller_key),
        "failed TypeVar binding must not stamp a precise method edge"
    );
    assert_eq!(
        engine.function_dependencies.get(caller_key),
        Some(&BTreeSet::from([callee_name.to_string()])),
        "the conservative bare callee edge remains the fallback"
    );
}

/// Issue #5939: legacy bare-name callee backedges still over-invalidate. A
/// cache entry that only records `edges = {callee}` cannot prove which callee
/// method it dispatched to, so mutating `callee(::Float64)` also retires an
/// `Int64` caller. This characterization keeps the current conservative
/// behavior explicit until method-instance backedges replace bare edges.
#[test]
fn test_issue_5939_bare_callee_edge_overinvalidates_unmatched_signature() {
    let callee_name = "callee_bare_edge_5939";
    let caller_name = "caller_bare_edge_5939";
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let mut table = MethodTable::new(callee_name.to_string());
    table.add_method(int_identity_method_sig());
    table.add_method(float_identity_method_sig());
    let mut method_tables = HashMap::new();
    method_tables.insert(callee_name.to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    let mut bare_edges = BTreeSet::new();
    bare_edges.insert(callee_name.to_string());
    let cache_key = InferenceCacheKey::new(caller_name, &int_args);
    engine.return_type_cache.insert(
        cache_key,
        CachedReturn::new(
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            engine.method_world_for_tests(),
            bare_edges,
            vec![],
            BTreeSet::new(),
        ),
    );
    assert_eq!(
        engine.get_cached_return_type(caller_name, &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );

    engine.add_method(callee_name.to_string(), float_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type(caller_name, &int_args),
        None,
        "bare callee edges do not carry method identity, so an unmatched \
         callee signature still conservatively retires the caller"
    );
}

/// Issue #5939: `infer_function_with_arg_types` stores primary cache entries
/// under `name(declared_param_types)` instead of co-writing a legacy bare
/// `func.name` key. Two method identities with the same call-site argtypes stay
/// distinct under the primary key, while a bare-name lookup refuses to choose
/// between them.
#[test]
fn test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write() {
    let fn_name = "legacy_dual_key_5939";
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let any_method = constant_string_function(fn_name, JuliaType::Any, "fallback");
    let int_method = int_identity_function(fn_name);
    let mut engine = InferenceEngine::new();

    assert_eq!(
        engine.infer_function_with_arg_types(&any_method, &int_args),
        LatticeType::Const(ConstValue::String("fallback".to_string()))
    );
    assert_eq!(
        engine.infer_function_with_arg_types(&int_method, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );

    assert_eq!(
        engine.get_cached_return_type_for_function(&any_method, &int_args),
        Some(&LatticeType::Const(ConstValue::String(
            "fallback".to_string()
        ))),
        "the primary cache-key lookup preserves the fallback method identity"
    );
    assert_eq!(
        engine.get_cached_return_type_for_function(&int_method, &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "the primary cache-key lookup preserves the Int64 method identity"
    );

    let any_key = InferenceCacheKey::new(&inference_cache_function_id(&any_method), &int_args);
    let int_key = InferenceCacheKey::new(&inference_cache_function_id(&int_method), &int_args);
    assert_eq!(
        engine
            .return_type_cache
            .get(&any_key)
            .map(|cached| &cached.ty),
        Some(&LatticeType::Const(ConstValue::String(
            "fallback".to_string()
        )))
    );
    assert_eq!(
        engine
            .return_type_cache
            .get(&int_key)
            .map(|cached| &cached.ty),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert_eq!(
        engine.get_cached_return_type(fn_name, &int_args),
        None,
        "bare cache lookup has no method identity, so it must not return the \
         first primary-key result when multiple method identities match the \
         same call-site argtypes"
    );
}

/// Issue #5939: dependency edges must be stamped under the same method identity
/// as the primary return cache key. Otherwise two same-name method bodies share
/// one dependency bucket, and a callee edge recorded by one method is copied
/// onto the other method's cache entry.
#[test]
fn test_issue_5939_method_identity_dependency_edges_do_not_cross_stamp_same_name_methods() {
    let fn_name = "method_identity_deps_5939";
    let callee_name = "callee_identity_deps_5939";
    let int_method = typed_forwarder_function_5603(fn_name, JuliaType::Int64, callee_name);
    let float_method = constant_string_function(fn_name, JuliaType::Float64, "independent");
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let float_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Float64),
    ))];

    let mut table = MethodTable::new(callee_name.to_string());
    table.add_method(int_identity_method_sig());
    let mut method_tables = HashMap::new();
    method_tables.insert(callee_name.to_string(), table);

    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    assert_eq!(
        engine.infer_function_with_arg_types(&int_method, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.infer_function_with_arg_types(&float_method, &float_args),
        LatticeType::Const(ConstValue::String("independent".to_string()))
    );

    let int_key = InferenceCacheKey::new(&inference_cache_function_id(&int_method), &int_args);
    let float_key =
        InferenceCacheKey::new(&inference_cache_function_id(&float_method), &float_args);
    let int_edges = engine
        .return_type_cache
        .get(&int_key)
        .expect("Int64 method cache")
        .method_edges
        .clone();
    let float_edges = engine
        .return_type_cache
        .get(&float_key)
        .expect("Float64 method cache")
        .method_edges
        .clone();
    assert_eq!(
        int_edges,
        vec![DispatchedMethodEdge {
            callee: callee_name.to_string(),
            arg_types: vec![JuliaType::Int64],
        }],
        "the Int64 method records its precise callee method edge"
    );
    assert!(
        float_edges.is_empty(),
        "the Float64 method must not inherit the Int64 method's dependency edge"
    );

    engine.add_method(callee_name.to_string(), int_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type_for_function(&int_method, &int_args),
        None,
        "the Int64 method depends on callee(::Int64), so it must retire"
    );
    assert_eq!(
        engine.get_cached_return_type_for_function(&float_method, &float_args),
        Some(&LatticeType::Const(ConstValue::String(
            "independent".to_string()
        ))),
        "same-name Float64 method cache remains live because it has no callee edge"
    );
}

/// Issue #5939: method-edge dependencies are transitive under method identity.
/// If `caller(::Int64)` dispatches to `mid(::Int64)`, and that method dispatches
/// to `leaf(::Int64)`, mutating `leaf(::Int64)` must retire both caches.
#[test]
fn test_issue_5939_method_identity_dependency_edges_propagate_transitively() {
    let leaf_name = "leaf_transitive_edge_5939";
    let mid_name = "mid_transitive_edge_5939";
    let caller_name = "caller_transitive_edge_5939";
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let leaf = int_identity_function(leaf_name);
    let mid = typed_forwarder_function_5603(mid_name, JuliaType::Int64, leaf_name);
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Int64, mid_name);
    let mut function_table = HashMap::new();
    function_table.insert(leaf_name.to_string(), leaf);
    function_table.insert(mid_name.to_string(), mid);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );

    let caller_key = InferenceCacheKey::new(&inference_cache_function_id(&caller), &int_args);
    let caller_cached = engine
        .return_type_cache
        .get(&caller_key)
        .expect("caller cache");
    assert!(
        caller_cached.method_edges.contains(&DispatchedMethodEdge {
            callee: leaf_name.to_string(),
            arg_types: vec![JuliaType::Int64],
        }),
        "caller cache should inherit the mid method's precise leaf edge"
    );

    engine.add_method(leaf_name.to_string(), int_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type_for_function(&caller, &int_args),
        None,
        "mutating the transitive leaf method must retire the caller cache"
    );
}

/// Issue #5939: call-site interprocedural inference must use the same primary
/// method identity key as direct `infer_function_with_arg_types`. Otherwise the
/// recursive/call path recreates the legacy bare-name cache entry that #5939 is
/// phasing out.
#[test]
fn test_issue_5939_call_site_user_function_cache_uses_method_identity() {
    let callee_name = "callee_callsite_key_5939";
    let caller =
        typed_forwarder_function_5603("caller_callsite_key_5939", JuliaType::Int64, callee_name);
    let callee = int_identity_function(callee_name);
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let mut function_table = HashMap::new();
    function_table.insert(callee_name.to_string(), callee.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &int_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );

    let primary_key = InferenceCacheKey::new(&inference_cache_function_id(&callee), &int_args);
    let bare_key = InferenceCacheKey::new(callee_name, &int_args);
    assert!(
        engine.return_type_cache.contains_key(&primary_key),
        "call-site inference stores the callee result under the primary method identity"
    );
    assert!(
        !engine.return_type_cache.contains_key(&bare_key),
        "call-site inference must not recreate the legacy bare-name callee key"
    );
    assert_eq!(
        engine.get_cached_return_type(callee_name, &int_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "legacy lookup still finds the unique primary-key entry for compatibility"
    );
}

/// Issue #5603: PartialStruct return facts are side-cache entries with
/// CodeInstance-style world ranges. A method mutation for an unrelated
/// function must not wipe them wholesale.
#[test]
fn test_issue_5603_unrelated_method_mutation_preserves_partial_struct_cache() {
    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let mut function_table = HashMap::new();
    function_table.insert(
        "make_partial_box_5603".to_string(),
        partial_box_constructor_function_5603("make_partial_box_5603"),
    );
    let mut engine = InferenceEngine::with_tables(partial_box_struct_table_5603(), function_table);

    let partial = engine
        .infer_function_partial_struct_return("make_partial_box_5603", &arg_types, &TypeEnv::new())
        .expect("partial struct return");
    assert_eq!(
        partial.fields.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert!(engine.has_cached_partial_struct_return_for_tests("make_partial_box_5603", &arg_types));
    let before = engine.partial_struct_return_cache_len_for_tests();

    engine.add_method(
        "unrelated_partial_5603".to_string(),
        int_identity_method_sig(),
    );

    assert_eq!(
        engine.partial_struct_return_cache_len_for_tests(),
        before,
        "unrelated method mutation must not clear PartialStruct side cache"
    );
    assert!(
        engine.has_cached_partial_struct_return_for_tests("make_partial_box_5603", &arg_types),
        "unrelated method mutation should leave the PartialStruct cache entry live"
    );
}

/// Issue #5603: PartialStruct side-cache entries carry backedges too. If a
/// cached caller's PartialStruct fact was produced by dispatching to a callee,
/// mutating that callee must retire the caller entry.
#[test]
fn test_issue_5603_callee_method_mutation_invalidates_partial_struct_caller_cache() {
    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];
    let mut function_table = HashMap::new();
    function_table.insert(
        "inner_partial_5603".to_string(),
        partial_box_constructor_function_5603("inner_partial_5603"),
    );
    function_table.insert(
        "outer_partial_5603".to_string(),
        partial_box_forwarder_function_5603("outer_partial_5603", "inner_partial_5603"),
    );
    let mut engine = InferenceEngine::with_tables(partial_box_struct_table_5603(), function_table);

    let partial = engine
        .infer_function_partial_struct_return("outer_partial_5603", &arg_types, &TypeEnv::new())
        .expect("outer partial struct return");
    assert_eq!(
        partial.fields.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert!(engine.has_cached_partial_struct_return_for_tests("outer_partial_5603", &arg_types));
    assert!(engine.has_cached_partial_struct_return_for_tests("inner_partial_5603", &arg_types));

    engine.add_method("inner_partial_5603".to_string(), int_identity_method_sig());

    assert!(
        !engine.has_cached_partial_struct_return_for_tests("outer_partial_5603", &arg_types),
        "caller PartialStruct cache entry must retire when a depended-on callee mutates"
    );
    assert!(
        !engine.has_cached_partial_struct_return_for_tests("inner_partial_5603", &arg_types),
        "mutated callee's own PartialStruct cache entry must retire"
    );
}

/// Issue #5939: PartialStruct side-cache entries should reuse the same precise
/// function-table method edges as return-type inference. If a caller only
/// dispatched to `inner(::Int64)`, adding/replacing `inner(::Float64)` must not
/// retire the caller's PartialStruct fact through a legacy bare callee edge.
#[test]
fn test_issue_5939_partial_struct_callee_method_mutation_preserves_unmatched_caller_cache() {
    let inner_name = "inner_partial_edge_5939";
    let outer_name = "outer_partial_edge_5939";
    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let mut function_table = HashMap::new();
    function_table.insert(
        inner_name.to_string(),
        partial_box_constructor_function_5603(inner_name),
    );
    function_table.insert(
        outer_name.to_string(),
        partial_box_forwarder_function_5603(outer_name, inner_name),
    );

    let mut table = MethodTable::new(inner_name.to_string());
    table.add_method(int_identity_method_sig());
    table.add_method(float_identity_method_sig());
    let mut method_tables = HashMap::new();
    method_tables.insert(inner_name.to_string(), table);

    let mut engine = InferenceEngine::with_tables_and_method_tables(
        partial_box_struct_table_5603(),
        function_table,
        method_tables,
    );

    let partial = engine
        .infer_function_partial_struct_return(outer_name, &arg_types, &TypeEnv::new())
        .expect("outer partial struct return");
    assert_eq!(
        partial.fields.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );

    let outer_key = InferenceCacheKey::new(outer_name, &arg_types);
    let outer_cached = engine
        .partial_struct_return_cache
        .get(&outer_key)
        .expect("outer partial cache");
    assert!(
        !outer_cached.edges.contains(inner_name),
        "precise PartialStruct dependency must not stamp a legacy bare callee edge"
    );
    assert_eq!(
        outer_cached.method_edges,
        vec![DispatchedMethodEdge {
            callee: inner_name.to_string(),
            arg_types: vec![JuliaType::Int64],
        }],
        "PartialStruct dependency carries the observed callee argument types"
    );

    engine.add_method(inner_name.to_string(), float_identity_method_sig());

    assert!(
        engine.has_cached_partial_struct_return_for_tests(outer_name, &arg_types),
        "Int64 caller PartialStruct cache should survive an unmatched Float64 callee mutation"
    );
}

/// Issue #5939: PartialStruct side-cache dependencies are transitive under
/// method identity too. If `outer(::Int64)` returns `mid(x)` and `mid(::Int64)`
/// returns `inner(x)`, mutating `inner(::Int64)` must retire the outer
/// PartialStruct fact.
#[test]
fn test_issue_5939_partial_struct_method_edges_propagate_transitively() {
    let inner_name = "inner_partial_transitive_edge_5939";
    let mid_name = "mid_partial_transitive_edge_5939";
    let outer_name = "outer_partial_transitive_edge_5939";
    let arg_types = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let mut function_table = HashMap::new();
    function_table.insert(
        inner_name.to_string(),
        partial_box_constructor_function_5603(inner_name),
    );
    function_table.insert(
        mid_name.to_string(),
        partial_box_forwarder_function_5603(mid_name, inner_name),
    );
    function_table.insert(
        outer_name.to_string(),
        partial_box_forwarder_function_5603(outer_name, mid_name),
    );
    let mut engine = InferenceEngine::with_tables(partial_box_struct_table_5603(), function_table);

    let partial = engine
        .infer_function_partial_struct_return(outer_name, &arg_types, &TypeEnv::new())
        .expect("outer partial struct return");
    assert_eq!(
        partial.fields.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );

    let outer_key = InferenceCacheKey::new(outer_name, &arg_types);
    let outer_cached = engine
        .partial_struct_return_cache
        .get(&outer_key)
        .expect("outer partial cache");
    assert!(
        outer_cached.method_edges.contains(&DispatchedMethodEdge {
            callee: inner_name.to_string(),
            arg_types: vec![JuliaType::Int64],
        }),
        "outer PartialStruct cache should inherit the mid method's precise inner edge"
    );

    engine.add_method(inner_name.to_string(), int_identity_method_sig());

    assert!(
        !engine.has_cached_partial_struct_return_for_tests(outer_name, &arg_types),
        "mutating the transitive inner method must retire the outer PartialStruct cache"
    );
}

/// Issue #5603: PartialStruct side-cache entries also carry global binding
/// reads. A binding change must retire only entries that read the changed
/// binding rather than clearing the whole side cache.
#[test]
fn test_issue_5603_global_binding_change_invalidates_partial_struct_cache() {
    let no_args: [LatticeType; 0] = [];
    let mut function_table = HashMap::new();
    function_table.insert(
        "global_partial_5603".to_string(),
        partial_box_global_function_5603("global_partial_5603", "G_PARTIAL_5603"),
    );
    let mut engine = InferenceEngine::with_tables(partial_box_struct_table_5603(), function_table);

    let mut globals = HashMap::new();
    globals.insert(
        "G_PARTIAL_5603".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals);

    let partial = engine
        .infer_function_partial_struct_return("global_partial_5603", &no_args, &TypeEnv::new())
        .expect("global partial struct return");
    assert_eq!(
        partial.fields.get("x"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
    assert!(engine.has_cached_partial_struct_return_for_tests("global_partial_5603", &no_args));

    let mut globals2 = HashMap::new();
    globals2.insert(
        "G_PARTIAL_5603".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );
    engine.set_global_types(globals2);

    assert!(
        !engine.has_cached_partial_struct_return_for_tests("global_partial_5603", &no_args),
        "PartialStruct cache entry that read a changed global binding must retire"
    );
}

/// Issue #5603: tentative recursive results are side-cache entries too. A
/// method-table mutation for an unrelated function must not clear them
/// wholesale, while a matching mutation must retire the affected entry.
#[test]
fn test_issue_5603_method_mutation_targets_tentative_results() {
    let no_args: [LatticeType; 0] = [];
    let mut engine = InferenceEngine::new();
    engine.seed_tentative_result_for_tests(
        "tentative_keep_5603",
        &no_args,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.seed_tentative_result_for_tests(
        "tentative_drop_5603",
        &no_args,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );

    engine.add_method(
        "unrelated_tentative_5603".to_string(),
        zero_arg_i64_method_sig(),
    );
    assert!(
        engine.has_tentative_result_for_tests("tentative_keep_5603", &no_args),
        "unrelated method mutation should preserve tentative result entries"
    );
    assert!(
        engine.has_tentative_result_for_tests("tentative_drop_5603", &no_args),
        "unrelated method mutation should not clear the tentative side cache"
    );

    engine.add_method("tentative_drop_5603".to_string(), zero_arg_i64_method_sig());
    assert!(
        engine.has_tentative_result_for_tests("tentative_keep_5603", &no_args),
        "unmatched tentative entries should stay live"
    );
    assert!(
        !engine.has_tentative_result_for_tests("tentative_drop_5603", &no_args),
        "matching method mutation should retire the tentative result"
    );
}

/// Issue #5939: limited-accuracy and tentative side-cache entries also carry
/// precise method edges. An unmatched callee method mutation must not retire
/// them through the legacy bare-name dependency path, while a matching mutation
/// still invalidates the entries.
#[test]
fn test_issue_5939_side_cache_method_edges_preserve_unmatched_callee_mutation() {
    let callee_name = "callee_side_cache_edge_5939";
    let limited_name = "limited_side_cache_edge_5939";
    let tentative_name = "tentative_side_cache_edge_5939";
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let mut table = MethodTable::new(callee_name.to_string());
    table.add_method(int_identity_method_sig());
    let mut method_tables = HashMap::new();
    method_tables.insert(callee_name.to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    let edge = DispatchedMethodEdge {
        callee: callee_name.to_string(),
        arg_types: vec![JuliaType::Int64],
    };
    let limited_key = InferenceCacheKey::new(limited_name, &int_args);
    engine.limited_results.insert(
        limited_key,
        CachedLimitedAccuracy::new(
            engine.method_world_for_tests(),
            BTreeSet::new(),
            vec![edge.clone()],
            BTreeSet::new(),
        ),
    );
    let tentative_key = InferenceCacheKey::new(tentative_name, &int_args);
    engine.tentative_results.insert(
        tentative_key,
        CachedTentativeResult::new(
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            engine.method_world_for_tests(),
            BTreeSet::new(),
            vec![edge],
            BTreeSet::new(),
        ),
    );

    assert!(engine.is_limited_return_type(limited_name, &int_args));
    assert!(engine.has_tentative_result_for_tests(tentative_name, &int_args));

    engine.add_method(callee_name.to_string(), float_identity_method_sig());

    assert!(
        engine.is_limited_return_type(limited_name, &int_args),
        "limited marker should survive an unmatched Float64 callee mutation"
    );
    assert!(
        engine.has_tentative_result_for_tests(tentative_name, &int_args),
        "tentative result should survive an unmatched Float64 callee mutation"
    );

    engine.add_method(callee_name.to_string(), int_identity_method_sig());

    assert!(
        !engine.is_limited_return_type(limited_name, &int_args),
        "limited marker should retire when its Int64 callee method mutates"
    );
    assert!(
        !engine.has_tentative_result_for_tests(tentative_name, &int_args),
        "tentative result should retire when its Int64 callee method mutates"
    );
}

/// Issue #5603: tentative recursive results also carry global binding reads.
/// Binding changes retire only entries that read the changed binding.
#[test]
fn test_issue_5603_binding_change_targets_tentative_results() {
    let no_args: [LatticeType; 0] = [];
    let mut engine = InferenceEngine::new();
    engine.record_global_read_for_tests("tentative_global_a_5603", "G_TENT_A_5603");
    engine.record_global_read_for_tests("tentative_global_b_5603", "G_TENT_B_5603");
    engine.seed_tentative_result_for_tests(
        "tentative_global_a_5603",
        &no_args,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.seed_tentative_result_for_tests(
        "tentative_global_b_5603",
        &no_args,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );

    let mut globals = HashMap::new();
    globals.insert(
        "G_TENT_A_5603".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals);

    assert!(
        !engine.has_tentative_result_for_tests("tentative_global_a_5603", &no_args),
        "tentative entry that read a changed global binding must retire"
    );
    assert!(
        engine.has_tentative_result_for_tests("tentative_global_b_5603", &no_args),
        "tentative entry that did not read the changed binding should stay live"
    );
    assert!(
        !engine.has_global_read_for_tests("tentative_global_a_5603", "G_TENT_A_5603"),
        "retired tentative entries must clear dependency records for re-inference"
    );
    assert!(
        engine.has_global_read_for_tests("tentative_global_b_5603", "G_TENT_B_5603"),
        "unaffected tentative dependency records should stay available"
    );
}
