use subset_julia_vm::compile::host_support::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

fn main() {
    let src = r#"
function simulate(steps)
    for i in 1:steps
        println("Step $i")
        sleep(0.5)
    end
end

println("Starting simulation...")
simulate(5)
println("Done!")
"#;

    println!("=== Testing Real-time Output with sleep ===\n");

    let mut parser = Parser::new().expect("Parser init failed");
    let parsed = parser.parse(src).expect("Parse failed");
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(parsed).expect("Lowering failed");
    let compiled = compile_core_program(&program).expect("Compile failed");
    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);

    println!("Executing Julia code (output should appear every 0.5 seconds):\n");

    match vm.run() {
        Ok(_) => println!("\n[✓ Execution completed successfully]"),
        Err(e) => eprintln!("\n[✗ Error: {}]", e),
    }
}
