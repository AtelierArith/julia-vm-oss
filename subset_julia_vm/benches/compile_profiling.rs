//! Detailed compilation profiling benchmark
//!
//! This benchmark measures individual phases within the compilation step
//! to identify the exact bottleneck.
//!
//! Run with: cargo bench --bench compile_profiling

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use std::time::Instant;
use subset_julia_vm::compile::compile_with_cache;

// Access private parse_and_lower by using compile_and_run_value
fn parse_and_lower_helper(src: &str) -> subset_julia_vm::ir::core::Program {
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;

    // First parse
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(src).unwrap();
    let mut lowering = Lowering::new(src);
    let mut user_program = lowering.lower(outcome).unwrap();

    // Merge with prelude (like lib.rs does)
    if let Some(prelude) = subset_julia_vm::get_prelude_program() {
        fn get_method_signature(func: &subset_julia_vm::ir::core::Function) -> String {
            let param_types: Vec<String> = func
                .params
                .iter()
                .map(|p| p.effective_type().to_string())
                .collect();
            format!("{}({})", func.name, param_types.join(", "))
        }

        let user_method_sigs: std::collections::HashSet<_> = user_program
            .functions
            .iter()
            .map(get_method_signature)
            .collect();

        let user_func_names_non_base: std::collections::HashSet<_> = user_program
            .functions
            .iter()
            .filter(|f| !f.is_base_extension)
            .map(|f| f.name.as_str())
            .collect();

        let user_struct_names: std::collections::HashSet<_> = user_program
            .structs
            .iter()
            .map(|s| s.name.as_str())
            .collect();

        let mut all_structs: Vec<subset_julia_vm::ir::core::StructDef> = prelude
            .structs
            .iter()
            .filter(|s| !user_struct_names.contains(s.name.as_str()))
            .cloned()
            .collect();
        all_structs.extend(user_program.structs);
        user_program.structs = all_structs;

        let mut all_functions: Vec<subset_julia_vm::ir::core::Function> = prelude
            .functions
            .iter()
            .filter(|f| {
                if f.is_base_extension {
                    !user_method_sigs.contains(&get_method_signature(f))
                } else {
                    !user_func_names_non_base.contains(f.name.as_str())
                }
            })
            .cloned()
            .collect();
        let base_function_count = all_functions.len();
        all_functions.extend(user_program.functions);
        user_program.functions = all_functions;
        user_program.base_function_count = base_function_count;

        let user_abstract_type_names: std::collections::HashSet<_> = user_program
            .abstract_types
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        let mut all_abstract_types: Vec<subset_julia_vm::ir::core::AbstractTypeDef> = prelude
            .abstract_types
            .iter()
            .filter(|a| !user_abstract_type_names.contains(a.name.as_str()))
            .cloned()
            .collect();
        all_abstract_types.extend(user_program.abstract_types);
        user_program.abstract_types = all_abstract_types;
    }

    user_program
}

fn profile_compilation(c: &mut Criterion) {
    let source = r#"
function fib(n)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end
fib(10)
"#;

    // Pre-parse and merge with Base (like the real pipeline)
    let program = parse_and_lower_helper(source);

    // Print program statistics
    println!("\n=== Program Statistics ===");
    println!("Total functions: {}", program.functions.len());
    println!("Base functions: {}", program.base_function_count);
    println!(
        "User functions: {}",
        program.functions.len() - program.base_function_count
    );
    println!("Structs: {}", program.structs.len());
    println!("Abstract types: {}", program.abstract_types.len());
    println!("==========================\n");

    c.bench_function("compile_with_base", |b| {
        b.iter_custom(|iters| {
            let mut total_time = std::time::Duration::ZERO;

            for _ in 0..iters {
                let start = Instant::now();
                let _ = compile_with_cache(black_box(&program)).unwrap();
                total_time += start.elapsed();
            }

            total_time
        });
    });
}

fn profile_simple_program(c: &mut Criterion) {
    let source = "1 + 2 * 3";

    // Pre-parse and merge with Base (like the real pipeline)
    let program = parse_and_lower_helper(source);

    println!("\n=== Simple Program Statistics ===");
    println!("Total functions: {}", program.functions.len());
    println!("Base functions: {}", program.base_function_count);
    println!(
        "User functions: {}",
        program.functions.len() - program.base_function_count
    );
    println!("==================================\n");

    c.bench_function("compile_simple_with_base", |b| {
        b.iter(|| compile_with_cache(black_box(&program)).unwrap());
    });
}

criterion_group!(benches, profile_compilation, profile_simple_program);
criterion_main!(benches);
