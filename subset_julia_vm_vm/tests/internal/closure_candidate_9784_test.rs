#[test]
fn closure_candidates_preserve_source_helper_provenance_9784() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));
    let source = function_info(
        "same_closure_name_9784",
        vec![JuliaType::Int64],
        vec![],
        vec![],
        None,
    );
    let mut helper = source.clone();
    helper.is_lowering_helper = true;
    vm.functions.push(Rc::new(source));
    vm.functions.push(Rc::new(helper));
    vm.function_name_index
        .insert("same_closure_name_9784".to_string(), vec![0]);
    vm.lowering_helper_name_index
        .insert("same_closure_name_9784".to_string(), vec![1]);

    let source_closure = Value::Closure(vm.closure_value_with_candidates(
        "same_closure_name_9784",
        vec![("captured".to_string(), Value::I64(100))],
        vec![0],
    ));
    let helper_closure = Value::Closure(vm.closure_value_with_candidates(
        "same_closure_name_9784",
        vec![("captured".to_string(), Value::I64(1))],
        vec![1],
    ));
    let unresolved = Value::Closure(ClosureValue::new(
        "same_closure_name_9784",
        vec![("captured".to_string(), Value::I64(7))],
    ));

    for (value, expected) in [(&source_closure, 0), (&helper_closure, 1)] {
        let candidates = vm
            .collect_runtime_callable_candidates(value, "same_closure_name_9784")
            .unwrap_or_default();
        assert_eq!(
            candidates
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            vec![expected]
        );
    }
    let unresolved_candidates = vm
        .collect_runtime_callable_candidates(&unresolved, "same_closure_name_9784")
        .unwrap_or_default();
    assert_eq!(
        unresolved_candidates
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        vec![0],
        "legacy name-only closures must never discover private helpers"
    );

    assert_ne!(
        vm.get_type_name(&source_closure),
        vm.get_type_name(&helper_closure),
        "same-spelled source and lowering helper need distinct singleton datatypes"
    );
    let source_fingerprint = vm
        .call_site_arg_fingerprint(&source_closure)
        .unwrap_or_default();
    let helper_fingerprint = vm
        .call_site_arg_fingerprint(&helper_closure)
        .unwrap_or_default();
    assert_ne!(
        source_fingerprint, helper_fingerprint,
        "dispatch cache keys must use the stable provenance-aware singleton identity"
    );
}
