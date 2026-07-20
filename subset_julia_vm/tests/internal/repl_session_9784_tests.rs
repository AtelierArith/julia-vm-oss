// Private-state REPL recovery regressions for Issue #9784.

use super::*;

fn with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f);
    let Ok(handle) = handle else {
        panic!("failed to spawn large-stack REPL test thread");
    };
    assert!(
        handle.join().is_ok(),
        "large-stack REPL test thread panicked"
    );
}

fn i64_of(result: &REPLResult) -> Option<i64> {
    match &result.value {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

#[test]
fn runtime_value_projection_rejects_qualified_main_scope_names_9784() {
    let written: std::collections::HashSet<String> = ["main_value_9784", "Module9784.member"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut main_scope_names = written.clone();
    let mut nonvalue_bindings = std::collections::HashSet::new();
    nonvalue_bindings.insert("imported_value_9784".to_string());
    let mut written = written;
    written.insert("imported_value_9784".to_string());
    written.insert("#sjulia_imported_binding_source#Main#value".to_string());
    main_scope_names.extend(written.iter().cloned());
    let projected = runtime_value_rebindings(
        &written,
        &std::collections::HashSet::new(),
        &main_scope_names,
        &nonvalue_bindings,
        &std::collections::HashSet::new(),
    );

    assert_eq!(
        projected,
        ["main_value_9784".to_string()].into_iter().collect()
    );
}

#[test]
fn stdlib_structural_intrinsic_survives_cross_eval_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(9784);
        let loaded = session.eval("using LinearAlgebra");
        assert!(loaded.success, "{:?}", loaded.error);
        assert!(session
            .variable_names()
            .iter()
            .all(|name| !name.starts_with("#sjulia_imported_binding_")
                && !name.starts_with("#sjulia_module_alias_ambiguous#")));
        assert!(
            session.variable_names().is_empty(),
            "using-only input persisted value bindings: {:?}",
            session.variable_names()
        );
        assert!(
            session.global_types.is_empty(),
            "using-only input persisted type hints: {:?}",
            session.global_types.keys().collect::<Vec<_>>()
        );
        let determinant = session.eval("det([1.0 2.0; 3.0 4.0])");
        assert!(determinant.success, "{:?}", determinant.error);
        assert!(matches!(determinant.value, Some(Value::F64(value)) if value == -2.0));
    });
}

/// Frame-0 storage names do not define persistence ownership by themselves.
/// Pin the boundary across Main values, called-function globals, import
/// publication, and qualified module values before more #9784 mirrors retire.
#[test]
fn repl_binding_ownership_matrix_survives_fresh_rebuild_11725() {
    with_large_stack(|| {
        #[derive(Clone, Copy)]
        enum ExpectedValue {
            I64(i64),
            F64(f64),
            Bool(bool),
        }

        #[derive(Clone, Copy)]
        enum ExpectedDefinition {
            None,
            Struct(&'static str),
            Abstract(&'static str),
            Primitive(&'static str),
            Enum(&'static str),
            Module(&'static str),
        }

        struct OwnershipCase {
            label: &'static str,
            steps: &'static [(&'static str, bool)],
            expected_main: &'static [&'static str],
            expected_module: &'static [&'static str],
            expected_definition: ExpectedDefinition,
            probe: &'static str,
            expected_value: ExpectedValue,
            redefinition_removes: Option<(&'static str, &'static str, &'static str)>,
        }

        let cases = [
            OwnershipCase {
                label: "direct Main assignment",
                steps: &[("owned_main_11725 = 11", true)],
                expected_main: &["owned_main_11725"],
                expected_module: &[],
                expected_definition: ExpectedDefinition::None,
                probe: "owned_main_11725",
                expected_value: ExpectedValue::I64(11),
                redefinition_removes: None,
            },
            OwnershipCase {
                label: "called-function Main global before error",
                steps: &[
                    (
                        "set_called_main_11725() = (global called_main_11725 = 12)",
                        true,
                    ),
                    ("set_called_main_11725(); error(\"stop\")", false),
                ],
                expected_main: &["called_main_11725"],
                expected_module: &[],
                expected_definition: ExpectedDefinition::None,
                probe: "called_main_11725",
                expected_value: ExpectedValue::I64(12),
                redefinition_removes: None,
            },
            OwnershipCase {
                label: "stdlib import and compiler metadata publication",
                steps: &[("using LinearAlgebra", true)],
                expected_main: &[],
                expected_module: &[],
                expected_definition: ExpectedDefinition::None,
                probe: "det([1.0 2.0; 3.0 4.0])",
                expected_value: ExpectedValue::F64(-2.0),
                redefinition_removes: None,
            },
            OwnershipCase {
                label: "runtime-created qualified module global",
                steps: &[
                    (
                        "module OwnedBindings11725\n\
                         x = 4\n\
                         set_dynamic() = (global dynamic = 5)\n\
                         end",
                        true,
                    ),
                    ("OwnedBindings11725.set_dynamic()", true),
                ],
                expected_main: &[],
                expected_module: &["OwnedBindings11725.x", "OwnedBindings11725.dynamic"],
                expected_definition: ExpectedDefinition::Module("OwnedBindings11725"),
                probe: "OwnedBindings11725.dynamic",
                expected_value: ExpectedValue::I64(5),
                redefinition_removes: Some((
                    "module OwnedBindings11725\ny = 6\nend",
                    "isdefined(OwnedBindings11725, :dynamic)",
                    "OwnedBindings11725.dynamic",
                )),
            },
            OwnershipCase {
                label: "Main struct publication",
                steps: &[("struct OwnedStruct11725; x::Int; end", true)],
                expected_main: &[],
                expected_module: &[],
                expected_definition: ExpectedDefinition::Struct("OwnedStruct11725"),
                probe: "OwnedStruct11725(13).x",
                expected_value: ExpectedValue::I64(13),
                redefinition_removes: None,
            },
            OwnershipCase {
                label: "Main abstract type publication",
                steps: &[("abstract type OwnedAbstract11725 end", true)],
                expected_main: &[],
                expected_module: &[],
                expected_definition: ExpectedDefinition::Abstract("OwnedAbstract11725"),
                probe: "OwnedAbstract11725 isa Type",
                expected_value: ExpectedValue::Bool(true),
                redefinition_removes: None,
            },
            OwnershipCase {
                label: "Main primitive type publication",
                steps: &[("primitive type OwnedPrimitive11725 8 end", true)],
                expected_main: &[],
                expected_module: &[],
                expected_definition: ExpectedDefinition::Primitive("OwnedPrimitive11725"),
                probe: "sizeof(OwnedPrimitive11725)",
                expected_value: ExpectedValue::I64(1),
                redefinition_removes: None,
            },
            OwnershipCase {
                label: "Main enum and member publication",
                steps: &[("@enum OwnedEnum11725 owned_member_11725=14", true)],
                expected_main: &[],
                expected_module: &[],
                expected_definition: ExpectedDefinition::Enum("OwnedEnum11725"),
                probe: "Int(owned_member_11725)",
                expected_value: ExpectedValue::I64(14),
                redefinition_removes: None,
            },
            OwnershipCase {
                label: "module declaration publication",
                steps: &[("module OwnedModule11725\nx = 15\nend", true)],
                expected_main: &[],
                expected_module: &["OwnedModule11725.x"],
                expected_definition: ExpectedDefinition::Module("OwnedModule11725"),
                probe: "OwnedModule11725.x",
                expected_value: ExpectedValue::I64(15),
                redefinition_removes: None,
            },
        ];

        for (index, case) in cases.iter().enumerate() {
            let mut session = REPLSession::new(11_725 + index as u64);
            for (source, expected_success) in case.steps {
                let result = session.eval(source);
                assert_eq!(
                    result.success, *expected_success,
                    "{}: unexpected result for {source:?}: {:?}",
                    case.label, result.error
                );
            }

            let assert_ownership = |session: &REPLSession| {
                let mut actual_main: std::collections::HashSet<String> =
                    session.variable_names().into_iter().collect();
                // `ans` is the explicit host-facing result mirror, not a
                // user-authored binding whose store owner is under test.
                actual_main.remove("ans");
                let expected_main: std::collections::HashSet<String> = case
                    .expected_main
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect();
                assert_eq!(actual_main, expected_main, "{}: Main mirror", case.label);
                let mut actual_types: std::collections::HashSet<String> =
                    session.global_types.keys().cloned().collect();
                actual_types.remove("ans");
                assert_eq!(
                    actual_types, expected_main,
                    "{}: Main type mirror",
                    case.label
                );
                assert!(
                    session.global_struct_names.is_empty(),
                    "{}: unexpected Main struct mirror: {:?}",
                    case.label,
                    session.global_struct_names
                );
                for name in case.expected_module {
                    assert!(
                        session.module_globals.contains_key(*name),
                        "{}: missing module-owned binding {name:?}; keys={:?}",
                        case.label,
                        session.module_globals.keys().collect::<Vec<_>>()
                    );
                }
                assert!(
                    session.module_globals.keys().all(|name| {
                        !name.starts_with("#sjulia_imported_binding_")
                            && !name.starts_with("#sjulia_module_alias_ambiguous#")
                    }),
                    "{}: compiler import metadata entered module values",
                    case.label
                );
                match case.expected_definition {
                    ExpectedDefinition::None => {}
                    ExpectedDefinition::Struct(name) => assert!(
                        session.struct_index.contains_key(name),
                        "{}: missing struct definition {name:?}",
                        case.label
                    ),
                    ExpectedDefinition::Abstract(name) => assert!(
                        session.abstract_type_index.contains_key(name),
                        "{}: missing abstract type definition {name:?}",
                        case.label
                    ),
                    ExpectedDefinition::Primitive(name) => assert!(
                        session.primitive_type_index.contains_key(name),
                        "{}: missing primitive type definition {name:?}",
                        case.label
                    ),
                    ExpectedDefinition::Enum(name) => assert!(
                        session.enum_index.contains_key(name),
                        "{}: missing enum definition {name:?}",
                        case.label
                    ),
                    ExpectedDefinition::Module(name) => assert!(
                        session.module_index.contains_key(name),
                        "{}: missing module definition {name:?}",
                        case.label
                    ),
                }
            };

            assert_ownership(&session);
            let rebuilt = session.eval(&format!(
                "macro force_ownership_rebuild_11725_{index}(); :(1); end"
            ));
            assert!(rebuilt.success, "{}: {:?}", case.label, rebuilt.error);
            assert_ne!(session.last_vm_build_nanos(), Some(0), "{}", case.label);
            assert_ownership(&session);

            let observed = session.eval(case.probe);
            assert!(observed.success, "{}: {:?}", case.label, observed.error);
            match case.expected_value {
                ExpectedValue::I64(expected) => {
                    assert_eq!(i64_of(&observed), Some(expected), "{}", case.label)
                }
                ExpectedValue::F64(expected) => assert!(
                    matches!(observed.value, Some(Value::F64(value)) if value == expected),
                    "{}: unexpected probe value {:?}",
                    case.label,
                    observed.value
                ),
                ExpectedValue::Bool(expected) => assert!(
                    matches!(observed.value, Some(Value::Bool(value)) if value == expected),
                    "{}: unexpected probe value {:?}",
                    case.label,
                    observed.value
                ),
            }

            if let Some((redefinition, removed_probe, removed_key)) = case.redefinition_removes {
                let redefined = session.eval(redefinition);
                assert!(redefined.success, "{}: {:?}", case.label, redefined.error);
                let removed = session.eval(removed_probe);
                assert!(
                    matches!(removed.value, Some(Value::Bool(false))),
                    "{}: module redefinition resurrected {removed_key:?}: {:?}",
                    case.label,
                    removed.value
                );
                assert!(
                    !session.module_globals.contains_key(removed_key),
                    "{}: stale module carrier retained {removed_key:?}",
                    case.label
                );
            }
        }
    });
}

/// A helper compiled after an immediately throwing statement is never
/// observed by a global or source definition. Repeating that failure must
/// not append private bodies to the live VM or persistent IR forever.
#[test]
fn unreached_helper_only_failures_do_not_accumulate_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(0);
        assert!(session.eval("helper_failure_seed_9784 = 1").success);
        let prefix_functions = session
            .persistent_compile
            .as_ref()
            .map(|state| state.prefix_function_count());
        let stored_functions = session.functions.len();

        for _ in 0..3 {
            let failed = session.eval("error(\"stop\"); identity(x -> x + 1)");
            assert!(!failed.success);
            assert!(session.has_live_vm());
            assert_eq!(
                session
                    .persistent_compile
                    .as_ref()
                    .map(|state| state.prefix_function_count()),
                prefix_functions
            );
            assert_eq!(session.functions.len(), stored_functions);
            assert_eq!(
                session.live_vm.as_ref().map(Vm::functions_len),
                prefix_functions
            );
        }

        assert_eq!(
            i64_of(&session.eval("helper_failure_seed_9784 + 1")),
            Some(2)
        );
    });
}

/// A public FunctionValue without frozen candidates must not make a private
/// suffix helper reachable merely because their spellings collide.
#[test]
fn public_source_name_collision_does_not_retain_unreached_helper_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(0);
        assert!(session.eval("__lambda_repl_1_0(x) = x + 100").success);
        let prefix_functions = session
            .persistent_compile
            .as_ref()
            .map(|state| state.prefix_function_count());
        let stored_functions = session.functions.len();

        assert!(
            !session
                .eval("error(\"stop\"); identity(x -> x + 1)")
                .success
        );
        assert_eq!(
            session
                .persistent_compile
                .as_ref()
                .map(|state| state.prefix_function_count()),
            prefix_functions
        );
        assert_eq!(session.functions.len(), stored_functions);
        assert_eq!(
            session.live_vm.as_ref().map(Vm::functions_len),
            prefix_functions
        );

        let rebuilt = session
            .eval("macro force_source_collision_rebuild_9784(); :(1); end; __lambda_repl_1_0(1)");
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert_eq!(i64_of(&rebuilt), Some(101));
    });
}

/// A source definition after the throw is just as unreachable as a helper.
/// Repeated failures must not retain dormant method/code tails.
#[test]
fn wholly_unreached_source_failures_do_not_accumulate_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(0);
        assert!(session.eval("source_failure_seed_9784 = 1").success);
        let prefix_functions = session
            .persistent_compile
            .as_ref()
            .map(|state| state.prefix_function_count());
        let stored_functions = session.functions.len();

        for _ in 0..3 {
            assert!(
                !session
                    .eval("error(\"stop\"); wholly_unreached_source_9784(x) = x + 1")
                    .success
            );
            assert_eq!(
                session
                    .persistent_compile
                    .as_ref()
                    .map(|state| state.prefix_function_count()),
                prefix_functions
            );
            assert_eq!(session.functions.len(), stored_functions);
            assert_eq!(
                session.live_vm.as_ref().map(Vm::functions_len),
                prefix_functions
            );
        }
    });
}

/// Preserve reached scalar writes while rolling an unobserved helper/code
/// suffix back, including replacement of a prior type alias.
#[test]
fn global_writes_before_unreached_helpers_do_not_accumulate_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(0);
        assert!(session.eval("written_failure_count_9784 = 0").success);
        assert!(session.eval("WrittenFailureAlias9784 = Int64").success);
        let prefix_functions = session
            .persistent_compile
            .as_ref()
            .map(|state| state.prefix_function_count());
        let stored_functions = session.functions.len();

        for expected in 1..=3 {
            assert!(!session
                .eval(
                    "written_failure_count_9784 += 1; WrittenFailureAlias9784 = 7; error(\"stop\"); identity(x -> x + 1)",
                )
                .success);
            assert_eq!(
                session
                    .persistent_compile
                    .as_ref()
                    .map(|state| state.prefix_function_count()),
                prefix_functions
            );
            assert_eq!(session.functions.len(), stored_functions);
            assert_eq!(
                session.live_vm.as_ref().map(Vm::functions_len),
                prefix_functions
            );
            assert_eq!(
                i64_of(&session.eval("written_failure_count_9784")),
                Some(expected)
            );
        }

        let rebuilt = session.eval(
            "macro force_written_failure_rebuild_9784(); :(1); end; (written_failure_count_9784, WrittenFailureAlias9784)",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(
            rebuilt.value,
            Some(Value::Tuple(tuple))
                if matches!(tuple.elements.as_slice(), [Value::I64(3), Value::I64(7)])
        ));
    });
}

/// A syntactically present value assignment is not a committed rebinding when
/// its branch never executes. The prior static alias must remain available to
/// the next full rebuild (Issue #9784).
#[test]
fn successful_unreached_alias_rebinding_keeps_prior_alias_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(9784);
        assert!(session.eval("SuccessAlias9784 = Int64").success);

        let skipped = session.eval(
            "macro force_skipped_alias_full_rebuild_9784(); :(1); end; \
             if false; SuccessAlias9784 = 7; end; nothing",
        );
        assert!(skipped.success, "{:?}", skipped.error);
        assert!(
            session
                .type_aliases
                .iter()
                .any(|alias| alias.name == "SuccessAlias9784"),
            "an untaken value assignment must not invalidate the prior alias"
        );

        let rebuilt = session.eval(
            "macro force_success_alias_rebuild_9784(); :(1); end; \
             success_alias_method_9784(x::SuccessAlias9784) = x + 1",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));
        let called = session.eval("success_alias_method_9784(41)");
        assert!(called.success, "{:?}", called.error);
        assert_eq!(i64_of(&called), Some(42));
    });
}

#[test]
fn runtime_global_write_provenance_controls_persistence_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(9784);

        assert!(
            session
                .eval("if false; never_written_9784 = 1; end; nothing")
                .success
        );
        assert!(!session.eval("never_written_9784").success);

        assert_eq!(
            i64_of(&session.eval("let lexical_only_9784 = 1; lexical_only_9784 end")),
            Some(1)
        );
        assert!(!session.eval("lexical_only_9784").success);

        assert!(
            session
                .eval("let; global hard_global_9784 = 2; end")
                .success
        );
        assert_eq!(i64_of(&session.eval("hard_global_9784")), Some(2));

        assert!(
            session
                .eval("set_called_global_9784() = (global called_global_9784 = 3)")
                .success
        );
        assert!(session.eval("set_called_global_9784()").success);
        assert_eq!(i64_of(&session.eval("called_global_9784")), Some(3));

        assert!(
            session
                .eval("set_error_global_9784() = (global error_global_9784 = 4)")
                .success
        );
        assert!(
            !session
                .eval("set_error_global_9784(); error(\"stop\")")
                .success
        );
        assert_eq!(i64_of(&session.eval("error_global_9784")), Some(4));

        assert!(session.eval("RuntimeAlias9784 = Int64").success);
        assert!(
            session
                .eval("set_runtime_alias_9784() = (global RuntimeAlias9784 = 5)")
                .success
        );
        assert!(
            !session
                .eval("set_runtime_alias_9784(); error(\"stop\")")
                .success
        );
        assert!(
            session.live_vm.as_ref().is_some_and(|vm| vm
                .repl_explicit_global_write_names()
                .contains("RuntimeAlias9784")),
            "the VM must retain executed StoreGlobalAny provenance across recovery"
        );
        assert!(
            session
                .type_aliases
                .iter()
                .all(|alias| alias.name != "RuntimeAlias9784"),
            "the executed global store must invalidate the static alias"
        );
        let forced = session.eval("macro force_runtime_alias_rebuild_9784(); :(1); end");
        assert!(forced.success, "{:?}", forced.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));
        assert!(
            session.live_vm.as_ref().is_some_and(|vm| !vm
                .repl_explicit_global_write_names()
                .contains("RuntimeAlias9784")),
            "a new evaluation must clear the prior execution's write trace"
        );
        assert!(
            session
                .type_aliases
                .iter()
                .all(|alias| alias.name != "RuntimeAlias9784"),
            "a later full rebuild must not resurrect invalidated alias metadata"
        );
        let definition = session.eval("runtime_alias_method_9784(x::RuntimeAlias9784) = x");
        if definition.success {
            // sjulia currently accepts a value-bound annotation as an unresolved
            // nominal struct (Issue #11711). Until that validation gap is fixed,
            // prove the old Int64 alias was still invalidated by observing dispatch.
            let invalid_call = session.eval("runtime_alias_method_9784(5)");
            assert!(
                !invalid_call.success,
                "a value binding remained usable as the stale Int64 alias: {invalid_call:?}"
            );
        }
    });
}

/// Qualified stores belong to their module mirror, never to Main's persistent
/// value map. Otherwise redefining the module can resurrect removed members.
#[test]
fn module_global_writes_never_leak_into_main_persistence_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(9784);
        let defined = session.eval(
            "module QualifiedWrites9784\n\
             x = 1\n\
             setx() = (global x = 2)\n\
             set_dynamic() = (global dynamic = 5)\n\
             failx() = (global x = 3; error(\"stop\"))\n\
             end",
        );
        assert!(defined.success, "{:?}", defined.error);
        assert!(!session
            .variable_names()
            .iter()
            .any(|name| name == "QualifiedWrites9784.x"));

        assert!(session.eval("QualifiedWrites9784.set_dynamic()").success);
        let rebuilt = session.eval("macro force_qualified_rebuild_9784(); :(1); end");
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));
        assert_eq!(
            i64_of(&session.eval("QualifiedWrites9784.dynamic")),
            Some(5),
            "a runtime-created module binding must survive a fresh VM rebuild"
        );
        assert!(!session
            .variable_names()
            .iter()
            .any(|name| name == "QualifiedWrites9784.dynamic"));

        assert!(session.eval("QualifiedWrites9784.setx()").success);
        assert_eq!(i64_of(&session.eval("QualifiedWrites9784.x")), Some(2));
        assert!(!session
            .variable_names()
            .iter()
            .any(|name| name == "QualifiedWrites9784.x"));

        assert!(!session.eval("QualifiedWrites9784.failx()").success);
        assert_eq!(i64_of(&session.eval("QualifiedWrites9784.x")), Some(3));
        assert!(!session
            .variable_names()
            .iter()
            .any(|name| name == "QualifiedWrites9784.x"));

        let redefined = session.eval("module QualifiedWrites9784\ny = 4\nend");
        assert!(redefined.success, "{:?}", redefined.error);
        let removed = session.eval("isdefined(QualifiedWrites9784, :x)");
        assert!(removed.success, "{:?}", removed.error);
        assert!(matches!(removed.value, Some(Value::Bool(false))));
        let removed_dynamic = session.eval("isdefined(QualifiedWrites9784, :dynamic)");
        assert!(removed_dynamic.success, "{:?}", removed_dynamic.error);
        assert!(matches!(removed_dynamic.value, Some(Value::Bool(false))));
    });
}

/// Struct-bearing full rebuilds must direct-seed prior struct values whenever
/// an assignment expression can replace them, including nested expression
/// positions that are not statement bodies.
#[test]
fn nested_expression_struct_rebindings_are_seeded_9784() {
    with_large_stack(|| {
        let mut session = REPLSession::new(9784);
        assert!(
            session
                .eval("struct NestedSeed9784; x::Int; end; nested_seed_9784 = NestedSeed9784(1)")
                .success
        );

        let condition = session.eval(
            "struct ConditionCarrier9784; x::Int; end; \
             if (nested_seed_9784 = NestedSeed9784(2); true); nothing; end",
        );
        assert!(condition.success, "{:?}", condition.error);
        assert_eq!(i64_of(&session.eval("nested_seed_9784.x")), Some(2));

        let comprehension = session.eval(
            "struct ComprehensionCarrier9784; x::Int; end; \
             [(nested_seed_9784 = NestedSeed9784(i)).x for i in 3:3]",
        );
        assert!(comprehension.success, "{:?}", comprehension.error);
        assert_eq!(i64_of(&session.eval("nested_seed_9784.x")), Some(3));
    });
}
