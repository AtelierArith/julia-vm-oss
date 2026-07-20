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
    use subset_julia_vm::compile::host_support::compile_core_program;
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
        Ok(subset_julia_vm_bytecode::Value::I64(x)) => x as f64,
        Ok(subset_julia_vm_bytecode::Value::F64(x)) => x,
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

#[test]
fn test_eval_include_file_path_in_expression_position_7766() {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    let temp_dir = create_test_files(&[("included.jl", "ok = true\nok\n")]);
    let include_path = temp_dir.path().join("included.jl");
    let src = format!("println(include(\"{}\"))", include_path.display());

    let program = parse_and_lower(&src).expect("parse and lower include expression");
    let compiled = compile_with_cache(&program).expect("compile include expression");
    let mut vm = Vm::new_program(compiled, StableRng::new(42));
    vm.run().expect("run include expression");

    assert_eq!(vm.get_output(), "true\n");
}

#[test]
fn test_eval_include_file_with_using_statement_8474() {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    let temp_dir = create_test_files(&[("included.jl", "using LinearAlgebra\ntrue\n")]);
    let include_path = temp_dir.path().join("included.jl");
    let src = format!("println(include(\"{}\"))", include_path.display());

    let program = parse_and_lower(&src).expect("parse and lower include expression");
    let compiled = compile_with_cache(&program).expect("compile include expression");
    let mut vm = Vm::new_program(compiled, StableRng::new(42));
    vm.run()
        .expect("run include expression with using statement");

    assert_eq!(vm.get_output(), "true\n");
}

/// Representative source exercising every construct the source-file
/// top-level loop special-cases: a docstring on a struct/function/const, a
/// dangling docstring that must NOT leak to a later definition, an abstract
/// type, a primitive type, a short-function definition, a user-defined macro
/// definition + call, a plain type-alias assignment, a nested `module`, and a
/// `@kwdef struct`. No `include()` call appears, so an `IncludeContext`-aware
/// pass has nothing include-specific to diverge on.
const REPRESENTATIVE_LOWERING_SOURCE_10628: &str = r#"
"""
Point docstring
"""
struct Point
    x::Int
    y::Int
end

abstract type Shape end

primitive type Meters 64 end

"""
area doc
"""
function area(p::Point)
    p.x * p.y
end

norm2(p::Point) = sqrt(p.x^2 + p.y^2)

macro double(ex)
    return :(2 * $(esc(ex)))
end

"""
MY_CONST doc
"""
const MY_CONST = 42

MyAlias = Point

"""
stray doc that should not leak
"""
some_global = 1

module Inner
x = 1
y = 2
end

@kwdef struct KwPoint
    x::Int = 0
    y::Int = 0
end

result = @double(21)
"#;

/// Issue #10628: `Lowering` (plain — Base/prelude and REPL one-shot eval, no
/// `include()` support) and `LoweringWithInclude` (file/CLI lowering) drive
/// two hand-synced copies of the same source-file top-level `NodeKind` loop.
/// This locks in that, for [`REPRESENTATIVE_LOWERING_SOURCE_10628`], both
/// entry points lower the exact same include()-free source text to
/// byte-for-byte identical `Program` IR (`Program`/`Function`/`Stmt`/`Expr`
/// all derive `PartialEq`, including spans — both paths walk the same parsed
/// source, so spans line up too).
///
/// This must stay green across the entry-point unification: a future
/// lowering feature that is implemented in only one of the two loops (the
/// bug class Issue #10628 targets) would show up here as a diff instead of
/// silently shipping with divergent Base-vs-user-program behavior.
#[test]
fn plain_and_include_aware_lowering_agree_on_representative_source_10628() {
    use subset_julia_vm::lowering::{IncludeContext, Lowering, LoweringWithInclude};
    use subset_julia_vm::parser::Parser;

    let source = REPRESENTATIVE_LOWERING_SOURCE_10628;

    // Macro expansion seam (Issue #8656): both real entry points
    // (`pipeline::parse_source` / `parse_source_with_include`) install this
    // before lowering; the source above uses a user-defined macro (`@double`).
    subset_julia_vm::macro_runtime::install();

    let mut plain_parser = Parser::new().expect("parser init");
    let plain_outcome = plain_parser.parse(source).expect("plain parse");
    let mut plain_lowering = Lowering::new(source);
    let plain_program = plain_lowering
        .lower(plain_outcome)
        .expect("plain `Lowering` should lower the representative source");

    let mut include_parser = Parser::new().expect("parser init");
    let include_outcome = include_parser.parse(source).expect("include-aware parse");
    let mut include_lowering = LoweringWithInclude::new(source, IncludeContext::new(None));
    let include_program = include_lowering
        .lower(include_outcome)
        .expect("`LoweringWithInclude` should lower the representative source");

    assert_eq!(
        plain_program, include_program,
        "plain `Lowering` and `LoweringWithInclude` must lower the same \
         include()-free source to identical IR (Issue #10628); a diff here \
         means one of the two hand-synced source-file loops special-cases a \
         construct the other does not"
    );
}
