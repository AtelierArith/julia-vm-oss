//! Precise transitive invalidation over the recorded backedge graph
//! (Issue #8554, slice 2/3 of Issue #8442).
//!
//! These tests pin the production consumer of the #8553 backedge graph:
//! a method/binding mutation seeds the directly affected specializations
//! (signature-intersection + dispatch-winner tests on recorded call edges,
//! `binding → readers` edges), walks the reverse graph transitively, and caps
//! only the reached entries — while entries not covered by the precise graph
//! keep the conservative decision, and `InvalidationStrategy::Broad` restores
//! the pre-#8554 behavior for differential comparison.

use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

fn int_lattice() -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )))
}

fn float_lattice() -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Float64,
    )))
}

fn string_identity_method_sig() -> MethodSig {
    MethodSig::for_tests(
        0,
        0,
        vec![("x".to_string(), JuliaType::String)],
        ValueType::Any,
        None,
        false,
        vec![],
        crate::inference_core::CoreType::Bottom,
        None,
        None,
    )
}

/// Engine whose method table for `callee` holds exactly one
/// `callee(x::Int64)` method (so a `Float64` call site is a dispatch miss
/// recorded as an attempted-static dynamic-fallback edge).
fn engine_with_int_method_table(callee: &str) -> InferenceEngine {
    let mut table = MethodTable::new(callee.to_string());
    table.add_method(int_identity_method_sig());
    let mut method_tables = HashMap::new();
    method_tables.insert(callee.to_string(), table);
    InferenceEngine::with_tables_and_method_tables(HashMap::new(), HashMap::new(), method_tables)
}

/// Issue #8554 precision win: a caller whose only dependency on the mutated
/// function is a dynamic-fallback edge with argtypes provably disjoint from
/// the new method's signature SURVIVES the mutation — while an unrelated
/// cached function also survives, and telemetry reports the survivor.
#[test]
fn test_issue_8554_disjoint_new_method_keeps_dynamic_fallback_caller() {
    let callee_name = "g8554_dyn_disjoint";
    let caller_name = "caller8554_dyn_disjoint";
    let float_args = [float_lattice()];
    let int_args = [int_lattice()];

    let mut engine = engine_with_int_method_table(callee_name);
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Float64, callee_name);
    let unrelated = int_identity_function("unrelated8554_dyn_disjoint");

    engine.infer_function_with_arg_types(&caller, &float_args);
    engine.infer_function_with_arg_types(&unrelated, &int_args);
    assert!(
        engine
            .get_cached_return_type(caller_name, &float_args)
            .is_some(),
        "caller result should be cached after inference"
    );

    // The new method's signature (String) is disjoint from the recorded
    // dynamic-fallback argtypes (Float64): the call still misses, so the
    // caller's cached result cannot change.
    engine.add_method(callee_name.to_string(), string_identity_method_sig());

    assert!(
        engine
            .get_cached_return_type(caller_name, &float_args)
            .is_some(),
        "a dynamic-fallback caller with argtypes disjoint from the new \
         method's signature must SURVIVE the mutation (Issue #8554)"
    );
    assert!(
        engine
            .get_cached_return_type("unrelated8554_dyn_disjoint", &int_args)
            .is_some(),
        "an unrelated cached function must survive the mutation"
    );

    let telemetry = engine.invalidation_telemetry_for_tests();
    assert_eq!(telemetry.mutations, 1);
    assert_eq!(
        telemetry.last_broad_affected, 1,
        "the conservative bare-name edge would have retired the caller"
    );
    assert_eq!(telemetry.last_invalidated, 0);
    assert_eq!(
        telemetry.last_precise_survivors, 1,
        "telemetry must report the covered entry the precise walk kept alive"
    );
}

/// The `InvalidationStrategy::Broad` escape hatch restores the pre-#8554
/// conservative behavior: the same disjoint mutation retires the caller.
#[test]
fn test_issue_8554_broad_strategy_retires_dynamic_fallback_caller() {
    let callee_name = "g8554_broad_flag";
    let caller_name = "caller8554_broad_flag";
    let float_args = [float_lattice()];

    let mut engine = engine_with_int_method_table(callee_name);
    engine.set_invalidation_strategy_for_tests(InvalidationStrategy::Broad);
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Float64, callee_name);
    engine.infer_function_with_arg_types(&caller, &float_args);
    assert!(engine
        .get_cached_return_type(caller_name, &float_args)
        .is_some());

    engine.add_method(callee_name.to_string(), string_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type(caller_name, &float_args),
        None,
        "the broad strategy must keep the pre-#8554 conservative retirement"
    );
    let telemetry = engine.invalidation_telemetry_for_tests();
    assert_eq!(telemetry.last_invalidated, 1);
    assert_eq!(telemetry.last_precise_survivors, 0);
}

/// Regression guard against under-invalidation: a new method whose signature
/// DOES intersect the recorded dynamic-fallback argtypes (and could capture
/// the call) must retire the caller.
#[test]
fn test_issue_8554_intersecting_new_method_invalidates_dynamic_fallback_caller() {
    let callee_name = "g8554_dyn_capture";
    let caller_name = "caller8554_dyn_capture";
    let float_args = [float_lattice()];

    let mut engine = engine_with_int_method_table(callee_name);
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Float64, callee_name);
    engine.infer_function_with_arg_types(&caller, &float_args);
    assert!(engine
        .get_cached_return_type(caller_name, &float_args)
        .is_some());

    // `g(x::Float64)` now captures the previously-missing call.
    engine.add_method(callee_name.to_string(), float_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type(caller_name, &float_args),
        None,
        "a new method capturing a recorded dynamic-fallback call site must \
         retire the caller (Issue #8554)"
    );
    let telemetry = engine.invalidation_telemetry_for_tests();
    assert_eq!(telemetry.last_invalidated, 1);
    assert_eq!(telemetry.last_precise_survivors, 0);
}

/// Method replacement propagates transitively through the reverse backedge
/// walk: `h → mid → g` are all retired when `g` is redefined, while an
/// unrelated cached function survives.
#[test]
fn test_issue_8554_method_replacement_invalidates_transitive_callers() {
    let int_args = [int_lattice()];
    let g = int_identity_function("g8554_chain");
    let mid = typed_forwarder_function_5603("mid8554_chain", JuliaType::Int64, "g8554_chain");
    let h = typed_forwarder_function_5603("h8554_chain", JuliaType::Int64, "mid8554_chain");
    let unrelated = int_identity_function("unrelated8554_chain");

    let mut function_table = HashMap::new();
    function_table.insert("g8554_chain".to_string(), g);
    function_table.insert("mid8554_chain".to_string(), mid);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    assert_eq!(
        engine.infer_function_with_arg_types(&h, &int_args),
        int_lattice()
    );
    engine.infer_function_with_arg_types(&unrelated, &int_args);
    assert!(engine
        .get_cached_return_type("mid8554_chain", &int_args)
        .is_some());

    // Same-signature redefinition of the leaf callee.
    engine.add_method("g8554_chain".to_string(), int_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type("g8554_chain", &int_args),
        None,
        "the redefined function's own cached result must be retired"
    );
    assert_eq!(
        engine.get_cached_return_type("mid8554_chain", &int_args),
        None,
        "the direct caller must be retired via its recorded call edge"
    );
    assert_eq!(
        engine.get_cached_return_type("h8554_chain", &int_args),
        None,
        "the transitive caller must be retired via the reverse backedge walk"
    );
    assert!(
        engine
            .get_cached_return_type("unrelated8554_chain", &int_args)
            .is_some(),
        "an unrelated cached function must survive the mutation"
    );
}

/// The transitive precision win (the point of Issue #8554): a bare-name
/// (dynamic-fallback) hop in the middle of a chain no longer smears
/// invalidation up the chain. Broad record-time folding retires BOTH `mid`
/// and `h`; the precise walk proves the Float64 call site disjoint from the
/// new `String` method and keeps both — and re-inference after invalidation
/// is identical across the strategies (differential mode: precise ⊆ broad).
#[test]
fn test_issue_8554_transitive_chain_survives_disjoint_method_addition() {
    let callee_name = "g8554_diff";
    let mid_name = "mid8554_diff";
    let h_name = "h8554_diff";
    let float_args = [float_lattice()];

    let build_engine = |strategy: InvalidationStrategy| {
        let mut table = MethodTable::new(callee_name.to_string());
        table.add_method(int_identity_method_sig());
        let mut method_tables = HashMap::new();
        method_tables.insert(callee_name.to_string(), table);
        let mid = typed_forwarder_function_5603(mid_name, JuliaType::Float64, callee_name);
        let mut function_table = HashMap::new();
        function_table.insert(mid_name.to_string(), mid);
        let mut engine = InferenceEngine::with_tables_and_method_tables(
            HashMap::new(),
            function_table,
            method_tables,
        );
        engine.set_invalidation_strategy_for_tests(strategy);
        let h = typed_forwarder_function_5603(h_name, JuliaType::Float64, mid_name);
        engine.infer_function_with_arg_types(&h, &float_args);
        assert!(engine
            .get_cached_return_type(mid_name, &float_args)
            .is_some());
        assert!(engine.get_cached_return_type(h_name, &float_args).is_some());
        engine.add_method(callee_name.to_string(), string_identity_method_sig());
        (engine, h)
    };

    let (mut precise_engine, h) = build_engine(InvalidationStrategy::Precise);
    let (mut broad_engine, _) = build_engine(InvalidationStrategy::Broad);

    // Precise: both hops survive (the Float64 call site cannot reach the
    // new String method).
    assert!(
        precise_engine
            .get_cached_return_type(mid_name, &float_args)
            .is_some(),
        "the dynamic-fallback hop must survive a disjoint method addition"
    );
    assert!(
        precise_engine
            .get_cached_return_type(h_name, &float_args)
            .is_some(),
        "the transitive caller must survive when the walk stops at the hop"
    );
    assert_eq!(
        precise_engine
            .invalidation_telemetry_for_tests()
            .last_precise_survivors,
        2
    );
    // Differential: precise invalidated ⊆ broad invalidated.
    let precise_telemetry = precise_engine.invalidation_telemetry_for_tests();
    let broad_telemetry = broad_engine.invalidation_telemetry_for_tests();
    assert!(
        precise_telemetry.last_invalidated <= broad_telemetry.last_invalidated,
        "the precise decision set must stay within the broad one \
         (precise {} vs broad {})",
        precise_telemetry.last_invalidated,
        broad_telemetry.last_invalidated,
    );

    // Broad: the record-time bare-name folding retires both.
    assert_eq!(
        broad_engine.get_cached_return_type(mid_name, &float_args),
        None
    );
    assert_eq!(
        broad_engine.get_cached_return_type(h_name, &float_args),
        None
    );

    // Post-invalidation recompilation parity: re-inference produces the same
    // result under both strategies.
    let precise_reinferred = precise_engine.infer_function_with_arg_types(&h, &float_args);
    let broad_reinferred = broad_engine.infer_function_with_arg_types(&h, &float_args);
    assert_eq!(
        precise_reinferred, broad_reinferred,
        "re-inference after invalidation must be strategy-independent"
    );
}

/// Binding mutation invalidates via the recorded `binding → readers` edges,
/// transitively through callers, while a non-reader survives.
#[test]
fn test_issue_8554_binding_change_walks_binding_readers_transitively() {
    let int_args = [int_lattice()];
    let reader = typed_global_reader_function("reader8554_bind", JuliaType::Int64, "G8554_BIND");
    let caller =
        typed_forwarder_function_5603("caller8554_bind", JuliaType::Int64, "reader8554_bind");
    let nonreader = int_identity_function("nonreader8554_bind");

    let mut function_table = HashMap::new();
    function_table.insert("reader8554_bind".to_string(), reader);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let mut globals = HashMap::new();
    globals.insert("G8554_BIND".to_string(), int_lattice());
    engine.set_global_types(globals);

    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &int_args),
        int_lattice()
    );
    engine.infer_function_with_arg_types(&nonreader, &int_args);
    assert!(engine
        .get_cached_return_type("reader8554_bind", &int_args)
        .is_some());

    // Rebind the global to Float64.
    let mut globals = HashMap::new();
    globals.insert("G8554_BIND".to_string(), float_lattice());
    engine.set_global_types(globals);

    assert_eq!(
        engine.get_cached_return_type("reader8554_bind", &int_args),
        None,
        "the binding reader must be retired via the binding → readers edge"
    );
    assert_eq!(
        engine.get_cached_return_type("caller8554_bind", &int_args),
        None,
        "the reader's caller must be retired via the transitive walk"
    );
    assert!(
        engine
            .get_cached_return_type("nonreader8554_bind", &int_args)
            .is_some(),
        "a function that never read the binding must survive"
    );

    // Re-inference reflects the new binding type.
    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &int_args),
        float_lattice()
    );
}

/// Safety valve (Issue #8554): a method addition for a name that had NO
/// static table before the mutation cannot rely on the precise graph —
/// dynamic-fallback recording skips table-less names — so the conservative
/// scan must run and retire the attempted-static caller.
#[test]
fn test_issue_8554_new_name_method_addition_falls_back_to_broad() {
    let int_args = [int_lattice()];
    let caller = typed_forwarder_function_5603(
        "caller8554_newname",
        JuliaType::Int64,
        "brandnew8554_target",
    );
    let mut engine = InferenceEngine::new();
    engine.infer_function_with_arg_types(&caller, &int_args);
    assert!(engine
        .get_cached_return_type("caller8554_newname", &int_args)
        .is_some());

    // First-ever definition of the callee name: the precise graph provably
    // has no edges for it, so the broad decision applies.
    engine.add_method("brandnew8554_target".to_string(), int_identity_method_sig());

    assert_eq!(
        engine.get_cached_return_type("caller8554_newname", &int_args),
        None,
        "defining a previously table-less name must retire its \
         attempted-static callers via the broad fallback (Issue #8554)"
    );
}
