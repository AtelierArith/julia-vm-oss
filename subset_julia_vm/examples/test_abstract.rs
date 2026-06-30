use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

fn main() {
    // Test 1: Check isa with just struct (no abstract)
    println!("=== Test 1: isa(d, Dog) ===");
    let src1 = r#"
struct Dog
    name::String
end
d = Dog("Rex")
result = 0
if isa(d, Dog)
    result = 1
end
result
"#;
    run_test(src1);

    // Test 2: Check isa with abstract parent
    println!("\n=== Test 2: isa(d, Animal) with abstract type ===");
    let src2 = r#"
abstract type Animal end
struct Dog <: Animal
    name::String
end
d = Dog("Rex")
result = 0
if isa(d, Animal)
    result = 1
end
result
"#;
    run_test(src2);
}

fn run_test(src: &str) {
    let mut parser = Parser::new().expect("Parser init failed");
    let parsed = parser.parse(src).expect("Parse failed");
    println!("Parsed OK");
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(parsed).expect("Lowering failed");
    println!(
        "Lowered OK: {:?} abstract types",
        program.abstract_types.len()
    );
    println!(
        "Abstract types: {:?}",
        program
            .abstract_types
            .iter()
            .map(|a| (&a.name, &a.parent))
            .collect::<Vec<_>>()
    );
    println!(
        "Structs: {:?}",
        program
            .structs
            .iter()
            .map(|s| (&s.name, &s.parent_type))
            .collect::<Vec<_>>()
    );
    let compiled = compile_core_program(&program).expect("Compile failed");
    println!(
        "Compiled OK: {:?} abstract types",
        compiled.abstract_types.len()
    );
    println!(
        "Struct defs: {:?}",
        compiled
            .struct_defs
            .iter()
            .map(|s| (&s.name, &s.parent_type))
            .collect::<Vec<_>>()
    );
    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    match vm.run() {
        Ok(v) => println!("Result: {:?}", v),
        Err(e) => println!("Error: {:?}", e),
    }
}
