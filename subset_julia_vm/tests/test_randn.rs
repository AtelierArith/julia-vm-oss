use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::compile_and_run_str;
use subset_julia_vm::julia::base;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

/// Helper to run code and return the VM output (println output)
fn run_with_output(src: &str, seed: u64) -> String {
    // Parse Base source
    let prelude_src = base::get_base();
    let mut parser = Parser::new().expect("Parser init failed");
    let prelude_parsed = parser.parse(&prelude_src).expect("Prelude parse failed");
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_parsed)
        .expect("Prelude lowering failed");

    // Parse user source
    let mut parser = Parser::new().expect("Parser init failed");
    let parsed = parser.parse(src).expect("Parse failed");
    let mut lowering = Lowering::new(src);
    let mut program = lowering.lower(parsed).expect("Lowering failed");

    // Merge prelude functions
    let mut all_functions = prelude_program.functions;
    all_functions.extend(program.functions);
    program.functions = all_functions;

    // Merge prelude structs
    let mut all_structs = prelude_program.structs;
    all_structs.extend(program.structs);
    program.structs = all_structs;

    let compiled = compile_core_program(&program).expect("Compile failed");

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    let _ = vm.run();
    vm.get_output().to_string()
}

#[test]
fn test_randn_basic() {
    let src = r#"
samples = randn(10)
length(samples)
"#;
    let result = compile_and_run_str(src, 42);
    println!("Length: {}", result);
    assert!((result - 10.0).abs() < 1e-10, "Expected 10, got {}", result);
}

#[test]
fn test_bins_access() {
    let src = r#"
bins = zeros(6)
bins[3] = 10.0
bins[4] = 20.0
bins[3] + bins[4]
"#;
    let result = compile_and_run_str(src, 0);
    println!("bins[3] + bins[4] = {}", result);
    assert!((result - 30.0).abs() < 1e-10, "Expected 30, got {}", result);
}

#[test]
fn test_add_assign_to_bins() {
    let src = r#"
bins = zeros(3)
bins[1] += 1
bins[1] += 1
bins[1]
"#;
    let result = compile_and_run_str(src, 0);
    println!("bins[1] after += 2: {}", result);
    assert!((result - 2.0).abs() < 1e-10, "Expected 2, got {}", result);
}

#[test]
fn test_randn_values_match_julia() {
    // Verify randn(n) produces the same first value as Julia's StableRNG(42)
    let src = r#"
samples = randn(10)
samples[1]
"#;
    let result = compile_and_run_str(src, 42);
    // First randn value from Julia StableRNG(42) is -0.6702516921145671
    println!("VM result (first sample): {}", result);
    let expected = -0.6702516921145671;
    assert!(
        (result - expected).abs() < 1e-10,
        "Expected {}, got {}. Values should match Julia StableRNG(42).",
        expected,
        result
    );
}

#[test]
fn test_randn_first_10_values() {
    // Verify first 10 randn values match Julia exactly
    let src = r#"
samples = randn(1000)
# Print first 10 values
for i in 1:10
    println(samples[i])
end
samples[10]
"#;
    let result = compile_and_run_str(src, 42);
    // Julia's 10th value is 1.2973461452176338
    println!("VM 10th value: {}", result);
    let expected = 1.2973461452176338;
    assert!(
        (result - expected).abs() < 1e-10,
        "10th value mismatch: Expected {}, got {}",
        expected,
        result
    );
}

#[test]
fn test_minimal_bug() {
    // Minimal reproduction: intermediate variable breaks Float64 comparison
    let src = r#"
arr = [1.5, -0.5, 0.5]
x = arr[2]
println("x = ", x)
println("x < 0 = ", x < 0)
x < 0
"#;
    let output = run_with_output(src, 0);
    println!("VM Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    // -0.5 < 0 should be true
    assert!(
        (result - 1.0).abs() < 1e-10,
        "arr[2] stored in x: x < 0 should be true for -0.5, got {}",
        result
    );
}

#[test]
fn test_direct_index_comparison() {
    // Test comparison directly on array index (no intermediate variable)
    let src = r#"
samples = randn(1)
println("samples[1] = ", samples[1])
println("samples[1] < 0 = ", samples[1] < 0)
println("samples[1] < 0.0 = ", samples[1] < 0.0)
samples[1] < 0
"#;
    let output = run_with_output(src, 42);
    println!("VM Output:\n{}", output);
    let result = compile_and_run_str(src, 42);
    // -0.67 < 0 should be true
    assert!(
        (result - 1.0).abs() < 1e-10,
        "samples[1] < 0 should be true for -0.67, got {}",
        result
    );
}

#[test]
fn test_array_value_comparison() {
    // Test comparison with array-fetched value
    let src = r#"
samples = randn(1)
x = samples[1]
println("x = ", x)
println("typeof(x) = ", typeof(x))
println("x < 0 = ", x < 0)
println("x < 0.0 = ", x < 0.0)
println("x < -1 = ", x < -1)
println("x < -1.0 = ", x < -1.0)
result = 0
if x < 0.0
    result = 1
end
println("if x < 0.0 then result = ", result)
x < 0.0
"#;
    let output = run_with_output(src, 42);
    println!("VM Output:\n{}", output);
    // First value is -0.6702516921145671 which is < 0
    let result = compile_and_run_str(src, 42);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "samples[1] < 0.0 should be true for -0.67, got {}",
        result
    );
}

#[test]
fn test_negative_comparison() {
    // Test x < 0 comparison with negative value
    let src = r#"
x = -0.5
println("x = ", x)
println("x < 0 = ", x < 0)
println("x < -1 = ", x < -1)
println("x < 1 = ", x < 1)
x < 0
"#;
    let output = run_with_output(src, 0);
    println!("VM Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    // -0.5 < 0 should be true (1.0)
    assert!(
        (result - 1.0).abs() < 1e-10,
        "-0.5 < 0 should be true, got {}",
        result
    );
}

#[test]
fn test_binning_with_array() {
    // Test binning first 10 randn values
    let src = r#"
samples = randn(10)
println("Testing first 10 samples:")
for i in 1:10
    x = samples[i]
    if x < -2
        println(i, ": x=", x, " -> bin 1 (x < -2)")
    elseif x < -1
        println(i, ": x=", x, " -> bin 2 (-2 <= x < -1)")
    elseif x < 0
        println(i, ": x=", x, " -> bin 3 (-1 <= x < 0)")
    elseif x < 1
        println(i, ": x=", x, " -> bin 4 (0 <= x < 1)")
    elseif x < 2
        println(i, ": x=", x, " -> bin 5 (1 <= x < 2)")
    else
        println(i, ": x=", x, " -> bin 6 (x >= 2)")
    end
end
0.0
"#;
    let output = run_with_output(src, 42);
    println!("VM Output:\n{}", output);
}

#[test]
fn test_elseif_negative_value() {
    // Test elseif with x = -0.5 (should go to bin 3)
    let src = r#"
x = -0.5
bin = 0
if x < -2
    bin = 1
elseif x < -1
    bin = 2
elseif x < 0
    bin = 3
elseif x < 1
    bin = 4
else
    bin = 5
end
println("x = ", x, " -> bin = ", bin)
bin
"#;
    let output = run_with_output(src, 0);
    println!("VM Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    // x = -0.5 should be in bin 3 (-1 <= x < 0)
    assert!(
        (result - 3.0).abs() < 1e-10,
        "Expected bin 3, got {}",
        result
    );
}

#[test]
fn test_randn_bins_match_julia() {
    // Verify that bin counting matches Julia's result (668)
    let src = r#"
n = 1000
samples = randn(n)

bins = zeros(6)

for i in 1:n
    x = samples[i]
    if x < -2
        bins[1] += 1
    elseif x < -1
        bins[2] += 1
    elseif x < 0
        bins[3] += 1
    elseif x < 1
        bins[4] += 1
    elseif x < 2
        bins[5] += 1
    else
        bins[6] += 1
    end
end

println("bins[1] = ", bins[1])
println("bins[2] = ", bins[2])
println("bins[3] = ", bins[3])
println("bins[4] = ", bins[4])
println("bins[5] = ", bins[5])
println("bins[6] = ", bins[6])
println("bins[3] + bins[4] = ", bins[3] + bins[4])
bins[3] + bins[4]
"#;
    // Get VM output
    let output = run_with_output(src, 42);
    println!("VM Output:\n{}", output);

    let result = compile_and_run_str(src, 42);
    // Julia StableRNG(42) bins:
    // bins[1] = 31.0, bins[2] = 142.0, bins[3] = 339.0
    // bins[4] = 329.0, bins[5] = 137.0, bins[6] = 22.0
    // bins[3] + bins[4] = 668.0
    println!("VM result: bins[3] + bins[4] = {}", result);
    let expected = 668.0;
    assert!(
        (result - expected).abs() < 1e-10,
        "Expected {} (Julia), got {}.",
        expected,
        result
    );
}
