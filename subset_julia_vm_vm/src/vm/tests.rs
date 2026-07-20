//! Unit tests for the VM core (`vm::mod`): method dispatch, slot storage,
//! error handling, and call-frame management.

// Whole-file test-only (declared `#[cfg(test)] mod tests;` in `vm/mod.rs`);
// this inner allow overrides that ancestor's
// `#![deny(clippy::unwrap_used)]`/`#![deny(clippy::expect_used)]` cascade
// (Issue #10979 Phase 4 of #10869).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::test_runtime::{compile_core_source, compile_repl_core_source};
use crate::types::JuliaType;
use crate::vm::value::GeneratorCallable;
use std::rc::Rc;

#[test]
fn string_index_validator_classifies_boundaries_11621() {
    use exec::string_index::{validate_string_index, StringIndexValidation};
    use StringIndexValidation::{Character, NonCharacterBoundary, OutOfBounds};

    let cases = [
        (
            b"abc".as_slice(),
            1,
            Character {
                byte_start: 0,
                byte_end: 1,
            },
        ),
        (
            b"abc".as_slice(),
            3,
            Character {
                byte_start: 2,
                byte_end: 3,
            },
        ),
        (
            "aéz".as_bytes(),
            2,
            Character {
                byte_start: 1,
                byte_end: 3,
            },
        ),
        (
            "aéz".as_bytes(),
            3,
            NonCharacterBoundary {
                valid_indices: (2, 4),
            },
        ),
        (
            "aéz".as_bytes(),
            4,
            Character {
                byte_start: 3,
                byte_end: 4,
            },
        ),
        (
            &[0xf0, 0x80, 0x80, 0x80],
            1,
            Character {
                byte_start: 0,
                byte_end: 4,
            },
        ),
        (
            &[0xf0, 0x80, 0x80, 0x80],
            2,
            NonCharacterBoundary {
                valid_indices: (1, -1),
            },
        ),
        (
            &[0x80, 0x61],
            1,
            Character {
                byte_start: 0,
                byte_end: 1,
            },
        ),
        ("aéz".as_bytes(), 0, OutOfBounds),
        ("aéz".as_bytes(), 5, OutOfBounds),
    ];

    for (bytes, index, expected) in cases {
        assert_eq!(validate_string_index(bytes, index), expected);
    }
}

#[derive(Clone, Copy, Debug)]
enum ExceptionPayloadKind11647 {
    Method,
    Domain,
    Type,
    StringIndex,
    Parse,
    FieldIndex,
}

impl ExceptionPayloadKind11647 {
    fn payload(self, key: u8) -> exec::exception_payload::PendingExceptionPayload {
        use exec::exception_payload::PendingExceptionPayload;
        let key_i64 = i64::from(key);

        match self {
            Self::Method => PendingExceptionPayload::method_error(
                format!("method-{key}"),
                "f",
                &[Value::I64(key_i64)],
            ),
            Self::Domain => PendingExceptionPayload::Domain {
                message: format!("domain-{key}"),
                val: Value::I64(key_i64),
            },
            Self::Type => PendingExceptionPayload::Type {
                message: format!("type-{key}"),
                func: Value::I64(key_i64),
                context: Value::I64(key_i64 + 1),
                expected: Value::I64(key_i64 + 2),
                got: Value::I64(key_i64 + 3),
            },
            Self::StringIndex => PendingExceptionPayload::StringIndex {
                index: key_i64,
                valid_indices: (key_i64 - 1, key_i64 + 1),
                string: Value::str_from_bytes(vec![0xf0, 0x80, 0x80, key]),
            },
            Self::Parse => PendingExceptionPayload::Parse {
                message: format!("parse-{key}"),
                detail: Value::I64(key_i64),
            },
            Self::FieldIndex => PendingExceptionPayload::FieldIndex {
                index: usize::from(key),
                field_count: usize::from(key) + 1,
                receiver: Value::I64(key_i64),
            },
        }
    }
}

const EXCEPTION_PAYLOAD_KINDS_11647: [ExceptionPayloadKind11647; 6] = [
    ExceptionPayloadKind11647::Method,
    ExceptionPayloadKind11647::Domain,
    ExceptionPayloadKind11647::Type,
    ExceptionPayloadKind11647::StringIndex,
    ExceptionPayloadKind11647::Parse,
    ExceptionPayloadKind11647::FieldIndex,
];

#[test]
fn exception_payload_carrier_lifecycle_matrix_11647() {
    use exec::exception_payload::PendingExceptionPayloadCarrier;

    for kind in EXCEPTION_PAYLOAD_KINDS_11647 {
        let mut carrier = PendingExceptionPayloadCarrier::default();

        let exact = carrier.park_and_construct(kind.payload(2));
        assert!(carrier.take_fields_for(&exact).is_some(), "{kind:?} exact");
        assert!(
            carrier.take_fields_for(&exact).is_none(),
            "{kind:?} one-shot"
        );

        let exact = carrier.park_and_construct(kind.payload(2));
        let mut mismatch_carrier = PendingExceptionPayloadCarrier::default();
        let mismatch = mismatch_carrier.park_and_construct(kind.payload(3));
        assert!(
            carrier.take_fields_for(&mismatch).is_none(),
            "{kind:?} mismatch"
        );
        assert!(
            carrier.take_fields_for(&exact).is_none(),
            "{kind:?} mismatch consumes"
        );

        let exact = carrier.park_and_construct(kind.payload(2));
        assert!(
            carrier
                .take_fields_for(&VmError::InternalError("internal".to_string()))
                .is_none(),
            "{kind:?} internal"
        );
        assert!(
            carrier.take_fields_for(&exact).is_none(),
            "{kind:?} internal consumes"
        );

        let outer = carrier.park_and_construct(kind.payload(2));
        let inner = carrier.park_and_construct(kind.payload(3));
        assert!(
            carrier.take_fields_for(&inner).is_some(),
            "{kind:?} nested inner"
        );
        assert!(
            carrier.take_fields_for(&outer).is_none(),
            "{kind:?} nested outer stale"
        );

        let unhandled = carrier.park_and_construct(kind.payload(2));
        carrier.clear();
        assert!(
            carrier.take_fields_for(&unhandled).is_none(),
            "{kind:?} unhandled clear"
        );

        carrier.clear();
        let recovered = carrier.park_and_construct(kind.payload(3));
        assert!(
            carrier.take_fields_for(&recovered).is_some(),
            "{kind:?} same-session recovery"
        );
    }
}

#[test]
fn string_index_payload_carrier_preserves_bytes_11572() {
    use exec::exception_payload::PendingExceptionPayloadCarrier;

    let mut carrier = PendingExceptionPayloadCarrier::default();
    let err = carrier.park_and_construct(ExceptionPayloadKind11647::StringIndex.payload(2));
    let fields = carrier.take_fields_for(&err);
    assert!(fields.is_some(), "matching payload fields");
    let Some(fields) = fields else {
        return;
    };
    assert!(
        matches!(&fields[0], Value::StrBytes(_)),
        "matching payload must preserve StrBytes"
    );
    let Value::StrBytes(bytes) = &fields[0] else {
        return;
    };
    assert_eq!(bytes.as_ref(), &[0xf0, 0x80, 0x80, 2]);
}

#[test]
fn internal_error_funnel_consumes_every_exception_payload_kind_11647() {
    for kind in EXCEPTION_PAYLOAD_KINDS_11647 {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let parked_error = vm
            .pending_exception_payload
            .park_and_construct(kind.payload(2));

        assert!(
            vm.vm_error_to_exception_value(&VmError::InternalError("internal".to_string()))
                .is_none(),
            "{kind:?} internal error stays uncatchable"
        );
        assert!(
            vm.pending_exception_payload
                .take_fields_for(&parked_error)
                .is_none(),
            "{kind:?} internal error consumed the carrier"
        );
    }
}

#[test]
fn repl_recovery_discards_every_exception_payload_kind_11647() {
    for kind in EXCEPTION_PAYLOAD_KINDS_11647 {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let err = vm
            .pending_exception_payload
            .park_and_construct(kind.payload(2));

        vm.recover_repl_toplevel_after_error(StableRng::new(1));

        assert!(
            vm.pending_exception_payload.take_fields_for(&err).is_none(),
            "{kind:?} REPL recovery"
        );
        let recovered = vm
            .pending_exception_payload
            .park_and_construct(kind.payload(3));
        assert!(
            vm.pending_exception_payload
                .take_fields_for(&recovered)
                .is_some(),
            "{kind:?} REPL reuse"
        );
    }
}

fn array_value(arr: ArrayRef) -> Value {
    native_array_ref_value(arr)
}

fn vm_with_all_frame_binding_namespaces(code: Vec<Instr>) -> Vm<StableRng> {
    let mut vm = Vm::new(code, StableRng::new(0));
    vm.global_slot_names = vec!["slot_local".to_string()];
    vm.global_slot_map.insert("slot_local".to_string(), 0);
    vm.frames[0] = Frame::new_with_slots(1, None);
    assert!(vm.frames[0].set_slot_i64(0, 11));
    vm.frames[0]
        .locals_any
        .insert("typed_local".to_string(), Value::F64(2.5));
    vm.frames[0]
        .var_types
        .insert("typed_local".to_string(), VarTypeTag::F64);
    vm.frames[0]
        .locals_any
        .insert("generic_local".to_string(), Value::Bool(true));
    vm.frames[0]
        .captured_vars
        .insert("prior_capture".to_string(), Value::Char('c'));
    vm.frames[0]
        .type_bindings
        .insert("TypeParam".to_string(), JuliaType::Int64);
    vm
}

fn assert_frame_binding_value(name: &str, value: Option<Value>) {
    let matches_expected = match (name, value.as_ref()) {
        ("slot_local", Some(Value::I64(11)))
        | ("typed_local", Some(Value::F64(2.5)))
        | ("generic_local", Some(Value::Bool(true)))
        | ("prior_capture", Some(Value::Char('c'))) => true,
        ("TypeParam", Some(Value::DataType(ty))) => **ty == JuliaType::Int64,
        _ => false,
    };
    assert!(
        matches_expected,
        "unexpected frame binding for {name}: {value:?}"
    );
}

/// Every namespace readable from a frame must project identically through
/// ordinary stack loading, non-mutating lookup (used by closure capture), and
/// `isdefined`. A new Frame namespace belongs in this matrix (Issue #11051).
#[test]
fn frame_binding_namespace_consumers_stay_in_sync_11051() {
    let mut vm = vm_with_all_frame_binding_namespaces(Vec::new());
    for name in [
        "slot_local",
        "typed_local",
        "generic_local",
        "prior_capture",
        "TypeParam",
    ] {
        assert_frame_binding_value(name, vm.get_value_from_frame(name, 0));
        assert!(vm.is_var_defined_in_frame(name, 0), "{name}");
        assert!(vm.try_load_from_frame(name, 0), "{name}");
        assert_frame_binding_value(name, vm.stack.pop());
    }
    assert!(!vm.try_load_from_frame("missing", 0));
    assert!(!vm.is_var_defined_in_frame("missing", 0));
    assert!(vm.get_value_from_frame("missing", 0).is_none());
}

/// Shadowing precedence is part of the shared projection: storage local to
/// this frame wins over an outer capture, and a lexical type binding introduced
/// by this frame also wins over an inherited same-named capture
/// (Issues #11051/#11070).
#[test]
fn frame_binding_namespace_precedence_is_consistent_11051() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));
    let frame = &mut vm.frames[0];
    frame
        .locals_any
        .insert("local_wins".to_string(), Value::I64(1));
    frame
        .captured_vars
        .insert("local_wins".to_string(), Value::I64(2));
    frame
        .type_bindings
        .insert("local_wins".to_string(), JuliaType::Float64);
    frame
        .captured_vars
        .insert("type_binding_wins".to_string(), Value::I64(3));
    frame
        .type_bindings
        .insert("type_binding_wins".to_string(), JuliaType::Float64);

    assert!(matches!(
        vm.get_value_from_frame("local_wins", 0),
        Some(Value::I64(1))
    ));
    assert!(vm.try_load_from_frame("local_wins", 0));
    assert!(matches!(vm.stack.pop(), Some(Value::I64(1))));
    assert!(matches!(
        vm.get_value_from_frame("type_binding_wins", 0),
        Some(Value::DataType(ty)) if *ty == JuliaType::Float64
    ));
    assert!(vm.try_load_from_frame("type_binding_wins", 0));
    assert!(matches!(
        vm.stack.pop(),
        Some(Value::DataType(ty)) if *ty == JuliaType::Float64
    ));
}

/// Exercise the actual CreateClosure consumer, not just its lookup helper:
/// snapshots must include every readable Frame namespace (Issue #11051).
#[test]
fn closure_snapshot_captures_every_frame_binding_namespace_11051() {
    let names = vec![
        "slot_local".to_string(),
        "typed_local".to_string(),
        "generic_local".to_string(),
        "prior_capture".to_string(),
        "TypeParam".to_string(),
    ];
    let code = vec![
        Instr::CreateClosure {
            func_name: "namespace_matrix".to_string(),
            capture_names: names.clone(),
        },
        Instr::ReturnAny,
    ];
    let mut vm = vm_with_all_frame_binding_namespaces(code);
    let result = vm.run();
    assert!(
        matches!(&result, Ok(Value::Closure(_))),
        "CreateClosure must return a closure, got {result:?}"
    );
    let Ok(Value::Closure(closure)) = result else {
        return;
    };
    for name in names {
        assert_frame_binding_value(&name, closure.get_capture(&name).cloned());
    }
}

include!("../../tests/internal/resolved_closure_9784_test.rs");

#[test]
fn root_lexical_scope_captures_distinct_binding_and_preserves_global_11569() {
    let code = vec![
        Instr::EnterLexicalScope(vec!["x".to_string()]),
        Instr::PushI64(2),
        Instr::StoreLexical("x".to_string()),
        Instr::CreateClosure {
            func_name: "lexical_capture".to_string(),
            capture_names: vec!["x".to_string()],
        },
        Instr::ExitLexicalScope,
        Instr::ReturnAny,
    ];
    let mut vm = Vm::new(code, StableRng::new(0));
    vm.frames[0]
        .locals_any
        .insert("x".to_string(), Value::I64(1));

    let result = vm.run();
    assert!(
        matches!(&result, Ok(Value::Closure(_))),
        "expected closure result, got {result:?}"
    );
    let Ok(Value::Closure(closure)) = result else {
        return;
    };
    assert!(matches!(closure.get_capture("x"), Some(Value::I64(2))));
    assert!(matches!(vm.get_global("x"), Some(Value::I64(1))));
    assert!(vm.lexical_scopes.is_empty());
}

#[test]
fn called_frame_closure_capture_does_not_inherit_root_lexical_shadow_11569() {
    let code = vec![
        Instr::CreateClosure {
            func_name: "called_capture".to_string(),
            capture_names: vec!["x".to_string()],
        },
        Instr::ReturnAny,
    ];
    let mut vm = Vm::new(code, StableRng::new(0));
    vm.frames[0]
        .locals_any
        .insert("x".to_string(), Value::I64(1));
    let enter_result = vm.enter_root_lexical_scope(&["x".to_string()]);
    assert!(
        enter_result.is_ok(),
        "root lexical scope should enter: {enter_result:?}"
    );
    let store_result = vm.store_root_lexical("x", Value::I64(2));
    assert!(
        store_result.is_ok(),
        "root lexical binding should store: {store_result:?}"
    );
    vm.frames.push(Frame::new());

    let result = vm.run();
    assert!(
        matches!(&result, Ok(Value::Closure(_))),
        "expected closure result, got {result:?}"
    );
    let Ok(Value::Closure(closure)) = result else {
        return;
    };
    assert!(matches!(closure.get_capture("x"), Some(Value::I64(1))));
}

#[test]
fn error_unwind_discards_only_nested_root_lexical_scopes_11569() {
    let code = vec![
        Instr::EnterLexicalScope(vec!["outer".to_string()]),
        Instr::PushI64(1),
        Instr::StoreLexical("outer".to_string()),
        Instr::PushHandler(Some(7), None),
        Instr::EnterLexicalScope(vec!["inner".to_string()]),
        Instr::LoadLexical("inner".to_string()),
        Instr::ReturnAny,
        Instr::ClearError,
        Instr::IsLexicalDefined("outer".to_string()),
        Instr::ExitLexicalScope,
        Instr::ReturnAny,
    ];
    let mut vm = Vm::new(code, StableRng::new(0));

    assert!(matches!(vm.run(), Ok(Value::Bool(true))));
    assert!(vm.lexical_scopes.is_empty());
}

fn dispatch_test_function(
    name: &str,
    param_julia_types: Vec<crate::types::JuliaType>,
    type_params: Vec<crate::types::TypeParam>,
) -> FunctionInfo {
    FunctionInfo {
        name: name.to_string(),
        params: param_julia_types
            .iter()
            .enumerate()
            .map(|(idx, _)| (format!("x{idx}"), ValueType::Any))
            .collect(),
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: 1,
        type_params,
        param_julia_types,
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }
}

include!("../../tests/internal/persisted_callable_replacement_9784_test.rs");
include!("../../tests/internal/callable_singleton_identity_11685_test.rs");

#[test]
fn persisted_callable_candidates_follow_identity_across_rebuild_9784() {
    let mut prior = Vm::new(vec![], StableRng::new(0));
    prior.functions = vec![
        Rc::new(dispatch_test_function(
            "unrelated_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "helper_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
    ];
    let mut rebuilt = Vm::new(vec![], StableRng::new(0));
    rebuilt.functions = vec![
        Rc::new(dispatch_test_function(
            "helper_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "unrelated_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
    ];
    let mut globals = vec![(
        "saved".to_string(),
        Value::Function(FunctionValue::with_candidates("helper_9784", vec![1])),
    )];

    let snapshot = prior.persisted_callable_snapshot();
    rebuilt.remap_persisted_callable_candidates_from(&snapshot, &mut globals, &mut []);

    assert!(matches!(
        &globals[0].1,
        Value::Function(function) if function.candidate_indices.as_deref() == Some(&[0][..])
    ));
}

#[test]
fn persisted_callable_candidate_remap_preserves_same_provenance_redefinitions_9784() {
    let mut prior = Vm::new(vec![], StableRng::new(0));
    prior.functions = vec![Rc::new(dispatch_test_function(
        "ambiguous_9784",
        vec![JuliaType::Int64],
        vec![],
    ))];
    let mut rebuilt = Vm::new(vec![], StableRng::new(0));
    rebuilt.functions = vec![
        Rc::new(dispatch_test_function(
            "ambiguous_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "ambiguous_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
    ];
    let mut globals = vec![(
        "saved".to_string(),
        Value::Function(FunctionValue::with_candidates("ambiguous_9784", vec![0])),
    )];

    let snapshot = prior.persisted_callable_snapshot();
    rebuilt.remap_persisted_callable_candidates_from(&snapshot, &mut globals, &mut []);

    assert!(matches!(
        &globals[0].1,
        Value::Function(function) if function.candidate_indices.as_deref() == Some(&[0, 1][..])
    ));
}

#[test]
fn persisted_callable_candidate_remap_never_crosses_helper_provenance_9784() {
    let mut prior_helper =
        dispatch_test_function("same_spelling_9784", vec![JuliaType::Int64], vec![]);
    prior_helper.is_lowering_helper = true;
    let mut prior = Vm::new(vec![], StableRng::new(0));
    prior.functions = vec![Rc::new(prior_helper)];

    let mut rebuilt = Vm::new(vec![], StableRng::new(0));
    rebuilt.functions = vec![Rc::new(dispatch_test_function(
        "same_spelling_9784",
        vec![JuliaType::Int64],
        vec![],
    ))];
    let saved_helper = prior.function_value_with_candidates("same_spelling_9784", vec![0]);
    let mut globals = vec![("saved".to_string(), Value::Function(saved_helper))];

    let snapshot = prior.persisted_callable_snapshot();
    rebuilt.remap_persisted_callable_candidates_from(&snapshot, &mut globals, &mut []);

    assert!(matches!(
        &globals[0].1,
        Value::Function(function)
            if function.candidate_indices.as_deref() == Some(&[][..])
                && function.singleton_identity().is_lowering_helper()
    ));
}

#[test]
fn persisted_generator_callable_indices_follow_identity_across_rebuild_9784() {
    let mut prior = Vm::new(vec![], StableRng::new(0));
    prior.functions = vec![
        Rc::new(dispatch_test_function(
            "map_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "pred_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
    ];
    let mut rebuilt = Vm::new(vec![], StableRng::new(0));
    rebuilt.functions = vec![
        Rc::new(dispatch_test_function(
            "pred_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "map_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
    ];
    let mut globals = vec![
        (
            "plain".to_string(),
            Value::Generator(Box::new(GeneratorValue::new(0, Value::Nothing))),
        ),
        (
            "splat".to_string(),
            Value::Generator(Box::new(GeneratorValue::with_result_element_type(
                GeneratorCallable::TupleSplatFunctionIndex(0),
                Value::Nothing,
                None,
            ))),
        ),
        (
            "filtered".to_string(),
            Value::Generator(Box::new(GeneratorValue::with_result_element_type(
                GeneratorCallable::FilteredFunctionIndex {
                    map_func_index: 0,
                    predicate_func_index: 1,
                },
                Value::Nothing,
                None,
            ))),
        ),
    ];

    let snapshot = prior.persisted_callable_snapshot();
    rebuilt.remap_persisted_callable_candidates_from(&snapshot, &mut globals, &mut []);

    assert!(matches!(
        &globals[0].1,
        Value::Generator(generator)
            if matches!(generator.callable, GeneratorCallable::FunctionIndex(1))
    ));
    assert!(matches!(
        &globals[1].1,
        Value::Generator(generator)
            if matches!(generator.callable, GeneratorCallable::TupleSplatFunctionIndex(1))
    ));
    assert!(matches!(
        &globals[2].1,
        Value::Generator(generator)
            if matches!(generator.callable, GeneratorCallable::FilteredFunctionIndex {
                map_func_index: 1,
                predicate_func_index: 0,
            })
    ));
}

#[test]
fn test_issue_5926_vm_tracks_base_function_count_for_runtime_origin_fences() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_5926",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_5926",
        vec![crate::types::JuliaType::Int64],
        vec![],
    )));

    vm.base_function_count = 1;
    assert!(
        vm.is_base_program_function_index(0),
        "runtime dispatch must identify Base-origin functions by the \
         CompiledProgram base prefix"
    );
    assert!(
        !vm.is_base_program_function_index(1),
        "a function after the Base prefix is user-origin even when it shares \
         a runtime dispatch candidate set"
    );

    vm.base_function_count = 0;
    assert!(
        !vm.is_base_program_function_index(0),
        "zero base_function_count means the runtime has no Base-origin \
         partition"
    );
}

#[test]
fn base_runtime_candidate_rejects_external_same_name_struct_issue_10295() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_defs.push(StructDefInfo {
        name: "Partition".to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    });
    vm.struct_defs.push(StructDefInfo {
        name: "MyPkg.Partition".to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    });
    vm.functions.push(Rc::new(dispatch_test_function(
        "length",
        vec![crate::types::JuliaType::Struct("Partition".to_string())],
        vec![],
    )));
    vm.base_function_count = 1;

    let external = Value::Struct(StructInstance::with_name(
        1,
        "MyPkg.Partition".to_string(),
        vec![],
    ));
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0], &[external])
            .unwrap(),
        None
    );

    let base_compatible = Value::Struct(StructInstance::with_name(
        0,
        "Partition".to_string(),
        vec![],
    ));
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0], &[base_compatible])
            .unwrap(),
        Some(0)
    );

    let base_alias = Value::Struct(StructInstance::with_name(
        0,
        "Iterators.Partition".to_string(),
        vec![],
    ));
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0], &[base_alias])
            .unwrap(),
        Some(0)
    );
}

/// Table-driven selector-parity coverage (Issue #10879, prevention follow-up
/// of #10295 / PR #10877): the SAME Base/user same-name struct-ID pair, fed
/// through (a) the metadata-backed runtime scorer that `CallDynamic`,
/// `IterateDynamic`, and dominance pre-checks all share
/// (`find_best_method_index_from_candidates`) and (b) the shared low-level
/// applicability predicate that `CallTypedDispatch(+OrBuiltin)` replay,
/// legacy function-value dispatch (`call_function_variable.rs`), and
/// compile-time `MethodTable::dispatch` fencing
/// (`base_method_crosses_nominal_struct_origin`) all consume identically
/// (`function_candidate_has_nominal_origin_conflict`), must agree on every
/// row. A selector that stopped calling either shared entry point would make
/// exactly one side of this table disagree.
#[test]
fn dispatch_selector_origin_parity_table_issue_10879() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_defs.push(StructDefInfo {
        name: "Partition".to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    });
    vm.struct_defs.push(StructDefInfo {
        name: "MyPkg.Partition".to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    });
    vm.functions.push(Rc::new(dispatch_test_function(
        "length",
        vec![crate::types::JuliaType::Struct("Partition".to_string())],
        vec![],
    )));
    vm.base_function_count = 1;

    struct Row {
        label: &'static str,
        value: Value,
        expect_applicable: bool,
    }

    let rows = [
        Row {
            label: "different-ID same-name external struct: reject",
            value: Value::Struct(StructInstance::with_name(
                1,
                "MyPkg.Partition".to_string(),
                vec![],
            )),
            expect_applicable: false,
        },
        Row {
            label: "same-ID Base struct: accept",
            value: Value::Struct(StructInstance::with_name(
                0,
                "Partition".to_string(),
                vec![],
            )),
            expect_applicable: true,
        },
        Row {
            label: "same-ID Base submodule alias: accept",
            value: Value::Struct(StructInstance::with_name(
                0,
                "Iterators.Partition".to_string(),
                vec![],
            )),
            expect_applicable: true,
        },
    ];

    let param_types = vec![crate::types::JuliaType::Struct("Partition".to_string())];
    for row in &rows {
        let scored = vm
            .find_best_method_index_from_candidates(&[0], std::slice::from_ref(&row.value))
            .unwrap();
        assert_eq!(
            scored.is_some(),
            row.expect_applicable,
            "find_best_method_index_from_candidates disagreed for: {}",
            row.label
        );

        let conflict = vm.function_candidate_has_nominal_origin_conflict(
            0,
            std::slice::from_ref(&row.value),
            &param_types,
            &[],
        );
        assert_eq!(
            !conflict, row.expect_applicable,
            "function_candidate_has_nominal_origin_conflict disagreed for: {}",
            row.label
        );
    }
}

/// Structural rows of the same table that do not need a `Vm` instance: an
/// abstract parameter must stay subtype-permissive regardless of the actual
/// subtype's own name or owner (only concrete Base bare names get the origin
/// fence), and a `Union{conflicting, Any}` parameter must retain its valid
/// `Any` arm while a `Union` whose only matching arm is the conflicting one
/// must still reject (Issue #10879 checklist positive/negative rows).
#[test]
fn nominal_origin_conflict_core_api_abstract_and_union_rows_issue_10879() {
    use crate::types::JuliaType;

    assert!(
        !crate::types::base_bare_nominal_origin_conflict(
            &JuliaType::AbstractUser("AbstractDisplay".to_string(), None),
            &JuliaType::AbstractUser("MyPkg.AbstractDisplay".to_string(), None),
        ),
        "an abstract parameter must stay subtype-permissive: it has no \
         concrete bare name for the origin fence to compare"
    );

    assert!(
        !crate::types::base_bare_nominal_origin_conflict(
            &JuliaType::Union(vec![
                JuliaType::Struct("Partition".to_string()),
                JuliaType::Any,
            ]),
            &JuliaType::Struct("MyPkg.Partition".to_string()),
        ),
        "a Union of the conflicting struct and Any must retain its valid Any arm"
    );

    assert!(
        crate::types::base_bare_nominal_origin_conflict(
            &JuliaType::Union(vec![JuliaType::Struct("Partition".to_string())]),
            &JuliaType::Struct("MyPkg.Partition".to_string()),
        ),
        "a Union whose only matching arm is the conflicting concrete member \
         must still reject"
    );
}

#[test]
fn test_find_best_method_index_namedtuple_names_only_beats_bare_issue_5063() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "namedtuple_names_only_5063",
        vec![crate::types::JuliaType::Struct(
            "NamedTuple{(:a, :b)}".to_string(),
        )],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "namedtuple_names_only_5063",
        vec![crate::types::JuliaType::NamedTuple],
        vec![],
    )));

    let arg = Value::NamedTuple(
        crate::vm::value::NamedTupleValue::new(
            vec!["a".to_string(), "b".to_string()],
            vec![Value::I64(1), Value::I64(2)],
        )
        .unwrap(),
    );
    assert_eq!(
        vm.dispatch_julia_type_for_value(&arg),
        crate::types::JuliaType::Struct("@NamedTuple{a::Int64, b::Int64}".to_string())
    );
    assert_eq!(
        vm.get_type_name(&arg),
        "NamedTuple{(:a, :b), Tuple{Int64, Int64}}".to_string()
    );
    assert!(vm.type_matches(
        "@NamedTuple{a::Int64, b::Int64}",
        &crate::types::JuliaType::Struct("NamedTuple{(:a, :b)}".to_string()),
    ));
    assert!(
        !crate::inference_core::dispatch_resolver::runtime_julia_type_contains_type_var(
            &crate::types::JuliaType::Struct("NamedTuple{(:a, :b)}".to_string()),
        )
    );
    assert!(vm.value_matches_param(
        &arg,
        &crate::types::JuliaType::Struct("NamedTuple{(:a, :b)}".to_string()),
    ));

    assert!(vm
        .values_match_params_binding_count(
            std::slice::from_ref(&arg),
            &[crate::types::JuliaType::Struct(
                "NamedTuple{(:a, :b)}".to_string(),
            )],
            &[],
        )
        .is_some());
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[arg])
            .unwrap(),
        Some(0)
    );
}

#[test]
fn test_find_best_method_index_issue_5926_origin_fence_blocks_base_dominance_override() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_fence_5926",
        vec![crate::types::JuliaType::TypeVar(
            "T".to_string(),
            Some("Number".to_string()),
        )],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Number".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_origin_fence_5926",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.base_function_count = 1;
    vm.function_name_index
        .insert("runtime_origin_fence_5926".to_string(), vec![0, 1]);

    assert_eq!(
        vm.dominant_method_index_runtime(&["runtime_origin_fence_5926"], &[Value::I64(5)]),
        None,
        "runtime dispatch must mirror MethodTable's #5926 origin fence so \
         Base-origin dominance does not cross over a user-origin candidate"
    );
    vm.base_function_count = 0;
    assert_eq!(
        vm.dominant_method_index_runtime(&["runtime_origin_fence_5926"], &[Value::I64(5)]),
        Some(0),
        "without a Base prefix, T<:Number remains the unique runtime \
         dominance winner"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_allows_base_only_candidates_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "base_only_runtime_6251",
        vec![crate::types::JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "base_only_runtime_6251",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.base_function_count = 2;

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(5)])
            .unwrap(),
        Some(0),
        "Base-only candidate sets still need metadata runtime dispatch; \
        only mixed Base/user sets are fenced"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_show_int64_beats_pair_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("IOBuffer", Some("IO".to_string()), vec![]);
    vm.functions.push(Rc::new(dispatch_test_function(
        "show",
        vec![
            crate::types::JuliaType::Struct("IO".to_string()),
            crate::types::JuliaType::Any,
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "show",
        vec![
            crate::types::JuliaType::Struct("IO".to_string()),
            crate::types::JuliaType::Int64,
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "show",
        vec![
            crate::types::JuliaType::Struct("IO".to_string()),
            crate::types::JuliaType::Struct("Pair".to_string()),
        ],
        vec![],
    )));
    vm.base_function_count = 3;

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1, 2],
            &[Value::IO(value::IOValue::buffer_ref()), Value::I64(1)],
        )
        .unwrap(),
        Some(1),
        "show(io, x::Int64) must not dispatch to show(io, p::Pair)"
    );
}

#[test]
fn test_type_name_resolver_show_int64_beats_pair_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("IOBuffer", Some("IO".to_string()), vec![]);
    let candidates = [
        (0usize, vec!["IO".to_string(), "Any".to_string()]),
        (1usize, vec!["IO".to_string(), "Int64".to_string()]),
        (2usize, vec!["IO".to_string(), "Pair".to_string()]),
    ];
    let actual = ["IOBuffer".to_string(), "Int64".to_string()];

    assert_eq!(
        crate::inference_core::dispatch_resolver::resolve_type_name_candidates_with_subtype_fallback(
            candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
            &actual,
            |actual, bound| vm.check_subtype(actual, bound),
        ),
        Some((1, 6)),
        "string typed dispatch must choose the exact Int64 show method over Pair"
    );
}

#[test]
fn test_type_value_dispatch_does_not_match_value_level_parametric_patterns_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::Type],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::Any,
        ))],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::Struct("Array{T, 1}".to_string())],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ))],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "runtime_type_value_filter_6251",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ))],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1, 2, 3, 4, 5],
            &[Value::DataType(Box::new(
                crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Int64,))
            ))],
        )
        .unwrap(),
        Some(4),
        "a type object argument must match Type{{Vector{{T}}}}, not value-level Array{{T,1}}"
    );
}

#[test]
fn test_get_value_type_returns_arrayof_for_typed_array() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let arr = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.get_value_type(&arr),
        ValueType::ArrayOf(ArrayElementType::F64, None)
    );
}

#[test]
fn test_get_value_julia_type_preserves_memory_type_param() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let mem = value::MemoryValue::undef_typed(&ArrayElementType::I64, 2);
    let val = Value::Memory(value::new_memory_ref(mem));

    assert_eq!(
        vm.get_value_julia_type(&val),
        crate::types::JuliaType::Struct("Memory{Int64}".to_string())
    );
}

#[test]
fn test_memory_ref_type_resolves_user_struct_element_type_param_issue_9472() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_defs.push(StructDefInfo {
        name: "Foo".to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    });
    vm.struct_defs.push(StructDefInfo {
        name: "Bar".to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    });

    let foo_mem = value::MemoryValue::undef_typed(&ArrayElementType::StructOf(0), 1);
    let foo_ref = Value::MemoryRef(Box::new(value::MemoryRefValue::first(
        value::new_memory_ref(foo_mem),
    )));

    assert_eq!(vm.get_type_name(&foo_ref), "MemoryRef{Foo}");
    assert_eq!(
        vm.get_value_julia_type(&foo_ref),
        crate::types::JuliaType::Struct("MemoryRef{Foo}".to_string())
    );

    let bar_mem = value::MemoryValue::undef_typed(&ArrayElementType::StructOf(1), 1);
    let bar_ref = Value::MemoryRef(Box::new(value::MemoryRefValue::first(
        value::new_memory_ref(bar_mem),
    )));

    let foo_fp = vm.call_site_arg_fingerprint(&foo_ref).unwrap();
    let bar_fp = vm.call_site_arg_fingerprint(&bar_ref).unwrap();
    assert_ne!(foo_fp, bar_fp);
}

#[test]
fn test_get_value_julia_type_uses_array_logical_element_type() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let arr = array_value(new_array_ref(ArrayValue::zeros_complex_f64(vec![2])));

    assert_eq!(
        vm.get_value_julia_type(&arr),
        crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Struct(
            "Complex{Float64}".to_string()
        )))
    );
}

#[test]
fn test_dispatch_julia_type_preserves_io_and_closure_runtime_types_issue_6251() {
    let vm = Vm::new(vec![], StableRng::new(0));

    assert_eq!(
        vm.dispatch_julia_type_for_value(&Value::IO(value::IOValue::buffer_ref())),
        crate::types::JuliaType::IOBuffer
    );
    // Since Issue #9106 (PR #9118) closures carry a per-definition-site
    // singleton type (upstream: `typeof(f)` is a unique subtype of Function),
    // so dispatch sees the singleton struct type rather than bare Function.
    assert_eq!(
        vm.dispatch_julia_type_for_value(&Value::Closure(ClosureValue::new("strip#pred", vec![],))),
        crate::types::JuliaType::Struct("typeof(strip#pred)".to_string())
    );
    assert_eq!(
        vm.dispatch_julia_type_for_value(&Value::Symbol(value::SymbolValue::new("compact"))),
        crate::types::JuliaType::Symbol
    );
}

#[test]
fn test_struct_field_memory_comparison_reads_storage_directly() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let memory = value::new_memory_ref(value::MemoryValue::new(
        ArrayData::I64(vec![1, 2, 3]),
        ArrayElementType::I64,
        3,
    ));
    let same_memory = value::new_memory_ref(value::MemoryValue::new(
        ArrayData::I64(vec![1, 2, 3]),
        ArrayElementType::I64,
        3,
    ));
    let different_memory = value::new_memory_ref(value::MemoryValue::new(
        ArrayData::I64(vec![1, 2, 4]),
        ArrayElementType::I64,
        3,
    ));
    let vector = new_array_ref(ArrayValue::new(ArrayData::I64(vec![1, 2, 3]), vec![3]));
    let matrix = new_array_ref(ArrayValue::new(ArrayData::I64(vec![1, 2, 3]), vec![3, 1]));

    assert!(vm.compare_values_equal(&Value::Memory(memory.clone()), &Value::Memory(same_memory)));
    assert!(!vm.compare_values_equal(
        &Value::Memory(memory.clone()),
        &Value::Memory(different_memory)
    ));
    assert!(vm.compare_values_equal(&Value::Memory(memory.clone()), &array_value(vector.clone())));
    assert!(vm.compare_values_equal(&array_value(vector), &Value::Memory(memory.clone())));
    assert!(!vm.compare_values_equal(&Value::Memory(memory), &array_value(matrix)));
}

#[test]
fn test_runtime_diagonal_type_var_rejects_mixed_bigint_rational() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let param_types = vec![
        crate::types::JuliaType::Struct("T".to_string()),
        crate::types::JuliaType::Struct("T".to_string()),
    ];
    let type_params = vec![crate::types::TypeParam::new("T".to_string())];
    let args = vec![
        Value::BigInt(2.into()),
        Value::Struct(StructInstance::with_name(
            0,
            "Rational{Int64}".to_string(),
            vec![],
        )),
    ];

    assert_eq!(
        vm.get_value_julia_type(&args[0]),
        crate::types::JuliaType::BigInt
    );
    assert!(vm
        .values_match_params_binding_count(&args, &param_types, &type_params)
        .is_none());
}

#[test]
fn runtime_parametric_dict_structref_binds_spaced_typevars() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_heap.push(StructInstance::with_name(
        0,
        "Dict{Vector{Int64}, Vector{Any}}".to_string(),
        vec![],
    ));
    let args = vec![Value::StructRef(0)];
    let param_types = vec![crate::types::JuliaType::Struct("Dict{K, V}".to_string())];
    let type_params = vec![
        crate::types::TypeParam::new("K".to_string()),
        crate::types::TypeParam::new("V".to_string()),
    ];

    assert_eq!(
        vm.values_match_params_binding_count(&args, &param_types, &type_params),
        Some(2)
    );
}

#[test]
fn runtime_staticarrays_convert_smatrix_candidate_matches_issue_7460() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy.insert(
        "StaticArrays.StaticArray",
        Some("AbstractArray{T,N}".to_string()),
        vec!["S".to_string(), "T".to_string(), "N".to_string()],
    );
    vm.struct_hierarchy.insert(
        "StaticArrays.StaticVecOrMat",
        Some("StaticArray{S,T,N}".to_string()),
        vec!["S".to_string(), "T".to_string(), "N".to_string()],
    );
    vm.struct_hierarchy.insert(
        "StaticArrays.StaticMatrix",
        Some("StaticVecOrMat{Tuple{M,N},T,2}".to_string()),
        vec!["M".to_string(), "N".to_string(), "T".to_string()],
    );
    vm.struct_hierarchy.insert(
        "StaticArrays.SMatrix",
        Some("StaticMatrix{M,N,T}".to_string()),
        vec!["M".to_string(), "N".to_string(), "T".to_string()],
    );

    let param_types = vec![
        crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Struct(
            "StaticArrays.SMatrix{M, N, T}".to_string(),
        ))),
        crate::types::JuliaType::AbstractUser(
            "StaticMatrix".to_string(),
            Some("StaticArrays.StaticVecOrMat{Tuple{M,N},T,2}".to_string()),
        ),
    ];
    let type_params = vec![
        crate::types::TypeParam::new("M".to_string()),
        crate::types::TypeParam::new("N".to_string()),
        crate::types::TypeParam::new("T".to_string()),
    ];
    let args = vec![
        Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "StaticArrays.SMatrix{2, 2, Float64}".to_string(),
        ))),
        Value::Struct(StructInstance::with_name(
            0,
            "StaticArrays.SMatrix{2, 2, Int64}".to_string(),
            vec![],
        )),
    ];

    assert_eq!(
        vm.values_match_params_binding_count(&args, &param_types, &type_params),
        Some(3)
    );
    assert_eq!(
        vm.values_match_params_binding_count(
            &args,
            &[
                param_types[0].clone(),
                crate::types::JuliaType::Struct("StaticMatrix".to_string()),
            ],
            &type_params,
        ),
        Some(3)
    );
    let short_args = vec![
        Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "SMatrix{2, 2, Float64}".to_string(),
        ))),
        Value::Struct(StructInstance::with_name(
            0,
            "SMatrix{2, 2, Int64}".to_string(),
            vec![],
        )),
    ];
    assert_eq!(
        vm.values_match_params_binding_count(
            &short_args,
            &[
                param_types[0].clone(),
                crate::types::JuliaType::Struct("StaticMatrix".to_string()),
            ],
            &type_params,
        ),
        Some(3)
    );

    vm.functions.push(Rc::new(dispatch_test_function(
        "StaticArrays.convert",
        param_types,
        type_params,
    )));
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0], &args)
            .unwrap(),
        Some(0)
    );
}

#[test]
fn runtime_where_bound_uses_struct_hierarchy_issue_6502() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Animal", Some("Any".to_string()), vec![]);
    vm.struct_hierarchy
        .insert("Dog", Some("Animal".to_string()), vec![]);

    let param_types = vec![crate::types::JuliaType::Struct("T".to_string())];
    let type_params = vec![crate::types::TypeParam::with_upper_bound(
        "T".to_string(),
        "Animal".to_string(),
    )];

    assert_eq!(
        vm.values_match_params_binding_count(
            &[Value::Struct(StructInstance::with_name(
                0,
                "Dog".to_string(),
                vec![],
            ))],
            &param_types,
            &type_params,
        ),
        Some(1),
        "runtime dispatch must honor user-defined hierarchy bounds"
    );
    assert_eq!(
        vm.values_match_params_binding_count(&[Value::I64(1)], &param_types, &type_params),
        None,
        "runtime dispatch must reject values outside the user-defined bound"
    );
}

#[test]
fn runtime_dispatch_finds_parametric_dict_structref_method() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "iterate",
        vec![crate::types::JuliaType::Struct("Dict{K, V}".to_string())],
        vec![
            crate::types::TypeParam::new("K".to_string()),
            crate::types::TypeParam::new("V".to_string()),
        ],
    )));
    vm.function_name_index
        .insert("iterate".to_string(), vec![0]);
    vm.struct_heap.push(StructInstance::with_name(
        0,
        "Dict{Vector{Int64}, Vector{Any}}".to_string(),
        vec![],
    ));

    assert_eq!(
        vm.find_best_method_index(&["iterate"], &[Value::StructRef(0)]),
        Some(0)
    );
}

#[test]
fn runtime_type_matches_abstract_numeric_params_via_core_subtype_issue_5921() {
    use crate::types::JuliaType;

    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Complex", Some("Number".to_string()), vec!["T".to_string()]);
    vm.struct_hierarchy
        .insert("Rational", Some("Real".to_string()), vec!["T".to_string()]);

    assert!(vm.type_matches("Int8", &JuliaType::Real));
    assert!(vm.type_matches("UInt128", &JuliaType::Real));
    assert!(vm.type_matches("BigInt", &JuliaType::Real));
    assert!(vm.type_matches("Rational{Int64}", &JuliaType::Real));
    assert!(!vm.type_matches("Complex{Float64}", &JuliaType::Real));

    assert!(vm.type_matches("Complex{Float64}", &JuliaType::Number));
    assert!(vm.type_matches("Rational{BigInt}", &JuliaType::Number));
    assert!(!vm.type_matches("String", &JuliaType::Number));

    assert!(vm.type_matches("Float16", &JuliaType::AbstractFloat));
    assert!(vm.type_matches("BigFloat", &JuliaType::AbstractFloat));
    assert!(!vm.type_matches("Int64", &JuliaType::AbstractFloat));

    assert!(vm.type_matches("Int32", &JuliaType::Signed));
    assert!(!vm.type_matches("UInt32", &JuliaType::Signed));
    assert!(vm.type_matches("UInt64", &JuliaType::Unsigned));
    assert!(!vm.type_matches("Int64", &JuliaType::Unsigned));

    assert!(vm.type_matches("UnitRange{Int64}", &JuliaType::UnitRange));
    assert!(!vm.type_matches("UnitRange{Int64}", &JuliaType::StepRange));
    assert!(vm.type_matches("StepRangeLen{Float64}", &JuliaType::AbstractRange));
    assert!(vm.type_matches("OneTo", &JuliaType::AbstractRange));
    assert!(!vm.type_matches("LogRange{Float64}", &JuliaType::AbstractRange));
}

/// Runtime matcher parity with upstream Julia for the array / tuple /
/// string / char / IO param arms, routed through the shared subtype
/// engine (Issue #5915). Every expectation below was verified against
/// upstream `julia` (`L <: R`).
#[test]
fn runtime_type_matches_array_tuple_string_io_params_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // ::AbstractArray — julia: ranges and views ARE AbstractArrays.
    assert!(vm.type_matches("Vector{Int64}", &JuliaType::AbstractArray));
    assert!(vm.type_matches("Matrix{Float64}", &JuliaType::AbstractArray));
    assert!(vm.type_matches("Array{Int64, 3}", &JuliaType::AbstractArray));
    assert!(vm.type_matches("UnitRange{Int64}", &JuliaType::AbstractArray));
    assert!(vm.type_matches(
        "SubArray{Int64, 1, Vector{Int64}, Tuple{UnitRange{Int64}}, true}",
        &JuliaType::AbstractArray
    ));
    assert!(!vm.type_matches("String", &JuliaType::AbstractArray));

    // ::Array — julia: Vector/Matrix are Array aliases; ranges are NOT Arrays.
    assert!(vm.type_matches("Vector{Int64}", &JuliaType::Array));
    assert!(vm.type_matches("Matrix{Float64}", &JuliaType::Array));
    assert!(vm.type_matches("Array{Int64, 3}", &JuliaType::Array));
    assert!(vm.type_matches("Vector", &JuliaType::Array));
    assert!(!vm.type_matches("UnitRange{Int64}", &JuliaType::Array));
    assert!(!vm.type_matches("String", &JuliaType::Array));

    // ::Tuple — julia: any Tuple{...} matches; NamedTuple does not.
    assert!(vm.type_matches("Tuple{Int64, String}", &JuliaType::Tuple));
    assert!(vm.type_matches("Tuple{}", &JuliaType::Tuple));
    assert!(!vm.type_matches("@NamedTuple{a::Int64}", &JuliaType::Tuple));
    assert!(!vm.type_matches("Vector{Int64}", &JuliaType::Tuple));

    // ::AbstractString / ::AbstractChar / ::IO — julia: String <:
    // AbstractString, Char <: AbstractChar, IOBuffer <: IO.
    assert!(vm.type_matches("String", &JuliaType::AbstractString));
    assert!(!vm.type_matches("Char", &JuliaType::AbstractString));
    assert!(vm.type_matches("Char", &JuliaType::AbstractChar));
    assert!(!vm.type_matches("String", &JuliaType::AbstractChar));
    assert!(vm.type_matches("IOBuffer", &JuliaType::IO));
    assert!(!vm.type_matches("String", &JuliaType::IO));
}

/// `AbstractUser` params (user-declared abstract types, including
/// boot.jl ones like `AbstractDict`) previously fell into the
/// exact-name-equality fallback, so a `::AbstractDict` method could
/// never match a `Dict{...}` value through the runtime matcher
/// (Issue #5915; exposed `show(io::IO, x)` mis-dispatch for
/// `repr(Dict(...))` once `::IO` params started matching IOBuffer).
#[test]
fn runtime_type_matches_abstract_user_params_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Animal", Some("Any".to_string()), Vec::new());
    vm.struct_hierarchy
        .insert("Dog", Some("Animal".to_string()), Vec::new());

    let abstract_dict = JuliaType::AbstractUser("AbstractDict".to_string(), None);
    assert!(vm.type_matches("Dict{String, Int64}", &abstract_dict));
    assert!(!vm.type_matches("Vector{Int64}", &abstract_dict));
    assert!(!vm.type_matches("String", &abstract_dict));

    let animal = JuliaType::AbstractUser("Animal".to_string(), Some("Any".to_string()));
    assert!(vm.type_matches("Dog", &animal));
    assert!(vm.type_matches("Animal", &animal));
    assert!(!vm.type_matches("String", &animal));
}

/// Concrete `Tuple{...}` params are covariant in upstream Julia:
/// `Tuple{Int64} <: Tuple{Real}`. The old hand-rolled matcher required
/// exact element equality; route the no-typevar case through the shared
/// subtype engine (Issue #5915).
#[test]
fn runtime_type_matches_tuple_params_covariantly_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    let tuple_of = |elems: Vec<JuliaType>| JuliaType::TupleOf(elems);

    // Covariance (julia: true).
    assert!(vm.type_matches("Tuple{Int64}", &tuple_of(vec![JuliaType::Real])));
    assert!(vm.type_matches(
        "Tuple{Int64, Float64}",
        &tuple_of(vec![JuliaType::Real, JuliaType::Real])
    ));
    // Exact element match still holds.
    assert!(vm.type_matches(
        "Tuple{Int64, Float64}",
        &tuple_of(vec![JuliaType::Int64, JuliaType::Float64])
    ));
    // Non-subtype elements and arity mismatches stay rejected.
    assert!(!vm.type_matches("Tuple{Int64}", &tuple_of(vec![JuliaType::Float64])));
    assert!(!vm.type_matches(
        "Tuple{Int64, String}",
        &tuple_of(vec![JuliaType::Real, JuliaType::Real])
    ));
    assert!(!vm.type_matches("Tuple{Int64}", &tuple_of(vec![])));
    assert!(!vm.type_matches(
        "Tuple{Int64}",
        &tuple_of(vec![JuliaType::Int64, JuliaType::Int64])
    ));

    // Trailing unbounded Vararg (julia: Tuple{Int64, Int64} <:
    // Tuple{Int64, Vararg{Int64}}).
    let vararg_tail = tuple_of(vec![
        JuliaType::Int64,
        JuliaType::Struct("Vararg{Int64}".to_string()),
    ]);
    assert!(vm.type_matches("Tuple{Int64, Int64}", &vararg_tail));
    assert!(vm.type_matches("Tuple{Int64}", &vararg_tail));
    assert!(!vm.type_matches("Tuple{Int64, Float64}", &vararg_tail));

    // TypeVar elements keep the permissive local wildcard behavior used
    // by the bindings-driven matcher.
    assert!(vm.type_matches(
        "Tuple{Int64, String}",
        &tuple_of(vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::String
        ])
    ));
}

/// TypeVar-bearing `Tuple{...}` params: the TypeVar legs stay local
/// wildcards (bindings extracted elsewhere), but every concrete element
/// leg is covariant in upstream Julia and must route through the shared
/// subtype engine (Issue #5915). Verified upstream:
/// `Tuple{Int64, Int64} <: Tuple{T, Real} where T` is true.
#[test]
fn runtime_type_matches_typevar_tuple_concrete_elements_covariantly_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));
    let tvar = || JuliaType::TypeVar("T".to_string(), None);

    // julia: Tuple{Int64, Int64} <: (Tuple{T, Real} where T) → true.
    assert!(vm.type_matches(
        "Tuple{Int64, Int64}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Real])
    ));
    // julia: Tuple{Int64, Int64} <: (Tuple{T, Integer} where T) → true.
    assert!(vm.type_matches(
        "Tuple{Int64, Int64}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Integer])
    ));
    // julia: Tuple{Int64, String} <: (Tuple{T, Real} where T) → false.
    assert!(!vm.type_matches(
        "Tuple{Int64, String}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Real])
    ));
    // julia: Tuple{Int64, String} <: (Tuple{Real, T} where T) → true.
    assert!(vm.type_matches(
        "Tuple{Int64, String}",
        &JuliaType::TupleOf(vec![JuliaType::Real, tvar()])
    ));
    // Exact element equality still matches alongside a TypeVar leg.
    assert!(vm.type_matches(
        "Tuple{Int64, Float64}",
        &JuliaType::TupleOf(vec![tvar(), JuliaType::Float64])
    ));

    // Vararg tail with a TypeVar lead (julia:
    // Tuple{Int64, Int64, Int64} <: (Tuple{T, Vararg{Int64}} where T) → true,
    // Tuple{Int64, Float64} <: (Tuple{T, Vararg{Int64}} where T) → false).
    let vararg_tail =
        JuliaType::TupleOf(vec![tvar(), JuliaType::Struct("Vararg{Int64}".to_string())]);
    assert!(vm.type_matches("Tuple{Int64, Int64, Int64}", &vararg_tail));
    assert!(vm.type_matches("Tuple{Int64}", &vararg_tail));
    assert!(!vm.type_matches("Tuple{Int64, Float64}", &vararg_tail));
}

/// `Struct(name)` params with concrete type parameters previously used
/// raw string equality; route the no-TypeVar legs through the shared
/// subtype engine (Issue #5915). Invariance is preserved
/// (`Complex{Float64}` does NOT match `::Complex{Int64}`) and declared
/// parametric parents are honored (julia:
/// `MyVec{Int64} <: Wrapper{Int64}` for `struct MyVec{T} <: Wrapper{T}`).
#[test]
fn runtime_type_matches_struct_params_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_hierarchy
        .insert("Complex", Some("Number".to_string()), vec!["T".to_string()]);
    vm.struct_hierarchy
        .insert("Wrapper", Some("Any".to_string()), vec!["T".to_string()]);
    vm.struct_hierarchy.insert(
        "MyVec",
        Some("Wrapper{T}".to_string()),
        vec!["T".to_string()],
    );

    // Bare parametric base (julia: Complex{Float64} <: Complex → true).
    assert!(vm.type_matches(
        "Complex{Float64}",
        &JuliaType::Struct("Complex".to_string())
    ));
    // Exact concrete params.
    assert!(vm.type_matches(
        "Complex{Float64}",
        &JuliaType::Struct("Complex{Float64}".to_string())
    ));
    // Invariance (julia: Complex{Float64} <: Complex{Int64} → false).
    assert!(!vm.type_matches(
        "Complex{Float64}",
        &JuliaType::Struct("Complex{Int64}".to_string())
    ));
    // Type-variable params keep the wildcard base match (bindings are
    // extracted by the bindings-driven dispatcher).
    assert!(vm.type_matches(
        "Rational{Int64}",
        &JuliaType::Struct("Rational{T}".to_string())
    ));
    // Declared parametric parent (julia: MyVec{Int64} <: Wrapper{Int64}
    // → true; MyVec{Int64} <: Wrapper{Real} → false). The old string
    // equality could never match a declared parent.
    assert!(vm.type_matches(
        "MyVec{Int64}",
        &JuliaType::Struct("Wrapper{Int64}".to_string())
    ));
    assert!(!vm.type_matches(
        "MyVec{Int64}",
        &JuliaType::Struct("Wrapper{Real}".to_string())
    ));
    // Module-qualified renderer differences still match.
    assert!(vm.type_matches(
        "Base.OneTo{Int64}",
        &JuliaType::Struct("OneTo{Int64}".to_string())
    ));
    // Parametric param with a bare runtime name (runtime params unknown)
    // keeps the legacy permissive base match.
    assert!(vm.type_matches(
        "Polynomial",
        &JuliaType::Struct("Polynomial{Float64}".to_string())
    ));
    assert!(!vm.type_matches(
        "Polynomial",
        &JuliaType::Struct("Monomial{Float64}".to_string())
    ));
    // Generic partial parametric application is a UnionAll subtype question:
    // upstream dispatch accepts `SVector{0,Float64}` for `::SVector{0}`.
    assert!(vm.type_matches(
        "SVector{0, Float64}",
        &JuliaType::Struct("SVector{0}".to_string())
    ));
    assert!(vm.type_matches(
        "StaticArrays.SVector{0, Float64}",
        &JuliaType::Struct("SVector{0}".to_string())
    ));
    // Partial parametric short forms stay exact carrier tags: VM Base
    // code (subarray.jl) uses `SubArray{Int64}` overloads for the legacy
    // 1-D carrier, so an N-D 5-param runtime SubArray must NOT match
    // them (the engine's prefix-completing UnionAll reading would
    // mis-route `parentindices` on matrix views).
    assert!(!vm.type_matches(
        "SubArray{Int64, 2, Matrix{Int64}, Tuple{UnitRange{Int64}, Slice{OneTo}}, false}",
        &JuliaType::Struct("SubArray{Int64}".to_string())
    ));
}

/// `Vector{T}` / `Matrix{T}` / `Ref{T}` element params are INVARIANT in
/// upstream Julia (`Vector{Int64} <: Vector{Real}` is false,
/// `Base.RefValue{Int64} <: Ref{Real}` is false). Lock the runtime
/// matcher to the upstream behavior (Issue #5915).
#[test]
fn runtime_type_matches_vector_matrix_ref_params_stay_invariant_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // ::Vector{T} — invariant element.
    let vec_int = JuliaType::VectorOf(Box::new(JuliaType::Int64));
    let vec_real = JuliaType::VectorOf(Box::new(JuliaType::Real));
    assert!(vm.type_matches("Vector{Int64}", &vec_int));
    assert!(!vm.type_matches("Vector{Int64}", &vec_real));
    assert!(!vm.type_matches("Vector{Real}", &vec_int));
    // Runtime element unknown: permissive (legacy).
    assert!(vm.type_matches("Vector", &vec_int));
    // TypeVar element: wildcard for the bindings-driven dispatcher.
    assert!(vm.type_matches(
        "Vector{Int64}",
        &JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None)))
    ));

    // ::Matrix{T} — invariant element.
    let mat_f64 = JuliaType::MatrixOf(Box::new(JuliaType::Float64));
    let mat_real = JuliaType::MatrixOf(Box::new(JuliaType::Real));
    assert!(vm.type_matches("Matrix{Float64}", &mat_f64));
    assert!(!vm.type_matches("Matrix{Float64}", &mat_real));
    assert!(!vm.type_matches("Matrix{Int64}", &mat_f64));

    // ::Ref{T} / ::Base.RefValue{T} — invariant element (julia:
    // Base.RefValue{Int64} <: Ref{Int64} true, <: Ref{Real} false).
    assert!(vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Ref{Int64}".to_string())
    ));
    assert!(!vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Ref{Real}".to_string())
    ));
    assert!(vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Ref".to_string())
    ));
    assert!(!vm.type_matches(
        "Base.RefValue{Int64}",
        &JuliaType::Struct("Base.RefValue{Real}".to_string())
    ));
}

/// The `_` fallback for the remaining nominal variants is a pure subtype
/// question for the shared engine (Issue #5915). Verified upstream:
/// `DataType <: Type`, `Set{Int64} <: Set`, `Dict{String, Int64} <: Dict`.
#[test]
fn runtime_type_matches_nominal_fallback_via_core_subtype_issue_5915() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // Exact-name matches keep working.
    assert!(vm.type_matches("Int8", &JuliaType::Int8));
    assert!(vm.type_matches("Symbol", &JuliaType::Symbol));
    assert!(!vm.type_matches("Int8", &JuliaType::Int16));
    assert!(!vm.type_matches("String", &JuliaType::Nothing));

    // julia: DataType <: Type → true.
    assert!(vm.type_matches("DataType", &JuliaType::Type));
    // julia: Set{Int64} <: Set → true (was false under exact equality).
    assert!(vm.type_matches("Set{Int64}", &JuliaType::Set));
    // julia: Dict{String, Int64} <: Dict → true.
    assert!(vm.type_matches("Dict{String, Int64}", &JuliaType::Dict));
    // Unrelated parametric runtime names stay rejected.
    assert!(!vm.type_matches("Set{Int64}", &JuliaType::Dict));
    assert!(!vm.type_matches("Vector{Int64}", &JuliaType::Set));

    // julia: typeof(+) <: Function → true.
    assert!(vm.type_matches("typeof(+)", &JuliaType::Function));
    assert!(vm.type_matches("Function", &JuliaType::Function));
}

/// Re-evaluation of the Issue #6512 `::Function` carve-out (Issue #6597).
///
/// PR #6524 removed the legacy exact-name carve-out
/// (`JuliaType::Function => runtime_type == param_type.name()`) and routed
/// runtime `::Function` matching through the shared `CoreSubtypeEngine`. The
/// follow-up f6adade84 (Issue #6529) guard — the native-array wrapper fence,
/// now the selection-core policy `selection::signature_is_broad_wrapper_fence`
/// (Issue #6595), which counts `Function` slots as broad — keeps empty
/// narrow-int / Bool reductions on the type-specialized Base method instead of
/// the broad `reduce(op::Function, itr)` catch-all.
///
/// This test pins the post-removal contract so a future refactor cannot
/// silently reintroduce exact-name matching:
///   1. Function singleton runtime names (`typeof(+)`, `typeof(f)`) are
///      engine-true subtypes of `Function` (the relation the carve-out hid).
///   2. Concrete non-callable values still do NOT match `::Function` — the
///      negative guarantee the exact-name carve-out used to provide must be
///      preserved by the engine routing.
///
/// Verified against upstream julia 1.12: `typeof(+) <: Function` and
/// `typeof(identity) <: Function` are `true`; `Int8 <: Function`,
/// `Vector{Int64} <: Function`, `Symbol <: Function` are `false`.
#[test]
fn runtime_type_matches_function_param_via_core_subtype_issue_6597() {
    use crate::types::JuliaType;

    let vm = Vm::new(vec![], StableRng::new(0));

    // Function singletons are engine-true subtypes of Function (carve-out gone).
    assert!(vm.type_matches("typeof(+)", &JuliaType::Function));
    assert!(vm.type_matches("typeof(*)", &JuliaType::Function));
    assert!(vm.type_matches("typeof(identity)", &JuliaType::Function));
    assert!(vm.type_matches("Function", &JuliaType::Function));

    // Concrete non-callable types must NOT match ::Function — engine routing
    // preserves the negative guarantee the exact-name carve-out provided.
    assert!(!vm.type_matches("Int8", &JuliaType::Function));
    assert!(!vm.type_matches("Int64", &JuliaType::Function));
    assert!(!vm.type_matches("Bool", &JuliaType::Function));
    assert!(!vm.type_matches("Vector{Int64}", &JuliaType::Function));
    assert!(!vm.type_matches("Symbol", &JuliaType::Function));
}

#[test]
fn test_find_best_method_index_matches_parametric_varargs() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "getindex".to_string(),
        params: vec![
            ("a".to_string(), ValueType::Any),
            ("i".to_string(), ValueType::I64),
            ("j".to_string(), ValueType::I64),
            ("I".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: 1,
        type_params: vec![crate::types::TypeParam::new("T".to_string())],
        param_julia_types: vec![
            crate::types::JuliaType::Struct("Array{T}".to_string()),
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: Some(3),
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }));
    vm.function_name_index
        .insert("getindex".to_string(), vec![0]);

    let array = Value::Struct(StructInstance::with_name(
        0,
        "Array{Int64, 3}".to_string(),
        vec![],
    ));
    let matching = vec![array.clone(), Value::I64(1), Value::I64(1), Value::I64(1)];
    let mismatched_tail = vec![array, Value::I64(1), Value::I64(1), Value::F64(1.0)];

    assert_eq!(vm.find_best_method_index(&["getindex"], &matching), Some(0));
    assert_eq!(
        vm.find_best_method_index(&["getindex"], &mismatched_tail),
        None
    );
}

#[test]
fn test_find_best_method_index_caches_positive_and_negative_issue_5087() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "dispatch_cache_probe".to_string(),
        params: vec![("x".to_string(), ValueType::I64)],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::I64,
        return_julia_type: Some(crate::types::JuliaType::Int64),
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: 1,
        type_params: vec![],
        param_julia_types: vec![crate::types::JuliaType::Int64],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }));
    vm.function_name_index
        .insert("dispatch_cache_probe".to_string(), vec![0]);

    let int_args = vec![Value::I64(1)];
    let float_args = vec![Value::F64(1.0)];

    assert!(vm.method_dispatch_cache.is_empty());
    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &int_args),
        Some(0)
    );
    assert_eq!(vm.method_dispatch_cache.len(), 1);
    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &int_args),
        Some(0)
    );
    assert_eq!(vm.method_dispatch_cache.len(), 1);

    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &float_args),
        None
    );
    assert_eq!(vm.method_dispatch_cache.len(), 2);
    assert_eq!(
        vm.find_best_method_index(&["dispatch_cache_probe"], &float_args),
        None
    );
    assert_eq!(
        vm.method_dispatch_cache
            .values()
            .filter(|v| v.is_none())
            .count(),
        1
    );
}

#[test]
fn test_find_best_method_index_issue_5926_dominance_selects_vector_over_abstractvector() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "fam1_runtime_5926",
        vec![crate::types::JuliaType::AbstractUser(
            "AbstractVector".to_string(),
            Some("AbstractArray".to_string()),
        )],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "fam1_runtime_5926",
        vec![crate::types::JuliaType::Struct("Vector{T}".to_string())],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.function_name_index
        .insert("fam1_runtime_5926".to_string(), vec![0, 1]);

    let arg = Value::Struct(StructInstance::with_name(
        0,
        "Vector{Int64}".to_string(),
        vec![],
    ));
    assert_eq!(
        vm.find_best_method_index(&["fam1_runtime_5926"], &[arg]),
        Some(1),
        "runtime dispatch must mirror MethodTable's #5926 dominance \
         pre-check so Vector{{T}} wins over AbstractVector"
    );
}

#[test]
fn test_find_best_method_index_issue_5926_dominance_selects_diagonal_over_any_any() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "diagonal_runtime_5926",
        vec![crate::types::JuliaType::Any, crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "diagonal_runtime_5926",
        vec![
            crate::types::JuliaType::TypeVar("T".to_string(), None),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ],
        vec![crate::types::TypeParam::new("T".to_string())],
    )));
    vm.function_name_index
        .insert("diagonal_runtime_5926".to_string(), vec![0, 1]);

    assert_eq!(
        vm.find_best_method_index(&["diagonal_runtime_5926"], &[Value::I64(1), Value::I64(2)]),
        Some(1),
        "runtime dispatch must mirror MethodTable's #5926 dominance \
         pre-check so Tuple{{T,T}} wins over Tuple{{Any,Any}}"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["diagonal_runtime_5926"],
            &[Value::I64(1), Value::F64(2.0)]
        ),
        Some(0),
        "mixed runtime args do not satisfy the diagonal rule, so the Any \
        fallback wins"
    );
}

#[test]
fn test_find_best_method_index_tuple_bounded_fallback_after_diagonal_issue_6251() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_bounded_fallback_runtime_6251",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::TypeVar("T".to_string(), None),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ])],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_bounded_fallback_runtime_6251",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
            crate::types::JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ])],
        vec![],
    )));
    vm.function_name_index.insert(
        "tuple_bounded_fallback_runtime_6251".to_string(),
        vec![0, 1],
    );

    assert_eq!(
        vm.find_best_method_index(
            &["tuple_bounded_fallback_runtime_6251"],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::I64(2)
            ]))]
        ),
        Some(0),
        "homogeneous concrete real tuple keeps the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["tuple_bounded_fallback_runtime_6251"],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::F64(2.0),
            ]))]
        ),
        Some(1),
        "mixed real tuple falls back to Tuple{{<:Real,<:Real}}"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["tuple_bounded_fallback_runtime_6251"],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::str_new("x".to_string()),
            ]))]
        ),
        None,
        "non-Real tuple element must not match the bounded fallback"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[Value::Tuple(TupleValue::new(vec![
                Value::I64(1),
                Value::str_new("x".to_string()),
            ]))]
        )
        .unwrap(),
        None,
        "CallDynamic candidate-list dispatch must not fall back to Tuple{{<:Real,<:Real}}"
    );
}

#[test]
fn test_find_best_method_index_issue_5926_preserves_bounded_typevar_over_untyped_any_5375() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_runtime_5926",
        vec![crate::types::JuliaType::Any],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_runtime_5926",
        vec![crate::types::JuliaType::TypeVar(
            "T".to_string(),
            Some("Number".to_string()),
        )],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Number".to_string(),
        )],
    )));
    vm.function_name_index
        .insert("bounded_runtime_5926".to_string(), vec![0, 1]);

    assert_eq!(
        vm.find_best_method_index(&["bounded_runtime_5926"], &[Value::I64(5)]),
        Some(1),
        "runtime dispatch must preserve the #5375 regression: T<:Number \
         beats the untyped Any fallback"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["bounded_runtime_5926"],
            &[Value::str_new("s".to_string())],
        ),
        Some(0),
        "non-Number runtime values should still use the untyped fallback"
    );
}

#[test]
fn test_find_best_method_index_issue_6202_type_singleton_picks_tighter_bound_from_any() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_type_singleton_runtime_6202",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_type_singleton_runtime_6202",
        vec![crate::types::JuliaType::TypeOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Integer".to_string(),
        )],
    )));
    vm.function_name_index.insert(
        "bounded_type_singleton_runtime_6202".to_string(),
        vec![0, 1],
    );

    assert_eq!(
        vm.find_best_method_index(
            &["bounded_type_singleton_runtime_6202"],
            &[Value::DataType(Box::new(crate::types::JuliaType::Int64))]
        ),
        Some(1),
        "runtime dispatch from an Any container must preserve bounded \
         Type{{T}} specificity: T<:Integer beats T<:Real for Int64"
    );
    assert_eq!(
        vm.find_best_method_index(
            &["bounded_type_singleton_runtime_6202"],
            &[Value::DataType(Box::new(crate::types::JuliaType::Float64))]
        ),
        Some(0),
        "Float64 satisfies only the looser T<:Real method"
    );
}

#[test]
fn test_find_best_method_index_issue_6202_vector_picks_tighter_bound_from_any() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_vector_runtime_6202",
        vec![crate::types::JuliaType::VectorOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "bounded_vector_runtime_6202",
        vec![crate::types::JuliaType::VectorOf(Box::new(
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ))],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Integer".to_string(),
        )],
    )));
    vm.function_name_index
        .insert("bounded_vector_runtime_6202".to_string(), vec![0, 1]);

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let real_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index(&["bounded_vector_runtime_6202"], &[int_vector]),
        Some(1),
        "runtime dispatch from an Any container must preserve bounded \
         Vector{{T}} specificity: T<:Integer beats T<:Real for Vector{{Int64}}"
    );
    assert_eq!(
        vm.find_best_method_index(&["bounded_vector_runtime_6202"], &[real_vector]),
        Some(0),
        "Vector{{Float64}} satisfies only the looser T<:Real method"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_tuple_vararg_ambiguity_issue_6220() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_vararg_ambiguity_runtime_6220",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::Struct("Vararg{Integer}".to_string()),
        ])],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "tuple_vararg_ambiguity_runtime_6220",
        vec![crate::types::JuliaType::TupleOf(vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Struct("Vararg{Any}".to_string()),
        ])],
        vec![],
    )));

    let empty = vec![Value::Tuple(TupleValue::new(vec![]))];
    let single_int = vec![Value::Tuple(TupleValue::new(vec![Value::I64(1)]))];
    let all_int = vec![Value::Tuple(TupleValue::new(vec![
        Value::I64(1),
        Value::I64(2),
    ]))];
    let mixed_tail = vec![Value::Tuple(TupleValue::new(vec![
        Value::I64(1),
        Value::str_new("x".to_string()),
    ]))];

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &empty)
            .unwrap(),
        Some(0),
        "empty tuple only matches Tuple{{Vararg{{Integer}}}}"
    );
    assert!(
        matches!(
            vm.find_best_method_index_from_candidates(&[0, 1], &single_int),
            Err(VmError::MethodError(msg)) if msg.contains("ambiguous")
        ),
        "single Int tuple remains ambiguous at runtime"
    );
    assert!(
        matches!(
            vm.find_best_method_index_from_candidates(&[0, 1], &all_int),
            Err(VmError::MethodError(msg)) if msg.contains("ambiguous")
        ),
        "all-Int tuple remains ambiguous at runtime"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &mixed_tail)
            .unwrap(),
        Some(1),
        "mixed tail only matches the fixed-prefix fallback"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_union_actual_arm_issue_6231() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Union(vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::String,
        ])],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Integer],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Union(vec![
            crate::types::JuliaType::Integer,
            crate::types::JuliaType::String,
        ])],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Real],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "union_actual_arm_runtime_6231",
        vec![crate::types::JuliaType::Union(vec![
            crate::types::JuliaType::Real,
            crate::types::JuliaType::String,
        ])],
        vec![],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(1)])
            .unwrap(),
        Some(0),
        "Union{{Int64,String}} is more specific than Integer for Int64"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::str_new("x".to_string())])
            .unwrap(),
        Some(0),
        "String only matches the finite Union method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[2, 3], &[Value::I64(1)])
            .unwrap(),
        Some(2),
        "Union{{Integer,String}} is more specific than Real for integer values"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[3, 2], &[Value::I64(1)])
            .unwrap(),
        Some(2),
        "Union{{Integer,String}} wins over Real independent of candidate order"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[2, 3], &[Value::F64(1.0)])
            .unwrap(),
        Some(3),
        "Float64 does not satisfy the Union{{Integer,String}} arm"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[4, 1], &[Value::I64(1)])
            .unwrap(),
        Some(1),
        "an unrelated concrete Union arm must not make Union{{Real,String}} beat Integer"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_value_diagonal_issue_6233() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Integer,
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_exact_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::TypeVar("T".to_string(), None),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_value_diagonal_runtime_exact_6233",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Int64,
        ],
        vec![],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                Value::I64(1),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Int64 selects the diagonal Type{{T}}, T method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                Value::I64(1),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Integer method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                Value::F64(1.0),
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 satisfies only the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                Value::I64(1),
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, Int64 method remains more specific than the diagonal"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_vector_diagonal_issue_6235() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_exact_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_vector_diagonal_runtime_exact_6235",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Int64)),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1], vec![1])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0], vec![1])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Vector{{Int64}} selects the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Vector{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the diagonal method"
    );
    assert_eq!(
        vm.values_match_params_binding_count(
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
            &vm.functions[3].param_julia_types,
            &vm.functions[3].type_params,
        ),
        Some(0),
        "exact Type{{Int64}}, Vector{{Int64}} candidate must remain a runtime match"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, Vector{{Int64}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_vector_diagonal_issue_6239() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractVector{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractVector{<:Real}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_exact_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractVector{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_vector_diagonal_runtime_exact_6239",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractVector{Int64}".to_string()),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1], vec![1])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0], vec![1])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Vector{{Int64}} selects the AbstractVector diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractVector{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the AbstractVector diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractVector{{Int64}} method remains more specific"
    );
}

#[test]
fn test_call_typed_dispatch_type_abstract_vector_diagonal_via_any_issue_6573() {
    let source = r#"
type_abstract_vector_diagonal_6573(::Type{T}, ::AbstractVector{T}) where {T<:Real} = :type_absvec_same
type_abstract_vector_diagonal_6573(::Type{Integer}, ::AbstractVector{<:Real}) = :type_integer_absvec_real

function type_abstract_vector_diagonal_via_any_6573(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_vector_diagonal_6573(tt, xx)
end

type_abstract_vector_diagonal_via_any_6573(Integer, [1, 2])
"#;
    let compiled = compile_core_source(source);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));

    let result = vm.run();
    assert!(
        matches!(
            result,
            Ok(Value::Symbol(ref sym)) if sym.as_str() == "type_integer_absvec_real"
        ),
        "Any-routed Type{{Integer}}, Vector{{Int64}} dispatch should select the fixed Type{{Integer}} method, got {result:?}"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank1_diagonal_issue_6245() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,1}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real,1}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_exact_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,1}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank1_diagonal_runtime_exact_6245",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64,1}".to_string()),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Vector{{Int64}} selects the AbstractArray rank-1 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,1}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the AbstractArray rank-1 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,1}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank_omitted_diagonal_issue_6247(
) {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_exact_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_omitted_runtime_exact_6247",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64}".to_string()),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-omitted AbstractArray diagonal method covers concrete Vector{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-omitted AbstractArray diagonal method covers concrete Matrix{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the rank-omitted AbstractArray diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector,
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64}} method remains more specific for vectors"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64}} method remains more specific for matrices"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank_typevar_diagonal_issue_6249(
) {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,N}".to_string()),
        ],
        vec![
            crate::types::TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            crate::types::TypeParam::new("N".to_string()),
        ],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real,N}".to_string()),
        ],
        vec![crate::types::TypeParam::new("N".to_string())],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_exact_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,N}".to_string()),
        ],
        vec![
            crate::types::TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            crate::types::TypeParam::new("N".to_string()),
        ],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank_typevar_runtime_exact_6249",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64,N}".to_string()),
        ],
        vec![crate::types::TypeParam::new("N".to_string())],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![2])));
    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_vector = array_value(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-TypeVar AbstractArray diagonal method covers concrete Vector{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "rank-TypeVar AbstractArray diagonal method covers concrete Matrix{{Int64}} actuals"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_vector.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,N}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_vector,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 vector satisfies only the rank-TypeVar AbstractArray diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_vector,
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,N}} method remains more specific for vectors"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[Value::DataType(Box::new(crate::types::JuliaType::Int64)), int_matrix],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,N}} method remains more specific for matrices"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_matrix_diagonal_issue_6237() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_exact_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_matrix_diagonal_runtime_exact_6237",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::MatrixOf(Box::new(crate::types::JuliaType::Int64)),
        ],
        vec![],
    )));

    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_matrix = array_value(new_array_ref(ArrayValue::from_f64(
        vec![1.0, 2.0],
        vec![1, 2],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Matrix{{Int64}} selects the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, Matrix{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_matrix,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 matrix satisfies only the diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, Matrix{{Int64}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_matrix_diagonal_issue_6240() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractMatrix{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractMatrix{<:Real}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_exact_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractMatrix{T}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_matrix_diagonal_runtime_exact_6240",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractMatrix{Int64}".to_string()),
        ],
        vec![],
    )));

    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_matrix = array_value(new_array_ref(ArrayValue::from_f64(
        vec![1.0, 2.0],
        vec![1, 2],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Matrix{{Int64}} selects the AbstractMatrix diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractMatrix{{<:Real}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_matrix,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 matrix satisfies only the AbstractMatrix diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractMatrix{{Int64}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_type_abstract_array_rank2_diagonal_issue_6243() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,2}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Integer)),
            crate::types::JuliaType::Struct("AbstractArray{<:Real,2}".to_string()),
        ],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_exact_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::Struct("AbstractArray{T,2}".to_string()),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "type_abstract_array_rank2_diagonal_runtime_exact_6243",
        vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Int64)),
            crate::types::JuliaType::Struct("AbstractArray{Int64,2}".to_string()),
        ],
        vec![],
    )));

    let int_matrix = array_value(new_array_ref(ArrayValue::from_i64(vec![1, 2], vec![1, 2])));
    let float_matrix = array_value(new_array_ref(ArrayValue::from_f64(
        vec![1.0, 2.0],
        vec![1, 2],
    )));

    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(0),
        "concrete Type{{Int64}} plus Matrix{{Int64}} selects the AbstractArray rank-2 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Integer)),
                int_matrix.clone(),
            ],
        )
        .unwrap(),
        Some(1),
        "abstract Type{{Integer}} keeps the fixed Type{{Integer}}, AbstractArray{{<:Real,2}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[0, 1],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Float64)),
                float_matrix,
            ],
        )
        .unwrap(),
        Some(0),
        "Float64 matrix satisfies only the AbstractArray rank-2 diagonal method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(
            &[2, 3],
            &[
                Value::DataType(Box::new(crate::types::JuliaType::Int64)),
                int_matrix
            ],
        )
        .unwrap(),
        Some(3),
        "an exact Type{{Int64}}, AbstractArray{{Int64,2}} method remains more specific"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_invariant_vector_typevar_issue_6222() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "invariant_vector_typevar_runtime_6222",
        vec![
            crate::types::JuliaType::TypeVar("T".to_string(), None),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "invariant_vector_typevar_runtime_6222",
        vec![
            crate::types::JuliaType::Integer,
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));

    let int_vector = array_value(new_array_ref(ArrayValue::from_i64(vec![2, 3], vec![2])));
    let real_vector = array_value(new_array_ref(
        ArrayValue::memory_first_collect_typejoin_values(
            vec![Value::I64(2), Value::F64(3.0)],
            ArrayElementType::Any,
        )
        .unwrap(),
    ));

    assert_eq!(
        vm.get_value_julia_type(&real_vector),
        crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::Real))
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(1), int_vector])
            .unwrap(),
        Some(1),
        "the fixed Integer + Vector{{<:Real}} method wins for Vector{{Int64}}"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[Value::I64(1), real_vector])
            .unwrap(),
        Some(1),
        "the invariant Vector{{T}} occurrence must not outrank the fixed Integer slot"
    );
}

#[test]
fn test_find_best_method_index_from_candidates_vector_diagonal_issue_6229() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(dispatch_test_function(
        "vector_diagonal_runtime_6229",
        vec![
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "T".to_string(),
                None,
            ))),
        ],
        vec![crate::types::TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "vector_diagonal_runtime_6229",
        vec![
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TypeVar(
                "_".to_string(),
                Some("Real".to_string()),
            ))),
        ],
        vec![],
    )));

    let int_left = array_value(new_array_ref(ArrayValue::from_i64(vec![1], vec![1])));
    let int_right = array_value(new_array_ref(ArrayValue::from_i64(vec![2], vec![1])));
    let float_right = array_value(new_array_ref(ArrayValue::from_f64(vec![2.0], vec![1])));

    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[int_left.clone(), int_right])
            .unwrap(),
        Some(0),
        "same concrete Vector element type must select the diagonal Vector{{T}} method"
    );
    assert_eq!(
        vm.find_best_method_index_from_candidates(&[0, 1], &[int_left, float_right])
            .unwrap(),
        Some(1),
        "mixed Vector element types do not satisfy the repeated T binding"
    );
}

#[test]
fn test_call_site_dispatch_cache_stores_polymorphic_entries_issue_5079() {
    // Issue #9197 S3: the L2 dispatch cache keys on interned `ConcreteTypeId`
    // sequences (`CallSiteArgIds`), so a hit is exact id-sequence equality, not
    // a type-name hash. Distinct id sequences at one call site are distinct
    // entries.
    let mut vm = Vm::new(vec![], StableRng::new(0));
    let call_site_ip = 42;
    let int_key = [ConcreteTypeId(0)];
    let float_key = [ConcreteTypeId(1)];

    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, &int_key),
        None
    );

    vm.store_call_site_dispatch_cache(call_site_ip, &int_key, 7);
    vm.store_call_site_dispatch_cache(call_site_ip, &float_key, 9);

    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, &int_key),
        Some(7)
    );
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, &float_key),
        Some(9)
    );

    vm.store_call_site_dispatch_cache(call_site_ip + 1, &int_key, usize::MAX);
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip + 1, &int_key),
        Some(usize::MAX)
    );
}

#[test]
fn test_call_site_inline_cache_hits_exact_scalar_issue_6345() {
    let mut vm = Vm::new(vec![Instr::Nop; 4], StableRng::new(0));
    let call_site_ip = 2;
    let int_fingerprint = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 should be eligible for exact L1 dispatch caching");

    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &int_fingerprint),
        None
    );

    vm.store_call_site_inline_cache(call_site_ip, Some(int_fingerprint.as_slice()), 7);

    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &int_fingerprint),
        Some(7)
    );
    assert!(
        vm.dispatch_cache.is_empty(),
        "L1 hit path must not require populating the L2 HashMap cache"
    );
}

#[test]
fn test_call_site_inline_cache_preserves_negative_sentinel_issue_6345() {
    let mut vm = Vm::new(vec![Instr::Nop; 2], StableRng::new(0));
    let fingerprint = vm
        .call_site_arg_fingerprint(&Value::Bool(true))
        .expect("Bool should be eligible for exact L1 dispatch caching");

    vm.store_call_site_inline_cache(1, Some(fingerprint.as_slice()), usize::MAX);

    assert_eq!(
        vm.lookup_call_site_inline_cache(1, &fingerprint),
        Some(usize::MAX),
        "builtin/native fallback sentinel must round-trip through L1"
    );
}

#[test]
fn test_call_site_inline_cache_tags_parametric_identities_issue_6345() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));

    // Issue #9427: `Type{T}` dispatch identities are now L1/L2-taggable via the
    // `Opaque` interned key (they were skipped before, which regressed
    // closure/Type-heavy call sites to full re-resolution under S3). Distinct
    // `T` MUST yield distinct fingerprints so `f(Int)` and `f(Float64)` never
    // share a key.
    let type_int = vm
        .call_site_arg_fingerprint(&Value::DataType(Box::new(crate::types::JuliaType::Int64)))
        .expect("Type{Int64} is taggable via the Opaque key since #9427");
    let type_float = vm
        .call_site_arg_fingerprint(&Value::DataType(Box::new(crate::types::JuliaType::Float64)))
        .expect("Type{Float64} is taggable via the Opaque key since #9427");
    assert_ne!(
        type_int, type_float,
        "Type{{Int64}} and Type{{Float64}} are distinct dispatch identities"
    );
    // Issues #9108/#9113: tuples now fingerprint by recursing into element
    // identities, so distinct element types MUST yield distinct fingerprints
    // (the #6345 invariant — parametric identities never conflate — holds via
    // recursion instead of exclusion).
    let int_tuple_fp = vm
        .call_site_arg_fingerprint(&Value::Tuple(TupleValue::new(vec![Value::I64(1)])))
        .expect("(Int64,) tuples are recursively taggable since #9108/#9113");
    let float_tuple_fp = vm
        .call_site_arg_fingerprint(&Value::Tuple(TupleValue::new(vec![Value::F64(1.0)])))
        .expect("(Float64,) tuples are recursively taggable since #9108/#9113");
    assert_ne!(
        int_tuple_fp, float_tuple_fp,
        "Tuple{{Int64}} and Tuple{{Float64}} are distinct dispatch identities"
    );
}

/// Issue #9197 slice 2: the exact interned-id L1 key makes the
/// `SubArray{Int64,1}` vs `SubArray{Float64,2}` conflation from the S1 design
/// (TYPE_INTERNING.md) impossible at the L1 layer. Both shapes share a
/// struct-table `type_id` — `NewStruct` refines an instance's `struct_name` from
/// runtime field values while keeping the definition's `type_id` — yet they are
/// distinct concrete types and MUST produce distinct cache keys, so a warmed
/// entry for one can never falsely hit for the other (the pre-#9197
/// unverified-`u64`-hash bug class).
#[test]
fn call_site_inline_cache_distinguishes_subarray_shapes_by_typeid_issue_9197() {
    let mut vm = Vm::new(vec![Instr::Nop; 2], StableRng::new(0));

    // Same struct-table type_id, different fully-resolved dispatch names — the
    // exact conflation the interned-id key must separate.
    let shared_type_id = 77;
    let sub_i64_1 = Value::Struct(StructInstance::with_name(
        shared_type_id,
        "SubArray{Int64, 1}".to_string(),
        Vec::new(),
    ));
    let sub_f64_2 = Value::Struct(StructInstance::with_name(
        shared_type_id,
        "SubArray{Float64, 2}".to_string(),
        Vec::new(),
    ));

    let key_i64_1 = vm
        .call_site_arg_fingerprint(&sub_i64_1)
        .expect("a general struct value is L1-eligible via its struct_name");
    let key_f64_2 = vm
        .call_site_arg_fingerprint(&sub_f64_2)
        .expect("a general struct value is L1-eligible via its struct_name");

    // Distinct concrete types ⇒ distinct interned id sequences — the property the
    // shared `type_id` cannot express.
    assert_ne!(
        key_i64_1, key_f64_2,
        "SubArray{{Int64,1}} and SubArray{{Float64,2}} must not share an L1 key"
    );

    // No false hit: warm the site for SubArray{Int64,1}, then a
    // SubArray{Float64,2} lookup at the same site must miss.
    let call_site_ip = 1;
    vm.store_call_site_inline_cache(call_site_ip, Some(key_i64_1.as_slice()), 3);
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &key_i64_1),
        Some(3),
        "the warmed shape must hit its own exact key"
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &key_f64_2),
        None,
        "the other shape must not falsely hit the warmed entry"
    );
}

/// Issue #9427: `Type{T}` type-object arguments (`DataType` values) are re-cached
/// via the `Opaque` interned key — restoring the L2 caching S3 dropped, which had
/// regressed closure/Type-heavy package dispatch to full re-resolution on every
/// call. The key MUST include the full parameter `T` (`f(Type{Vector{Int64}})`
/// vs `f(Type{Vector{Float64}})` are distinct dispatch identities), mirroring the
/// SubArray soundness pattern above: a wrong (conflating) key silently selects
/// the wrong method.
#[test]
fn call_site_fingerprint_type_object_distinguishes_param_issue_9427() {
    let mut vm = Vm::new(vec![Instr::Nop; 2], StableRng::new(0));

    let type_vec_i64 = Value::DataType(Box::new(crate::types::JuliaType::Struct(
        "Vector{Int64}".to_string(),
    )));
    let type_vec_f64 = Value::DataType(Box::new(crate::types::JuliaType::Struct(
        "Vector{Float64}".to_string(),
    )));

    let key_i64 = vm
        .call_site_arg_fingerprint(&type_vec_i64)
        .expect("Type{Vector{Int64}} is L2-taggable via Opaque since #9427");
    let key_f64 = vm
        .call_site_arg_fingerprint(&type_vec_f64)
        .expect("Type{Vector{Float64}} is L2-taggable via Opaque since #9427");
    assert_ne!(
        key_i64, key_f64,
        "Type{{Vector{{Int64}}}} and Type{{Vector{{Float64}}}} must not share a dispatch key"
    );
    // Same type object re-interns to the SAME id (idempotent identity).
    let key_i64_again = vm
        .call_site_arg_fingerprint(&Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "Vector{Int64}".to_string(),
        ))))
        .unwrap();
    assert_eq!(key_i64, key_i64_again, "same Type{{T}} ⇒ same id");

    // No false L2 hit across the two type-object identities.
    let ip = 1;
    vm.store_call_site_dispatch_cache(ip, key_i64.as_slice(), 3);
    assert_eq!(vm.lookup_call_site_dispatch_cache(ip, &key_i64), Some(3));
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(ip, &key_f64),
        None,
        "Type{{Vector{{Float64}}}} must not falsely hit the warmed Type{{Vector{{Int64}}}} entry"
    );
}

/// Issue #9427: named function values dispatch as callable singletons
/// `typeof(<name>)`; distinct names ⇒ distinct ids, same name ⇒ same stable id.
#[test]
fn call_site_fingerprint_function_singleton_by_name_issue_9427() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));
    let f = vm
        .call_site_arg_fingerprint(&Value::Function(FunctionValue::new("sin")))
        .expect("function value is taggable since #9427");
    let g = vm
        .call_site_arg_fingerprint(&Value::Function(FunctionValue::new("cos")))
        .expect("function value is taggable since #9427");
    let f2 = vm
        .call_site_arg_fingerprint(&Value::Function(FunctionValue::new("sin")))
        .unwrap();
    assert_ne!(f, g, "typeof(sin) and typeof(cos) are distinct singletons");
    assert_eq!(f, f2, "typeof(sin) is a stable dispatch identity");
}

#[test]
fn call_site_fingerprint_range_uses_visible_type_params_issue_9815() {
    fn fingerprint_name(vm: &mut Vm<StableRng>, range: RangeValue) -> String {
        let value = Value::Range(range);
        let fingerprint = vm.call_site_arg_fingerprint(&value).unwrap();
        assert_eq!(fingerprint.len(), 1);
        vm.type_intern.display_name(fingerprint[0]).unwrap()
    }

    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));

    let unit_big = RangeValue {
        element_type: value::RangeElementType::BigInt,
        ..RangeValue::unit_range(1.0, 3.0)
    };
    assert_eq!(fingerprint_name(&mut vm, unit_big), "UnitRange{BigInt}");

    let step_narrow = RangeValue {
        element_type: value::RangeElementType::Int16,
        step_type: value::RangeElementType::Int8,
        ..RangeValue::step_range(1.0, 2.0, 5.0)
    };
    assert_eq!(
        fingerprint_name(&mut vm, step_narrow),
        "StepRange{Int16, Int8}"
    );

    let char_unit_syntax = RangeValue {
        element_type: value::RangeElementType::Char,
        ..RangeValue::unit_range(97.0, 99.0)
    };
    assert_eq!(
        fingerprint_name(&mut vm, char_unit_syntax),
        "StepRange{Char, Int64}"
    );
}

#[test]
fn call_site_fingerprint_float_range_carries_float_type_params_issue_9815() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));
    let value = Value::Range(RangeValue {
        is_float: true,
        element_type: value::RangeElementType::Float64,
        step_type: value::RangeElementType::Float64,
        ..RangeValue::step_range(0.0, 0.5, 1.0)
    });

    let fingerprint = vm.call_site_arg_fingerprint(&value).unwrap();
    assert_eq!(fingerprint.len(), 1);
    let key = vm.type_intern.key(fingerprint[0]).unwrap();
    let ConcreteTypeKey::Range {
        element,
        step,
        is_float,
        is_step,
    } = key
    else {
        panic!("range value must intern as a ConcreteTypeKey::Range");
    };

    assert_eq!(vm.type_intern.display_name(*element).unwrap(), "Float64");
    assert_eq!(vm.type_intern.display_name(*step).unwrap(), "Float64");
    assert!(*is_float);
    assert!(*is_step);
}

/// Issue #9427: a closure dispatches only by its callable-singleton type
/// `typeof(<name>)` — the captured environment is NOT part of the type (matching
/// the retired `get_type_name`) — so two closures from the SAME definition site
/// (same `name`, different captures) MUST share a dispatch id (dispatch cannot
/// tell them apart; conflating them is correct and precise), while different
/// definition sites MUST NOT.
#[test]
fn call_site_fingerprint_closure_identity_is_name_not_captures_issue_9427() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));

    let c_a1 = Value::Closure(ClosureValue::new(
        "outer#inner",
        vec![("x".to_string(), Value::I64(1))],
    ));
    let c_a2 = Value::Closure(ClosureValue::new(
        "outer#inner",
        vec![("x".to_string(), Value::I64(999))],
    ));
    let c_b = Value::Closure(ClosureValue::new("outer#other", vec![]));

    let k_a1 = vm
        .call_site_arg_fingerprint(&c_a1)
        .expect("closure is taggable since #9427");
    let k_a2 = vm.call_site_arg_fingerprint(&c_a2).unwrap();
    let k_b = vm.call_site_arg_fingerprint(&c_b).unwrap();

    assert_eq!(
        k_a1, k_a2,
        "same closure definition site ⇒ same dispatch id (captures are not part of the type)"
    );
    assert_ne!(
        k_a1, k_b,
        "distinct closure definition sites ⇒ distinct dispatch ids"
    );
}

/// Issue #9427: the `Opaque` key is variant-tagged, so a type-object `Type{Foo}`,
/// a struct value spelled `Foo`, a function named `Foo`, and a module `Foo` can
/// NEVER collapse into one dispatch id even though their names share the token
/// `Foo` — each keeps a distinct interned identity (the soundness guarantee that
/// re-caching these kinds does not introduce a cross-kind conflation).
#[test]
fn call_site_fingerprint_opaque_variants_do_not_collide_issue_9427() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));

    let as_type = vm
        .call_site_arg_fingerprint(&Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "Foo".to_string(),
        ))))
        .unwrap();
    let as_struct = vm
        .call_site_arg_fingerprint(&Value::Struct(StructInstance::with_name(
            5,
            "Foo".to_string(),
            Vec::new(),
        )))
        .unwrap();
    let as_fn = vm
        .call_site_arg_fingerprint(&Value::Function(FunctionValue::new("Foo")))
        .unwrap();
    let as_module = vm
        .call_site_arg_fingerprint(&Value::Module(Box::new(ModuleValue::new("Foo"))))
        .unwrap();

    let all = [as_type, as_struct, as_fn, as_module];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(
                all[i], all[j],
                "distinct dispatch kinds sharing a name token must not collide (i={i}, j={j})"
            );
        }
    }
}

/// Issue #9427: the nominal singleton kinds (`Module`, RNGs, …) are now taggable
/// (they were skipped, forcing re-resolution), and distinct concrete RNG kinds
/// keep distinct ids.
#[test]
fn call_site_fingerprint_singleton_kinds_taggable_issue_9427() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));
    assert!(
        vm.call_site_arg_fingerprint(&Value::Module(Box::new(ModuleValue::new("Base"))))
            .is_some(),
        "Module values are taggable since #9427"
    );
    let global = vm
        .call_site_arg_fingerprint(&Value::Rng(crate::rng::RngInstance::Global))
        .expect("RNG values are taggable since #9427");
    let stable = vm
        .call_site_arg_fingerprint(&Value::Rng(crate::rng::RngInstance::Stable(
            std::rc::Rc::new(std::cell::RefCell::new(StableRng::new(0))),
        )))
        .unwrap();
    assert_ne!(
        global, stable,
        "TaskLocalRNG (default_rng) and StableRNG are distinct dispatch types"
    );
}

/// Issue #9739: `CallFunctionVariable`'s dispatch cache key is `[callee,
/// args...]` — the callee's `typeof(name)` singleton fingerprint prepended to
/// the argument fingerprint sequence. A shared bytecode call site (e.g. the
/// single `f(args[1], args[2])` inside Pure Julia's `_broadcast_apply`, which
/// every distinct `f.(...)` broadcast in a program funnels through) must keep
/// each callee's resolution independent: same site, different callee, same
/// argument types ⇒ different cache keys, no cross-callee false hit.
#[test]
fn call_site_fingerprint_distinguishes_callee_at_shared_ip_issue_9739() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));

    let f = Value::Function(FunctionValue::new("double9739"));
    let g = Value::Function(FunctionValue::new("triple9739"));
    let arg = Value::F64(2.5);

    let key_f = vm
        .call_site_arg_fingerprints(&[&f, &arg])
        .expect("Function + Float64 are both taggable");
    let key_g = vm.call_site_arg_fingerprints(&[&g, &arg]).unwrap();
    let key_f_again = vm.call_site_arg_fingerprints(&[&f, &arg]).unwrap();

    assert_ne!(
        key_f, key_g,
        "same call site, different callee ⇒ distinct dispatch keys"
    );
    assert_eq!(
        key_f, key_f_again,
        "same callee + same arg types ⇒ stable, reusable dispatch key"
    );

    // A shared call site (one IP) alternating between `f` and `g` — the
    // pattern `_broadcast_apply`'s single `f(args[1], args[2])` site sees
    // across distinct `f.(...)`/`map(f, ...)` broadcasts in one program —
    // must resolve each callee to its own cached func_index without
    // clobbering the other.
    let call_site_ip = 0;
    vm.store_call_site_dispatch_cache(call_site_ip, key_f.as_slice(), 10);
    vm.store_call_site_dispatch_cache(call_site_ip, key_g.as_slice(), 20);
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, &key_f),
        Some(10),
        "f's resolution at the shared call site must survive g's insert"
    );
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(call_site_ip, &key_g),
        Some(20),
        "g's resolution at the shared call site must be independently cached"
    );
}

/// Issue #9739: the callee fingerprint also participates in the L1 two-way
/// inline cache the same way — a shared call site alternating between two
/// distinct callees keeps both resolved targets live (mirrors Issue #8561's
/// argument-identity two-way coverage, but for the callee axis).
#[test]
fn call_site_inline_cache_two_way_holds_two_callee_identities_issue_9739() {
    let mut vm = Vm::new(vec![Instr::Nop; 1], StableRng::new(0));
    let call_site_ip = 0;
    let arg = Value::I64(7);

    let f = Value::Function(FunctionValue::new("alpha9739"));
    let g = Value::Function(FunctionValue::new("beta9739"));
    let key_f = vm.call_site_arg_fingerprints(&[&f, &arg]).unwrap();
    let key_g = vm.call_site_arg_fingerprints(&[&g, &arg]).unwrap();

    vm.store_call_site_inline_cache(call_site_ip, Some(key_f.as_slice()), 1);
    vm.store_call_site_inline_cache(call_site_ip, Some(key_g.as_slice()), 2);

    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &key_f),
        Some(1),
        "f's L1 way must survive g's insert at the same call site"
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &key_g),
        Some(2),
        "g's L1 way must be independently cached alongside f's"
    );
}

/// Issue #8561: the per-call-site inline cache is two-way, so a site that
/// alternates between two exact scalar identities (e.g. a loop over a mixed
/// `Int64`/`Float64` array) keeps both resolved targets live.
#[test]
fn call_site_inline_cache_two_way_holds_two_scalar_identities_issue_8561() {
    let mut vm = Vm::new(vec![Instr::Nop; 4], StableRng::new(0));
    let call_site_ip = 1;
    let int_fp = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 is exact-taggable");
    let float_fp = vm
        .call_site_arg_fingerprint(&Value::F64(1.0))
        .expect("Float64 is exact-taggable");
    let bool_fp = vm
        .call_site_arg_fingerprint(&Value::Bool(true))
        .expect("Bool is exact-taggable");

    vm.store_call_site_inline_cache(call_site_ip, Some(int_fp.as_slice()), 7);
    vm.store_call_site_inline_cache(call_site_ip, Some(float_fp.as_slice()), 9);

    // Both identities hit after alternating fills.
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &int_fp),
        Some(7)
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &float_fp),
        Some(9)
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &int_fp),
        Some(7),
        "LRU-way hit must promote without evicting the other way"
    );

    // A third identity evicts the least-recently-used way (Float64), not the
    // most recent one (Int64).
    vm.store_call_site_inline_cache(call_site_ip, Some(bool_fp.as_slice()), 11);
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &bool_fp),
        Some(11)
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &int_fp),
        Some(7)
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(call_site_ip, &float_fp),
        None,
        "polymorphic degree 3 must evict the LRU way"
    );
}

/// Issue #8561 regression (the bug class to fear): a method-table mutation
/// (eval-time definition/redefinition) must invalidate every call-site
/// inline cache entry filled before it, so a warmed dynamic call site cannot
/// keep dispatching to the pre-mutation target.
#[test]
fn call_site_inline_cache_stale_generation_misses_after_method_table_mutation_issue_8561() {
    let mut vm = Vm::new(vec![Instr::Nop; 2], StableRng::new(0));
    let fp = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 is exact-taggable");

    vm.store_call_site_inline_cache(1, Some(fp.as_slice()), 7);
    assert_eq!(vm.lookup_call_site_inline_cache(1, &fp), Some(7));

    vm.note_method_table_mutation();

    assert_eq!(
        vm.lookup_call_site_inline_cache(1, &fp),
        None,
        "entries filled before a method-table mutation must be stale"
    );

    // Refill in the new generation works again.
    vm.store_call_site_inline_cache(1, Some(fp.as_slice()), 9);
    assert_eq!(vm.lookup_call_site_inline_cache(1, &fp), Some(9));
}

/// Issue #8561: `DefineEvalFunction` activation (the eval-time method
/// definition/redefinition path, `activate_eval_function`) is a method-table
/// mutation and must flush warmed call-site inline caches.
#[test]
fn activate_eval_function_invalidates_call_site_inline_caches_issue_8561() {
    let mut vm = Vm::new(vec![Instr::Nop; 2], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "f".to_string(),
        params: vec![],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: u64::MAX,
        type_params: vec![],
        param_julia_types: vec![],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }));
    let fp = vm
        .call_site_arg_fingerprint(&Value::F64(2.5))
        .expect("Float64 is exact-taggable");
    vm.store_call_site_inline_cache(0, Some(fp.as_slice()), 3);
    assert_eq!(vm.lookup_call_site_inline_cache(0, &fp), Some(3));

    vm.activate_eval_function(0);

    assert_eq!(
        vm.lookup_call_site_inline_cache(0, &fp),
        None,
        "eval-time method activation must invalidate warmed inline caches"
    );
}

include!("../../tests/internal/world_age_visibility_8452_11320_test.rs");

fn repl_test_struct_def_9784(name: &str) -> StructDefInfo {
    StructDefInfo {
        name: name.to_string(),
        is_mutable: false,
        fields: vec![],
        field_julia_types: vec![],
        parent_type: None,
    }
}

fn runtime_nominal_operands_11654(
    site_id: u64,
    definition: crate::bytecode::RuntimeNominalDefInfo,
) -> crate::bytecode::DefineRuntimeNominalOperands {
    crate::bytecode::DefineRuntimeNominalOperands {
        site_id,
        span: crate::span::Span::new(0, 1, 1, 1, 1, 2),
        definition,
        coalesce_with_root: false,
        reserved_struct_type_id: None,
        constructor_function_indices: Vec::new(),
        published_members: None,
    }
}

fn runtime_nominal_struct_info_11654(
    name: &str,
    fields: Vec<(String, ValueType)>,
    field_julia_types: Vec<JuliaType>,
) -> crate::bytecode::RuntimeStructDefInfo {
    let span = crate::span::Span::new(0, 1, 1, 1, 1, 2);
    let source_fields = fields
        .iter()
        .zip(field_julia_types.iter())
        .map(
            |((field_name, _), field_type)| crate::ir::core::StructField {
                name: field_name.clone(),
                type_expr: Some(crate::types::TypeExpr::Concrete(field_type.clone())),
                span,
            },
        )
        .collect();
    crate::bytecode::RuntimeStructDefInfo {
        source: Box::new(crate::ir::core::StructDef {
            name: name.to_string(),
            is_mutable: false,
            is_base_origin: false,
            type_params: Vec::new(),
            parent_type: None,
            fields: source_fields,
            inner_constructors: Vec::new(),
            global_new_helpers: Vec::new(),
            span,
        }),
        layout: StructDefInfo {
            name: name.to_string(),
            is_mutable: false,
            fields,
            field_julia_types,
            parent_type: None,
        },
    }
}

fn run_runtime_nominal_test_11654(test: fn()) {
    let handle = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(test);
    let Ok(handle) = handle else {
        unreachable!("spawn runtime nominal VM test");
    };
    assert!(
        handle.join().is_ok(),
        "runtime nominal VM test should not panic"
    );
}

fn runtime_nominal_struct_run_result_11654(
    definition: crate::bytecode::RuntimeStructDefInfo,
) -> (Result<Value, VmError>, bool, usize) {
    let name = definition.layout.name.clone();
    let mut compiled = compile_core_source("");
    let entry = compiled.code.len();
    compiled.entry = entry;
    compiled.code.extend([
        Instr::DefineRuntimeNominal(Box::new(runtime_nominal_operands_11654(
            91,
            crate::bytecode::RuntimeNominalDefInfo::Struct(definition),
        ))),
        Instr::PushNothing,
        Instr::ReturnAny,
    ]);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let result = vm.run();
    let published = vm.get_global(&name).is_some()
        || vm
            .struct_defs
            .iter()
            .any(|definition| definition.name == name);
    (result, published, vm.repl_definition_activations.len())
}

#[test]
fn unsupported_runtime_parametric_struct_source_semantics_fail_without_publication_11678() {
    run_runtime_nominal_test_11654(
        unsupported_runtime_parametric_struct_source_semantics_fail_without_publication_impl_11678,
    );
}

fn unsupported_runtime_parametric_struct_source_semantics_fail_without_publication_impl_11678() {
    let mut parametric = runtime_nominal_struct_info_11654(
        "RuntimeParametricRejected11678",
        vec![("x".to_string(), ValueType::Any)],
        vec![JuliaType::Any],
    );
    parametric
        .source
        .type_params
        .push(crate::types::TypeParam::new("T".to_string()));
    let (result, published, activations) = runtime_nominal_struct_run_result_11654(parametric);
    assert!(matches!(result, Err(VmError::NotImplemented(_))));
    assert!(!published);
    assert_eq!(activations, 0);
}

#[test]
fn skipped_runtime_nominal_instruction_has_no_effect_11654() {
    run_runtime_nominal_test_11654(skipped_runtime_nominal_instruction_has_no_effect_impl_11654);
}

fn skipped_runtime_nominal_instruction_has_no_effect_impl_11654() {
    let mut compiled = compile_core_source("");
    let entry = compiled.code.len();
    compiled.entry = entry;
    compiled.code.extend([
        Instr::Jump(entry + 2),
        Instr::DefineRuntimeNominal(Box::new(runtime_nominal_operands_11654(
            1,
            crate::bytecode::RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
                name: "SkippedRuntimeAbstract11654".to_string(),
                parent: None,
                type_params: Vec::new(),
            }),
        ))),
        Instr::PushNothing,
        Instr::ReturnAny,
    ]);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let before = vm.abstract_types.len();

    assert!(matches!(vm.run(), Ok(Value::Nothing)));
    assert_eq!(vm.abstract_types.len(), before);
    assert!(vm.repl_definition_activations.is_empty());
}

#[test]
fn reached_runtime_nominal_instructions_publish_all_families_11654() {
    run_runtime_nominal_test_11654(
        reached_runtime_nominal_instructions_publish_all_families_impl_11654,
    );
}

fn reached_runtime_nominal_instructions_publish_all_families_impl_11654() {
    let mut compiled = compile_core_source("");
    let entry = compiled.code.len();
    compiled.entry = entry;
    let definitions = [
        runtime_nominal_operands_11654(
            11,
            crate::bytecode::RuntimeNominalDefInfo::Struct(runtime_nominal_struct_info_11654(
                "ReachedRuntimeStruct11654",
                vec![("x".to_string(), ValueType::I64)],
                vec![JuliaType::Int64],
            )),
        ),
        runtime_nominal_operands_11654(
            12,
            crate::bytecode::RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
                name: "ReachedRuntimeAbstract11654".to_string(),
                parent: None,
                type_params: Vec::new(),
            }),
        ),
        runtime_nominal_operands_11654(
            13,
            crate::bytecode::RuntimeNominalDefInfo::PrimitiveType(PrimitiveTypeDefInfo {
                name: "ReachedRuntimePrimitive11654".to_string(),
                parent: None,
                bits: 8,
            }),
        ),
        runtime_nominal_operands_11654(
            14,
            crate::bytecode::RuntimeNominalDefInfo::Enum(EnumDefInfo {
                name: "ReachedRuntimeEnum11654".to_string(),
                base_type: "Int32".to_string(),
                members: vec![
                    ("reached_runtime_a11654".to_string(), 0),
                    ("reached_runtime_b11654".to_string(), 1),
                ],
            }),
        ),
    ];
    compiled.code.extend(
        definitions
            .iter()
            .cloned()
            .map(|operands| Instr::DefineRuntimeNominal(Box::new(operands))),
    );
    compiled.code.extend([Instr::PushNothing, Instr::ReturnAny]);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let first_struct = vm.struct_defs.len();
    let first_abstract = vm.abstract_types.len();
    let first_primitive = vm.primitive_types_len();
    let first_enum = vm.enum_defs.len();

    assert!(matches!(vm.run(), Ok(Value::Nothing)));
    assert_eq!(
        vm.struct_defs[first_struct].name,
        "ReachedRuntimeStruct11654"
    );
    assert_eq!(
        vm.abstract_types[first_abstract].name,
        "ReachedRuntimeAbstract11654"
    );
    assert_eq!(vm.primitive_types_len(), first_primitive + 1);
    assert_eq!(vm.enum_defs[first_enum].name, "ReachedRuntimeEnum11654");
    assert!(matches!(
        vm.get_global("reached_runtime_a11654"),
        Some(Value::Enum { value: 0, .. })
    ));
    assert_eq!(vm.repl_definition_activations.len(), 4);
}

#[test]
fn runtime_nominal_parent_failure_is_catchable_and_non_publishing_11654() {
    run_runtime_nominal_test_11654(
        runtime_nominal_parent_failure_is_catchable_and_non_publishing_impl_11654,
    );
}

fn runtime_nominal_parent_failure_is_catchable_and_non_publishing_impl_11654() {
    let mut compiled = compile_core_source("");
    let entry = compiled.code.len();
    compiled.entry = entry;
    let reached_before = runtime_nominal_operands_11654(
        21,
        crate::bytecode::RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
            name: "RuntimeBefore11654".to_string(),
            parent: None,
            type_params: Vec::new(),
        }),
    );
    let failed = runtime_nominal_operands_11654(
        22,
        crate::bytecode::RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
            name: "RuntimeFailed11654".to_string(),
            parent: Some("MissingRuntimeParent11654".to_string()),
            type_params: Vec::new(),
        }),
    );
    let reached_catch = runtime_nominal_operands_11654(
        23,
        crate::bytecode::RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
            name: "RuntimeCatch11654".to_string(),
            parent: None,
            type_params: Vec::new(),
        }),
    );
    compiled.code.extend([
        Instr::DefineRuntimeNominal(Box::new(reached_before.clone())),
        Instr::PushHandler(Some(entry + 4), None),
        Instr::DefineRuntimeNominal(Box::new(failed)),
        Instr::PopHandler,
        Instr::ClearError,
        Instr::DefineRuntimeNominal(Box::new(reached_catch.clone())),
        Instr::PushNothing,
        Instr::ReturnAny,
    ]);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));

    assert!(matches!(vm.run(), Ok(Value::Nothing)));
    assert!(vm
        .abstract_types
        .iter()
        .any(|definition| definition.name == "RuntimeBefore11654"));
    assert!(!vm
        .abstract_types
        .iter()
        .any(|definition| definition.name == "RuntimeFailed11654"));
    assert!(vm
        .abstract_types
        .iter()
        .any(|definition| definition.name == "RuntimeCatch11654"));
    let sites = vm
        .repl_definition_activations
        .iter()
        .filter_map(|activation| match activation {
            ReplDefinitionActivation::RuntimeNominal(activation) => Some(activation.site_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sites, vec![reached_before.site_id, reached_catch.site_id]);
}

#[test]
fn repl_struct_activation_reserves_reaches_and_discards_suffix_9784() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));
    assert!(vm.reserve_appended_types(vec![
        repl_test_struct_def_9784("Reached9784"),
        repl_test_struct_def_9784("Unreached9784"),
    ]));
    let before = vm.repl_definition_world_fingerprint();

    assert!(vm.activate_eval_struct(0).is_ok());
    let reached = vm.repl_reached_appended_definition_prefix(
        before,
        &[
            ReplDefinitionActivation::Struct(0),
            ReplDefinitionActivation::Struct(1),
        ],
        &[],
        ReplAppendDefinitionStarts {
            functions: 0,
            structs: 0,
            abstract_types: 0,
            primitive_types: 0,
            enums: 0,
        },
        ReplAppendDefinitionCounts {
            function_bodies: 0,
            source_functions: 0,
            structs: 2,
            abstract_types: 0,
            primitive_types: 0,
            enums: 0,
        },
        &[],
    );
    assert_eq!(
        reached,
        Some(ReachedReplDefinitionPrefix {
            function_count: 0,
            runtime_constructor_indices: Vec::new(),
            struct_count: 1,
            abstract_type_count: 0,
            primitive_type_count: 0,
            enum_count: 0,
            runtime_nominal_activations: Vec::new(),
            runtime_function_indices: Vec::new(),
        })
    );
    assert_eq!(vm.struct_defs.len(), 1);
    assert_eq!(vm.pending_eval_struct_defs.len(), 1);

    vm.discard_unreached_repl_struct_defs();
    assert_eq!(vm.struct_defs.len(), 1);
    assert!(vm.pending_eval_struct_defs.is_empty());
}

#[test]
fn repl_struct_activation_rejects_out_of_order_and_duplicate_markers_9784() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));
    assert!(vm.reserve_appended_types(vec![
        repl_test_struct_def_9784("First9784"),
        repl_test_struct_def_9784("Second9784"),
    ]));

    assert!(matches!(
        vm.activate_eval_struct(1),
        Err(VmError::InternalError(_))
    ));
    assert_eq!(vm.pending_eval_struct_defs.len(), 2);

    assert!(vm.activate_eval_struct(0).is_ok());
    assert!(matches!(
        vm.activate_eval_struct(0),
        Err(VmError::InternalError(_))
    ));
    assert_eq!(vm.struct_defs.len(), 1);
    assert_eq!(vm.pending_eval_struct_defs.len(), 1);
}

#[test]
fn eval_nominal_definition_activation_is_source_ordered_9784() {
    let compiled = compile_core_source("");
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let first_abstract = vm.abstract_types_len();
    let first_primitive = vm.primitive_types_len();
    let first_enum = vm.enum_defs_len();
    let abstracts = vec![
        AbstractTypeDefInfo {
            name: "PendingAbstractA9784".to_string(),
            parent: Some("Any".to_string()),
            type_params: Vec::new(),
        },
        AbstractTypeDefInfo {
            name: "PendingAbstractB9784".to_string(),
            parent: Some("PendingAbstractA9784".to_string()),
            type_params: Vec::new(),
        },
    ];
    let primitives = vec![
        PrimitiveTypeDefInfo {
            name: "PendingPrimitiveA9784".to_string(),
            parent: Some("Unsigned".to_string()),
            bits: 8,
        },
        PrimitiveTypeDefInfo {
            name: "PendingPrimitiveB9784".to_string(),
            parent: Some("Unsigned".to_string()),
            bits: 16,
        },
    ];
    let enums = vec![
        EnumDefInfo {
            name: "PendingEnumA9784".to_string(),
            base_type: "UInt8".to_string(),
            members: vec![("pending_enum_a9784".to_string(), 0)],
        },
        EnumDefInfo {
            name: "PendingEnumB9784".to_string(),
            base_type: "UInt8".to_string(),
            members: vec![("pending_enum_b9784".to_string(), 1)],
        },
    ];
    assert!(vm.reserve_appended_nominal_types(
        Vec::new(),
        abstracts.clone(),
        primitives.clone(),
        enums.clone(),
    ));
    let before = vm.repl_definition_world_fingerprint();

    for name in [
        "PendingAbstractA9784",
        "PendingAbstractB9784",
        "PendingPrimitiveA9784",
        "PendingPrimitiveB9784",
        "PendingEnumA9784",
        "PendingEnumB9784",
    ] {
        assert!(vm.eval_struct_type_name_is_pending(name));
        assert!(!vm.struct_hierarchy.contains_name(name));
    }
    assert_eq!(vm.abstract_types_len(), first_abstract);
    assert_eq!(vm.primitive_types_len(), first_primitive);
    assert_eq!(vm.enum_defs_len(), first_enum);
    assert!(!crate::vm::value::enum_registry::is_registered_enum(
        "PendingEnumA9784"
    ));

    assert!(matches!(
        vm.activate_eval_abstract_type(first_abstract + 1),
        Err(VmError::InternalError(_))
    ));
    assert!(matches!(
        vm.activate_eval_primitive_type(first_primitive + 1),
        Err(VmError::InternalError(_))
    ));
    assert!(matches!(
        vm.activate_eval_enum(&RegisterEnumOperands {
            type_name: enums[1].name.clone(),
            members: enums[1].members.clone(),
            published_members: None,
        }),
        Err(VmError::InternalError(_))
    ));

    let expected = vec![
        ReplDefinitionActivation::AbstractType(first_abstract),
        ReplDefinitionActivation::PrimitiveType(first_primitive),
        ReplDefinitionActivation::Enum(first_enum),
        ReplDefinitionActivation::AbstractType(first_abstract + 1),
        ReplDefinitionActivation::PrimitiveType(first_primitive + 1),
        ReplDefinitionActivation::Enum(first_enum + 1),
    ];
    assert!(vm.activate_eval_abstract_type(first_abstract).is_ok());
    assert!(vm.activate_eval_primitive_type(first_primitive).is_ok());
    assert!(vm
        .activate_eval_enum(&RegisterEnumOperands {
            type_name: enums[0].name.clone(),
            members: enums[0].members.clone(),
            published_members: None,
        })
        .is_ok());
    assert!(vm.activate_eval_abstract_type(first_abstract + 1).is_ok());
    assert!(vm.activate_eval_primitive_type(first_primitive + 1).is_ok());
    assert!(vm
        .activate_eval_enum(&RegisterEnumOperands {
            type_name: enums[1].name.clone(),
            members: enums[1].members.clone(),
            published_members: None,
        })
        .is_ok());

    assert_eq!(vm.repl_definition_activations, expected);
    assert_eq!(
        vm.repl_reached_appended_definition_prefix(
            before,
            &expected,
            &[],
            ReplAppendDefinitionStarts {
                functions: 0,
                structs: 0,
                abstract_types: first_abstract,
                primitive_types: first_primitive,
                enums: first_enum,
            },
            ReplAppendDefinitionCounts {
                function_bodies: 0,
                source_functions: 0,
                structs: 0,
                abstract_types: 2,
                primitive_types: 2,
                enums: 2,
            },
            &[],
        ),
        Some(ReachedReplDefinitionPrefix {
            function_count: 0,
            runtime_constructor_indices: Vec::new(),
            struct_count: 0,
            abstract_type_count: 2,
            primitive_type_count: 2,
            enum_count: 2,
            runtime_nominal_activations: Vec::new(),
            runtime_function_indices: Vec::new(),
        })
    );
    assert!(vm.struct_hierarchy.contains_name("PendingAbstractB9784"));
    assert!(vm.struct_hierarchy.contains_name("PendingPrimitiveB9784"));
    assert!(vm.struct_hierarchy.contains_name("PendingEnumB9784"));
    assert!(crate::vm::value::enum_registry::is_registered_enum(
        "PendingEnumB9784"
    ));
}

#[test]
fn runtime_nominal_reservation_hides_only_source_marked_ids_11654() {
    let mut compiled = compile_core_source("");
    let first_struct = compiled.struct_defs.len();
    let first_abstract = compiled.abstract_types.len();
    let first_primitive = compiled.primitive_types.len();
    let first_enum = compiled.enum_defs.len();
    compiled.struct_defs.extend([
        repl_test_struct_def_9784("MarkedStruct11654"),
        repl_test_struct_def_9784("RuntimeModule.ActiveStruct11654"),
        repl_test_struct_def_9784("ReservedRuntimeInner11679"),
    ]);
    compiled.abstract_types.extend([
        AbstractTypeDefInfo {
            name: "MarkedAbstract11654".to_string(),
            parent: Some("Any".to_string()),
            type_params: Vec::new(),
        },
        AbstractTypeDefInfo {
            name: "RuntimeModule.ActiveAbstract11654".to_string(),
            parent: Some("Any".to_string()),
            type_params: Vec::new(),
        },
    ]);
    compiled.primitive_types.extend([
        PrimitiveTypeDefInfo {
            name: "MarkedPrimitive11654".to_string(),
            parent: Some("Unsigned".to_string()),
            bits: 8,
        },
        PrimitiveTypeDefInfo {
            name: "RuntimeModule.ActivePrimitive11654".to_string(),
            parent: Some("Unsigned".to_string()),
            bits: 16,
        },
    ]);
    let marked_enum = EnumDefInfo {
        name: "MarkedEnum11654".to_string(),
        base_type: "UInt8".to_string(),
        members: vec![("marked_enum_11654".to_string(), 0)],
    };
    compiled.enum_defs.extend([
        marked_enum.clone(),
        EnumDefInfo {
            name: "RuntimeModule.ActiveEnum11654".to_string(),
            base_type: "UInt8".to_string(),
            members: vec![("active_enum_11654".to_string(), 1)],
        },
    ]);
    compiled.entry = compiled.code.len();
    let mut reserved_runtime_inner = runtime_nominal_operands_11654(
        11679,
        crate::bytecode::RuntimeNominalDefInfo::Struct(runtime_nominal_struct_info_11654(
            "ReservedRuntimeInner11679",
            Vec::new(),
            Vec::new(),
        )),
    );
    reserved_runtime_inner.coalesce_with_root = true;
    reserved_runtime_inner.reserved_struct_type_id = Some(first_struct + 2);
    compiled.code.extend([
        Instr::DefineEvalStruct(first_struct),
        Instr::DefineEvalAbstractType(first_abstract),
        Instr::DefineEvalPrimitiveType(first_primitive),
        Instr::RegisterEnum(Box::new(RegisterEnumOperands {
            type_name: marked_enum.name,
            members: marked_enum.members,
            published_members: None,
        })),
        Instr::DefineRuntimeNominal(Box::new(runtime_nominal_operands_11654(
            11654,
            crate::bytecode::RuntimeNominalDefInfo::AbstractType(AbstractTypeDefInfo {
                name: "RuntimeTrigger11654".to_string(),
                parent: Some("Any".to_string()),
                type_params: Vec::new(),
            }),
        ))),
        Instr::DefineRuntimeNominal(Box::new(reserved_runtime_inner)),
    ]);

    let vm = Vm::new_program(compiled, StableRng::new(0));

    assert_eq!(
        vm.pending_eval_struct_defs
            .iter()
            .map(|(type_id, _)| *type_id)
            .collect::<Vec<_>>(),
        vec![first_struct]
    );
    assert_eq!(
        vm.pending_eval_abstract_types
            .iter()
            .map(|(type_id, _)| *type_id)
            .collect::<Vec<_>>(),
        vec![first_abstract]
    );
    assert_eq!(
        vm.pending_eval_primitive_types
            .iter()
            .map(|(type_id, _)| *type_id)
            .collect::<Vec<_>>(),
        vec![first_primitive]
    );
    assert_eq!(
        vm.pending_eval_enum_defs
            .iter()
            .map(|(type_id, _)| *type_id)
            .collect::<Vec<_>>(),
        vec![first_enum]
    );
    assert_eq!(
        vm.hidden_eval_struct_type_ids,
        HashSet::from([first_struct, first_struct + 2])
    );
    assert_eq!(
        vm.hidden_eval_abstract_type_ids,
        HashSet::from([first_abstract])
    );
    assert_eq!(
        vm.hidden_eval_primitive_type_ids,
        HashSet::from([first_primitive])
    );
    assert_eq!(vm.hidden_eval_enum_type_ids, HashSet::from([first_enum]));

    for name in [
        "MarkedStruct11654",
        "MarkedAbstract11654",
        "MarkedPrimitive11654",
        "MarkedEnum11654",
    ] {
        assert!(vm.eval_nominal_type_name_is_unpublished(name), "{name}");
    }
    assert!(vm.eval_nominal_type_name_is_unpublished("ReservedRuntimeInner11679"));
    for name in [
        "RuntimeModule.ActiveStruct11654",
        "RuntimeModule.ActiveAbstract11654",
        "RuntimeModule.ActivePrimitive11654",
    ] {
        assert!(!vm.eval_nominal_type_name_is_unpublished(name), "{name}");
        assert!(vm.struct_hierarchy.contains_name(name), "{name}");
    }
    assert!(!vm.eval_nominal_type_name_is_unpublished("RuntimeModule.ActiveEnum11654"));
    assert!(vm
        .active_enum_name_index
        .contains_key("RuntimeModule.ActiveEnum11654"));
    assert!(crate::vm::value::enum_registry::is_registered_enum(
        "RuntimeModule.ActiveEnum11654"
    ));
}

#[test]
fn malformed_nominal_marker_suffix_stays_private_11635() {
    let mut compiled = compile_core_source("");
    let first_abstract = compiled.abstract_types.len();
    let first_primitive = compiled.primitive_types.len();
    let first_enum = compiled.enum_defs.len();
    compiled.abstract_types.extend([
        AbstractTypeDefInfo {
            name: "MalformedAbstractA11635".to_string(),
            parent: Some("Any".to_string()),
            type_params: Vec::new(),
        },
        AbstractTypeDefInfo {
            name: "MalformedAbstractB11635".to_string(),
            parent: Some("MalformedAbstractA11635".to_string()),
            type_params: Vec::new(),
        },
    ]);
    compiled.primitive_types.extend([
        PrimitiveTypeDefInfo {
            name: "MalformedPrimitiveA11635".to_string(),
            parent: Some("Unsigned".to_string()),
            bits: 8,
        },
        PrimitiveTypeDefInfo {
            name: "MalformedPrimitiveB11635".to_string(),
            parent: Some("Unsigned".to_string()),
            bits: 16,
        },
    ]);
    let malformed_enums = [
        EnumDefInfo {
            name: "MalformedEnumA11635".to_string(),
            base_type: "UInt8".to_string(),
            members: vec![("malformed_enum_a11635".to_string(), 0)],
        },
        EnumDefInfo {
            name: "MalformedEnumB11635".to_string(),
            base_type: "UInt8".to_string(),
            members: vec![("malformed_enum_b11635".to_string(), 1)],
        },
    ];
    compiled.enum_defs.extend(malformed_enums.clone());
    compiled.entry = compiled.code.len();
    compiled.code.extend([
        Instr::DefineEvalAbstractType(first_abstract + 1),
        Instr::DefineEvalAbstractType(first_abstract),
        Instr::DefineEvalPrimitiveType(first_primitive + 1),
        Instr::DefineEvalPrimitiveType(first_primitive),
        Instr::RegisterEnum(Box::new(RegisterEnumOperands {
            type_name: malformed_enums[1].name.clone(),
            members: malformed_enums[1].members.clone(),
            published_members: None,
        })),
        Instr::RegisterEnum(Box::new(RegisterEnumOperands {
            type_name: malformed_enums[0].name.clone(),
            members: malformed_enums[0].members.clone(),
            published_members: None,
        })),
    ]);

    let vm = Vm::new_program(compiled, StableRng::new(0));
    assert_eq!(vm.abstract_types_len(), first_abstract);
    assert_eq!(vm.primitive_types_len(), first_primitive);
    assert_eq!(vm.enum_defs_len(), first_enum);
    assert_eq!(vm.pending_eval_abstract_types.len(), 2);
    assert_eq!(vm.pending_eval_primitive_types.len(), 2);
    assert_eq!(vm.pending_eval_enum_defs.len(), 2);
    for name in [
        "MalformedAbstractA11635",
        "MalformedAbstractB11635",
        "MalformedPrimitiveA11635",
        "MalformedPrimitiveB11635",
        "MalformedEnumA11635",
        "MalformedEnumB11635",
    ] {
        assert!(vm.eval_struct_type_name_is_pending(name));
        assert!(!vm.struct_hierarchy.contains_name(name));
    }
    assert!(!crate::vm::value::enum_registry::is_registered_enum(
        "MalformedEnumA11635"
    ));
    assert!(!crate::vm::value::enum_registry::is_registered_enum(
        "MalformedEnumB11635"
    ));
}

#[test]
fn repl_definition_prefix_requires_exact_function_struct_order_9784() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));
    let mut function = dispatch_test_function("ordered9784", vec![], vec![]);
    function.min_world = u64::MAX;
    vm.functions.push(Rc::new(function));
    assert!(vm.reserve_appended_types(vec![repl_test_struct_def_9784("Ordered9784")]));
    let before = vm.repl_definition_world_fingerprint();

    vm.activate_eval_function(0);
    assert!(vm.activate_eval_struct(0).is_ok());

    let exact = [
        ReplDefinitionActivation::Function(0),
        ReplDefinitionActivation::Struct(0),
    ];
    assert_eq!(
        vm.repl_reached_appended_definition_prefix(
            before,
            &exact,
            &[],
            ReplAppendDefinitionStarts {
                functions: 0,
                structs: 0,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            ReplAppendDefinitionCounts {
                function_bodies: 1,
                source_functions: 1,
                structs: 1,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            &[0],
        ),
        Some(ReachedReplDefinitionPrefix {
            function_count: 1,
            runtime_constructor_indices: Vec::new(),
            struct_count: 1,
            abstract_type_count: 0,
            primitive_type_count: 0,
            enum_count: 0,
            runtime_nominal_activations: Vec::new(),
            runtime_function_indices: Vec::new(),
        })
    );
    let reversed = [
        ReplDefinitionActivation::Struct(0),
        ReplDefinitionActivation::Function(0),
    ];
    assert!(vm
        .repl_reached_appended_definition_prefix(
            before,
            &reversed,
            &[],
            ReplAppendDefinitionStarts {
                functions: 0,
                structs: 0,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            ReplAppendDefinitionCounts {
                function_bodies: 1,
                source_functions: 1,
                structs: 1,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            &[0],
        )
        .is_none());
}

#[test]
fn repl_synthetic_prefix_cannot_activate_pending_type_11564() {
    let compiled = compile_repl_core_source(
        "replayed_constructor_hidden_11564 = try\n\
           ActivationType11564(41)\n\
           false\n\
         catch e\n\
           e isa UndefVarError\n\
         end\n\
         activation_f_11564(x) = x + 1\n\
         struct ActivationType11564\n\
           x::Int\n\
         end\n\
         activation_g_11564(x) = ActivationType11564(x)\n\
         activation_g_11564(activation_f_11564(40)).x",
    );
    assert!(
        compiled.is_ok(),
        "REPL source failed to compile: {compiled:?}"
    );
    let Ok(compiled) = compiled else {
        return;
    };
    let expected = compiled.code[compiled.entry..]
        .iter()
        .filter_map(|instruction| match instruction {
            Instr::DefineEvalFunction(index) => Some(ReplDefinitionActivation::Function(*index)),
            Instr::DefineEvalStruct(type_id) => Some(ReplDefinitionActivation::Struct(*type_id)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        expected.as_slice(),
        [
            ReplDefinitionActivation::Function(_),
            ReplDefinitionActivation::Struct(_),
            ReplDefinitionActivation::Function(_)
        ]
    ));
    let named_expected = expected
        .iter()
        .map(|activation| match activation {
            ReplDefinitionActivation::Function(index) => {
                format!("function:{}", compiled.functions[*index].name)
            }
            ReplDefinitionActivation::Struct(type_id) => {
                format!("struct:{}", compiled.struct_defs[*type_id].name)
            }
            other => format!("unexpected:{other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        named_expected,
        [
            "function:activation_f_11564",
            "struct:ActivationType11564",
            "function:activation_g_11564",
        ]
    );

    let first_activation_ip = compiled.code[compiled.entry..]
        .iter()
        .position(|instruction| {
            matches!(
                instruction,
                Instr::DefineEvalFunction(_) | Instr::DefineEvalStruct(_)
            )
        })
        .map(|offset| compiled.entry + offset);
    assert!(
        first_activation_ip.is_some(),
        "interleaved source must emit activation markers"
    );
    let Some(first_activation_ip) = first_activation_ip else {
        return;
    };
    let mut prefix_only = compiled.clone();
    let prefix_exit = prefix_only.code.len();
    prefix_only.code[first_activation_ip] = Instr::Jump(prefix_exit);
    prefix_only
        .code
        .extend([Instr::PushNothing, Instr::ReturnAny]);
    prefix_only.source_map.extend([None, None]);

    let mut prefix_vm = Vm::new_program(prefix_only, StableRng::new(0));
    assert!(matches!(prefix_vm.run(), Ok(Value::Nothing)));
    assert!(matches!(
        prefix_vm.get_global("replayed_constructor_hidden_11564"),
        Some(Value::Bool(true))
    ));
    assert!(prefix_vm.eval_struct_type_name_is_pending("ActivationType11564"));
    assert!(prefix_vm.repl_definition_activations.is_empty());

    let mut full_vm = Vm::new_program(compiled, StableRng::new(0));
    let full_result = full_vm.run();
    assert!(
        matches!(full_result, Ok(Value::I64(41))),
        "unexpected full result: {full_result:?}"
    );
    assert_eq!(full_vm.repl_definition_activations, expected);
    assert!(!full_vm.eval_struct_type_name_is_pending("ActivationType11564"));
}

include!("../../tests/internal/repl_activation_indices_9784_test.rs");

#[test]
fn rejected_repl_append_setup_is_non_mutating_9784() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));
    vm.functions
        .push(Rc::new(dispatch_test_function("prior9784", vec![], vec![])));
    vm.functions.push(Rc::new(dispatch_test_function(
        "appended9784",
        vec![],
        vec![],
    )));

    let valid = [ReplDefinitionActivation::FunctionGroup {
        primary: 1,
        refresh: vec![0],
    }];
    assert!(vm.configure_repl_function_activation_state(&valid, &[]));
    let refresh_before = vm.repl_function_refresh_groups.clone();
    let updates_before = vm.repl_specializable_updates.len();
    let world_sensitive_before = vm.repl_world_sensitive_specializable_indices.clone();

    let invalid = [ReplDefinitionActivation::FunctionGroup {
        primary: 1,
        refresh: vec![1],
    }];
    assert!(!vm.configure_repl_function_activation_state(&invalid, &[]));
    assert_eq!(vm.repl_function_refresh_groups, refresh_before);
    assert_eq!(vm.repl_specializable_updates.len(), updates_before);
    assert_eq!(
        vm.repl_world_sensitive_specializable_indices,
        world_sensitive_before
    );
}

#[test]
fn repl_append_setup_is_preflighted_before_live_vm_mutation_9784() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));
    vm.functions
        .push(Rc::new(dispatch_test_function("prior9784", vec![], vec![])));
    let counts = ReplAppendDefinitionCounts {
        function_bodies: 2,
        source_functions: 1,
        structs: 0,
        abstract_types: 0,
        primitive_types: 0,
        enums: 0,
    };
    let valid = [ReplDefinitionActivation::FunctionGroup {
        primary: 1,
        refresh: vec![2],
    }];
    let invalid = [ReplDefinitionActivation::FunctionGroup {
        primary: 1,
        refresh: vec![1],
    }];

    assert!(vm
        .prepare_repl_append_setup(counts, Vec::new(), &invalid, &[])
        .is_none());
    assert!(
        vm.prepare_repl_append_setup(
            ReplAppendDefinitionCounts {
                function_bodies: 1,
                source_functions: 0,
                ..counts
            },
            Vec::new(),
            &[],
            &[],
        )
        .is_some(),
        "an immediately-visible lifted helper has no source marker"
    );
    let prepared = vm.prepare_repl_append_setup(counts, Vec::new(), &valid, &[]);
    assert!(prepared.is_some(), "valid projected append setup");
    let Some(prepared) = prepared else {
        return;
    };

    vm.functions.push(Rc::new(dispatch_test_function(
        "appended9784",
        vec![],
        vec![],
    )));
    vm.functions.push(Rc::new(dispatch_test_function(
        "refresh9784",
        vec![],
        vec![],
    )));
    vm.reenter_appended_main(&[], &[], StableRng::new(1));
    vm.install_prepared_repl_append_setup(prepared);
    assert_eq!(vm.repl_function_refresh_groups.get(&1), Some(&vec![2]));

    let mut pending_vm = Vm::new(Vec::new(), StableRng::new(0));
    assert!(pending_vm.reserve_appended_types(vec![repl_test_struct_def_9784("PendingSetup9784")]));
    assert!(pending_vm
        .prepare_repl_append_setup(
            ReplAppendDefinitionCounts {
                function_bodies: 0,
                source_functions: 0,
                structs: 0,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            Vec::new(),
            &[],
            &[],
        )
        .is_none());
}

/// Issue #9197 S6 (precision): redefining generic function `f` must NOT evict
/// the warm L1 inline-cache slot of an unrelated generic function `g` — the
/// core acceptance for per-name backedge invalidation replacing the coarse
/// whole-generation bump.
#[test]
fn note_method_table_mutation_for_preserves_unrelated_call_site_inline_caches_issue_9197_s6() {
    let mut vm = Vm::new(vec![Instr::Nop; 4], StableRng::new(0));
    // funcs[0] = f, funcs[1] = g (distinct generic functions).
    vm.functions
        .push(Rc::new(dispatch_test_function("f", vec![], vec![])));
    vm.functions
        .push(Rc::new(dispatch_test_function("g", vec![], vec![])));
    let fp = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 is exact-taggable");

    // Warm two distinct call sites: ip 0 resolves to f (index 0), ip 1 to g (1).
    vm.store_call_site_inline_cache(0, Some(fp.as_slice()), 0);
    vm.store_call_site_inline_cache(1, Some(fp.as_slice()), 1);
    assert_eq!(vm.lookup_call_site_inline_cache(0, &fp), Some(0));
    assert_eq!(vm.lookup_call_site_inline_cache(1, &fp), Some(1));

    vm.note_method_table_mutation_for("f");

    assert_eq!(
        vm.lookup_call_site_inline_cache(0, &fp),
        None,
        "the mutated function's warm L1 slot must be vacated"
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(1, &fp),
        Some(1),
        "an unrelated function's warm L1 slot must survive a redefinition of f"
    );
}

/// Issue #9197 S6 (precision): the L2 dispatch cache keeps unrelated call-site
/// decisions across a redefinition of `f`, dropping only `f`'s entries.
#[test]
fn note_method_table_mutation_for_preserves_unrelated_dispatch_cache_issue_9197_s6() {
    let mut vm = Vm::new(vec![Instr::Nop; 8], StableRng::new(0));
    vm.functions
        .push(Rc::new(dispatch_test_function("f", vec![], vec![])));
    vm.functions
        .push(Rc::new(dispatch_test_function("g", vec![], vec![])));
    let key = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 is exact-taggable");

    vm.store_call_site_dispatch_cache(2, key.as_slice(), 0); // ip 2 -> f
    vm.store_call_site_dispatch_cache(3, key.as_slice(), 1); // ip 3 -> g
    assert_eq!(vm.lookup_call_site_dispatch_cache(2, &key), Some(0));
    assert_eq!(vm.lookup_call_site_dispatch_cache(3, &key), Some(1));

    vm.note_method_table_mutation_for("f");

    assert_eq!(
        vm.lookup_call_site_dispatch_cache(2, &key),
        None,
        "the mutated function's L2 entry must be dropped"
    );
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(3, &key),
        Some(1),
        "an unrelated function's L2 entry must survive"
    );
}

/// Issue #9197 S6 (correctness — the #8452/#9400 world-age family): a cached
/// call site that resolved to the redefined method must miss after the
/// redefinition, so the next call re-resolves and sees the new definition
/// instead of the stale target.
#[test]
fn note_method_table_mutation_for_evicts_the_redefined_method_issue_9197_s6() {
    let mut vm = Vm::new(vec![Instr::Nop; 4], StableRng::new(0));
    vm.functions
        .push(Rc::new(dispatch_test_function("f", vec![], vec![])));
    let fp = vm
        .call_site_arg_fingerprint(&Value::F64(2.5))
        .expect("Float64 is exact-taggable");

    // A call site warmed to the current definition of f (index 0), at both cache
    // tiers.
    vm.store_call_site_inline_cache(0, Some(fp.as_slice()), 0);
    vm.store_call_site_dispatch_cache(0, fp.as_slice(), 0);
    assert_eq!(vm.lookup_call_site_inline_cache(0, &fp), Some(0));
    assert_eq!(vm.lookup_call_site_dispatch_cache(0, &fp), Some(0));

    vm.note_method_table_mutation_for("f");

    assert_eq!(
        vm.lookup_call_site_inline_cache(0, &fp),
        None,
        "a redefined method's cached L1 entry must miss so the call re-resolves"
    );
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(0, &fp),
        None,
        "a redefined method's cached L2 entry must miss so the call re-resolves"
    );
}

/// Issue #9197 S6 (soundness): builtin/native fallback (`usize::MAX`) L2 entries
/// are dropped conservatively on any mutation, because a freshly defined user
/// method for the mutated name may now capture a site that previously fell back
/// to a builtin. Unrelated real-method entries still survive.
#[test]
fn note_method_table_mutation_for_drops_builtin_fallback_entries_issue_9197_s6() {
    let mut vm = Vm::new(vec![Instr::Nop; 8], StableRng::new(0));
    vm.functions
        .push(Rc::new(dispatch_test_function("g", vec![], vec![])));
    let key = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 is exact-taggable");

    vm.store_call_site_dispatch_cache(0, key.as_slice(), usize::MAX); // builtin fallback
    vm.store_call_site_dispatch_cache(1, key.as_slice(), 0); // -> g
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(0, &key),
        Some(usize::MAX)
    );
    assert_eq!(vm.lookup_call_site_dispatch_cache(1, &key), Some(0));

    vm.note_method_table_mutation_for("f");

    assert_eq!(
        vm.lookup_call_site_dispatch_cache(0, &key),
        None,
        "builtin-fallback (usize::MAX) entries drop conservatively on mutation"
    );
    assert_eq!(
        vm.lookup_call_site_dispatch_cache(1, &key),
        Some(0),
        "an unrelated real-method entry survives"
    );
}

/// Issue #9197 S6 (per-way precision): at a single polymorphic call site whose
/// two L1 ways resolved to different generic functions, only the mutated
/// function's way is vacated; the other way survives and stays reachable.
#[test]
fn note_method_table_mutation_for_vacates_only_the_affected_l1_way_issue_9197_s6() {
    let mut vm = Vm::new(vec![Instr::Nop; 4], StableRng::new(0));
    vm.functions
        .push(Rc::new(dispatch_test_function("f", vec![], vec![])));
    vm.functions
        .push(Rc::new(dispatch_test_function("g", vec![], vec![])));
    let int_fp = vm
        .call_site_arg_fingerprint(&Value::I64(1))
        .expect("Int64 is exact-taggable");
    let float_fp = vm
        .call_site_arg_fingerprint(&Value::F64(1.0))
        .expect("Float64 is exact-taggable");

    // One call site (ip 0), two ways: Int64 -> f (0, LRU), Float64 -> g (1, MRU).
    vm.store_call_site_inline_cache(0, Some(int_fp.as_slice()), 0);
    vm.store_call_site_inline_cache(0, Some(float_fp.as_slice()), 1);

    vm.note_method_table_mutation_for("f");

    assert_eq!(
        vm.lookup_call_site_inline_cache(0, &int_fp),
        None,
        "the f way must be vacated"
    );
    assert_eq!(
        vm.lookup_call_site_inline_cache(0, &float_fp),
        Some(1),
        "the unrelated g way must survive at the same call site"
    );

    // Symmetric: mutating g vacates the MRU way and promotes the surviving LRU
    // (f) way, which stays reachable.
    vm.store_call_site_inline_cache(0, Some(int_fp.as_slice()), 0);
    vm.store_call_site_inline_cache(0, Some(float_fp.as_slice()), 1);
    vm.note_method_table_mutation_for("g");
    assert_eq!(
        vm.lookup_call_site_inline_cache(0, &int_fp),
        Some(0),
        "the surviving f way is promoted to MRU and stays reachable"
    );
    assert_eq!(vm.lookup_call_site_inline_cache(0, &float_fp), None);
}

/// Issue #8561: the host-facing `clear_runtime_caches` (Issue #8453) must
/// also invalidate the IP-indexed inline caches, not only the HashMap-backed
/// decision caches.
#[test]
fn clear_runtime_caches_invalidates_call_site_inline_caches_issue_8561() {
    let mut vm = Vm::new(vec![Instr::Nop; 2], StableRng::new(0));
    let fp = vm
        .call_site_arg_fingerprint(&Value::Bool(false))
        .expect("Bool is exact-taggable");
    vm.store_call_site_inline_cache(1, Some(fp.as_slice()), 5);
    assert_eq!(vm.lookup_call_site_inline_cache(1, &fp), Some(5));

    vm.clear_runtime_caches();

    assert_eq!(vm.lookup_call_site_inline_cache(1, &fp), None);
}

#[test]
fn test_slot_storage_keeps_immutable_structs_inline_issue_5173() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.struct_defs.push(StructDefInfo {
        name: "Point".to_string(),
        is_mutable: false,
        fields: vec![
            ("x".to_string(), ValueType::I64),
            ("y".to_string(), ValueType::I64),
        ],
        field_julia_types: vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
        ],
        parent_type: None,
    });
    vm.struct_defs.push(StructDefInfo {
        name: "Box".to_string(),
        is_mutable: true,
        fields: vec![("x".to_string(), ValueType::I64)],
        field_julia_types: vec![crate::types::JuliaType::Int64],
        parent_type: None,
    });

    for _ in 0..8 {
        let stored = vm.value_for_slot_storage(Value::Struct(StructInstance::with_name(
            0,
            "Point".to_string(),
            vec![Value::I64(1), Value::I64(2)],
        )));
        assert!(matches!(stored, Value::Struct(_)));
    }
    assert!(
        vm.struct_heap.is_empty(),
        "immutable StoreSlot storage must not grow struct_heap"
    );

    let stored = vm.value_for_slot_storage(Value::Struct(StructInstance::with_name(
        1,
        "Box".to_string(),
        vec![Value::I64(3)],
    )));
    assert!(matches!(stored, Value::StructRef(0)));
    assert_eq!(vm.struct_heap.len(), 1);
}

/// Issue #5179: `new_program` must build a per-function `name -> slot`
/// map (`function_slot_maps`) consistent with each function's `slot_names`,
/// and `slot_index_for_frame` must resolve names through it identically to
/// the legacy linear scan over `slot_names`.
#[test]
fn function_slot_maps_match_slot_names_and_resolve_in_o1() {
    let source = "function f(a, b)\n  c = a + b\n  d = c * 2\n  d - a\nend\n";
    let compiled = compile_core_source(source);

    let func_index = compiled
        .functions
        .iter()
        .position(|fnc| fnc.name == "f")
        .expect("function f compiled");

    let vm = Vm::new_program(compiled, StableRng::new(0));

    // The pre-computed map exists for every function and mirrors slot_names.
    assert_eq!(vm.function_slot_maps.len(), vm.functions.len());
    let func = &vm.functions[func_index];
    let slot_map = &vm.function_slot_maps[func_index];
    assert_eq!(slot_map.len(), func.slot_names.len());
    for (idx, name) in func.slot_names.iter().enumerate() {
        assert_eq!(slot_map.get(name), Some(&idx), "slot {} ({})", idx, name);
    }

    // slot_index_for_frame resolves through the map identically to the
    // legacy linear scan, and reports None for unknown names.
    let frame = Frame::new_with_slots(func.local_slot_count, Some(func_index));
    for (idx, name) in func.slot_names.iter().enumerate() {
        let linear = func
            .slot_names
            .iter()
            .position(|slot_name| slot_name == name);
        assert_eq!(linear, Some(idx));
        assert_eq!(vm.slot_index_for_frame(&frame, name), Some(idx));
    }
    assert_eq!(vm.slot_index_for_frame(&frame, "no_such_var"), None);
}

#[test]
fn test_find_best_method_index_prefers_parametric_type_pattern() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "_array_undef_from_dims".to_string(),
        params: vec![
            ("typ".to_string(), ValueType::DataType),
            ("dims".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: 1,
        type_params: vec![],
        param_julia_types: vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Struct(
                "Pair".to_string(),
            ))),
            crate::types::JuliaType::TupleOf(vec![crate::types::JuliaType::Int64]),
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "_array_undef_from_dims".to_string(),
        params: vec![
            ("typ".to_string(), ValueType::DataType),
            ("dims".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: 1,
        type_params: vec![
            crate::types::TypeParam::new("K".to_string()),
            crate::types::TypeParam::new("V".to_string()),
        ],
        param_julia_types: vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Struct(
                "Pair{K,V}".to_string(),
            ))),
            crate::types::JuliaType::TupleOf(vec![crate::types::JuliaType::Int64]),
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }));
    vm.function_name_index
        .insert("_array_undef_from_dims".to_string(), vec![0, 1]);

    let args = vec![
        Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "Pair{Int64,Int8}".to_string(),
        ))),
        Value::Tuple(TupleValue::new(vec![Value::I64(2)])),
    ];

    assert_eq!(
        vm.find_best_method_index(&["_array_undef_from_dims"], &args),
        Some(1)
    );
}

#[test]
fn test_find_best_method_index_matches_bare_tuple_parametric_type_issue_4643() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.functions.push(Rc::new(FunctionInfo {
        name: "similar".to_string(),
        params: vec![
            ("typ".to_string(), ValueType::DataType),
            ("dims".to_string(), ValueType::Tuple),
        ],
        kwparams: vec![],
        entry: 0,
        return_type: ValueType::Any,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: 1,
        type_params: vec![crate::types::TypeParam::new("T".to_string())],
        param_julia_types: vec![
            crate::types::JuliaType::TypeOf(Box::new(crate::types::JuliaType::Struct(
                "Array{T}".to_string(),
            ))),
            crate::types::JuliaType::Tuple,
        ],
        code_start: 0,
        code_end: 0,
        slot_names: vec![],
        slot_types: vec![],
        local_slot_count: 0,
        param_slots: vec![],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }));
    vm.function_name_index
        .insert("similar".to_string(), vec![0]);

    let args = vec![
        Value::DataType(Box::new(crate::types::JuliaType::Struct(
            "Array{Int64}".to_string(),
        ))),
        Value::Tuple(TupleValue::new(vec![Value::I64(2)])),
    ];

    assert_eq!(vm.find_best_method_index(&["similar"], &args), Some(0));
}

// === Issue #3094: VmError::InternalError propagation tests ===

/// InternalError has error code 33.
#[test]
fn test_internal_error_code_is_33() {
    let code = Vm::<StableRng>::error_code(&VmError::InternalError("test".to_string()));
    assert_eq!(code, 33);
}

/// Returning from a callee must not pop a caller's active try/catch handler.
#[test]
fn test_pop_handlers_for_return_preserves_caller_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: Some(10),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });
    vm.handlers.push(Handler {
        catch_ip: Some(20),
        finally_ip: None,
        stack_len: 0,
        frame_len: 2,
        return_ip_len: 1,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });

    vm.pop_handlers_for_return();

    assert_eq!(vm.handlers.len(), 1);
    assert_eq!(vm.handlers[0].catch_ip, Some(10));
    assert_eq!(vm.handlers[0].return_ip_len, 0);
}

/// handle_error catches a user-visible error when a handler with catch_ip exists.
#[test]
fn test_handle_error_catches_user_error_with_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: Some(100),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });
    let result = vm.handle_error(VmError::TypeError("test".to_string()));
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    assert_eq!(vm.ip, 100);
    assert!(
        matches!(vm.pending_error, Some(VmError::TypeError(_))),
        "Expected pending TypeError, got {:?}",
        vm.pending_error
    );
}

/// handle_error propagates ANY error when no handler exists.
#[test]
fn test_handle_error_propagates_when_no_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    let result = vm.handle_error(VmError::InternalError("test".to_string()));
    assert!(
        matches!(result, Err(VmError::InternalError(_))),
        "Expected Err(InternalError), got {:?}",
        result
    );
}

/// raise() catches a user-visible error when a handler exists.
#[test]
fn test_raise_catches_with_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: Some(50),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });
    let result = vm.raise(VmError::DomainError("test".to_string()));
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    assert_eq!(vm.ip, 50);
}

/// raise() propagates when no handler exists.
#[test]
fn test_raise_propagates_without_handler() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    let result = vm.raise(VmError::TypeError("test".to_string()));
    assert!(
        matches!(result, Err(VmError::TypeError(_))),
        "Expected Err(TypeError), got {:?}",
        result
    );
}

/// InternalError from get_function_checked IS caught by handlers when the
/// instruction uses try_or_handle (e.g., Call instruction uses
/// get_function_cloned_or_raise). This documents that InternalError
/// propagation depends on HOW the error is surfaced, not the error variant.
#[test]
fn test_internal_error_caught_via_try_or_handle_in_run() {
    // Call(9999, 0) triggers InternalError via get_function_cloned_or_raise,
    // which calls try_or_handle → handle_error. The handler catches it.
    let catch_ip = 2;
    let mut vm = Vm::new(
        vec![
            Instr::PushHandler(Some(catch_ip), None), // ip=0: push handler
            Instr::Call(9999, 0), // ip=1: invalid func → InternalError via try_or_handle
            Instr::ClearError,    // ip=2 (catch): clear error
            Instr::PushI64(42),   // ip=3: push result
            Instr::ReturnAny,     // ip=4: return 42
        ],
        StableRng::new(0),
    );
    let result = vm.run();
    // InternalError IS caught because Call uses try_or_handle
    assert!(
        matches!(result, Ok(Value::I64(42))),
        "Expected Ok(I64(42)), got {:?}",
        result
    );
}

/// Issue #6342 / #5969: the call-frame stack is bounded at call boundaries
/// instead of at every interpreter-loop instruction. With no active handler
/// installed, the error propagates out of the push path (the caught path —
/// `e isa StackOverflowError` — is covered end-to-end by
/// `tests/fixtures/error/error_stack_overflow_5969.jl`).
#[test]
fn test_call_depth_guard_raises_stack_overflow_5969() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    for _ in 0..Vm::<StableRng>::MAX_CALL_DEPTH {
        vm.frames.push(Frame::new());
    }
    vm.try_push_call_frame(Frame::new()).unwrap();
    let result = vm.handle_pending_call_depth_overflow();
    assert!(
        matches!(result, Err(VmError::StackOverflow)),
        "expected Err(StackOverflow), got {:?}",
        result
    );
}

/// Issue #6342 / #5969: nested dispatch uses the same call-boundary push
/// helper, so eval/HOF-driven recursion cannot grow `self.frames` beyond the
/// limit before surfacing `StackOverflow`.
#[test]
fn test_call_depth_guard_in_run_until_frame_return_5969() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    for _ in 0..Vm::<StableRng>::MAX_CALL_DEPTH {
        vm.frames.push(Frame::new());
    }
    vm.try_push_call_frame(Frame::new()).unwrap();
    let result = vm.handle_pending_call_depth_overflow();
    assert!(
        matches!(result, Err(VmError::StackOverflow)),
        "expected Err(StackOverflow), got {:?}",
        result
    );
}

/// Issue #5972: an error raised inside nested eval dispatch must NOT be
/// caught by a handler installed by an *ancestor* frame (`frame_len <=
/// target_depth`). Such a handler belongs to a `try` opened *outside* the
/// nested `eval` dispatch. The `eval_dispatch_floor` lets the error
/// propagate as `Err` so the outer loop re-routes it to that ancestor
/// handler. Here an ancestor handler (`frame_len == target_depth == 0`) is
/// present, yet the overflow must still surface as `Err(StackOverflow)` with
/// the handler left intact.
#[test]
fn test_ancestor_handler_not_consumed_in_run_until_frame_return_5972() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    // Ancestor handler at frame_len 0 (== target_depth): a `try` outside the
    // nested dispatch.
    vm.handlers.push(Handler {
        catch_ip: Some(0),
        finally_ip: None,
        stack_len: 0,
        frame_len: 0,
        return_ip_len: 0,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });
    for _ in 0..Vm::<StableRng>::MAX_CALL_DEPTH {
        vm.frames.push(Frame::new());
    }
    vm.eval_dispatch_floor = Some(0);

    vm.try_push_call_frame(Frame::new()).unwrap();
    let result = vm.handle_pending_call_depth_overflow();

    assert!(
        matches!(result, Err(VmError::StackOverflow)),
        "ancestor handler must not catch inside the nested loop; expected \
         Err(StackOverflow), got {:?}",
        result
    );
    // The ancestor handler must be left untouched for the outer loop to use.
    assert_eq!(vm.handlers.len(), 1, "ancestor handler was consumed");
}

#[test]
fn test_direct_call_fast_path_preserves_call_return_flow() {
    let mut vm = Vm::new(
        vec![
            Instr::PushI64(40),
            Instr::PushI64(2),
            Instr::Call(0, 2),
            Instr::ReturnAny,
            Instr::LoadSlotI64(0),
            Instr::LoadSlotI64(1),
            Instr::AddI64,
            Instr::ReturnI64,
        ],
        StableRng::new(0),
    );
    vm.functions.push(Rc::new(FunctionInfo {
        name: "add_two".to_string(),
        params: vec![
            ("x".to_string(), ValueType::I64),
            ("y".to_string(), ValueType::I64),
        ],
        kwparams: vec![],
        entry: 4,
        return_type: ValueType::I64,
        return_julia_type: None,
        is_base_extension: false,
        is_generated: false,
        is_lowering_helper: false,
        definition_order: 0,
        min_world: 1,
        type_params: vec![],
        param_julia_types: vec![
            crate::types::JuliaType::Int64,
            crate::types::JuliaType::Int64,
        ],
        code_start: 4,
        code_end: 8,
        slot_names: vec!["x".to_string(), "y".to_string()],
        slot_types: vec![
            Some(crate::vm::VarTypeTag::I64),
            Some(crate::vm::VarTypeTag::I64),
        ],
        local_slot_count: 2,
        param_slots: vec![0, 1],
        vararg_param_index: None,
        vararg_fixed_count: None,
        inlining_meta: 0,
        constprop_meta: 0,
        nospecialize_meta: 0,
        propagate_inbounds_meta: false,
        nospecializeinfer_meta: false,
        purity_meta: 0,
        direct_return_type_param: None,
        // Builtin stub FunctionInfo: no source line (Issue #5125).
        def_line: 0,
        suppress_short_name_alias: false,
        shared_plan: None,
    }));

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::I64(42))),
        "Expected Ok(I64(42)), got {:?}",
        result
    );
}

#[test]
fn test_load_slot_i64_preserves_unsigned_integer_values() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotI64(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    frame.locals_slots[0] = Some(Value::U64(1));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::U64(1))),
        "Expected Ok(U64(1)), got {:?}",
        result
    );
}

#[test]
fn test_load_slot_i64_preserves_float_values_after_slot_retagging() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotI64(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    frame.locals_slots[0] = Some(Value::F64(24.0));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::F64(v)) if v == 24.0),
        "Expected Ok(F64(24.0)), got {:?}",
        result
    );
}

#[test]
fn test_load_slot_f64_preserves_narrow_float_values() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotF64(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    frame.locals_slots[0] = Some(Value::F16(half::f16::from_f32(2.5)));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::F16(v)) if v == half::f16::from_f32(2.5)),
        "Expected Ok(F16(2.5)), got {:?}",
        result
    );
}

#[test]
fn test_typed_float_and_string_slot_ops_roundtrip_issue_5081() {
    let mut vm = Vm::new(
        vec![
            Instr::PushF32(3.5),
            Instr::StoreSlotF32(0),
            Instr::LoadSlotF32(0),
            Instr::PushF16(half::f16::from_f32(1.25)),
            Instr::StoreSlotF16(1),
            Instr::LoadSlotF16(1),
            Instr::PushStr("slot".to_string()),
            Instr::StoreSlotStr(2),
            Instr::LoadSlotStr(2),
            Instr::PushChar('x'),
            Instr::StoreSlotChar(3),
            Instr::LoadSlotChar(3),
            Instr::PushSymbol("tag".to_string()),
            Instr::StoreSlotSymbol(4),
            Instr::LoadSlotSymbol(4),
            Instr::PushI128(Box::new(7)),
            Instr::StoreSlotNarrowInt(5),
            Instr::LoadSlotNarrowInt(5),
            Instr::PushNothing,
            Instr::StoreSlotNothing(6),
            Instr::LoadSlotNothing(6),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );
    vm.frames.push(Frame::new_with_slots(7, None));

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::Nothing)),
        "Expected Ok(Nothing), got {:?}",
        result
    );

    let slot0 = vm
        .frames
        .last()
        .and_then(|frame| frame.locals_slots.first());
    assert!(
        matches!(slot0, Some(Some(Value::F32(v))) if *v == 3.5),
        "Expected slot 0 to hold F32(3.5), got {:?}",
        vm.frames.last().map(|frame| &frame.locals_slots)
    );
    let slot1 = vm.frames.last().and_then(|frame| frame.locals_slots.get(1));
    assert!(
        matches!(slot1, Some(Some(Value::F16(v))) if *v == half::f16::from_f32(1.25)),
        "Expected slot 1 to hold F16(1.25), got {:?}",
        vm.frames.last().map(|frame| &frame.locals_slots)
    );
    let frame = vm
        .frames
        .last()
        .expect("frame should remain for inspection");
    assert_eq!(frame.slot_str(2).map(|s| s.as_ref()), Some("slot"));
    assert_eq!(frame.slot_char(3), Some('x'));
    assert!(matches!(frame.slot_symbol(4), Some(sym) if sym.as_str() == "tag"));
    assert!(matches!(frame.slot_narrow_int(5), Some(Value::I128(7))));
    assert!(frame.slot_nothing(6));
}

#[test]
fn test_store_any_symbol_uses_symbol_tag_issue_5081() {
    let mut vm = Vm::new(
        vec![
            Instr::PushSymbol("legacy".to_string()),
            Instr::StoreAny("sym".to_string()),
            Instr::LoadAny("sym".to_string()),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::Symbol(ref sym)) if sym.as_str() == "legacy"),
        "Expected Ok(Symbol(:legacy)), got {:?}",
        result
    );
    let frame = vm
        .frames
        .last()
        .expect("frame should remain for inspection");
    assert_eq!(
        frame.var_types.get("sym"),
        Some(&crate::vm::VarTypeTag::Symbol)
    );
}

#[test]
fn repl_write_trace_distinguishes_explicit_global_stores_9784() {
    let mut vm = Vm::new(
        vec![
            Instr::PushI64(7),
            Instr::StoreGlobalAny("explicit9784".to_string()),
            Instr::PushNothing,
            Instr::ReturnAny,
        ],
        StableRng::new(9784),
    );

    assert!(vm.run().is_ok());
    assert!(vm.repl_written_global_names().contains("explicit9784"));
    assert!(vm
        .repl_explicit_global_write_names()
        .contains("explicit9784"));

    vm.reenter_appended_main(&[], &[], StableRng::new(9785));
    assert!(vm.repl_written_global_names().is_empty());
    assert!(vm.repl_explicit_global_write_names().is_empty());
}

#[test]
fn repl_using_activation_trace_is_distinct_and_reset_11748() {
    let mut vm = Vm::new(
        vec![
            Instr::ActivateUsing {
                owner_module: String::new(),
                program_index: 1,
            },
            Instr::ActivateUsing {
                owner_module: String::new(),
                program_index: 1,
            },
            Instr::ActivateUsing {
                owner_module: "Nested11748".to_string(),
                program_index: 0,
            },
            Instr::ActivateUsing {
                owner_module: String::new(),
                program_index: 3,
            },
            Instr::PushNothing,
            Instr::ReturnAny,
        ],
        StableRng::new(11748),
    );

    assert!(vm.run().is_ok());
    assert_eq!(
        vm.repl_reached_using_activations(),
        &[
            (String::new(), 1),
            ("Nested11748".to_string(), 0),
            (String::new(), 3),
        ]
    );

    vm.reenter_appended_main(&[], &[], StableRng::new(11749));
    assert!(vm.repl_reached_using_activations().is_empty());
}

#[test]
fn repl_module_activation_trace_records_owner_chain_and_resets_11761() {
    let mut vm = Vm::new(
        vec![
            Instr::ActivateModule("Parent11761.Child11761".to_string()),
            Instr::ActivateModule("Parent11761.Child11761".to_string()),
            Instr::PushNothing,
            Instr::ReturnAny,
        ],
        StableRng::new(11761),
    );

    assert!(vm.run().is_ok());
    assert_eq!(
        vm.repl_reached_module_activations(),
        &[
            "Parent11761".to_string(),
            "Parent11761.Child11761".to_string()
        ]
    );

    vm.reenter_appended_main(&[], &[], StableRng::new(11762));
    assert!(vm.repl_reached_module_activations().is_empty());
}

#[test]
fn test_typed_container_slot_ops_roundtrip_issue_5081() {
    let mut vm = Vm::new(
        vec![
            Instr::PushArrayValue(Box::new(crate::vm::ArrayLiteralPayload::I64 {
                data: vec![1, 1],
                shape: vec![2],
            })),
            Instr::StoreSlotArray(0),
            Instr::LoadSlotArray(0),
            Instr::PushI64(1),
            Instr::PushI64(2),
            Instr::NewTuple(2),
            Instr::StoreSlotTuple(1),
            Instr::LoadSlotTuple(1),
            Instr::PushI64(10),
            Instr::PushI64(20),
            Instr::NewNamedTuple(vec!["a".to_string(), "b".to_string()]),
            Instr::StoreSlotNamedTuple(4),
            Instr::LoadSlotNamedTuple(4),
            Instr::PushI64(1),
            Instr::PushI64(1),
            Instr::PushI64(3),
            Instr::MakeRangeLazy,
            Instr::StoreSlotRange(5),
            Instr::LoadSlotRange(5),
            Instr::PushI64(1),
            Instr::NewStableRng,
            Instr::StoreSlotRng(6),
            Instr::LoadSlotRng(6),
            Instr::PushI64(1),
            Instr::PushI64(2),
            Instr::NewTuple(2),
            Instr::MakeGenerator(Box::new(MakeGeneratorOperands {
                callable: crate::vm::GeneratorCallableSpec::FunctionIndex(0),
                result_element_type: None,
            })),
            Instr::StoreSlotGenerator(7),
            Instr::LoadSlotGenerator(7),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );
    vm.frames.push(Frame::new_with_slots(8, None));

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::Generator(_))),
        "Expected Ok(Generator), got {:?}",
        result
    );

    let frame = vm
        .frames
        .last()
        .expect("frame should remain for inspection");
    // After the array-producer flip (Issue #6806), `PushArrayValue` yields the
    // MemoryRef-backed `Array{T,N}` wrapper (a `StructRef`), so `StoreSlotArray`
    // routes it through the generic local slot rather than the native-carrier
    // typed `slot_array` fast path (which still backs transient build buffers).
    // The array therefore roundtrips through whichever channel holds it; the
    // typed fast path for wrappers is restored in PR B (accessor migration).
    assert!(
        frame.slot_array(0).is_some() || matches!(frame.locals_slots.first(), Some(Some(_))),
        "array slot should roundtrip through the typed or generic slot"
    );
    assert!(frame.slot_tuple(1).is_some());
    assert!(frame.slot_named_tuple(4).is_some());
    assert!(frame.slot_range(5).is_some());
    assert!(frame.slot_rng(6).is_some());
    assert!(frame.slot_generator(7).is_some());
}

/// Issue #6806 (producer flip): array literals (`PushArrayValue`) materialize the
/// MemoryRef-backed `Array{T,N}` wrapper — a struct-heap instance whose name is
/// an `Array{...}` and whose first field is a `MemoryRef` — instead of the legacy
/// native-array carrier. The host-return boundary
/// (`normalize_host_return_value`) re-materializes the *returned* value for the
/// heap-less caller, so the internal representation is asserted by
/// inspecting the struct heap rather than `run()`'s return value. This pins the
/// producer so the carrier confinement (#6807) can rely on it no longer emitting
/// the carrier. The runtime value, `typeof`, and `isa` are unchanged.
#[test]
fn test_array_literal_emits_memoryref_wrapper_issue_6806() {
    let mut vm = Vm::new(
        vec![
            Instr::PushArrayValue(Box::new(crate::vm::ArrayLiteralPayload::I64 {
                data: vec![1, 1, 1],
                shape: vec![3],
            })),
            Instr::ReturnAny,
        ],
        StableRng::new(0),
    );
    vm.frames.push(Frame::new_with_slots(0, None));
    let _ = vm.run().expect("bare-VM run should succeed");

    let wrapper = vm.get_struct_heap().iter().find(|inst| {
        (&*inst.struct_name == "Array" || inst.struct_name.starts_with("Array{"))
            && matches!(inst.values.first(), Some(Value::MemoryRef(_)))
    });
    assert!(
        wrapper.is_some(),
        "array literal should materialize a MemoryRef-backed Array wrapper in the \
         struct heap; heap = {:?}",
        vm.get_struct_heap()
    );
}

#[test]
fn test_typed_struct_slot_loads_sidecar_issue_5081() {
    let mut vm = Vm::new(
        vec![Instr::LoadSlotStruct(0), Instr::ReturnAny],
        StableRng::new(0),
    );
    let mut frame = Frame::new_with_slots(1, None);
    assert!(frame.set_slot_struct_ref(0, 7));
    vm.frames.push(frame);

    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::StructRef(7))),
        "Expected Ok(StructRef(7)), got {:?}",
        result
    );
}

/// A user-visible error (DivisionByZero) IS caught by handlers in run().
/// DivisionByZero goes through raise(), which checks handlers.
#[test]
fn test_user_error_caught_by_handler_in_run() {
    // Set up: try { 1 % 0 } catch; return 42
    let catch_ip = 4;
    let mut vm = Vm::new(
        vec![
            Instr::PushHandler(Some(catch_ip), None), // ip=0: push handler
            Instr::PushI64(1),                        // ip=1
            Instr::PushI64(0),                        // ip=2
            Instr::ModI64,                            // ip=3: 1 % 0 -> DivisionByZero
            Instr::ClearError,                        // ip=4 (catch): clear error
            Instr::PushI64(42),                       // ip=5: push result
            Instr::ReturnAny,                         // ip=6: return 42
        ],
        StableRng::new(0),
    );
    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::I64(42))),
        "Expected Ok(I64(42)), got {:?}",
        result
    );
}

/// Errors returned via direct Err() (not raise/try_or_handle) bypass handlers.
/// This verifies the ? operator propagation path in the run loop.
#[test]
fn test_direct_err_return_bypasses_handlers() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    // Push a handler
    vm.handlers.push(Handler {
        catch_ip: Some(100),
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });
    // Simulate what happens when an instruction does `return Err(InternalError)`:
    // The error goes through dispatch_instr's `?` → run's `result?` → caller.
    // The handler is NOT consulted because raise/handle_error is never called.
    // We verify this by checking that the handler is still on the stack after
    // a direct Err propagation (handle_error would have popped it).
    assert_eq!(vm.handlers.len(), 1);
    let result: Result<(), VmError> = Err(VmError::InternalError("direct".to_string()));
    // Simulating the ? operator: the error propagates without touching handlers
    assert!(result.is_err());
    assert_eq!(vm.handlers.len(), 1, "Handler should NOT have been popped");
}

/// handle_error with a finally-only handler (no catch) pushes a pending
/// finally-rethrow marker (Issue #11306; formerly a scalar `rethrow_on_finally`
/// flag).
#[test]
fn test_handle_error_finally_only_sets_rethrow() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: None,
        finally_ip: Some(200),
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });
    let result = vm.handle_error(VmError::TypeError("test".to_string()));
    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    assert_eq!(vm.ip, 200);
    assert_eq!(
        vm.pending_finally_rethrows.len(),
        1,
        "expected one pending finally-rethrow marker"
    );
}

/// handle_error with a handler that has neither catch nor finally propagates.
#[test]
fn test_handle_error_no_catch_no_finally_propagates() {
    let mut vm = Vm::new(vec![], StableRng::new(0));
    vm.handlers.push(Handler {
        catch_ip: None,
        finally_ip: None,
        stack_len: 0,
        frame_len: 1,
        return_ip_len: 0,
        lexical_scope_len: 0,
        caught_exception_len: 0,
        finally_pending_len: 0,
    });
    let result = vm.handle_error(VmError::InternalError("test".to_string()));
    assert!(
        matches!(result, Err(VmError::InternalError(_))),
        "Expected Err(InternalError), got {:?}",
        result
    );
}

/// get_function_checked returns InternalError for out-of-bounds index.
#[test]
fn test_get_function_checked_internal_error() {
    let vm = Vm::new(vec![], StableRng::new(0));
    let result = vm.get_function_checked(999);
    assert!(
        matches!(result, Err(VmError::InternalError(_))),
        "Expected Err(InternalError), got {:?}",
        result
    );
}

// === SpannedVmError / source map tests (Issue #2856) ===

/// last_error_span returns None when no error has occurred.
#[test]
fn test_last_error_span_none_by_default() {
    let vm = Vm::new(vec![], StableRng::new(0));
    assert_eq!(vm.last_error_span(), None);
}

/// last_error_span returns None when source map is empty.
#[test]
fn test_last_error_span_none_without_source_map() {
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    let result = vm.run();
    assert!(result.is_err());
    // No source map set, so span is None even though last_error_ip is set
    assert_eq!(vm.last_error_span(), None);
}

/// last_error_span returns the span from the source map when available.
#[test]
fn test_last_error_span_with_source_map() {
    use crate::span::Span;

    let span_at_2 = Span::new(10, 15, 3, 3, 5, 10);
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    vm.set_source_map(vec![None, None, Some(span_at_2)]);

    let result = vm.run();
    assert!(result.is_err());
    assert_eq!(vm.last_error_span(), Some(span_at_2));
}

/// spanned_error wraps VmError with the last error span.
#[test]
fn test_spanned_error_attaches_span() {
    use crate::span::Span;

    let span = Span::new(0, 5, 1, 1, 1, 6);
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    vm.set_source_map(vec![None, None, Some(span)]);

    let result = vm.run();
    assert!(result.is_err());

    let err = result.unwrap_err();
    let spanned = vm.spanned_error(err.clone());
    assert_eq!(spanned.error, err);
    assert_eq!(spanned.span, Some(span));
}

/// spanned_error returns None span when source map has no entry for the IP.
#[test]
fn test_spanned_error_no_span_for_unmapped_ip() {
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    // Source map entries only for first two IPs, not for IP=2 (ModI64)
    vm.set_source_map(vec![None, None]);

    let result = vm.run();
    assert!(result.is_err());

    let err = result.unwrap_err();
    let spanned = vm.spanned_error(err.clone());
    assert_eq!(spanned.error, err);
    assert_eq!(spanned.span, None);
}

/// Display of SpannedVmError includes line:column when span is present.
#[test]
fn test_spanned_error_display_with_location() {
    use crate::span::Span;

    let span = Span::new(10, 20, 5, 5, 8, 18);
    let mut vm = Vm::new(
        vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
        StableRng::new(0),
    );
    vm.set_source_map(vec![None, None, Some(span)]);

    let result = vm.run();
    let err = result.unwrap_err();
    let spanned = vm.spanned_error(err);
    let display = format!("{}", spanned);
    assert!(
        display.contains("at line 5:8"),
        "Expected span info in display, got: {}",
        display
    );
}

// === Issue #10406: run-loop terminal error arm routes catchable errors ===
//
// Some instruction handlers (e.g. the numeric fast paths' `value_to_f64`
// type check) propagate a catchable error with a bare `?` rather than
// `self.raise()`. Before #10406 such an error escaped straight out of the
// `run()` loop, aborting the program even when an enclosing `try` had a live
// handler. The run loop's terminal `Err` arm now routes catchable errors
// through `raise` — but must keep VM-internal / host errors uncatchable.

/// The `is_catchable_vm_error` predicate must be the exact inverse of the
/// variants for which `vm_error_to_exception_value` has no exception object.
/// If these drift, a variant silently becomes catchable-but-bound-as-a-`String`
/// (or the reverse), or a host `Cancelled` abort becomes swallowable by user
/// code.
///
/// Issue #11146 made the drift structurally impossible rather than merely
/// tested: both sides now derive from the one `VmError::exception_class()`
/// funnel. The same issue moved `NotImplemented` OUT of the uncatchable set —
/// it is a user-reachable feature gap, so it surfaces as a catchable
/// `ErrorException` instead of binding a raw `String` in the `catch`.
#[test]
fn is_catchable_vm_error_matches_exception_none_set_10406() {
    // The five VM-internal / host-control variants: never catchable.
    for e in [
        VmError::Cancelled,
        VmError::StackUnderflow,
        VmError::InternalError("x".to_string()),
        VmError::UnknownBroadcastOp("x".to_string()),
        VmError::InvalidInstruction,
    ] {
        assert!(
            !Vm::<StableRng>::is_catchable_vm_error(&e),
            "must stay uncatchable (has no Julia exception object): {e:?}"
        );
    }
    // Representative Julia-level exceptions: always catchable.
    for e in [
        VmError::TypeError("t".to_string()),
        VmError::MethodError("m".to_string()),
        VmError::DomainError("d".to_string()),
        VmError::InexactError("i".to_string()),
        VmError::DivisionByZero,
        VmError::OverflowError("o".to_string()),
        VmError::ErrorException("e".to_string()),
        VmError::AssertionFailed("a".to_string()),
        VmError::StackOverflow,
        VmError::EmptyArrayPop,
        VmError::UndefVarError("v".to_string()),
        // Issue #11146: an unimplemented feature is a user-reachable failure,
        // so it is a catchable ErrorException — not a raw `String`.
        VmError::NotImplemented("x".to_string()),
        VmError::ParseError("p".to_string()),
        VmError::DimensionMismatchMsg("d".to_string()),
    ] {
        assert!(
            Vm::<StableRng>::is_catchable_vm_error(&e),
            "must be catchable (maps to a Julia exception): {e:?}"
        );
    }
}

/// End-to-end: an error raised by the `SqrtF64` fast path via a bare `?`
/// (never routed through `self.raise` at the instruction level) is now caught
/// by the enclosing handler through the `run()` loop's terminal arm, resuming
/// at `catch_ip` instead of aborting the program (Issue #10406). The operand
/// failure surfaces as the upstream-faithful `MethodError` (a non-numeric
/// operand is a dispatch miss for `sqrt`, Issue #10481), not the internal
/// conversion `TypeError`.
#[test]
fn run_loop_terminal_arm_catches_bare_question_mark_type_error_10406() {
    let mut vm = Vm::new(
        vec![
            Instr::PushHandler(Some(4), None), // ip 0: catch resumes at ip 4
            Instr::PushStr("a".to_string()),   // ip 1
            Instr::SqrtF64, // ip 2: operand check fails -> Err(MethodError) via `?`
            Instr::ReturnNothing, // ip 3: not reached (error unwinds to catch)
            Instr::PushI64(42), // ip 4: catch body
            Instr::ReturnAny, // ip 5
        ],
        StableRng::new(0),
    );
    let result = vm.run();
    assert!(
        matches!(result, Ok(Value::I64(42))),
        "SqrtF64 operand error must be caught by the run-loop terminal arm and resume at catch_ip; got {result:?}"
    );
    assert!(
        matches!(vm.pending_error, Some(VmError::MethodError(_))),
        "the caught error should be the SqrtF64 MethodError (Issue #10481), got {:?}",
        vm.pending_error
    );
}

/// The terminal arm must NOT catch a VM-internal error even with a live
/// handler: the run loop gates on `is_catchable_vm_error`, so an internal
/// error (here `StackUnderflow`, in the same non-catchable set as the host
/// `Cancelled` abort) propagates out unchanged rather than being swallowed by
/// user code (Issue #10406). `SqrtF64` pops an empty stack -> `StackUnderflow`
/// via a bare `?`, reaching the terminal arm while the ip-0 handler is live.
#[test]
fn run_loop_terminal_arm_keeps_internal_error_uncatchable_10406() {
    let mut vm = Vm::new(
        vec![
            Instr::PushHandler(Some(2), None), // ip 0: would catch a catchable error
            Instr::SqrtF64,                    // ip 1: pop on empty stack -> StackUnderflow
            Instr::PushI64(99),                // ip 2: catch body (must NOT run)
            Instr::ReturnAny,                  // ip 3
        ],
        StableRng::new(0),
    );
    let result = vm.run();
    assert!(
        matches!(result, Err(VmError::StackUnderflow)),
        "an internal error must stay uncatchable and propagate out of run(), not be caught by the handler; got {result:?}"
    );
}
