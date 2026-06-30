use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::intrinsics::Intrinsic;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, Instr, Value, Vm};

fn main() {
    // Test 1: Direct VM execution
    println!("=== Test 1: Direct VM execution ===");
    let code = vec![
        Instr::PushI64(10),
        Instr::PushI64(32),
        Instr::CallIntrinsic(Intrinsic::AddInt),
        Instr::ReturnI64,
    ];

    let program = CompiledProgram {
        code,
        functions: vec![],
        struct_defs: vec![],
        abstract_types: vec![],
        primitive_types: vec![],
        show_methods: vec![],
        entry: 0,
        specializable_functions: vec![],
        runtime_specialization_map: vec![],
        compile_context: None,
        base_function_count: 0,
        macro_bindings: std::collections::HashMap::new(),
        global_slot_names: vec![],
        global_slot_types: vec![],
        global_slot_count: 0,
    };

    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(program, rng);

    match vm.run() {
        Ok(Value::I64(x)) => println!("Result: {} (expected 42)", x),
        Ok(other) => println!("Unexpected value: {:?}", other),
        Err(e) => println!("Error: {:?}", e),
    }

    // Test 2: Full pipeline
    println!("\n=== Test 2: Full pipeline ===");
    let src = r#"
function f(N)
    return N + 10
end
f(32)
"#;

    let mut parser = Parser::new().expect("Parser init failed");
    let parsed = parser.parse(src).expect("Parse failed");
    println!("Parsed successfully");

    let mut lowering = Lowering::new(src);
    let program = lowering.lower(parsed).expect("Lowering failed");
    println!("Lowered successfully");

    let compiled = compile_core_program(&program).expect("Compile failed");
    println!(
        "Compiled successfully, {} instructions",
        compiled.code.len()
    );
    println!("Entry point: {}", compiled.entry);
    println!("Functions: {:?}", compiled.functions);

    // Print instructions
    println!("\nInstructions:");
    for (i, instr) in compiled.code.iter().enumerate() {
        let marker = if i == compiled.entry {
            " <-- ENTRY"
        } else {
            ""
        };
        println!("  {:4}: {:?}{}", i, instr, marker);
    }

    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {
        Ok(Value::I64(x)) => println!("\nResult: {} (expected 42)", x),
        Ok(Value::F64(x)) => println!("\nResult F64: {} (expected 42.0)", x),
        Ok(other) => println!("\nUnexpected value: {:?}", other),
        Err(e) => println!("\nError: {:?}", e),
    }
}
