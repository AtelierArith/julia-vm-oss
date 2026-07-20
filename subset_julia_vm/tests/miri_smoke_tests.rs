use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Instr, Value};

fn minimal_stack_program() -> CompiledProgram {
    CompiledProgram {
        code: vec![
            Instr::PushI64(40),
            Instr::StoreSlotI64(0),
            Instr::LoadSlotI64(0),
            Instr::PushI64(2),
            Instr::AddI64,
            Instr::ReturnI64,
        ],
        source_map: Vec::new(),
        functions: Vec::new(),
        struct_defs: Vec::new(),
        abstract_types: Vec::new(),
        primitive_types: Vec::new(),
        enum_defs: Vec::new(),
        show_methods: Vec::new(),
        print_methods: Vec::new(),
        entry: 0,
        specializable_functions: Vec::new(),
        runtime_specialization_map: Vec::new(),
        inference_global_types_snapshot: Vec::new(),
        specialization_disable_flags: Default::default(),
        compile_context: None,
        base_function_count: 0,
        macro_bindings: Default::default(),
        module_registry: Default::default(),
        global_slot_names: vec!["x".to_string()],
        global_slot_types: Vec::new(),
        global_slot_count: 1,
        main_scope_names: Default::default(),
    }
}

#[test]
fn miri_runs_minimal_vm_bytecode_program_9004() {
    let mut vm = Vm::new_program(minimal_stack_program(), StableRng::new(0));
    let value = vm.run().expect("minimal VM bytecode should run");
    assert!(
        matches!(value, Value::I64(42)),
        "unexpected value: {value:?}"
    );
}
