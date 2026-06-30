//! Bytecode Compilation and VM Execution Demonstration
//!
//! This example shows how Core IR is compiled to bytecode and executed.
//!
//! Key concepts:
//! - Stack-based VM with typed values (I64, F64, Str, Array, Complex, etc.)
//! - Minimal instruction set: Push, Load, Store, arithmetic ops, control flow
//! - Deterministic PRNG (StableRNG or Xoshiro256++)
//! - Output captured to buffer (for println)
//!
//! Run with: `cargo run --example bytecode_demo`

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

fn main() {
    println!("=== Bytecode Compilation & VM Execution Demo ===\n");

    let examples = vec![
        (
            "Simple arithmetic",
            "1 + 2 * 3",
            "Shows basic arithmetic bytecode generation",
        ),
        (
            "Variable assignment and usage",
            "x = 10\ny = 20\nz = x + y\nz",
            "Shows variable load/store instructions",
        ),
        (
            "println call",
            r#"println("Hello, World!")"#,
            "Shows string handling and print instructions",
        ),
        (
            "For loop with accumulator",
            r#"sum = 0
for i in 1:5
    sum = sum + i
end
sum"#,
            "Shows loop bytecode with jump instructions",
        ),
        (
            "Function call",
            r#"function double(x)
    return x * 2
end
double(21)"#,
            "Shows function compilation and call instructions",
        ),
        (
            "Random number generation",
            "rand()",
            "Shows deterministic PRNG instruction",
        ),
        (
            "Array operations",
            r#"arr = [1.0, 2.0, 3.0]
arr[2]"#,
            "Shows array creation and indexing",
        ),
    ];

    for (name, source, description) in examples {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("【{}】", name);
        println!("  {}", description);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        println!("Source:");
        for line in source.lines() {
            println!("  {}", line);
        }
        println!();

        // Parse and lower
        let mut parser = Parser::new().expect("Failed to create parser");
        let outcome = parser.parse(source).expect("Parse error");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(outcome).expect("Lowering error");

        // Compile to bytecode
        let compiled = compile_core_program(&program).expect("Compile error");

        println!("【Compiled Bytecode】");
        println!("  Total instructions: {}", compiled.code.len());
        println!("  Entry point: {}", compiled.entry);
        if !compiled.functions.is_empty() {
            println!("  Functions: {}", compiled.functions.len());
            for (i, func) in compiled.functions.iter().enumerate() {
                println!("    [{}] {} (entry: {})", i, func.name, func.entry);
            }
        }
        println!();

        println!("Instructions:");
        for (i, instr) in compiled.code.iter().enumerate() {
            let marker = if i == compiled.entry {
                " ◄ entry"
            } else {
                ""
            };

            // Check if this is a function entry point
            let func_marker = compiled
                .functions
                .iter()
                .find(|f| f.entry == i)
                .map(|f| format!(" ◄ func '{}'", f.name))
                .unwrap_or_default();

            println!("  {:4}: {:?}{}{}", i, instr, marker, func_marker);
        }
        println!();

        // Execute on VM
        println!("【VM Execution】");
        let seed = 42u64;
        println!("  RNG seed: {}", seed);

        let rng = StableRng::new(seed);
        let mut vm = Vm::new_program(compiled, rng);

        match vm.run() {
            Ok(result) => {
                let output = vm.get_output();
                if !output.is_empty() {
                    println!("  Output:");
                    for line in output.lines() {
                        println!("    {}", line);
                    }
                }
                println!("  Result: {:?}", result);
            }
            Err(e) => {
                println!("  Runtime error: {:?}", e);
            }
        }

        println!();
    }

    // Demonstrate deterministic execution
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("【Deterministic Execution Demo】");
    println!("  Same code + same seed = same result");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let rand_source = r#"
sum = 0.0
for i in 1:5
    sum = sum + rand()
end
sum
"#;

    println!("Source: sum of 5 random numbers");

    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(rand_source).unwrap();
    let mut lowering = Lowering::new(rand_source);
    let program = lowering.lower(outcome).unwrap();
    let compiled = compile_core_program(&program).unwrap();

    for seed in [42, 42, 123, 42] {
        let rng = StableRng::new(seed);
        let mut vm = Vm::new_program(compiled.clone(), rng);
        let result = vm.run().unwrap();
        println!("  seed={:3} → result={:?}", seed, result);
    }

    println!("\n  ↑ Notice: seed=42 always produces the same result!");

    println!("\n=== Bytecode Demo Complete ===");
}
