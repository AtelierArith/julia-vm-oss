//! Tests for the include() function.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a temporary directory with Julia files for testing.
fn create_test_files(files: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    for (name, content) in files {
        let path = temp_dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directory");
        }
        fs::write(&path, content).expect("Failed to write test file");
    }
    temp_dir
}

/// Helper to run Julia code with a specific base directory.
fn run_with_base_dir(src: &str, base_dir: PathBuf) -> f64 {
    use subset_julia_vm::compile::compile_core_program;
    use subset_julia_vm::lowering::LoweringWithInclude;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    let mut parser = Parser::new().expect("Parser init failed");
    let outcome = parser.parse(src).expect("Parse failed");

    let mut lowering = LoweringWithInclude::with_base_dir(src, Some(base_dir));
    let program = lowering.lower(outcome).expect("Lowering failed");

    // Merge prelude (simplified version - in real usage this is done by lib.rs)
    let compiled = compile_core_program(&program).expect("Compile failed");

    let rng = StableRng::new(42);
    let mut vm = Vm::new_program(compiled, rng);
    match vm.run() {
        Ok(subset_julia_vm::vm::Value::I64(x)) => x as f64,
        Ok(subset_julia_vm::vm::Value::F64(x)) => x,
        _ => f64::NAN,
    }
}

#[test]
fn test_include_simple_function() {
    let temp_dir = create_test_files(&[(
        "math_utils.jl",
        r#"
function add(a, b)
    a + b
end
"#,
    )]);

    let main_code = r#"
include("math_utils.jl")
add(2, 3)
"#;

    let result = run_with_base_dir(main_code, temp_dir.path().to_path_buf());
    assert_eq!(result, 5.0);
}

#[test]
fn test_include_relative_path() {
    let temp_dir = create_test_files(&[(
        "subdir/helper.jl",
        r#"
function multiply(a, b)
    a * b
end
"#,
    )]);

    let main_code = r#"
include("subdir/helper.jl")
multiply(4, 5)
"#;

    let result = run_with_base_dir(main_code, temp_dir.path().to_path_buf());
    assert_eq!(result, 20.0);
}

#[test]
fn test_include_multiple_files() {
    let temp_dir = create_test_files(&[
        (
            "a.jl",
            r#"
function func_a(x)
    x + 1
end
"#,
        ),
        (
            "b.jl",
            r#"
function func_b(x)
    x * 2
end
"#,
        ),
    ]);

    let main_code = r#"
include("a.jl")
include("b.jl")
func_b(func_a(3))
"#;

    let result = run_with_base_dir(main_code, temp_dir.path().to_path_buf());
    assert_eq!(result, 8.0); // (3 + 1) * 2 = 8
}

#[test]
fn test_include_nested() {
    let temp_dir = create_test_files(&[
        (
            "a.jl",
            r#"
include("b.jl")
function func_a(x)
    func_b(x) + 1
end
"#,
        ),
        (
            "b.jl",
            r#"
function func_b(x)
    x * 2
end
"#,
        ),
    ]);

    let main_code = r#"
include("a.jl")
func_a(5)
"#;

    let result = run_with_base_dir(main_code, temp_dir.path().to_path_buf());
    assert_eq!(result, 11.0); // 5 * 2 + 1 = 11
}

#[test]
fn test_include_file_not_found() {
    use subset_julia_vm::lowering::LoweringWithInclude;
    use subset_julia_vm::parser::Parser;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let main_code = r#"include("nonexistent.jl")"#;

    let mut parser = Parser::new().expect("Parser init failed");
    let outcome = parser.parse(main_code).expect("Parse failed");

    let mut lowering =
        LoweringWithInclude::with_base_dir(main_code, Some(temp_dir.path().to_path_buf()));
    let result = lowering.lower(outcome);

    // Should fail with an error about file not found
    assert!(result.is_err());
}

#[test]
fn test_include_circular_detection() {
    let temp_dir = create_test_files(&[
        (
            "a.jl",
            r#"
include("b.jl")
function func_a(x)
    x + 1
end
"#,
        ),
        (
            "b.jl",
            r#"
include("a.jl")
function func_b(x)
    x * 2
end
"#,
        ),
    ]);

    use subset_julia_vm::lowering::LoweringWithInclude;
    use subset_julia_vm::parser::Parser;

    let main_code = r#"include("a.jl")"#;

    let mut parser = Parser::new().expect("Parser init failed");
    let outcome = parser.parse(main_code).expect("Parse failed");

    let mut lowering =
        LoweringWithInclude::with_base_dir(main_code, Some(temp_dir.path().to_path_buf()));
    let result = lowering.lower(outcome);

    // Should fail due to circular include
    assert!(result.is_err());
    let err = result.unwrap_err();
    // The error message should mention circular include
    assert!(err.to_string().contains("include") || err.to_string().contains("circular"));
}

#[test]
fn test_include_with_statements() {
    let temp_dir = create_test_files(&[(
        "init.jl",
        r#"
x = 10
y = 20
"#,
    )]);

    let main_code = r#"
include("init.jl")
x + y
"#;

    let result = run_with_base_dir(main_code, temp_dir.path().to_path_buf());
    assert_eq!(result, 30.0);
}
