//! Integration tests for the pure Rust parser (subset_julia_vm_parser)
//!
//! This test suite verifies that the new parser can parse all fixture files
//! without errors, ensuring compatibility with the existing VM.

use std::fs;
use std::path::PathBuf;
use subset_julia_vm_parser::{parse, parse_with_errors, NodeKind};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn collect_julia_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_julia_files(&path));
            } else if path.extension().is_some_and(|e| e == "jl") {
                files.push(path);
            }
        }
    }
    files
}

#[allow(dead_code)]
fn test_parse_all_fixtures() {
    let fixtures = fixtures_dir();
    let files = collect_julia_files(&fixtures);

    assert!(!files.is_empty(), "No fixture files found");

    let mut passed = 0;
    let mut failed = Vec::new();

    for file in &files {
        let source = fs::read_to_string(file).expect("Failed to read file");
        let (cst, errors) = parse_with_errors(&source);

        if errors.is_empty() {
            passed += 1;
            assert_eq!(cst.kind, NodeKind::SourceFile);
        } else {
            failed.push((file.clone(), errors));
        }
    }

    if !failed.is_empty() {
        eprintln!("\n=== Failed to parse {} files ===", failed.len());
        for (file, errors) in &failed {
            eprintln!("\n--- {} ---", file.display());
            for err in errors.errors() {
                eprintln!("  {:?}", err);
            }
        }
    }

    eprintln!("\nParsed {}/{} fixtures successfully", passed, files.len());
    assert!(failed.is_empty(), "Some fixtures failed to parse");
}

#[allow(dead_code)]
fn test_parse_ternary() {
    let source = r#"
x = 5
y = 3
x > y ? 1.0 : 0.0
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse ternary: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_for_loop() {
    let source = r#"
sum = 0
for i in 1:10
    sum += i
end
sum
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse for loop: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_function() {
    let source = r#"
function factorial(n)
    if n <= 1
        return 1
    else
        return n * factorial(n - 1)
    end
end
factorial(5)
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse function: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_array_operations() {
    let source = r#"
arr = [1, 2, 3, 4, 5]
sum = 0
for x in arr
    sum += x
end
sum
"#;
    let result = parse(source);
    assert!(result.is_ok(), "Failed to parse array: {:?}", result.err());
}

#[allow(dead_code)]
fn test_parse_comprehension() {
    let source = r#"
squares = [x^2 for x in 1:5]
sum(squares)
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse comprehension: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_struct() {
    let source = r#"
struct Point
    x::Float64
    y::Float64
end
p = Point(1.0, 2.0)
p.x + p.y
"#;
    let result = parse(source);
    assert!(result.is_ok(), "Failed to parse struct: {:?}", result.err());
}

#[allow(dead_code)]
fn test_parse_try_catch() {
    let source = r#"
try
    x = 1 / 0
catch e
    x = 0
finally
    println("done")
end
x
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse try-catch: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_while_loop() {
    let source = r#"
x = 10
while x > 0
    x -= 1
end
x
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse while loop: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_string_interpolation() {
    let source = r#"
name = "World"
greeting = "Hello, $name!"
greeting
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse string interpolation: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_lambda() {
    let source = r#"
f = x -> x^2
map(f, [1, 2, 3])
"#;
    let result = parse(source);
    assert!(result.is_ok(), "Failed to parse lambda: {:?}", result.err());
}

#[allow(dead_code)]
fn test_parse_do_syntax() {
    let source = r#"
map([1, 2, 3]) do x
    x^2
end
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse do syntax: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_broadcast() {
    let source = r#"
arr = [1, 2, 3]
arr .+ 1
sin.(arr)
"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse broadcast: {:?}",
        result.err()
    );
}

#[allow(dead_code)]
fn test_parse_matrix() {
    let source = r#"
A = [1 2; 3 4]
B = [5 6; 7 8]
A * B
"#;
    let result = parse(source);
    assert!(result.is_ok(), "Failed to parse matrix: {:?}", result.err());
}

#[allow(dead_code)]
fn test_parse_module() {
    let source = r#"
module MyModule
    export foo
    foo() = 42
end
"#;
    let result = parse(source);
    assert!(result.is_ok(), "Failed to parse module: {:?}", result.err());
}

#[allow(dead_code)]
fn test_parse_macro_call() {
    let source = r#"
@time sum(1:1000000)
@assert 1 + 1 == 2
"#;
    let result = parse(source);
    assert!(result.is_ok(), "Failed to parse macro: {:?}", result.err());
}

#[allow(dead_code)]
fn test_parse_show_macro() {
    let source = r#"@show 1 + 1"#;
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse @show macro: {:?}",
        result.err()
    );
}

/// Test that @show macro works through the entire pipeline:
/// Pure Rust parser -> Lowering -> Compile -> Execute
/// Note: This test uses compile_core_program directly, bypassing the normal pipeline.
/// Fixed in Issue #1330: Macro-local variables are now correctly substituted.
#[allow(dead_code)]
fn test_show_macro_end_to_end() {
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::{ParseOutcome, RustParsedSource};
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    let source = r#"
f(x) = 2x + 1
@show f(3)
"#;

    // This test drives Lowering directly (not through pipeline.rs), so it
    // must install the VM-backed macro expander seam itself or built-in
    // macros like @show fail to lower (Issues #8656 / #9115).
    subset_julia_vm::macro_runtime::install();

    // Parse with Pure Rust parser
    let cst = parse(source).expect("Failed to parse");

    // Create ParseOutcome for the full Lowering pipeline
    let parse_outcome = ParseOutcome::Rust(RustParsedSource {
        cst,
        source: source.to_string(),
    });

    // Lower CST to IR
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parse_outcome).expect("Failed to lower");

    // Compile
    let compiled = compile_core_program(&program).expect("Failed to compile");

    // Execute
    let rng = StableRng::new(42);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run();

    assert!(result.is_ok(), "Failed to run: {:?}", result.err());

    // Check output contains the @show output
    let output = vm.get_output();
    assert!(
        output.contains("f(3) = 7"),
        "Expected @show output 'f(3) = 7', got: {}",
        output
    );
}

#[test]
fn test_inference_meta_markers_lower_to_meta_stmt_issue_4286() {
    use subset_julia_vm::ir::core::{Expr, Stmt};
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::{ParseOutcome, RustParsedSource};

    let source = r#"
function meta_marker_4286(x, y)
    @inline
    @nospecialize x y
    Base.@constprop :aggressive
    Base.@assume_effects :foldable
    @specialize
    x + y
end

@inline function wrapped_meta_4286(x)
    x + 1
end

function callsite_meta_4286(x)
    y = @inline wrapped_meta_4286(x)
    z = @noinline wrapped_meta_4286(x)
    y + z
end
"#;

    let cst = parse(source).expect("Failed to parse");
    let parse_outcome = ParseOutcome::Rust(RustParsedSource {
        cst,
        source: source.to_string(),
    });
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parse_outcome).expect("Failed to lower");
    let func = program
        .functions
        .iter()
        .find(|func| func.name == "meta_marker_4286")
        .expect("meta_marker_4286 should lower as a function");

    let meta_names: Vec<&str> = func
        .body
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Meta { annotation, .. } => Some(annotation.name.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(
        meta_names,
        vec![
            "inline",
            "nospecialize",
            "constprop",
            "assume_effects",
            "specialize"
        ]
    );

    let wrapped = program
        .functions
        .iter()
        .find(|func| func.name == "wrapped_meta_4286")
        .expect("wrapped_meta_4286 should lower as a top-level function");
    let wrapped_meta_name = wrapped.body.stmts.iter().find_map(|stmt| match stmt {
        Stmt::Meta { annotation, .. } => Some(annotation.name.as_str()),
        _ => None,
    });
    assert_eq!(wrapped_meta_name, Some("inline"));

    let callsite = program
        .functions
        .iter()
        .find(|func| func.name == "callsite_meta_4286")
        .expect("callsite_meta_4286 should lower as a function");
    let callsite_wrappers: Vec<&str> = callsite
        .body
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Assign {
                value: Expr::Call { function, .. },
                ..
            } => Some(function.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        callsite_wrappers,
        vec!["#__sjulia_inline__", "#__sjulia_noinline__"]
    );
}

#[test]
fn test_nospecialize_argument_annotation_short_form_issue_5122() {
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::{ParseOutcome, RustParsedSource};

    // Short-form `f(@nospecialize(x)) = ...` keeps `@nospecialize`/`@specialize`
    // as a MacrocallExpression inside the argument list; lowering must unwrap the
    // inner parameter so it binds normally (Issue #5122).
    let source = r#"
f_short_5122(@nospecialize(x)) = x + 1
g_short_5122(@nospecialize(x::Number)) = x * 2
h_short_5122(@nospecialize(x), y::Int) = (x, y)
k_short_5122(@specialize(x)) = x - 1
"#;

    let cst = parse(source).expect("Failed to parse");
    let parse_outcome = ParseOutcome::Rust(RustParsedSource {
        cst,
        source: source.to_string(),
    });
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parse_outcome).expect("Failed to lower");

    let param_names = |name: &str| -> Vec<String> {
        program
            .functions
            .iter()
            .find(|func| func.name == name)
            .unwrap_or_else(|| panic!("{} should lower as a function", name))
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect()
    };

    // The annotated parameter must survive lowering rather than being dropped.
    assert_eq!(param_names("f_short_5122"), vec!["x".to_string()]);
    assert_eq!(param_names("g_short_5122"), vec!["x".to_string()]);
    assert_eq!(
        param_names("h_short_5122"),
        vec!["x".to_string(), "y".to_string()]
    );
    assert_eq!(param_names("k_short_5122"), vec!["x".to_string()]);

    // The type annotation inside `@nospecialize(x::Number)` is preserved.
    let g = program
        .functions
        .iter()
        .find(|func| func.name == "g_short_5122")
        .expect("g_short_5122 should lower as a function");
    assert!(
        g.params[0]
            .type_annotation
            .as_ref()
            .is_some_and(|t| t.to_string().contains("Number")),
        "g_short_5122 should retain the ::Number annotation, got {:?}",
        g.params[0].type_annotation
    );
}

#[test]
fn test_nospecialize_argument_annotation_executes_issue_5122() {
    let (output, _) = run_source(
        r#"
f_short_5122(@nospecialize(x)) = x + 1
g_short_5122(@nospecialize(x::Number)) = x * 2
println(f_short_5122(2))
println(f_short_5122(2.5))
println(g_short_5122(4))
"#,
    )
    .expect("short-form @nospecialize argument should run");
    assert_eq!(output, "3\n3.5\n8\n");
}

// ==================== Tests for failing web samples ====================

/// Helper function for end-to-end tests
fn run_source(source: &str) -> Result<(String, f64), String> {
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::{ParseOutcome, RustParsedSource};
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::Value;

    // Parse with Pure Rust parser
    let cst = parse(source).map_err(|e| format!("Parse error: {:?}", e))?;

    // Create ParseOutcome for the full Lowering pipeline
    let parse_outcome = ParseOutcome::Rust(RustParsedSource {
        cst,
        source: source.to_string(),
    });

    // Lower CST to IR
    let mut lowering = Lowering::new(source);
    let program = lowering
        .lower(parse_outcome)
        .map_err(|e| format!("Lowering error: {}", e))?;

    // Compile
    let compiled = compile_core_program(&program).map_err(|e| format!("Compile error: {:?}", e))?;

    // Execute
    let rng = StableRng::new(42);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run().map_err(|e| format!("Runtime error: {:?}", e))?;

    // Convert result to f64
    let result_f64 = match result {
        Value::F64(f) => f,
        Value::F32(f) => f as f64,
        Value::I64(i) => i as f64,
        Value::I32(i) => i as f64,
        Value::I16(i) => i as f64,
        Value::I8(i) => i as f64,
        Value::Bool(b) => {
            if b {
                1.0
            } else {
                0.0
            }
        }
        Value::Nothing => 0.0,
        _ => return Err(format!("Unexpected result type: {:?}", result)),
    };

    Ok((vm.get_output().to_string(), result_f64))
}

// ==================== #16 Broadcast Operations ====================

#[allow(dead_code)]
fn test_broadcast_dot_plus() {
    let source = r#"
a = [1, 2, 3]
b = [10, 20, 30]
c = a .+ b
c[1]
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 11.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_broadcast_dot_times() {
    let source = r#"
a = [1, 2, 3]
b = [10, 20, 30]
d = a .* b
d[2]
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 40.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_broadcast_scalar() {
    let source = r#"
a = [1, 2, 3]
e = a .* 10
e[3]
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 30.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_broadcast_power() {
    let source = r#"
a = [1, 2, 3]
f = a .^ 2
f[3]
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 9.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_broadcast_function_call() {
    let source = r#"
squares = [1, 4, 9, 16, 25]
roots = sqrt.(squares)
roots[3]
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 3.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

// ==================== #17 Multiplication Table ====================

#[allow(dead_code)]
fn test_transpose() {
    let source = r#"
r = 1:3
rt = r'
length(rt)
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_broadcast_outer_product() {
    let source = r#"
table = .*((1:3)', 1:3)
table[2, 3]
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 6.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

// ==================== #32 Do Syntax ====================

#[allow(dead_code)]
fn test_do_syntax_map() {
    let source = r#"
arr = [1.0, 2.0, 3.0]
result = map(arr) do x
    x^2 + 1
end
result[2]
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 5.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_do_syntax_filter() {
    let source = r#"
data = [1.0, 2.0, 3.0, 4.0, 5.0]
filtered = filter(data) do x
    x > 2
end
length(filtered)
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 3.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_do_syntax_reduce() {
    let source = r#"
data = [1.0, 2.0, 3.0, 4.0]
total = reduce(data) do acc, val
    acc + val
end
total
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 10.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

// ==================== #33/#46 Mutable Struct ====================

#[allow(dead_code)]
fn test_immutable_struct() {
    let source = r#"
struct Point
    x::Float64
    y::Float64
end
p = Point(3.0, 4.0)
p.x + p.y
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 7.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_mutable_struct() {
    let source = r#"
mutable struct Counter
    value::Float64
end
c = Counter(0.0)
c.value = c.value + 1.0
c.value
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 1.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

// ==================== #38 Keyword Arguments ====================

#[allow(dead_code)]
fn test_range_with_keyword() {
    let source = r#"
xs = range(0.0, 1.0; length=5)
length(xs)
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 5.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

#[allow(dead_code)]
fn test_function_with_keyword() {
    let source = r#"
function foo(x; multiplier=2)
    x * multiplier
end
foo(5; multiplier=3)
"#;
    match run_source(source) {
        Ok((output, result)) => {
            println!("Output: {}", output);
            println!("Result: {}", result);
            assert_eq!(result, 15.0);
        }
        Err(e) => panic!("Failed: {}", e),
    }
}

// Generated aggregate chunks for nextest process amortization.
#[test]
fn chunk_000() {
    test_parse_all_fixtures();
    test_parse_ternary();
    test_parse_for_loop();
    test_parse_function();
    test_parse_array_operations();
    test_parse_comprehension();
    test_parse_struct();
    test_parse_try_catch();
    test_parse_while_loop();
    test_parse_string_interpolation();
    test_parse_lambda();
    test_parse_do_syntax();
    test_parse_broadcast();
    test_parse_matrix();
    test_parse_module();
    test_parse_macro_call();
}

#[test]
fn chunk_001() {
    test_parse_show_macro();
    test_show_macro_end_to_end();
    test_broadcast_dot_plus();
    test_broadcast_dot_times();
    test_broadcast_scalar();
}

#[test]
fn chunk_002() {
    test_broadcast_power();
    test_broadcast_function_call();
    test_transpose();
    test_broadcast_outer_product();
}

#[test]
fn chunk_003() {
    test_do_syntax_map();
    test_do_syntax_filter();
    test_do_syntax_reduce();
    test_immutable_struct();
}

#[test]
fn chunk_004() {
    test_mutable_struct();
    test_range_with_keyword();
    test_function_with_keyword();
}
