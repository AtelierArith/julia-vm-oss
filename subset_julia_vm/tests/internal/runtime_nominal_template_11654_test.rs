#[test]
fn runtime_nominal_statement_compiles_to_inert_template_11654() {
    let handle = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let Ok(program) = crate::pipeline::parse_and_lower(
                "if true; abstract type RuntimeAbstract11654 end; end",
            ) else {
                unreachable!("runtime abstract should lower");
            };
            let Ok(compiled) = crate::compile::host_support::compile_core_program(&program) else {
                unreachable!("runtime abstract should compile");
            };

            assert!(!compiled
                .abstract_types
                .iter()
                .any(|definition| definition.name == "RuntimeAbstract11654"));
            assert!(compiled.code.iter().any(|instruction| matches!(
                instruction,
                Instr::DefineRuntimeNominal(operands)
                    if matches!(
                        &operands.definition,
                        RuntimeNominalDefInfo::AbstractType(definition)
                            if definition.name == "RuntimeAbstract11654"
                    )
            )));
        });
    let Ok(handle) = handle else {
        unreachable!("spawn compiler test thread");
    };
    assert!(
        handle.join().is_ok(),
        "compiler test thread should not panic"
    );
}
