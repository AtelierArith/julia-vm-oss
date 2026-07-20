//! Precise inference backedge recording tests (Issue #8553, slice 1/3 of
//! Issue #8442).
//!
//! These tests assert that inference RECORDS `caller specialization →
//! (callee method, call argtypes)` edges and `caller specialization →
//! global binding` edges into the [`BackedgeIndex`], with a stable
//! specialization identity (method definition + canonical specialized
//! signature, never method-table insertion order). This slice records only;
//! invalidation behavior is unchanged (Issue #8554 consumes the graph).

use super::super::backedges::{BackedgeCallee, CallEdge, CallEdgeKind, MethodKey};
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

fn int_core_tuple() -> CoreType {
    CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Int64)])
}

fn float_core_tuple() -> CoreType {
    CoreType::Tuple(vec![CoreType::Primitive(CorePrimitive::Float64)])
}

/// Builds `name(x::Any) = callee(x)` so distinct call-site argtypes produce
/// distinct caller specializations of one method definition.
fn any_forwarder_function(name: &str, callee: &str) -> Function {
    typed_forwarder_function_5603(name, JuliaType::Any, callee)
}

/// Builds `name(x::Int64) = MyMod.helper(x)` (a module-qualified call site).
fn module_call_forwarder_function(name: &str, module: &str, function: &str) -> Function {
    Function {
        name: name.to_string(),
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
                value: Some(Expr::ModuleCall {
                    module: module.to_string().into(),
                    function: function.to_string().into(),
                    args: vec![Expr::Var("x".to_string().into(), dummy_span())],
                    kwargs: vec![],
                    splat_mask: vec![false],
                    kwargs_splat_mask: vec![],
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
    }
}

/// The caller specialization key `infer_function_with_arg_types` establishes
/// for `func` invoked with `arg_types`.
fn caller_spec(
    func: &Function,
    arg_types: &[LatticeType],
) -> super::super::backedges::SpecializationKey {
    super::super::backedges::SpecializationKey::new(
        MethodKey::from_function(func),
        &widen_argtypes_for_cache_key(arg_types),
    )
}

/// Issue #8553: a resolved direct call must record a precise
/// `caller specialization → (callee method, call argtypes)` edge, and the
/// reverse index must surface the caller from the callee's name.
#[test]
fn test_issue_8553_direct_call_records_precise_backedge() {
    let callee_name = "callee_backedge_8553";
    let caller_name = "caller_backedge_8553";
    let callee = int_identity_function(callee_name);
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Int64, callee_name);
    let int_args = [int_lattice()];

    let mut function_table = HashMap::new();
    function_table.insert(callee_name.to_string(), callee.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    assert_eq!(
        engine.infer_function_with_arg_types(&caller, &int_args),
        int_lattice()
    );

    let caller_key = caller_spec(&caller, &int_args);
    let edges = engine
        .backedge_index()
        .call_edges_for(&caller_key)
        .expect("caller specialization should have recorded call edges");
    let expected = CallEdge {
        callee: BackedgeCallee::Method(std::rc::Rc::new(MethodKey::from_function(&callee))),
        call_argtypes: int_core_tuple(),
        kind: CallEdgeKind::Direct,
    };
    assert!(
        edges.contains(&expected),
        "expected precise direct call edge {expected:?}, got {edges:?}"
    );

    // Reverse index: the mutated-name walk (#8554) finds the caller.
    let callers = engine
        .backedge_index()
        .caller_specializations_of(callee_name);
    assert!(
        callers.iter().any(|spec| **spec == caller_key),
        "reverse backedge index must surface the caller specialization"
    );

    // Per-specialization world stamp (CodeInstance min/max world analogue).
    let world = engine
        .backedge_index()
        .specialization_world(&caller_key)
        .expect("caller specialization should carry a world range");
    assert_eq!(world.min_world, engine.method_world_for_tests());
    assert_eq!(world.max_world, World::MAX);
}

/// Issue #8553: a method-table dispatch edge must carry the *dispatch
/// winner's* method identity (declared signature), not just the bare name —
/// the Int64 caller edge points at the Int64 method, never the Float64 one.
#[test]
fn test_issue_8553_method_table_dispatch_records_winner_method_key() {
    let callee_name = "callee_mt_backedge_8553";
    let caller_name = "caller_mt_backedge_8553";
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Int64, callee_name);
    let int_args = [int_lattice()];

    let mut table = MethodTable::new(callee_name.to_string());
    table.add_method(int_identity_method_sig());
    table.add_method(float_identity_method_sig());
    let int_sig_core = int_identity_method_sig().core_signature();
    let float_sig_core = float_identity_method_sig().core_signature();
    let mut method_tables = HashMap::new();
    method_tables.insert(callee_name.to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    engine.infer_function_with_arg_types(&caller, &int_args);

    let caller_key = caller_spec(&caller, &int_args);
    let edges = engine
        .backedge_index()
        .call_edges_for(&caller_key)
        .expect("caller specialization should have recorded call edges");
    let winner = MethodKey::new(callee_name, int_sig_core);
    let loser = MethodKey::new(callee_name, float_sig_core);
    assert!(
        edges.iter().any(|edge| matches!(
            &edge.callee,
            BackedgeCallee::Method(key) if **key == winner
        )),
        "dispatch edge must carry the Int64 winner method identity, got {edges:?}"
    );
    assert!(
        !edges.iter().any(|edge| matches!(
            &edge.callee,
            BackedgeCallee::Method(key) if **key == loser
        )),
        "dispatch edge must not point at the unmatched Float64 method"
    );
}

/// Issue #8553: a module-qualified `Expr::ModuleCall` records a
/// `ModuleQualified` edge to the resolved method under the qualified name.
/// Recording is observation-only: the call's inferred type is unchanged.
#[test]
fn test_issue_8553_module_qualified_call_records_edge() {
    let qualified_name = "MyMod8553.helper";
    let caller_name = "caller_module_backedge_8553";
    let helper = int_identity_function(qualified_name);
    let caller = module_call_forwarder_function(caller_name, "MyMod8553", "helper");
    let int_args = [int_lattice()];

    let mut function_table = HashMap::new();
    function_table.insert(qualified_name.to_string(), helper.clone());
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    engine.infer_function_with_arg_types(&caller, &int_args);

    let caller_key = caller_spec(&caller, &int_args);
    let edges = engine
        .backedge_index()
        .call_edges_for(&caller_key)
        .expect("caller specialization should have recorded call edges");
    let expected = CallEdge {
        callee: BackedgeCallee::Method(std::rc::Rc::new(MethodKey::from_function(&helper))),
        call_argtypes: int_core_tuple(),
        kind: CallEdgeKind::ModuleQualified,
    };
    assert!(
        edges.contains(&expected),
        "expected module-qualified call edge {expected:?}, got {edges:?}"
    );
    let callers = engine
        .backedge_index()
        .caller_specializations_of(qualified_name);
    assert!(
        callers.iter().any(|spec| **spec == caller_key),
        "reverse index must key module-qualified edges under the qualified name"
    );
}

/// Issue #8553: when a static target exists but the call site cannot resolve
/// it precisely (imprecise argtypes → dynamic fallback), record the attempted
/// target as an `Unresolved` edge so a later method change under that name
/// can still find the caller.
#[test]
fn test_issue_8553_dynamic_fallback_records_attempted_target() {
    let callee_name = "callee_dyn_backedge_8553";
    let caller_name = "caller_dyn_backedge_8553";
    // `caller(x::Any) = callee(x)` inferred with `Any` argtypes: the callee
    // method table exists (static target attempted) but `Any` args are not
    // precise enough to dispatch statically.
    let caller = any_forwarder_function(caller_name, callee_name);
    let any_args = [LatticeType::Concrete(ConcreteType::Core(CoreType::Any))];

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

    engine.infer_function_with_arg_types(&caller, &any_args);

    let caller_key = caller_spec(&caller, &any_args);
    let edges = engine
        .backedge_index()
        .call_edges_for(&caller_key)
        .expect("caller specialization should have recorded call edges");
    assert!(
        edges.iter().any(|edge| matches!(
            &edge.callee,
            BackedgeCallee::Unresolved { function } if function == callee_name
        ) && edge.kind == CallEdgeKind::DynamicFallback),
        "expected a DynamicFallback edge naming the attempted target, got {edges:?}"
    );
}

/// Issue #8553: a call whose name matches no method/function table entry
/// (builtin-only resolution) records no backedge — the index must not flood
/// with names that can never be invalidated by a method mutation.
#[test]
fn test_issue_8553_builtin_only_call_records_no_backedge() {
    let caller_name = "caller_builtin_backedge_8553";
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Int64, "abs");
    let int_args = [int_lattice()];
    let mut engine = InferenceEngine::new();

    engine.infer_function_with_arg_types(&caller, &int_args);

    let caller_key = caller_spec(&caller, &int_args);
    assert!(
        engine
            .backedge_index()
            .call_edges_for(&caller_key)
            .is_none(),
        "builtin-only call sites must not record backedges"
    );
}

/// Issue #8553: reading a top-level global binding records
/// `caller specialization → binding` (upstream analogue:
/// `bindinginvalidations.jl` per-`GlobalRef` edges).
#[test]
fn test_issue_8553_global_read_records_binding_edge() {
    let caller_name = "global_reader_backedge_8553";
    let binding = "G_BACKEDGE_8553";
    let caller = global_reader_function(caller_name, binding);
    let no_args: [LatticeType; 0] = [];

    let mut engine = InferenceEngine::new();
    let mut globals = HashMap::new();
    globals.insert(binding.to_string(), int_lattice());
    engine.set_global_types(globals);

    engine.infer_function_with_arg_types(&caller, &no_args);

    let caller_key = caller_spec(&caller, &no_args);
    let reads = engine
        .backedge_index()
        .global_reads_for(&caller_key)
        .expect("caller specialization should have recorded global reads");
    assert!(
        reads.contains(binding),
        "expected global read edge to {binding}, got {reads:?}"
    );
    let readers = engine
        .backedge_index()
        .binding_reader_specializations_of(binding);
    assert!(
        readers.iter().any(|spec| **spec == caller_key),
        "reverse binding index must surface the reader specialization"
    );
}

/// Issue #8553: one method definition invoked with different call-site
/// argtypes yields *distinct* specializations, each with its own edges —
/// the specialization key is (method identity, specialized signature), not
/// just the method.
#[test]
fn test_issue_8553_distinct_argtypes_yield_distinct_specializations() {
    let callee_name = "callee_spec_split_8553";
    let caller_name = "caller_spec_split_8553";
    let caller = any_forwarder_function(caller_name, callee_name);
    let int_args = [int_lattice()];
    let float_args = [float_lattice()];

    let mut table = MethodTable::new(callee_name.to_string());
    table.add_method(int_identity_method_sig());
    table.add_method(float_identity_method_sig());
    let int_sig_core = int_identity_method_sig().core_signature();
    let float_sig_core = float_identity_method_sig().core_signature();
    let mut method_tables = HashMap::new();
    method_tables.insert(callee_name.to_string(), table);
    let mut engine = InferenceEngine::with_tables_and_method_tables(
        HashMap::new(),
        HashMap::new(),
        method_tables,
    );

    engine.infer_function_with_arg_types(&caller, &int_args);
    engine.infer_function_with_arg_types(&caller, &float_args);

    let int_spec = caller_spec(&caller, &int_args);
    let float_spec = caller_spec(&caller, &float_args);
    assert_ne!(
        int_spec, float_spec,
        "different call-site argtypes must produce distinct specializations"
    );

    let int_edges = engine
        .backedge_index()
        .call_edges_for(&int_spec)
        .expect("Int64 specialization edges");
    let float_edges = engine
        .backedge_index()
        .call_edges_for(&float_spec)
        .expect("Float64 specialization edges");
    let int_winner = MethodKey::new(callee_name, int_sig_core);
    let float_winner = MethodKey::new(callee_name, float_sig_core);
    assert!(
        int_edges.iter().any(|edge| matches!(
            &edge.callee,
            BackedgeCallee::Method(key) if **key == int_winner
        )) && edge_argtypes_contain(int_edges, &int_core_tuple()),
        "Int64 specialization records the Int64 winner, got {int_edges:?}"
    );
    assert!(
        float_edges.iter().any(|edge| matches!(
            &edge.callee,
            BackedgeCallee::Method(key) if **key == float_winner
        )) && edge_argtypes_contain(float_edges, &float_core_tuple()),
        "Float64 specialization records the Float64 winner, got {float_edges:?}"
    );
}

fn edge_argtypes_contain(edges: &[CallEdge], expected: &CoreType) -> bool {
    edges.iter().any(|edge| edge.call_argtypes == *expected)
}

/// Issue #8553: the method identity is derived from the definition (name +
/// canonical declared signature + vararg shape), so two structurally equal
/// definitions produce the same key and re-compiling in a different order
/// cannot change it.
#[test]
fn test_issue_8553_method_key_is_stable_across_recompilation() {
    let first = MethodKey::from_function(&int_identity_function("stable_key_8553"));
    let second = MethodKey::from_function(&int_identity_function("stable_key_8553"));
    assert_eq!(
        first, second,
        "structurally identical definitions must share one method identity"
    );

    let other_sig = MethodKey::from_function(&any_identity_function("stable_key_8553"));
    assert_ne!(
        first, other_sig,
        "a different declared signature is a different method identity"
    );
}

/// Issue #8553: the debug dump lists recorded edges deterministically so a
/// compiled program's backedge graph can be inspected and diffed.
#[test]
fn test_issue_8553_dump_lists_edges_deterministically() {
    let callee_name = "callee_dump_8553";
    let caller_name = "caller_dump_8553";
    let binding = "G_DUMP_8553";
    let callee = int_identity_function(callee_name);
    let caller = typed_forwarder_function_5603(caller_name, JuliaType::Int64, callee_name);
    let reader = global_reader_function("reader_dump_8553", binding);
    let int_args = [int_lattice()];
    let no_args: [LatticeType; 0] = [];

    let mut function_table = HashMap::new();
    function_table.insert(callee_name.to_string(), callee);
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);
    let mut globals = HashMap::new();
    globals.insert(binding.to_string(), int_lattice());
    engine.set_global_types(globals);

    engine.infer_function_with_arg_types(&caller, &int_args);
    engine.infer_function_with_arg_types(&reader, &no_args);

    let dump = engine.backedge_index_dump();
    assert!(
        dump.contains(caller_name) && dump.contains(callee_name),
        "dump should list the call edge endpoints:\n{dump}"
    );
    assert!(
        dump.contains(binding),
        "dump should list the global binding edge:\n{dump}"
    );
    assert_eq!(
        dump,
        engine.backedge_index_dump(),
        "dump must be deterministic"
    );
}
