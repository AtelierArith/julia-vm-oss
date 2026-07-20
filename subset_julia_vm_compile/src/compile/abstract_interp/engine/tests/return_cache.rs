use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

#[test]
fn cached_return_serializes_roundtrip_issue_5093() {
    let mut edges = BTreeSet::new();
    edges.insert("callee".to_string());
    let mut global_reads = BTreeSet::new();
    global_reads.insert("CONST_VALUE".to_string());
    let cached = CachedReturn::new(
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        4,
        edges,
        vec![],
        global_reads,
    );

    let encoded = bincode::serialize(&cached).expect("serialize cached return");
    let decoded: CachedReturn = bincode::deserialize(&encoded).expect("deserialize cached return");

    assert_eq!(decoded, cached);
    assert!(decoded.valid_worlds.contains(4));
}

#[test]
fn return_cache_snapshot_seeds_new_engine_issue_5093() {
    let mut engine = InferenceEngine::new();
    let func = Function {
        name: "identity_i64".to_string(),
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

    assert_eq!(
        engine.infer_function(&func),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64
        )))
    );
    let snapshot = engine.snapshot_return_cache();
    assert!(
        !snapshot.is_empty(),
        "snapshot should include the inferred return cache entry"
    );

    let mut seeded = InferenceEngine::new();
    seeded.seed_return_cache(snapshot);
    assert_eq!(
        seeded.get_cached_return_type(
            "identity_i64",
            &[LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            ))]
        ),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
}

#[test]
fn return_cache_seed_rebases_persisted_world_issue_5093() {
    let key = InferenceCacheKey::new(
        "cached_i64",
        &[LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))],
    );
    let cached = CachedReturn::new(
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        42,
        BTreeSet::new(),
        vec![],
        BTreeSet::new(),
    );

    let mut seeded = InferenceEngine::new();
    seeded.seed_return_cache(vec![(key.clone(), cached)]);

    let stored = seeded
        .return_type_cache
        .get(&key)
        .expect("open-ended persisted cache entry should be seeded");
    assert_eq!(stored.valid_worlds.min_world, seeded.method_world);
    assert_eq!(stored.valid_worlds.max_world, World::MAX);
    assert_eq!(
        seeded.get_cached_return_type(
            "cached_i64",
            &[LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            ))]
        ),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64)
        )))
    );
}

#[test]
fn return_cache_seed_skips_capped_persisted_world_issue_5093() {
    let key = InferenceCacheKey::new(
        "cached_i64",
        &[LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))],
    );
    let mut cached = CachedReturn::new(
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        1,
        BTreeSet::new(),
        vec![],
        BTreeSet::new(),
    );
    cached.valid_worlds.cap_before(2);

    let mut seeded = InferenceEngine::new();
    seeded.seed_return_cache(vec![(key.clone(), cached)]);

    assert!(
        !seeded.return_type_cache.contains_key(&key),
        "capped persisted cache entries must not be revived"
    );
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
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
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
