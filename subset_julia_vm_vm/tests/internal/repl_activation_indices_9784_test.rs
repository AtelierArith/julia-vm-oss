#[test]
fn repl_activation_indices_allow_helpers_around_primary_and_refresh_9784() {
    let vm = Vm::new(Vec::new(), StableRng::new(0));
    let counts = ReplAppendDefinitionCounts {
        function_bodies: 4,
        source_functions: 1,
        structs: 0,
        abstract_types: 0,
        primitive_types: 0,
        enums: 0,
    };
    let activation = [ReplDefinitionActivation::FunctionGroup {
        primary: 1,
        refresh: vec![3],
    }];
    assert!(vm
        .prepare_repl_append_setup(
            counts,
            Vec::new(),
            &[ReplDefinitionActivation::FunctionGroup {
                primary: 1,
                refresh: vec![1],
            }],
            &[],
        )
        .is_none());
    assert!(vm
        .prepare_repl_append_setup(
            counts,
            Vec::new(),
            &[ReplDefinitionActivation::Function(4)],
            &[],
        )
        .is_none());
    assert!(vm
        .prepare_repl_append_setup(
            counts,
            Vec::new(),
            &[
                ReplDefinitionActivation::Function(1),
                ReplDefinitionActivation::FunctionGroup {
                    primary: 2,
                    refresh: vec![1],
                },
            ],
            &[],
        )
        .is_none());

    let prepared = vm.prepare_repl_append_setup(counts, Vec::new(), &activation, &[]);
    assert!(
        prepared.is_some(),
        "helper-primary-helper-refresh layout must pass preflight"
    );
    let Some(prepared) = prepared else {
        return;
    };
    let mut vm = vm;
    for (name, min_world) in [
        ("helper_before_9784", 1),
        ("primary_9784", u64::MAX),
        ("helper_after_9784", 1),
        ("refresh_9784", u64::MAX),
    ] {
        let mut function = dispatch_test_function(name, vec![], vec![]);
        function.min_world = min_world;
        vm.functions.push(Rc::new(function));
    }
    vm.reenter_appended_main(&[], &[], StableRng::new(1));
    vm.install_prepared_repl_append_setup(prepared);
    let before = vm.repl_definition_world_fingerprint();

    assert_eq!(
        vm.repl_reached_appended_definition_prefix(
            before,
            &activation,
            &[],
            ReplAppendDefinitionStarts {
                functions: 0,
                structs: 0,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            counts,
            &[1],
        ),
        Some(ReachedReplDefinitionPrefix {
            function_count: 0,
            runtime_constructor_indices: Vec::new(),
            struct_count: 0,
            abstract_type_count: 0,
            primitive_type_count: 0,
            enum_count: 0,
            runtime_nominal_activations: Vec::new(),
            runtime_function_indices: Vec::new(),
        })
    );
    assert_eq!(vm.functions[0].min_world, 1);
    assert_eq!(vm.functions[1].min_world, u64::MAX);
    assert_eq!(vm.functions[2].min_world, 1);
    assert_eq!(vm.functions[3].min_world, u64::MAX);

    vm.activate_eval_function(1);
    assert_eq!(vm.functions[0].min_world, 1);
    assert_eq!(vm.functions[1].min_world, before.current_world + 1);
    assert_eq!(vm.functions[2].min_world, 1);
    assert_eq!(vm.functions[3].min_world, before.current_world + 1);
    assert_eq!(
        vm.repl_reached_appended_definition_prefix(
            before,
            &activation,
            &[],
            ReplAppendDefinitionStarts {
                functions: 0,
                structs: 0,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            counts,
            &[1],
        ),
        Some(ReachedReplDefinitionPrefix {
            function_count: 1,
            runtime_constructor_indices: Vec::new(),
            struct_count: 0,
            abstract_type_count: 0,
            primitive_type_count: 0,
            enum_count: 0,
            runtime_nominal_activations: Vec::new(),
            runtime_function_indices: Vec::new(),
        })
    );
    assert!(vm
        .repl_reached_appended_definition_prefix(
            before,
            &activation,
            &[],
            ReplAppendDefinitionStarts {
                functions: 0,
                structs: 0,
                abstract_types: 0,
                primitive_types: 0,
                enums: 0,
            },
            counts,
            &[0],
        )
        .is_none());
}
