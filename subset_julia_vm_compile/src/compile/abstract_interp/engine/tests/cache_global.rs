use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

/// Issue #4285: a cached inference result that read a top-level global binding
/// must be invalidated when that binding is later (re)defined with a different
/// type — modeling const-redefinition / global-rebinding invalidation rather
/// than silently serving the stale, more-precise cached result.
#[test]
fn test_issue_4285_changed_global_binding_invalidates_dependent_cache() {
    let mut engine = InferenceEngine::new();

    // The reader function reads the global `g_4285`.
    let reader = global_reader_function("reader_4285", "g_4285");
    let no_args: [LatticeType; 0] = [];

    // Initially `g_4285 :: Int64`.
    let mut globals = HashMap::new();
    globals.insert(
        "g_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals);

    // Prime the cache: the reader returns the global's type (Int64) and records
    // a dependency on `g_4285`.
    assert_eq!(
        engine.infer_function_with_arg_types(&reader, &no_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert_eq!(
        engine.get_cached_return_type("reader_4285", &no_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "reader result should be cached as Int64 after first inference"
    );

    let world_before = engine.method_world_for_tests();
    let binding_world_before = engine.binding_world_for_tests();

    // Redefine `g_4285 :: Float64` (an incompatible rebinding / const redefine).
    let mut globals2 = HashMap::new();
    globals2.insert(
        "g_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );
    engine.set_global_types(globals2);

    assert!(
        engine.binding_world_for_tests() > binding_world_before,
        "a changed binding must advance the binding world"
    );
    assert!(
        engine.method_world_for_tests() > world_before,
        "a changed binding must advance the gating inference world"
    );
    // The stale Int64 result must be gone (world-gated miss).
    assert_eq!(
        engine.get_cached_return_type("reader_4285", &no_args),
        None,
        "a binding change to a read global must invalidate the dependent cache \
         (no stale Int64 after redefining g_4285 :: Float64), Issue #4285"
    );

    // Re-inference now reflects the new binding type.
    assert_eq!(
        engine.infer_function_with_arg_types(&reader, &no_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64
        ))),
        "re-inference after the binding change should yield the new Float64 type"
    );
}

/// Issue #4285: changing a global binding that a cached result did NOT read
/// must leave that result valid (targeted binding invalidation, the precision
/// counterpart to soundness).
#[test]
fn test_issue_4285_unrelated_global_change_preserves_cache() {
    let mut engine = InferenceEngine::new();
    let no_args: [LatticeType; 0] = [];

    // `reader_a` reads `ga_4285`; `reader_b` reads `gb_4285`.
    let reader_a = global_reader_function("reader_a_4285", "ga_4285");
    let reader_b = global_reader_function("reader_b_4285", "gb_4285");

    let mut globals = HashMap::new();
    globals.insert(
        "ga_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    globals.insert(
        "gb_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals);

    engine.infer_function_with_arg_types(&reader_a, &no_args);
    engine.infer_function_with_arg_types(&reader_b, &no_args);
    assert!(engine
        .get_cached_return_type("reader_a_4285", &no_args)
        .is_some());
    assert!(engine
        .get_cached_return_type("reader_b_4285", &no_args)
        .is_some());

    // Change only `ga_4285`; `gb_4285` keeps its Int64 type.
    let mut globals2 = HashMap::new();
    globals2.insert(
        "ga_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );
    globals2.insert(
        "gb_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals2);

    // `reader_a` (read the changed binding) is invalidated...
    assert_eq!(
        engine.get_cached_return_type("reader_a_4285", &no_args),
        None,
        "reader of the changed binding must be invalidated"
    );
    // ...but `reader_b` (read only the unchanged binding) survives.
    assert_eq!(
        engine.get_cached_return_type("reader_b_4285", &no_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "a reader that did not read the changed binding must NOT be invalidated \
         (targeted binding invalidation, Issue #4285)"
    );
}

/// Issue #4285: a `set_global_types` call that does not actually change any
/// binding must be a no-op for cache validity — it must not advance the world
/// or drop unaffected cached results.
#[test]
fn test_issue_4285_noop_global_update_preserves_cache_and_world() {
    let mut engine = InferenceEngine::new();
    let no_args: [LatticeType; 0] = [];

    let reader = global_reader_function("stable_reader_4285", "gstable_4285");
    let mut globals = HashMap::new();
    globals.insert(
        "gstable_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals.clone());
    engine.infer_function_with_arg_types(&reader, &no_args);
    assert!(engine
        .get_cached_return_type("stable_reader_4285", &no_args)
        .is_some());

    let world_before = engine.method_world_for_tests();
    let binding_world_before = engine.binding_world_for_tests();

    // Re-apply the identical environment.
    engine.set_global_types(globals);

    assert_eq!(
        engine.method_world_for_tests(),
        world_before,
        "an identical global environment must not advance the inference world"
    );
    assert_eq!(
        engine.binding_world_for_tests(),
        binding_world_before,
        "an identical global environment must not advance the binding world"
    );
    assert_eq!(
        engine.get_cached_return_type("stable_reader_4285", &no_args),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        ))),
        "an unchanged binding environment must keep all cached results valid"
    );
}

/// Issue #4285: a function that reads a global only *transitively* through a
/// callee must still be invalidated when that global changes (transitive
/// binding-edge reachability, mirroring upstream `GlobalRef` edge folding).
#[test]
fn test_issue_4285_transitive_global_read_invalidates_caller() {
    let no_args: [LatticeType; 0] = [];

    // callee_4285() = gt_4285   (reads the global directly)
    let callee = global_reader_function("callee_4285", "gt_4285");
    // caller_4285() = callee_4285()   (reads the global only via the callee)
    let caller = Function {
        name: "caller_4285".to_string(),
        params: vec![],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "callee_4285".to_string().into(),
                    args: vec![],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![],
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
    function_table.insert("callee_4285".to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let mut globals = HashMap::new();
    globals.insert(
        "gt_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals);

    // Infer the caller; this folds the callee's global read (`gt_4285`) into the
    // caller's recorded global reads.
    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &no_args),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    assert!(
        engine
            .get_cached_return_type("caller_4285", &no_args)
            .is_some(),
        "caller result should be cached"
    );

    // Change the global. The caller depends on it transitively.
    let mut globals2 = HashMap::new();
    globals2.insert(
        "gt_4285".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );
    engine.set_global_types(globals2);

    assert_eq!(
        engine.get_cached_return_type("caller_4285", &no_args),
        None,
        "a caller that transitively reads a changed global must be invalidated \
         (transitive binding-edge reachability, Issue #4285)"
    );
}

/// Issue #5939: precise method-edge dependencies must also fold the callee's
/// global binding reads through the resolved method identity. A caller that
/// reaches `callee(::Int64)` through a `DispatchedMethodEdge` still depends on
/// the global read by that typed callee method.
#[test]
fn test_issue_5939_method_edge_transitive_global_read_invalidates_caller() {
    let callee_name = "callee_method_edge_global_5939";
    let caller_name = "caller_method_edge_global_5939";
    let global_name = "G_METHOD_EDGE_5939";
    let int_args = [LatticeType::Concrete(ConcreteType::Core(
        CoreType::Primitive(CorePrimitive::Int64),
    ))];

    let callee = typed_global_reader_function(callee_name, JuliaType::Int64, global_name);
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Int64, callee_name);
    let mut function_table = HashMap::new();
    function_table.insert(callee_name.to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let mut globals = HashMap::new();
    globals.insert(
        global_name.to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    engine.set_global_types(globals);

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
        caller_cached.global_reads.contains(global_name),
        "caller cache should inherit global reads through the precise method edge"
    );
    assert!(
        engine
            .method_dependencies
            .get(&caller_key.fn_id)
            .is_some_and(|edges| {
                edges.contains(&DispatchedMethodEdge {
                    callee: callee_name.to_string(),
                    arg_types: vec![JuliaType::Int64],
                })
            }),
        "caller dependency records should include the precise method edge"
    );

    let mut globals2 = HashMap::new();
    globals2.insert(
        global_name.to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
    );
    engine.set_global_types(globals2);

    assert_eq!(
        engine.get_cached_return_type_for_function(&caller, &int_args),
        None,
        "changing the typed callee's global read must retire the caller cache"
    );
    assert!(
        !engine.method_dependencies.contains_key(&caller_key.fn_id),
        "binding invalidation must clear retired method-edge dependency records \
         so re-inference rebuilds them from the current world"
    );
}
