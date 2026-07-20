#[test]
fn resolved_closure_snapshot_keeps_candidate_authority_9784() {
    let code = vec![
        Instr::CreateResolvedClosure(Box::new(ResolvedClosureOperands {
            name: "resolved_closure_9784".to_string(),
            capture_names: vec!["generic_local".to_string()],
            candidate_indices: vec![7, 9],
        })),
        Instr::ReturnAny,
    ];
    let mut vm = vm_with_all_frame_binding_namespaces(code);
    let result = vm.run();
    assert!(matches!(&result, Ok(Value::Closure(_))), "{result:?}");
    let Ok(Value::Closure(closure)) = result else {
        return;
    };
    assert_eq!(closure.candidate_indices, Some(vec![7, 9]));
    assert_frame_binding_value(
        "generic_local",
        closure.get_capture("generic_local").cloned(),
    );
}
