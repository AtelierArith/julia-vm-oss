//! AoT End-to-End Tests for Phase 1
//!
//! These tests verify that the AoT compiler correctly compiles Julia source code
//! to Rust code and that the type inference produces correct results.

#![cfg(feature = "aot")]

use std::collections::HashSet;
use std::fs;
use std::process::Command;
use subset_julia_vm::aot::analyze::program_to_aot_ir;
use subset_julia_vm::aot::call_graph::CallGraph;
use subset_julia_vm::aot::codegen::aot_codegen::AotCodeGenerator;
use subset_julia_vm::aot::codegen::{CAbiExport, CodegenConfig};
use subset_julia_vm::aot::inference::TypeInferenceEngine;
use subset_julia_vm::aot::optimizer::optimize_aot_program_full;
use subset_julia_vm::aot::types::StaticType;
use subset_julia_vm::base;
use subset_julia_vm::ir::core::{Block, Expr, Program, Stmt};
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::span::Span;

/// Helper function to compile Julia source to Rust code
fn compile_to_rust(source: &str) -> Result<String, String> {
    // Parse source
    let mut parser = Parser::new().map_err(|e| format!("Parser error: {:?}", e))?;
    let outcome = parser
        .parse(source)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Lower to Core IR
    let mut lowering = Lowering::new(source);
    let mut program = lowering
        .lower(outcome)
        .map_err(|e| format!("Lowering error: {:?}", e))?;
    localize_main_block(&mut program);

    // Type inference
    let mut type_engine = TypeInferenceEngine::new();
    let typed_program = type_engine
        .analyze_program(&program)
        .map_err(|e| format!("Type inference error: {:?}", e))?;

    // Convert Core IR to AoT IR
    let aot_program = program_to_aot_ir(&program, &typed_program)
        .map_err(|e| format!("IR conversion error: {:?}", e))?;

    // Generate Rust code
    let config = CodegenConfig::default();
    let mut codegen = AotCodeGenerator::new(config);
    codegen
        .generate_program(&aot_program)
        .map_err(|e| format!("Codegen error: {:?}", e))
}

fn compile_to_rust_with_c_abi_exports(
    source: &str,
    c_abi_exports: Vec<CAbiExport>,
) -> Result<String, String> {
    let mut parser = Parser::new().map_err(|e| format!("Parser error: {:?}", e))?;
    let outcome = parser
        .parse(source)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    let mut lowering = Lowering::new(source);
    let mut program = lowering
        .lower(outcome)
        .map_err(|e| format!("Lowering error: {:?}", e))?;
    localize_main_block(&mut program);

    let mut type_engine = TypeInferenceEngine::new();
    let typed_program = type_engine
        .analyze_program(&program)
        .map_err(|e| format!("Type inference error: {:?}", e))?;

    let aot_program = program_to_aot_ir(&program, &typed_program)
        .map_err(|e| format!("IR conversion error: {:?}", e))?;

    let config = CodegenConfig {
        c_abi_exports,
        ..CodegenConfig::default()
    };
    let mut codegen = AotCodeGenerator::new(config);
    codegen
        .generate_program(&aot_program)
        .map_err(|e| format!("Codegen error: {:?}", e))
}

/// Helper matching the CLI path for programs that depend on Base definitions.
fn compile_to_rust_with_base_optimized(source: &str) -> Result<String, String> {
    let mut parser = Parser::new().map_err(|e| format!("Parser error: {:?}", e))?;

    let prelude_src = base::get_aot_prelude();
    let prelude_outcome = parser
        .parse(&prelude_src)
        .map_err(|e| format!("Prelude parse error: {:?}", e))?;
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_outcome)
        .map_err(|e| format!("Prelude lowering error: {:?}", e))?;

    let outcome = parser
        .parse(source)
        .map_err(|e| format!("Parse error: {:?}", e))?;
    let mut lowering = Lowering::new(source);
    let mut program = lowering
        .lower(outcome)
        .map_err(|e| format!("Lowering error: {:?}", e))?;
    localize_main_block(&mut program);

    merge_prelude_program(&mut program, prelude_program);

    let call_graph = CallGraph::from_program(&program);
    let program = call_graph.filter_program(&program);

    let mut type_engine = TypeInferenceEngine::new();
    let typed_program = type_engine
        .analyze_program(&program)
        .map_err(|e| format!("Type inference error: {:?}", e))?;

    let mut aot_program = program_to_aot_ir(&program, &typed_program)
        .map_err(|e| format!("IR conversion error: {:?}", e))?;
    optimize_aot_program_full(&mut aot_program);

    let mut codegen = AotCodeGenerator::new(CodegenConfig::default());
    codegen
        .generate_program(&aot_program)
        .map_err(|e| format!("Codegen error: {:?}", e))
}

/// Like [`assert_generated_rust_checks_with_warnings_denied`] but without
/// `-D warnings`, so it fails on hard compile *errors* only (not lint
/// warnings). Use to isolate a codegen bug that produces ill-formed Rust from
/// unrelated lint warts in the same generated program.
fn assert_generated_rust_compiles(rust_code: &str, crate_name: &str) {
    let dir = tempfile::tempdir().expect("create generated Rust temp dir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create generated Rust src dir");
    fs::write(src_dir.join("main.rs"), rust_code).expect("write generated main.rs");

    let runtime_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("subset_julia_vm_runtime");
    let manifest = format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
subset_julia_vm_runtime = {{ path = "{}" }}
"#,
        runtime_path.display()
    );
    let manifest_path = dir.path().join("Cargo.toml");
    fs::write(&manifest_path, manifest).expect("write generated Cargo.toml");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("check")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .output()
        .expect("run cargo check for generated Rust");

    assert!(
        output.status.success(),
        "generated Rust must compile (no hard errors)\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_generated_rust_checks_with_warnings_denied(rust_code: &str, crate_name: &str) {
    let dir = tempfile::tempdir().expect("create generated Rust temp dir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create generated Rust src dir");
    fs::write(src_dir.join("main.rs"), rust_code).expect("write generated main.rs");

    let runtime_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("subset_julia_vm_runtime");
    let manifest = format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
subset_julia_vm_runtime = {{ path = "{}" }}
"#,
        runtime_path.display()
    );
    let manifest_path = dir.path().join("Cargo.toml");
    fs::write(&manifest_path, manifest).expect("write generated Cargo.toml");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("check")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .env("RUSTFLAGS", "-Dwarnings")
        .output()
        .expect("run cargo check for generated Rust");

    assert!(
        output.status.success(),
        "generated Rust must pass rustc -D warnings (Issue #7076)\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn localize_main_block(program: &mut Program) {
    if program.main.stmts.is_empty() {
        return;
    }

    let span = nonzero_span(program.main.span);
    let stmts = std::mem::take(&mut program.main.stmts);
    program.main.stmts.push(Stmt::Expr {
        expr: Expr::LetBlock {
            bindings: Vec::new(),
            body: Block { stmts, span },
            span,
        },
        span,
    });
}

fn nonzero_span(span: Span) -> Span {
    if span.start == 0 && span.end == 0 {
        Span::new(0, 0, 1, 1, 1, 1)
    } else {
        span
    }
}

fn merge_prelude_program(program: &mut Program, prelude_program: Program) {
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();
    let user_abstract_names: HashSet<_> = program
        .abstract_types
        .iter()
        .map(|a| a.name.as_str())
        .collect();

    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.append(&mut program.structs);
    program.structs = all_structs;

    let mut all_abstract_types: Vec<_> = prelude_program
        .abstract_types
        .into_iter()
        .filter(|a| !user_abstract_names.contains(a.name.as_str()))
        .collect();
    all_abstract_types.append(&mut program.abstract_types);
    program.abstract_types = all_abstract_types;

    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_func_names.contains(f.name.as_str()))
        .map(|mut f| {
            f.is_base_extension = true;
            f
        })
        .collect();
    all_functions.append(&mut program.functions);
    program.functions = all_functions;
}

// ============================================================================
// Arithmetic Literal Tests
// ============================================================================

#[test]
fn test_aot_e2e_arithmetic_literal() {
    let source = "1 + 2 * 3";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify the generated code contains the expected structure
    assert!(
        rust_code.contains("pub fn main()"),
        "Generated code should contain main function"
    );
}

#[test]
fn test_aot_e2e_arithmetic_with_parentheses() {
    let source = "(1 + 2) * 3";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_float_arithmetic() {
    let source = "3.14 * 2.0";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify float literals are present
    assert!(
        rust_code.contains("f64") || rust_code.contains("3.14"),
        "Generated code should contain float operations"
    );
}

// ============================================================================
// Typed Function Tests
// ============================================================================

#[test]
fn test_aot_e2e_typed_function() {
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end
add(10, 5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify function definition is present
    assert!(
        rust_code.contains("fn add"),
        "Generated code should contain add function"
    );
    assert!(
        rust_code.contains("i64"),
        "Generated code should contain i64 types"
    );
}

#[test]
fn test_aot_e2e_function_with_return() {
    let source = r#"
function double(x::Int64)::Int64
    return x * 2
end
double(21)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn double"),
        "Generated code should contain double function"
    );
    assert!(
        rust_code.contains("return"),
        "Generated code should contain return statement"
    );
}

#[test]
fn test_aot_e2e_untyped_function() {
    let source = r#"
function add(x, y)
    x + y
end
add(10, 5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Variable Assignment Tests
// ============================================================================

#[test]
fn test_aot_e2e_variable_assignment() {
    let source = r#"
x = 10
y = x + 5
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify main function is present (variables are in main block)
    assert!(
        rust_code.contains("pub fn main()"),
        "Generated code should contain main function"
    );
}

#[test]
fn test_aot_e2e_multiple_assignments() {
    let source = r#"
a = 1
b = 2
c = a + b
d = c * 2
d
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reassignment() {
    let source = r#"
x = 10
x = x + 1
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_time_println_preserves_timing_side_effect() {
    let source = r#"
@time println(1 + 2)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("seconds"),
        "Generated code should preserve @time timing output, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("println!(\"{}\", (1i64).wrapping_add(2i64))"),
        "Generated code should preserve the wrapped println call, got:\n{}",
        rust_code
    );
    // The `@time` macro lowers `elapsed_ns = time_ns() - t0`, where `time_ns()`
    // emits a trailing `... as i64` cast. The wrapping subtraction must wrap that
    // cast in parentheses, otherwise rustc rejects the program with the hard error
    // "cast cannot be followed by a method call" (Issue #8146). Guard the exact
    // broken substring, and require the parenthesized form.
    assert!(
        !rust_code.contains("as i64.wrapping_sub"),
        "@time elapsed-ns subtraction must parenthesize the cast receiver (Issue #8146), got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(" as i64).wrapping_sub("),
        "@time elapsed-ns subtraction should wrap the cast: `(... as i64).wrapping_sub(..)` (Issue #8146), got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_time_macro_generated_rust_compiles_issue_8146() {
    // Regression (Issue #8146): `@time` lowers `time_ns()` to an expression
    // ending in `... as i64`, and the elapsed-time subtraction used to emit
    // `... as i64.wrapping_sub(t0)`, which Rust rejects with the hard error
    // "cast cannot be followed by a method call". The wrapping-arithmetic
    // receiver must be parenthesized so the generated program builds.
    //
    // Uses a plain `cargo check` (not `-D warnings`): #8146 is a hard compile
    // *error*, and the `@time` expansion also emits an unrelated trailing
    // `result;` path-statement that only trips `-D warnings` (tracked
    // separately as Issue #8150), which must not mask this regression.
    let source = r#"
@time println(1 + 1)
"#;
    let rust_code = compile_to_rust(source).expect("@time program should compile to Rust source");
    assert!(
        !rust_code.contains("as i64.wrapping_sub"),
        "wrapping_sub receiver after an `as` cast must be parenthesized, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("as i64).wrapping_sub"),
        "elapsed-time subtraction should parenthesize the cast receiver, got:\n{}",
        rust_code
    );
    assert_generated_rust_compiles(&rust_code, "aot_time_macro_compiles_8146");
}

#[test]
fn test_aot_toplevel_time_no_trailing_path_statement_issue_8150() {
    // Regression (Issue #8150): a top-level `@time <expr>` lowers to the macro's
    // `local result = <expr>; ...; result` form. The trailing bare `result`
    // landed in `main()` as a dead `result;` path statement. It compiles under a
    // plain `cargo check`, but rustc's `path_statements` lint rejects it under
    // `-D warnings` (the gate generated AoT crates must pass, Issue #7076). The
    // IR converter now drops a bare trailing variable reference in statement
    // position (its value is discarded and reading it has no side effect).
    let source = r#"
@time println(1 + 1)
"#;
    let rust_code = compile_to_rust(source).expect("@time program should compile to Rust source");
    // The bare passthrough must not survive as a path statement.
    assert!(
        !rust_code.lines().any(|l| {
            let t = l.trim();
            t == "result;" || (t.ends_with(';') && !t.contains(' ') && t.starts_with("result"))
        }),
        "top-level @time must not emit a trailing bare `result;` path statement, got:\n{}",
        rust_code
    );
    // And the whole generated crate must pass rustc `-D warnings`.
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_toplevel_time_8150");
}

#[test]
fn test_aot_e2e_time_assignment_preserves_timing_side_effect() {
    let source = r#"
@time x = 42
println(x)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("seconds"),
        "Generated code should preserve @time timing output, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("let mut x: i64 = 42i64;"),
        "Generated code should preserve the assignment inside @time, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("println!(\"{}\", x);"),
        "Generated code should preserve downstream uses of the assigned value, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_time_macro_base_expansion_uses_time_ns_7059() {
    let source = r#"
@time println(1 + 2)
"#;
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("@time should compile through the Base macro expansion");

    assert!(
        rust_code.contains("std::time::SystemTime::now()"),
        "Base @time expansion must lower time_ns() to the AoT builtin, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("seconds"),
        "Base @time expansion should preserve timing output, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_elapsed_macro_base_expansion_returns_float_7059() {
    let source = r#"
elapsed = @elapsed 1 + 2
println(elapsed >= 0.0)
"#;
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("@elapsed should compile through the Base macro expansion");

    assert!(
        rust_code.contains("std::time::SystemTime::now()"),
        "Base @elapsed expansion must lower time_ns() to the AoT builtin, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("1000000000"),
        "Base @elapsed expansion should compute elapsed seconds as Float64, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_rejects_const_redefinition_policy_marker_7061() {
    let source = r#"
const X = 1
X = 2
println(X)
"#;
    let err = compile_to_rust(source).expect_err("const policy marker should be gated for AoT");
    assert!(err.contains("#7061"), "unexpected diagnostic: {err}");
    assert!(err.contains("const"), "unexpected diagnostic: {err}");
}

#[test]
fn test_aot_rejects_mutable_global_state_7061() {
    let source = r#"
g = 1
function bump()
    global g
    g = g + 1
    g
end
println(bump())
"#;
    let err = compile_to_rust(source).expect_err("mutable global state should be gated for AoT");
    assert!(err.contains("#7061"), "unexpected diagnostic: {err}");
    assert!(err.contains("global g"), "unexpected diagnostic: {err}");
}

#[test]
fn test_aot_rejects_same_signature_function_redefinition_7061() {
    let source = r#"
function f(x::Int64)
    x + 1
end
function f(x::Int64)
    x + 2
end
println(f(1))
"#;
    let err = compile_to_rust(source)
        .expect_err("same-signature function redefinition should be gated for AoT");
    assert!(err.contains("#7061"), "unexpected diagnostic: {err}");
    assert!(
        err.contains("redefining function `f`"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn test_aot_e2e_mandelbrot_broadcast_codegen_regression() {
    let source = include_str!("../../examples/mandelbrot.jl");
    let result = compile_to_rust_with_base_optimized(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();

    assert!(
        rust_code.contains("const im: Complex"),
        "AoT should emit the Base `im` constant as lowercase `im`, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("linspace(") && !rust_code.contains("fn _range("),
        "range(start, stop; length=n) should lower to linspace, not `_range`, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains(" == ()") && !rust_code.contains(" != ()"),
        "nothing-dispatch comparisons must not survive as Rust unit comparisons, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("-> Value"),
        "AoT functions should not return concrete range/broadcast values from `Value` signatures, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("fn op_add_f64_complex") && rust_code.contains("fn op_mul_complex_f64"),
        "broadcast operator references should have emitted Rust wrappers, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("__aot_broadcast_outer_product")
            && rust_code.contains("__aot_broadcast_call_matrix_scalar_2")
            && !rust_code.contains("Broadcasted::new")
            && !rust_code.contains("copy(")
            && !rust_code.contains("instantiate("),
        "broadcast materialization should lower to AoT helpers without generic Base calls, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_parameterized_complex_codegen_7041() {
    let source = r#"
z = Complex{Float32}(1, 2)
w = Complex{Float32}(3, 4)
println(real(z + w))
println(imag(z * w))
q = Complex{Int64}(2, 3)
r = Complex{Int64}(4, 5)
println(real(q + r))
println(imag(q * r))
"#;
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("parameterized Complex arithmetic should compile to Rust");

    assert!(
        rust_code.contains("pub struct Complex<T = f64>")
            && rust_code.contains("Complex::<f32>::new")
            && rust_code.contains("Complex::<i64>::new"),
        "parameterized Complex codegen should emit typed Rust carriers, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("fn real_complex<T: Copy>")
            && rust_code.contains("fn imag_complex<T: Copy>"),
        "real/imag helpers should be generic over Complex element type, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_complex_param_7041");
}

// ============================================================================
// If Statement Tests
// ============================================================================

#[test]
fn test_aot_e2e_if_statement() {
    let source = r#"
x = 5
if x > 0
    1
else
    -1
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("if"),
        "Generated code should contain if statement"
    );
}

#[test]
fn test_aot_e2e_if_elseif_else() {
    let source = r#"
x = 0
if x > 0
    1
elseif x < 0
    -1
else
    0
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_if() {
    let source = r#"
x = 10
y = 20
if x > 0
    if y > 0
        1
    else
        2
    end
else
    3
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// For Loop Tests
// ============================================================================

#[test]
fn test_aot_e2e_for_loop() {
    let source = r#"
sum = 0
for i in 1:10
    sum = sum + i
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("for"),
        "Generated code should contain for loop"
    );
}

#[test]
fn test_aot_e2e_for_loop_with_step() {
    let source = r#"
sum = 0
for i in 1:2:10
    sum = sum + i
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_for_loop() {
    let source = r#"
sum = 0
for i in 1:3
    for j in 1:3
        sum = sum + i * j
    end
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// While Loop Tests
// ============================================================================

#[test]
fn test_aot_e2e_while_loop() {
    let source = r#"
x = 0
while x < 10
    x = x + 1
end
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("while"),
        "Generated code should contain while loop"
    );
}

// ============================================================================
// Combined Tests
// ============================================================================

#[test]
fn test_aot_e2e_function_with_loop() {
    let source = r#"
function factorial(n::Int64)::Int64
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end
factorial(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn factorial"),
        "Generated code should contain factorial function"
    );
}

#[test]
fn test_aot_e2e_function_with_conditional() {
    let source = r#"
function abs_value(x::Int64)::Int64
    if x < 0
        return -x
    else
        return x
    end
end
abs_value(-5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_recursive_function() {
    let source = r#"
function fib(n::Int64)::Int64
    if n <= 1
        return n
    else
        return fib(n - 1) + fib(n - 2)
    end
end
fib(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Type Inference Tests
// ============================================================================

#[test]
fn test_aot_e2e_type_inference_int() {
    let source = "x = 42";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("i64") || rust_code.contains("42"),
        "Generated code should infer integer type"
    );
}

#[test]
fn test_aot_e2e_type_inference_float() {
    let source = "x = 3.14";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("f64") || rust_code.contains("3.14"),
        "Generated code should infer float type"
    );
}

#[test]
fn test_aot_e2e_type_inference_bool() {
    let source = "x = true";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("bool") || rust_code.contains("true"),
        "Generated code should infer boolean type"
    );
}

#[test]
fn test_aot_e2e_type_inference_string() {
    let source = r#"x = "hello""#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_aot_e2e_empty_function() {
    let source = r#"
function empty()
    nothing
end
empty()
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_single_expression() {
    let source = "42";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_boolean_operators() {
    let source = "true && false || true";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_comparison_operators() {
    let source = "1 < 2 && 3 >= 2 && 4 != 5";
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_unary_operators() {
    let source = r#"
x = 5
y = -x
z = !true
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Code Generation Quality Tests
// ============================================================================

#[test]
fn test_aot_e2e_code_has_header() {
    let source = "1 + 2";
    let result = compile_to_rust(source).unwrap();

    assert!(
        result.contains("Auto-generated"),
        "Generated code should have header comment"
    );
}

#[test]
fn test_aot_e2e_code_has_allow_attributes() {
    let source = "1 + 2";
    let result = compile_to_rust(source).unwrap();

    assert!(
        result.contains("#![allow(unused_variables)]"),
        "Generated code should have #![allow] attributes"
    );
}

#[test]
fn test_aot_e2e_multiple_functions() {
    let source = r#"
function add(x, y)
    x + y
end

function sub(x, y)
    x - y
end

function mul(x, y)
    x * y
end

add(1, 2) + sub(5, 3) + mul(2, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(rust_code.contains("fn add"));
    assert!(rust_code.contains("fn sub"));
    assert!(rust_code.contains("fn mul"));
}

// ============================================================================
// Static Function Call Tests
// ============================================================================

#[test]
fn test_aot_e2e_static_function_call_basic() {
    let source = r#"
function square(x::Int64)::Int64
    x * x
end
square(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn square"),
        "Generated code should contain square function"
    );
    assert!(
        rust_code.contains("square("),
        "Generated code should contain function call"
    );
}

#[test]
fn test_aot_e2e_static_function_call_multiple_args() {
    let source = r#"
function add3(a::Int64, b::Int64, c::Int64)::Int64
    a + b + c
end
add3(1, 2, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn add3"),
        "Generated code should contain add3 function"
    );
    assert!(
        rust_code.contains("add3("),
        "Generated code should contain function call with multiple args"
    );
}

#[test]
fn test_aot_e2e_static_function_call_nested() {
    let source = r#"
function double(x::Int64)::Int64
    x * 2
end

function quadruple(x::Int64)::Int64
    y::Int64 = double(x)
    double(y)
end

quadruple(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn double"),
        "Generated code should contain double function"
    );
    assert!(
        rust_code.contains("fn quadruple"),
        "Generated code should contain quadruple function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_in_expression() {
    let source = r#"
function inc(x::Int64)::Int64
    x + 1
end

function dec(x::Int64)::Int64
    x - 1
end

inc(5) + dec(10) * 2
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn inc"),
        "Generated code should contain inc function"
    );
    assert!(
        rust_code.contains("fn dec"),
        "Generated code should contain dec function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_recursive_fib() {
    let source = r#"
function fib(n::Int64)::Int64
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

fib(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn fib"),
        "Generated code should contain fib function"
    );
    // Check for recursive calls
    assert!(
        rust_code.contains("fib(n - 1)")
            || rust_code.contains("fib(n -")
            || rust_code.matches("fib(").count() >= 3,
        "Generated code should contain recursive calls"
    );
}

#[test]
fn test_aot_e2e_static_function_call_mutual_recursion_issue_7060() {
    let source = r#"
function is_even(n::Int64)::Bool
    if n == 0
        return true
    end
    return is_odd(n - 1)
end

function is_odd(n::Int64)::Bool
    if n == 0
        return false
    end
    return is_even(n - 1)
end

is_even(4)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn is_even"),
        "Generated code should contain is_even function"
    );
    assert!(
        rust_code.contains("fn is_odd"),
        "Generated code should contain is_odd function"
    );
    assert!(
        rust_code.contains("is_odd(n.wrapping_sub(1i64))")
            || rust_code.contains("is_odd((n).wrapping_sub(1i64))")
            || rust_code.contains("is_odd("),
        "Generated code should keep is_even -> is_odd as a static call, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("is_even(n.wrapping_sub(1i64))")
            || rust_code.contains("is_even((n).wrapping_sub(1i64))")
            || rust_code.matches("is_even(").count() >= 2,
        "Generated code should keep is_odd -> is_even as a static call, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_static_function_call_float_typed() {
    let source = r#"
function average(a::Float64, b::Float64)::Float64
    (a + b) / 2.0
end

average(3.0, 5.0)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn average"),
        "Generated code should contain average function"
    );
    assert!(
        rust_code.contains("f64"),
        "Generated code should contain f64 type"
    );
}

#[test]
fn test_aot_e2e_static_function_call_mixed_types() {
    let source = r#"
function scale(x::Int64, factor::Float64)::Float64
    x * factor
end

scale(10, 2.5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn scale"),
        "Generated code should contain scale function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_in_loop() {
    let source = r#"
function square(x::Int64)::Int64
    x * x
end

sum = 0
for i in 1:5
    sum = sum + square(i)
end
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn square"),
        "Generated code should contain square function"
    );
    assert!(
        rust_code.contains("for"),
        "Generated code should contain for loop"
    );
}

#[test]
fn test_aot_e2e_static_function_call_in_conditional() {
    let source = r#"
function abs(x::Int64)::Int64
    if x < 0
        return -x
    end
    return x
end

function sign(x::Int64)::Int64
    if abs(x) == 0
        return 0
    elseif x > 0
        return 1
    else
        return -1
    end
end

sign(-5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn abs") || rust_code.contains("fn abs_"),
        "Generated code should contain abs function"
    );
    assert!(
        rust_code.contains("fn sign"),
        "Generated code should contain sign function"
    );
}

#[test]
fn test_aot_e2e_static_function_call_chain() {
    let source = r#"
function step1(x::Int64)::Int64
    x + 1
end

function step2(x::Int64)::Int64
    x * 2
end

function step3(x::Int64)::Int64
    x - 3
end

step3(step2(step1(10)))
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn step1"),
        "Generated code should contain step1 function"
    );
    assert!(
        rust_code.contains("fn step2"),
        "Generated code should contain step2 function"
    );
    assert!(
        rust_code.contains("fn step3"),
        "Generated code should contain step3 function"
    );
}

#[test]
fn test_aot_e2e_static_function_with_local_vars() {
    let source = r#"
function compute(a::Int64, b::Int64)::Int64
    temp = a * 2
    result = temp + b
    return result
end

compute(5, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn compute"),
        "Generated code should contain compute function"
    );
    assert!(
        rust_code.contains("let"),
        "Generated code should contain local variable declarations"
    );
}

#[test]
fn test_aot_e2e_static_function_bool_return() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

function is_negative(x::Int64)::Bool
    x < 0
end

is_positive(5) && is_negative(-3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn is_positive"),
        "Generated code should contain is_positive function"
    );
    assert!(
        rust_code.contains("fn is_negative"),
        "Generated code should contain is_negative function"
    );
    assert!(
        rust_code.contains("bool"),
        "Generated code should contain bool type"
    );
}

#[test]
fn test_aot_e2e_static_function_early_return() {
    let source = r#"
function find_first_positive(a::Int64, b::Int64, c::Int64)::Int64
    if a > 0
        return a
    end
    if b > 0
        return b
    end
    if c > 0
        return c
    end
    return 0
end

find_first_positive(-1, 5, 10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn find_first_positive"),
        "Generated code should contain find_first_positive function"
    );
    assert!(
        rust_code.matches("return").count() >= 4,
        "Generated code should contain multiple return statements"
    );
}

// ============================================================================
// Multiple Dispatch Tests
// ============================================================================

#[test]
fn test_aot_e2e_multiple_dispatch_two_methods() {
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end

function add(x::Float64, y::Float64)::Float64
    x + y
end

add(1, 2)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have mangled function names
    assert!(
        rust_code.contains("add_i64_i64") || rust_code.contains("fn add"),
        "Generated code should contain mangled function names for dispatch"
    );
}

#[test]
fn test_aot_e2e_multiple_dispatch_three_methods() {
    let source = r#"
function compute(x::Int64)::Int64
    x * 2
end

function compute(x::Float64)::Float64
    x * 2.0
end

function compute(x::Bool)::Int64
    if x
        1
    else
        0
    end
end

compute(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Check that multiple dispatch comment is generated
    assert!(
        rust_code.contains("compute") && rust_code.contains("fn"),
        "Generated code should contain compute function definitions"
    );
}

#[test]
fn test_aot_e2e_multiple_dispatch_mixed_arg_count() {
    let source = r#"
function process(x::Int64)::Int64
    x
end

function process(x::Int64, y::Int64)::Int64
    x + y
end

process(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_multiple_dispatch_with_call() {
    let source = r#"
function double(x::Int64)::Int64
    x * 2
end

function double(x::Float64)::Float64
    x * 2.0
end

a = double(5)
b = double(3.14)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify both calls are present
    assert!(
        rust_code.contains("double"),
        "Generated code should contain double function calls"
    );
}

#[test]
fn test_aot_e2e_any_overloads_stay_in_dispatch_table_issue_7158() {
    let source = r#"
function pick(x::Int64, y::Any)
    1
end

function pick(x::Any, y::Int64)
    2
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub fn pick_i64_any(x: i64, y: Value) -> i64"),
        "first Any overload should keep its own generated method:\n{rust_code}"
    );
    assert!(
        rust_code.contains("pub fn pick_any_i64(x: Value, y: i64) -> i64"),
        "second Any overload should keep its own generated method:\n{rust_code}"
    );
    assert!(
        rust_code.contains("pub fn pick(arg0: Value, arg1: Value) -> RuntimeResult<Value>"),
        "Any overload set should emit a runtime dispatcher:\n{rust_code}"
    );
    assert!(
        rust_code.contains("pick(::Int64, ::Int64) is ambiguous"),
        "dispatcher should guard the overlapping Int64/Int64 call:\n{rust_code}"
    );
}

#[test]
fn test_aot_e2e_any_overloads_reject_ambiguous_static_call_issue_7158() {
    let source = r#"
function pick(x::Int64, y::Any)
    1
end

function pick(x::Any, y::Int64)
    2
end

pick(1, 2)
"#;
    let err = compile_to_rust(source).expect_err("ambiguous static call should be rejected");
    assert!(
        err.contains("pick(::Int64, ::Int64) is ambiguous"),
        "unexpected ambiguity diagnostic: {err}"
    );
}

#[test]
fn test_aot_e2e_single_method_rejects_no_method_static_call_issue_7158() {
    let source = r#"
function only_string(x::String)
    x
end

only_string(1)
"#;
    let err = compile_to_rust(source).expect_err("no-method static call should be rejected");
    assert!(
        err.contains("no method matching only_string(::Int64)"),
        "unexpected no-method diagnostic: {err}"
    );
}

#[test]
fn test_aot_e2e_multiple_dispatch_nested_calls() {
    let source = r#"
function transform(x::Int64)::Int64
    x + 1
end

function transform(x::Float64)::Float64
    x + 1.0
end

function apply_twice(x::Int64)::Int64
    y::Int64 = transform(x)
    transform(y)
end

apply_twice(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_single_function_no_dispatch() {
    let source = r#"
function single(x::Int64, y::Int64)::Int64
    x + y
end

single(1, 2)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Single function should NOT have mangled name
    assert!(
        rust_code.contains("fn single("),
        "Single method function should use original name"
    );
}

#[test]
fn test_aot_e2e_dispatch_in_expression() {
    let source = r#"
function negate(x::Int64)::Int64
    -x
end

function negate(x::Float64)::Float64
    -x
end

result = negate(5) + negate(3)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_dispatch_with_different_return_types() {
    let source = r#"
function convert(x::Int64)::Float64
    x * 1.0
end

function convert(x::Float64)::Int64
    x
end

convert(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Higher-Order Function Tests
// ============================================================================

#[test]
fn test_aot_e2e_map_with_function() {
    let source = r#"
function double(x::Int64)::Int64
    x * 2
end

arr = [1, 2, 3, 4, 5]
result = map(double, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("map") || rust_code.contains("iter"),
        "Generated code should contain map or iter operation"
    );
}

#[test]
fn test_aot_e2e_filter_with_function() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

arr = [-2, -1, 0, 1, 2]
result = filter(is_positive, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("filter") || rust_code.contains("iter"),
        "Generated code should contain filter operation"
    );
}

#[test]
fn test_aot_e2e_reduce_with_function() {
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end

arr = [1, 2, 3, 4, 5]
result = reduce(add, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("reduce") || rust_code.contains("fold"),
        "Generated code should contain reduce or fold operation"
    );
}

#[test]
fn test_aot_e2e_foreach_with_function() {
    let source = r#"
function print_item(x::Int64)::Nothing
    println(x)
    nothing
end

arr = [1, 2, 3]
foreach(print_item, arr)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_any_with_function() {
    let source = r#"
function is_even(x::Int64)::Bool
    x % 2 == 0
end

arr = [1, 3, 5, 7, 8]
result = any(is_even, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("any") || rust_code.contains("iter"),
        "Generated code should contain any operation"
    );
}

#[test]
fn test_aot_e2e_all_with_function() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

arr = [1, 2, 3, 4, 5]
result = all(is_positive, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("all") || rust_code.contains("iter"),
        "Generated code should contain all operation"
    );
}

#[test]
fn test_aot_e2e_map_chain() {
    let source = r#"
function inc(x::Int64)::Int64
    x + 1
end

function double(x::Int64)::Int64
    x * 2
end

arr = [1, 2, 3]
result = map(double, map(inc, arr))
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_filter_then_map() {
    let source = r#"
function is_positive(x::Int64)::Bool
    x > 0
end

function square(x::Int64)::Int64
    x * x
end

arr = [-2, -1, 0, 1, 2, 3]
filtered = filter(is_positive, arr)
result = map(square, filtered)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reduce_sum() {
    let source = r#"
function add(a::Int64, b::Int64)::Int64
    a + b
end

arr = [1, 2, 3, 4, 5]
total = reduce(add, arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reduce_product() {
    let source = r#"
function mul(a::Int64, b::Int64)::Int64
    a * b
end

arr = [1, 2, 3, 4, 5]
product = reduce(mul, arr)
product
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_hof_with_float_array() {
    let source = r#"
function halve(x::Float64)::Float64
    x / 2.0
end

arr = [2.0, 4.0, 6.0, 8.0]
result = map(halve, arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_sum_builtin() {
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
total = sum(arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("sum"),
        "Generated code should contain sum operation"
    );
}

#[test]
fn issue_7070_named_hof_functions_survive_dce_and_keep_types() {
    let source = r#"
function double(x::Int64)::Int64
    x * 2
end

function add(x::Int64, y::Int64)::Int64
    x + y
end

function is_positive(x::Int64)::Bool
    x > 0
end

function f()
    xs = [1, 2, 3]
    ys = map(double, xs)
    zs = filter(is_positive, ys)
    total = reduce(add, zs)
    mapped_sum = sum(double, xs)
    reduced_map = mapreduce(double, +, xs)
    println(total + mapped_sum + reduced_map)
end

f()
"#;
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("named HOF functions should compile after DCE");

    assert!(
        rust_code.contains("fn double("),
        "map/sum/mapreduce callee should remain emitted: {rust_code}"
    );
    assert!(
        rust_code.contains("fn add("),
        "reduce callee should remain emitted: {rust_code}"
    );
    assert!(
        rust_code.contains("fn is_positive("),
        "filter predicate should remain emitted: {rust_code}"
    );
    assert!(
        rust_code.contains(".iter().cloned().map(|x| double(x)).collect::<Vec<_>>()"),
        "map should call the emitted Rust function: {rust_code}"
    );
    assert!(
        rust_code
            .contains(".iter().cloned().filter(|x| is_positive((*x).clone())).collect::<Vec<_>>()"),
        "filter should call the emitted Rust predicate with cloned elements: {rust_code}"
    );
    assert!(
        rust_code.contains(".iter().cloned().reduce(|a, b| add(a, b)).unwrap_or_default()"),
        "reduce should call the emitted Rust reducer: {rust_code}"
    );
    assert!(
        rust_code.contains(".iter().cloned().map(|x| double(x)).sum::<i64>()"),
        "sum(f, xs) should map through f and keep Int64 result type: {rust_code}"
    );
    assert!(
        rust_code.contains(
            ".iter().cloned().map(|x| double(x)).reduce(|a, b| a + b).unwrap_or_default()"
        ),
        "mapreduce should map then reduce with the operator: {rust_code}"
    );
}

#[test]
fn issue_7070_string_map_filter_keep_non_copy_element_types() {
    let source = r#"
function idstr(x::String)::String
    x
end

function keepbb(x::String)::Bool
    x == "bb"
end

function f()
    xs = ["a", "bb"]
    ys = map(idstr, xs)
    zs = filter(keepbb, ys)
    println(zs[1])
end

f()
"#;
    let rust_code =
        compile_to_rust_with_base_optimized(source).expect("String HOFs should compile after DCE");

    assert!(
        rust_code.contains("fn idstr("),
        "String map callee should remain emitted: {rust_code}"
    );
    assert!(
        rust_code.contains("fn keepbb("),
        "String filter predicate should remain emitted: {rust_code}"
    );
    assert!(
        rust_code.contains(": Vec<String> =") && !rust_code.contains(": Vec<Value> ="),
        "String map/filter results should keep Vec<String> instead of Value: {rust_code}"
    );
    assert!(
        rust_code.contains(".iter().cloned().map(|x| idstr(x)).collect::<Vec<_>>()"),
        "String map should clone non-Copy elements before calling idstr: {rust_code}"
    );
    assert!(
        rust_code.contains(".iter().cloned().filter(|x| keepbb((*x).clone())).collect::<Vec<_>>()"),
        "String filter should clone non-Copy elements for predicate/result: {rust_code}"
    );
}

// ============================================================================
// Closure Tests (Phase 3)
// ============================================================================

#[test]
fn test_aot_e2e_simple_lambda() {
    // Simple single-parameter lambda: x -> x + 1
    // Note: In the current implementation, lambdas assigned to variables
    // are lowered to named functions, not Rust closures.
    let source = r#"
f = x -> x + 1
result = f(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Verify the lambda was converted to a function named 'f'
    assert!(
        rust_code.contains("fn f("),
        "Generated code should contain function f: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_two_param_lambda() {
    // Two-parameter lambda: (x, y) -> x + y
    let source = r#"
add = (x, y) -> x + y
result = add(3, 4)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_with_multiplication() {
    // Lambda with multiplication: x -> x * 2
    let source = r#"
double = x -> x * 2
result = double(10)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_with_power() {
    // Lambda with power operation: x -> x ^ 2
    let source = r#"
square = x -> x ^ 2
result = square(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn issue_7073_bool_power_signed_integer_stays_bool() {
    let source = r#"
function bool_pow(n::Int64)
    a = true ^ n
    b = false ^ 0
    println(a)
    println(b)
end

bool_pow(-1)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn bool_pow") && rust_code.contains("-> ()"),
        "Bool power helper should not return boxed Any/Value:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("let mut a: bool"),
        "true^n should infer Bool, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("Value::from(1.0_f64)"),
        "true negative Bool power must not box Float64:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_lambda_returning_bool() {
    // Lambda returning boolean: x -> x > 0
    let source = r#"
is_positive = x -> x > 0
result = is_positive(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_map_with_lambda() {
    // map with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5]
squared = map(x -> x ^ 2, arr)
squared
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("map") && rust_code.contains("|"),
        "Generated code should contain map with closure"
    );
}

#[test]
fn test_aot_e2e_filter_with_lambda() {
    // filter with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
evens = filter(x -> x % 2 == 0, arr)
evens
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("filter"),
        "Generated code should contain filter"
    );
}

#[test]
fn test_aot_e2e_any_with_lambda() {
    // any with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5]
has_three = any(x -> x == 3, arr)
has_three
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_all_with_lambda() {
    // all with inline lambda
    let source = r#"
arr = [2, 4, 6, 8]
all_even = all(x -> x % 2 == 0, arr)
all_even
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_reduce_with_lambda() {
    // reduce with inline lambda
    let source = r#"
arr = [1, 2, 3, 4, 5]
total = reduce((a, b) -> a + b, arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_complex_body() {
    // Lambda with more complex expression body
    let source = r#"
normalize = x -> (x - 5) / 10
result = normalize(15)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_lambda_calls() {
    // Using lambda result with another lambda
    let source = r#"
double = x -> x * 2
triple = x -> x * 3
result = triple(double(5))
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_lambda_with_float() {
    // Lambda with float operations
    let source = r#"
half = x -> x / 2.0
result = half(10.0)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_map_float_lambda() {
    // map with float lambda
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0]
doubled = map(x -> x * 2.0, arr)
doubled
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_chained_hof_with_lambda() {
    // Chained HOF operations with lambdas
    let source = r#"
arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
evens = filter(x -> x % 2 == 0, arr)
doubled = map(x -> x * 2, evens)
doubled
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Typed Array Tests (Phase 4)
// ============================================================================

#[test]
fn test_aot_e2e_array_literal_int() {
    // Integer array literal
    let source = r#"
arr = [1, 2, 3, 4, 5]
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec!["),
        "Generated code should contain vec! macro: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_literal_float() {
    // Float array literal
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("f64"),
        "Generated code should contain f64 type"
    );
}

#[test]
fn test_aot_e2e_array_index_get() {
    // Array indexing (get element)
    let source = r#"
arr = [10, 20, 30, 40, 50]
x = arr[3]
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have checked 1-based to 0-based index conversion.
    assert!(
        rust_code.contains("BoundsError")
            && rust_code.contains("_sjulia_idx - 1")
            && rust_code.contains("as usize"),
        "Generated code should contain checked index conversion (Issue #7155): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_index_set() {
    // Array indexing (set element)
    let source = r#"
arr = [10, 20, 30, 40, 50]
arr[3] = 100
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_array_length() {
    // Array length
    let source = r#"
arr = [1, 2, 3, 4, 5]
n = length(arr)
n
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".len()"),
        "Generated code should contain .len() method"
    );
}

#[test]
fn test_aot_e2e_array_push() {
    // push! operation
    let source = r#"
arr = [1, 2, 3]
push!(arr, 4)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".push("),
        "Generated code should contain .push() method"
    );
}

#[test]
fn test_aot_e2e_array_pop() {
    // pop! operation
    let source = r#"
arr = [1, 2, 3]
x = pop!(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".pop()"),
        "Generated code should contain .pop() method"
    );
}

#[test]
fn test_aot_e2e_array_first() {
    // first() operation
    let source = r#"
arr = [10, 20, 30]
x = first(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("[0]"),
        "Generated code should access index 0 for first: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_last() {
    // last() operation
    let source = r#"
arr = [10, 20, 30]
x = last(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".len() - 1]"),
        "Generated code should access last index: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_isempty() {
    // isempty() operation
    let source = r#"
arr = [1, 2, 3]
empty = isempty(arr)
empty
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".is_empty()"),
        "Generated code should contain .is_empty() method"
    );
}

#[test]
fn test_aot_e2e_array_insert() {
    // insert! operation
    let source = r#"
arr = [1, 2, 4, 5]
insert!(arr, 3, 3)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".insert("),
        "Generated code should contain .insert() method"
    );
}

#[test]
fn test_aot_e2e_array_deleteat() {
    // deleteat! operation
    let source = r#"
arr = [1, 2, 3, 4, 5]
deleteat!(arr, 3)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".remove("),
        "Generated code should contain .remove() method"
    );
}

#[test]
fn test_aot_e2e_array_append() {
    // append! operation
    let source = r#"
arr1 = [1, 2, 3]
arr2 = [4, 5, 6]
append!(arr1, arr2)
arr1
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".extend("),
        "Generated code should contain .extend() method"
    );
}

#[test]
fn test_aot_e2e_array_zeros() {
    // zeros() operation
    let source = r#"
arr = zeros(5)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec![0.0_f64;"),
        "Generated code should contain vec![0.0_f64;...]: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_ones() {
    // ones() operation
    let source = r#"
arr = ones(5)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec![1.0_f64;"),
        "Generated code should contain vec![1.0_f64;...]: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_fill() {
    // fill() operation — fill is now Pure Julia (Issue #2640),
    // so the generated code emits a function call, not vec!
    let source = r#"
arr = fill(42, 5)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fill("),
        "Generated code should contain fill function call: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_sum() {
    // sum() operation
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]
total = sum(arr)
total
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".sum::<"),
        "Generated code should contain .sum::<>(): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_multiple_operations() {
    // Multiple array operations in sequence
    let source = r#"
arr = [1, 2, 3]
push!(arr, 4)
push!(arr, 5)
n = length(arr)
n
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_array_in_function() {
    // Array operations inside a function
    let source = r#"
function sum_array(arr::Array{Int64,1})::Int64
    total = 0
    for x in arr
        total = total + x
    end
    total
end

arr = [1, 2, 3, 4, 5]
result = sum_array(arr)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn issue_7416_top_level_for_compound_assign_preserves_body() {
    let source = r#"
a = [1, 2, 3]
total = 0
for x in a
    total += x
end
println(total)
"#;
    let rust_code = compile_to_rust(source).expect("top-level for body should compile");

    assert!(
        rust_code.contains("for x in a.iter().cloned()"),
        "array for-each should bind owned element values: {rust_code}"
    );
    assert!(
        rust_code.contains("total = (total).wrapping_add(x);"),
        "top-level for body should preserve total += x: {rust_code}"
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_top_for_compound_7416");
}

#[test]
fn test_aot_e2e_array_empty_check() {
    // Check if array is empty in conditional
    let source = r#"
arr = [1, 2, 3]
if isempty(arr)
    0
else
    first(arr)
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_array_pushfirst() {
    // pushfirst! operation
    let source = r#"
arr = [2, 3, 4]
pushfirst!(arr, 1)
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".insert(0,"),
        "Generated code should contain .insert(0,...) for pushfirst"
    );
}

#[test]
fn test_aot_e2e_array_popfirst() {
    // popfirst! operation
    let source = r#"
arr = [1, 2, 3, 4]
x = popfirst!(arr)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".remove(0)"),
        "Generated code should contain .remove(0) for popfirst"
    );
}

// ============================================================================
// Multidimensional Array Tests (Phase 5)
// ============================================================================

#[test]
fn test_aot_e2e_matrix_zeros_2d() {
    // zeros(m, n) creates a 2D matrix
    let source = r#"
mat = zeros(3, 4)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should generate nested Vec for 2D array
    assert!(
        rust_code.contains("map(|_|") || rust_code.contains("collect::<Vec<_>>()"),
        "Generated code should create 2D Vec structure: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_matrix_ones_2d() {
    // ones(m, n) creates a 2D matrix of ones
    let source = r#"
mat = ones(2, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("1.0")
            && (rust_code.contains("map(|_|") || rust_code.contains("collect")),
        "Generated code should create 2D ones matrix: {}",
        rust_code
    );
}

#[test]
fn test_aot_nd_array_codegen_7033() {
    let source = r#"
a = ones(2, 2, 2)
b = zeros(2, 1, 2, 1)
println(a[1,1,1] + a[2,2,2] + a[8])
println(length(a) + ndims(a) + size(a, 1) + size(a, 2) + size(a, 3))
println(length(b) + ndims(b) + size(b, 3))
println(b[2,1,2,1])
"#;
    let rust_code = compile_to_rust(source).expect("3D+ arrays should compile to Rust");

    assert!(
        rust_code.contains("Vec<Vec<Vec<f64>>>") && rust_code.contains("Vec<Vec<Vec<Vec<f64>>>>"),
        "3D/4D arrays should keep nested Vec static types, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("vec![vec![vec![1.0_f64;")
            && rust_code.contains("vec![vec![vec![vec![0.0_f64;"),
        "ones/zeros should construct nested Vec carriers, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("_sjulia_dim_2")
            && rust_code.contains("_sjulia_idx_2")
            && rust_code.contains("_sjulia_remaining"),
        "3D+ length/size/direct/linear indexing should emit rank-aware helpers, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_nd_array_7033");
}

#[test]
fn test_aot_rand_randn_codegen_7036() {
    let source = r#"
println(rand())
println(rand())
println(randn())
xs = rand(3)
println(xs[1] + xs[2] + xs[3])
ys = randn(2, 2)
println(length(ys))
println(ys[1,1] + ys[2,2])
"#;
    let rust_code = compile_to_rust(source).expect("rand/randn should compile to Rust");

    assert!(
        rust_code.contains("__SJULIA_AOT_RNG")
            && rust_code.contains("StableRng::new(42)")
            && rust_code.contains("__sjulia_aot_rand()")
            && rust_code.contains("__sjulia_aot_randn()"),
        "rand/randn should use the VM-compatible runtime RNG helpers, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("collect::<Vec<_>>()"),
        "rand/randn dimensional forms should build nested Vec carriers, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_rng_7036");
}

#[test]
fn test_aot_e2e_matrix_fill_2d() {
    // fill(value, m, n) creates a 2D matrix filled with value
    // fill is now Pure Julia (Issue #2640), so it generates a function call
    let source = r#"
mat = fill(5, 2, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fill(") && rust_code.contains("5"),
        "Generated code should contain fill function call: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_shape_preserved() {
    // Array shape should be preserved in IR
    let source = r#"
arr = [1, 2, 3, 4, 5]
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec!["),
        "1D array should generate vec!: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_matrix_size_1d() {
    // size() for 1D array
    let source = r#"
arr = [1, 2, 3, 4, 5]
s = size(arr)
s
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".len()"),
        "size() should use .len() for 1D arrays: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_array_zeros_comparison() {
    // Compare 1D zeros vs 2D zeros generation
    let source = r#"
arr1d = zeros(5)
arr1d
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "1D zeros failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // 1D should be simple vec!
    assert!(
        rust_code.contains("vec![0.0_f64;"),
        "1D zeros should be vec![0.0_f64;...]: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_matrix_function_with_2d() {
    // Function that takes a 2D matrix parameter
    let source = r#"
function process_matrix()
    mat = zeros(2, 2)
    mat
end
process_matrix()
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_nested_vec_generation() {
    // Verify nested Vec generation for 2D
    let source = r#"
mat = zeros(3, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should create nested Vec<Vec<_>> structure
    assert!(
        rust_code.contains("Vec<_>") || rust_code.contains("collect"),
        "2D arrays should generate Vec of Vecs: {}",
        rust_code
    );
}

// ============================================================================
// Struct Tests (Phase 4)
// ============================================================================

#[test]
fn test_aot_e2e_struct_definition_immutable() {
    // Immutable struct definition and instantiation
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should define the struct
    assert!(
        rust_code.contains("pub struct Point"),
        "Generated code should define struct Point: {}",
        rust_code
    );
    // Should have fields
    assert!(
        rust_code.contains("x: f64") || rust_code.contains("pub x:"),
        "Generated code should have field x: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_instantiation() {
    // Struct constructor call
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(3.0, 4.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should call constructor
    assert!(
        rust_code.contains("Point::new(") || rust_code.contains("Point {"),
        "Generated code should instantiate Point: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_field_access() {
    // Field access (read)
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
x_val = p.x
x_val
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should access field
    assert!(
        rust_code.contains("p.x"),
        "Generated code should access p.x: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_mutable_struct() {
    // Mutable struct with field modification
    let source = r#"
mutable struct Counter
    count::Int64
end

c = Counter(0)
c
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should define the struct
    assert!(
        rust_code.contains("struct Counter"),
        "Generated code should define Counter struct: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_mutable_struct_field_write() {
    // Mutable struct field assignment
    let source = r#"
mutable struct Counter
    count::Int64
end

c = Counter(0)
c.count = 10
c
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should modify field
    assert!(
        rust_code.contains("c.count = ") || rust_code.contains("c.count="),
        "Generated code should assign to c.count: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_in_function() {
    // Struct used in function
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

function distance(p::Point)::Float64
    sqrt(p.x * p.x + p.y * p.y)
end

p = Point(3.0, 4.0)
d = distance(p)
d
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have function with Point parameter
    assert!(
        rust_code.contains("fn distance"),
        "Generated code should define distance function: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_constructor() {
    // Struct with new() constructor generated
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(5.0, 12.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have impl block with new
    assert!(
        rust_code.contains("impl Point") && rust_code.contains("fn new"),
        "Generated code should have impl Point with new(): {}",
        rust_code
    );
    assert!(
        rust_code.contains("pub fn new(__sjulia_field_x: f64, __sjulia_field_y: f64) -> Self"),
        "Struct constructor parameters should avoid field/global name collisions (Issue #7154): {}",
        rust_code
    );
    assert!(
        rust_code.contains("x: __sjulia_field_x,")
            && rust_code.contains("y: __sjulia_field_y,"),
        "Struct constructor should initialize fields from escaped parameter names (Issue #7154): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_with_int_fields() {
    // Struct with integer fields
    let source = r#"
struct Rectangle
    width::Int64
    height::Int64
end

r = Rectangle(10, 20)
r
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("i64"),
        "Generated code should have i64 type for Int64 fields: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_derive_traits() {
    // Issue #5158: an immutable isbits struct (all-Copy-primitive fields) derives
    // Copy structurally, no longer special-cased to the name "Complex". `Point`
    // here has two Float64 fields, so it gets `#[derive(Debug, Clone, Copy)]`.
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("#[derive(Debug, Clone, Copy)]"),
        "Isbits struct should derive Debug, Clone, Copy: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_derive_non_isbits_clone_only() {
    // Issue #5158: a struct with a non-Copy field (String) must NOT derive Copy.
    let source = r#"
struct Named
    name::String
    id::Int64
end

n = Named("a", 1)
n
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("#[derive(Debug, Clone)]")
            && !rust_code.contains("#[derive(Debug, Clone, Copy)]"),
        "Struct with a String field should derive Clone, not Copy: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_struct_multiple_instances() {
    // Multiple struct instances
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)
p1
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_struct_field_in_expression() {
    // Struct fields used in expressions
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(3.0, 4.0)
sum = p.x + p.y
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("p.x") && rust_code.contains("p.y"),
        "Generated code should access both fields: {}",
        rust_code
    );
}

// ============================================================================
// Tuple Tests (Phase 4)
// ============================================================================

#[test]
fn test_aot_e2e_tuple_literal_basic() {
    // Basic tuple literal
    let source = r#"
t = (1, 2, 3)
t
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("(") && rust_code.contains(")"),
        "Generated code should contain tuple literal: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_single_element() {
    // Single element tuple (trailing comma)
    let source = r#"
t = (42,)
t
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Single element tuple should have trailing comma in Rust too
    assert!(
        rust_code.contains(",)"),
        "Generated code should have trailing comma for single-element tuple: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_heterogeneous() {
    // Tuple with different types
    let source = r#"
t = (1, 3.14)
t
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_tuple_index_access() {
    // Tuple indexing should use .0, .1 syntax
    let source = r#"
t = (10, 20, 30)
x = t[1]
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Julia t[1] should become Rust t.0
    assert!(
        rust_code.contains(".0"),
        "Generated code should use tuple field access (.0): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_index_second() {
    // Tuple indexing second element
    let source = r#"
t = (10, 20, 30)
y = t[2]
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Julia t[2] should become Rust t.1
    assert!(
        rust_code.contains(".1"),
        "Generated code should use tuple field access (.1): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_index_third() {
    // Tuple indexing third element
    let source = r#"
t = (10, 20, 30)
z = t[3]
z
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Julia t[3] should become Rust t.2
    assert!(
        rust_code.contains(".2"),
        "Generated code should use tuple field access (.2): {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_in_expression() {
    // Tuple elements used in expression
    let source = r#"
t = (3, 4)
sum = t[1] + t[2]
sum
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains(".0") && rust_code.contains(".1"),
        "Generated code should access both tuple elements: {}",
        rust_code
    );
}

#[test]
fn test_aot_e2e_tuple_multiple() {
    // Multiple tuples
    let source = r#"
t1 = (1, 2)
t2 = (3, 4)
t1
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_tuple_in_function_return() {
    // Function returning a tuple
    let source = r#"
function make_pair(a::Int64, b::Int64)
    (a, b)
end

p = make_pair(1, 2)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_tuple_with_float() {
    // Tuple with float values
    let source = r#"
t = (1.0, 2.0, 3.0)
x = t[1]
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Type Specialization Tests (Issue #1018)
// ============================================================================

#[test]
fn test_aot_e2e_type_specialization_int64() {
    // Fully typed function should generate specialized version
    let source = r#"
function add_nums(x::Int64, y::Int64)::Int64
    x + y
end

result = add_nums(1, 2)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    // Should generate a function with mangled name containing type info
    assert!(
        code.contains("add_nums_i64_i64") || code.contains("fn add_nums"),
        "Expected typed function in generated code"
    );
}

#[test]
fn test_aot_e2e_type_specialization_float64() {
    // Float64 typed function
    let source = r#"
function multiply(x::Float64, y::Float64)::Float64
    x * y
end

result = multiply(2.0, 3.0)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    assert!(
        code.contains("multiply") && (code.contains("f64") || code.contains("Float64")),
        "Expected float64 typed function"
    );
}

#[test]
fn test_aot_e2e_type_specialization_bool() {
    // Bool typed function
    let source = r#"
function negate(x::Bool)::Bool
    !x
end

result = negate(true)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_mixed_types() {
    // Function with mixed parameter types
    let source = r#"
function scale(x::Int64, factor::Float64)::Float64
    x * factor
end

result = scale(5, 2.0)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_multiple_functions() {
    // Multiple typed functions with same name different signatures
    let source = r#"
function compute(x::Int64)::Int64
    x * 2
end

function compute(x::Float64)::Float64
    x * 2.0
end

a = compute(5)
b = compute(3.0)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    // Should have two specialized versions
    assert!(
        code.contains("compute"),
        "Expected compute function in generated code"
    );
}

#[test]
fn test_aot_e2e_type_specialization_nested_calls() {
    // Typed functions calling other typed functions - using two separate calls
    let source = r#"
function double_it(x::Int64)::Int64
    x * 2
end

a = double_it(5)
b = double_it(a)
b
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_no_value_type() {
    // Ensure no Value type appears in generated code for typed functions
    let source = r#"
function typed_add(a::Int64, b::Int64)::Int64
    a + b
end

x = typed_add(10, 20)
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    // For fully typed functions, we should not see Value::Int64 wrapping
    assert!(
        !code.contains("Value::Int64(a)") || code.contains("i64"),
        "Fully typed function should use native types"
    );
}

#[test]
fn test_aot_e2e_type_specialization_return_type_inference() {
    // Function with explicit return type
    let source = r#"
function get_double(x::Int64)::Int64
    x * 2
end

y = get_double(7)
y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let code = result.unwrap();
    assert!(
        code.contains("-> i64") || code.contains("Int64") || code.contains("i64"),
        "Return type should be specialized"
    );
}

#[test]
fn test_aot_e2e_type_specialization_recursive() {
    // Recursive typed function
    let source = r#"
function factorial(n::Int64)::Int64
    if n <= 1
        1
    else
        n * factorial(n - 1)
    end
end

result = factorial(5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_aot_e2e_type_specialization_with_locals() {
    // Typed function with local variables
    let source = r#"
function compute_sum(a::Int64, b::Int64, c::Int64)::Int64
    temp1 = a + b
    temp2 = temp1 + c
    temp2
end

result = compute_sum(1, 2, 3)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ============================================================================
// Full Pipeline Verification Tests (Issue #2595)
//
// These tests verify the complete AoT pipeline end-to-end:
// Julia source → parse → lower → type inference → AoT IR → codegen → Rust code
// Each test checks both successful compilation AND correctness of generated code.
// ============================================================================

// ----------------------------------------------------------------------------
// Arithmetic: integers and floats
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_integer_arithmetic_all_ops() {
    // Verify all basic integer arithmetic operations generate correct Rust
    let source = r#"
a = 10
b = 3
c = a + b
d = a - b
e = a * b
f = a ÷ b
g = a % b
c + d + e + f + g
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub fn main()"),
        "Should contain main function"
    );
}

#[test]
fn test_e2e_pipeline_float_arithmetic_precision() {
    // Verify float arithmetic preserves f64 types
    let source = r#"
x = 1.5
y = 2.7
z = x + y
w = x * y - z / 2.0
w
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("f64") || rust_code.contains("1.5"),
        "Should contain float type or literal: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_mixed_int_float_arithmetic() {
    // Mixed integer and float operations
    let source = r#"
function convert_and_add(x::Int64, y::Float64)::Float64
    x + y
end

result = convert_and_add(3, 4.5)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn convert_and_add"),
        "Should contain function definition"
    );
}

#[test]
fn test_e2e_pipeline_negative_numbers() {
    // Negative number handling
    let source = r#"
x = -5
y = -3.14
z = x + 10
w = y * (-2.0)
z
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

// ----------------------------------------------------------------------------
// Variable assignment and scoping
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_variable_chain() {
    // Chain of variable assignments and usage
    // Top-level variables are emitted as `static` in the AoT pipeline
    let source = r#"
a = 1
b = a + 1
c = b + a
d = c * b
e = d - c + b - a
e
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("static") || rust_code.contains("let"),
        "Should contain variable bindings: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_variable_shadowing() {
    // Variable reassignment (shadowing in Julia)
    let source = r#"
x = 10
x = x + 1
x = x * 2
x = x - 5
x
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_e2e_pipeline_variables_in_function() {
    // Local variables inside functions
    let source = r#"
function compute(a::Int64, b::Int64)::Int64
    sum = a + b
    diff = a - b
    product = sum * diff
    product
end

compute(10, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn compute"),
        "Should define compute function"
    );
    assert!(
        rust_code.contains("let"),
        "Should use let for local variables: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Functions with multiple dispatch
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_dispatch_int_vs_float() {
    // Multiple dispatch with Int64 and Float64
    let source = r#"
function process(x::Int64)::Int64
    x * 2
end

function process(x::Float64)::Float64
    x * 2.0
end

a = process(5)
b = process(3.14)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have dispatch mechanism (either mangled names or enum-based)
    assert!(
        rust_code.contains("process"),
        "Should contain process function(s)"
    );
}

#[test]
fn test_e2e_pipeline_dispatch_arity() {
    // Dispatch on number of arguments
    let source = r#"
function area(r::Float64)::Float64
    3.14159 * r * r
end

function area(w::Float64, h::Float64)::Float64
    w * h
end

circle_area = area(5.0)
rect_area = area(3.0, 4.0)
circle_area
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_e2e_pipeline_dispatch_struct_parameter() {
    // Dispatch on struct types
    let source = r#"
struct Circle
    radius::Float64
end

struct Rectangle
    width::Float64
    height::Float64
end

function area(c::Circle)::Float64
    3.14159 * c.radius * c.radius
end

function area(r::Rectangle)::Float64
    r.width * r.height
end

c = Circle(5.0)
r = Rectangle(3.0, 4.0)
area(c)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("struct Circle"),
        "Should define Circle struct: {}",
        rust_code
    );
    assert!(
        rust_code.contains("struct Rectangle"),
        "Should define Rectangle struct: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Control flow: if/else, while, for
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_if_else_chain() {
    // Complex if/elseif/else chain
    let source = r#"
function classify(x::Int64)::Int64
    if x > 100
        return 3
    elseif x > 10
        return 2
    elseif x > 0
        return 1
    else
        return 0
    end
end

classify(50)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("if") && rust_code.contains("else"),
        "Should contain if/else: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_while_with_counter() {
    // While loop with counter
    let source = r#"
function sum_to(n::Int64)::Int64
    total = 0
    i = 1
    while i <= n
        total = total + i
        i = i + 1
    end
    total
end

sum_to(100)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("while"),
        "Should contain while loop: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_for_accumulation() {
    // For loop with accumulation
    let source = r#"
function sum_squares(n::Int64)::Int64
    total = 0
    for i in 1:n
        total = total + i * i
    end
    total
end

sum_squares(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("for"),
        "Should contain for loop: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_nested_loops_with_condition() {
    // Nested loops with conditional
    let source = r#"
function count_pairs(n::Int64)::Int64
    count = 0
    for i in 1:n
        for j in 1:n
            if i + j > n
                count = count + 1
            end
        end
    end
    count
end

count_pairs(5)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn count_pairs"),
        "Should define count_pairs function"
    );
}

#[test]
fn test_e2e_pipeline_loop_with_early_return() {
    // Loop with early return
    let source = r#"
function find_first_gt(n::Int64)::Int64
    for i in 1:100
        if i * i > n
            return i
        end
    end
    return -1
end

find_first_gt(50)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("return"),
        "Should contain return statement: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Struct definitions and usage
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_struct_with_methods() {
    // Struct with associated functions
    let source = r#"
struct Vec2D
    x::Float64
    y::Float64
end

function magnitude(v::Vec2D)::Float64
    sqrt(v.x * v.x + v.y * v.y)
end

function dot(a::Vec2D, b::Vec2D)::Float64
    a.x * b.x + a.y * b.y
end

v1 = Vec2D(3.0, 4.0)
v2 = Vec2D(1.0, 0.0)
m = magnitude(v1)
d = dot(v1, v2)
m
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub struct Vec2D"),
        "Should define Vec2D struct: {}",
        rust_code
    );
    assert!(
        rust_code.contains("fn magnitude"),
        "Should define magnitude function"
    );
    assert!(rust_code.contains("fn dot"), "Should define dot function");
}

#[test]
fn test_e2e_pipeline_mutable_struct_update() {
    // Mutable struct with field updates
    let source = r#"
mutable struct Accumulator
    total::Int64
    count::Int64
end

function add_value(acc::Accumulator, val::Int64)
    acc.total = acc.total + val
    acc.count = acc.count + 1
end

a = Accumulator(0, 0)
add_value(a, 10)
add_value(a, 20)
a
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("struct Accumulator"),
        "Should define Accumulator struct: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_struct_mixed_field_types() {
    // Struct with different field types
    let source = r#"
struct Particle
    x::Float64
    y::Float64
    mass::Float64
end

function kinetic_energy(p::Particle, vx::Float64, vy::Float64)::Float64
    0.5 * p.mass * (vx * vx + vy * vy)
end

p = Particle(0.0, 0.0, 2.5)
ke = kinetic_energy(p, 3.0, 4.0)
ke
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("struct Particle"),
        "Should define Particle struct"
    );
    assert!(
        rust_code.contains("f64"),
        "Should use f64 type for Float64 fields"
    );
}

// ----------------------------------------------------------------------------
// Array operations
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_array_iteration() {
    // Iterating over array with for loop
    let source = r#"
function sum_array(arr::Array{Int64,1})::Int64
    total = 0
    for x in arr
        total = total + x
    end
    total
end

arr = [10, 20, 30, 40, 50]
sum_array(arr)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn sum_array"),
        "Should define sum_array function"
    );
}

#[test]
fn test_e2e_pipeline_array_index_computation() {
    // Array indexing with computed indices
    let source = r#"
arr = [1, 2, 3, 4, 5]
i = 3
x = arr[i]
y = arr[1] + arr[5]
x + y
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
}

#[test]
fn test_e2e_pipeline_array_build_with_loop() {
    // Building array in a loop
    let source = r#"
arr = zeros(5)
for i in 1:5
    arr[i] = i * i * 1.0
end
arr
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("vec![0.0_f64;"),
        "Should generate zeros as vec!: {}",
        rust_code
    );
}

// ----------------------------------------------------------------------------
// Combined complex programs
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_fibonacci_iterative() {
    // Iterative Fibonacci — tests variables, loops, and conditionals together
    let source = r#"
function fib_iter(n::Int64)::Int64
    if n <= 1
        return n
    end
    a = 0
    b = 1
    for i in 2:n
        temp = a + b
        a = b
        b = temp
    end
    b
end

fib_iter(10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(rust_code.contains("fn fib_iter"), "Should define fib_iter");
    assert!(rust_code.contains("for"), "Should contain for loop");
    assert!(rust_code.contains("if"), "Should contain if statement");
}

#[test]
fn test_e2e_pipeline_gcd_euclid() {
    // Euclidean GCD algorithm — tests while loops with modulo
    let source = r#"
function gcd(a::Int64, b::Int64)::Int64
    while b != 0
        temp = b
        b = a % b
        a = temp
    end
    a
end

gcd(48, 18)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(rust_code.contains("fn gcd"), "Should define gcd function");
    assert!(rust_code.contains("while"), "Should contain while loop");
}

#[test]
fn test_e2e_pipeline_bubble_sort() {
    // Bubble sort — tests nested loops, array indexing, and swapping
    let source = r#"
function bubble_sort(arr::Array{Int64,1})
    n = length(arr)
    for i in 1:n
        for j in 1:(n - i)
            if arr[j] > arr[j + 1]
                temp = arr[j]
                arr[j] = arr[j + 1]
                arr[j + 1] = temp
            end
        end
    end
    arr
end

data = [5, 3, 8, 1, 2]
bubble_sort(data)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn bubble_sort"),
        "Should define bubble_sort function"
    );
}

#[test]
fn test_e2e_pipeline_matrix_zeros_2d() {
    // 2D matrix creation
    let source = r#"
mat = zeros(3, 3)
mat
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("collect") || rust_code.contains("map(|_|"),
        "Should generate 2D Vec structure: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_struct_with_function() {
    // Struct and function used together
    let source = r#"
struct Point
    x::Float64
    y::Float64
end

function make_point(i::Int64)::Point
    Point(i * 1.0, i * 2.0)
end

p = make_point(5)
p
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub struct Point"),
        "Should define Point struct"
    );
    assert!(
        rust_code.contains("fn make_point"),
        "Should define make_point function"
    );
}

#[test]
fn test_e2e_pipeline_power_function() {
    // Power computation using loop
    let source = r#"
function power(base::Int64, exp::Int64)::Int64
    result = 1
    for i in 1:exp
        result = result * base
    end
    result
end

power(2, 10)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn power"),
        "Should define power function"
    );
}

#[test]
fn test_e2e_pipeline_string_literal() {
    // String literal handling
    let source = r#"
msg = "hello world"
msg
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("hello world"),
        "Should contain string literal: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_boolean_logic() {
    // Boolean logic operations
    let source = r#"
function check(a::Bool, b::Bool)::Bool
    (a && b) || (!a && !b)
end

check(true, true)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn check"),
        "Should define check function"
    );
    assert!(
        rust_code.contains("bool"),
        "Should use bool type: {}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_collatz_sequence() {
    // Collatz conjecture step count — tests while + if/else + modulo
    let source = r#"
function collatz_steps(n::Int64)::Int64
    steps = 0
    while n != 1
        if n % 2 == 0
            n = n ÷ 2
        else
            n = 3 * n + 1
        end
        steps = steps + 1
    end
    steps
end

collatz_steps(27)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn collatz_steps"),
        "Should define collatz_steps function"
    );
    assert!(
        rust_code.contains("while") && rust_code.contains("if"),
        "Should contain while loop and if statement"
    );
}

// ----------------------------------------------------------------------------
// Generated code quality checks
// ----------------------------------------------------------------------------

#[test]
fn test_e2e_pipeline_no_panic_in_generated_code() {
    // Verify generated code doesn't contain panic or unwrap calls
    let source = r#"
function safe_divide(a::Int64, b::Int64)::Int64
    if b == 0
        return 0
    end
    a ÷ b
end

safe_divide(10, 3)
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        !rust_code.contains("panic!"),
        "Generated code should not contain panic!: {}",
        rust_code
    );
    assert!(
        !rust_code.contains(".unwrap()"),
        "Generated code should not contain .unwrap(): {}",
        rust_code
    );
}

#[test]
fn test_e2e_nothing_comparison_not_unit_compare_5658() {
    // Issue #5658: `x == nothing` in the dynamic path must lower to a
    // `Value::Nothing` check (`.is_nothing()`), NOT a Rust unit comparison
    // `x == ()` (invalid Rust). The iteration protocol (`for x in itr`) compares
    // the `iterate` result against `nothing`, which exercises this path.
    let source = r#"
function count_items(itr)
    n = 0
    next = iterate(itr)
    while next !== nothing
        val, st = next
        n = n + 1
        next = iterate(itr, st)
    end
    n
end
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    assert!(
        !rust_code.contains(" == ()") && !rust_code.contains(" != ()"),
        "nothing-comparison must not lower to a Rust unit comparison, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("is_nothing()"),
        "nothing-comparison should lower to `.is_nothing()`, got:\n{}",
        rust_code
    );
}

#[test]
fn test_e2e_pipeline_valid_rust_structure() {
    // Verify the overall structure of generated Rust code
    let source = r#"
function add(x::Int64, y::Int64)::Int64
    x + y
end

result = add(1, 2)
result
"#;
    let result = compile_to_rust(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let rust_code = result.unwrap();
    // Should have header
    assert!(
        rust_code.contains("Auto-generated"),
        "Should have auto-generated header"
    );
    // Should have allow attributes
    assert!(
        rust_code.contains("#![allow("),
        "Should have #![allow] attributes"
    );
    // Should have main function
    assert!(
        rust_code.contains("pub fn main()"),
        "Should have pub fn main()"
    );
}

// ---------------------------------------------------------------------------
// Issue #7037: static subtype operator `<:` const-folding
// ---------------------------------------------------------------------------

#[test]
fn test_aot_subtype_builtin_true_7037() {
    // `Int <: Real` is a statically resolvable type relation → folds to `true`.
    let source = "println(Int <: Real)";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "Static `<:` should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("true"),
        "Int <: Real should fold to `true`, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("<:"),
        "Folded subtype must not emit a `<:` operator, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_subtype_builtin_false_7037() {
    // `Float64 <: Integer` folds to `false`.
    let source = "println(Float64 <: Integer)";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "Static `<:` should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("false"),
        "Float64 <: Integer should fold to `false`, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_subtype_user_types_7037() {
    // User-declared struct / abstract hierarchy resolves statically too.
    let source = r#"
abstract type Animal end
struct Dog <: Animal
end
println(Dog <: Animal)
println(Dog <: Integer)
"#;
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "User-type `<:` should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("true") && rust_code.contains("false"),
        "Dog <: Animal → true, Dog <: Integer → false, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7052: string interpolation "$x" → string() concat codegen
// ---------------------------------------------------------------------------

#[test]
fn test_aot_string_interpolation_simple_7052() {
    // `"value is $x"` lowers to a Core IR `StringConcat`; AoT must lower it to
    // the same `format!`-based concat as `string(...)`, not silently to `()`.
    let source = "x = 5\nprintln(\"value is $x\")";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "String interpolation should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("value is "),
        "interpolated literal text must survive, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("format!"),
        "interpolation must lower to a format!/concat, got:\n{}",
        rust_code
    );
    // Regression: the interpolation argument must NOT collapse to unit `()`.
    assert!(
        !rust_code.contains("println!(\"{}\", ())"),
        "interpolation must not collapse to `()`, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_string_interpolation_expr_and_float_7052() {
    // Interpolating an expression and a Float64 must reuse the StringConcat
    // float formatting (`a=3.0 b=5.0 done`).
    let source = "a = 3.0\nb = 2\nprintln(\"a=$a b=$(a+b) done\")";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "Expression interpolation should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("a=") && rust_code.contains(" done"),
        "interpolated literal text must survive, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("__sjulia_format_float64"),
        "Float64 interpolation must use Julia float formatting, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7057: bitwise `~`, `>>>` and Julia shift semantics
// ---------------------------------------------------------------------------

#[test]
fn test_aot_bitwise_not_and_ushr_define_helpers_7057() {
    // `~x` and `x >>> k` previously emitted calls to undefined `op_bnot` /
    // `op_urshift`. The prelude must now define them (and the SjuliaShift trait
    // carrying Julia over-shift / negative-shift semantics).
    let source = r#"
function f(x::Int64)
    a = ~x
    b = x >>> 2
    a + b
end
f(8)
"#;
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "`~` and `>>>` should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("fn op_bnot"),
        "prelude must define op_bnot, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("fn op_urshift") && rust_code.contains("trait SjuliaShift"),
        "prelude must define op_urshift and SjuliaShift trait, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_shift_routes_through_julia_helpers_7057() {
    // `<<` / `>>` must route through the Julia-faithful shift helpers so
    // over-shift (`1 << 64 == 0`) and negative shifts match upstream, instead
    // of Rust's panicking/masking native operators.
    let source = r#"
function g(x::Int64, k::Int64)
    (x << k) + (x >> k)
end
g(1, 4)
"#;
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "shift codegen should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("op_lshift") && rust_code.contains("op_rshift"),
        "`<<`/`>>` must route through op_lshift/op_rshift, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7051: Symbol literals `:foo` (interned-string carrier)
// ---------------------------------------------------------------------------

#[test]
fn test_aot_symbol_literal_carrier_7051() {
    // `:foo` lowers to a Core IR QuoteLiteral wrapping `SymbolNew("foo")`.
    // Without an arm it fell through to the LitNothing catch-all, so the symbol
    // printed as `nothing` and every symbol compared equal. It must now carry
    // the interned name so display and equality are correct.
    let source = r#"
function f()
    s = :hello
    println(s)
    println(s == :hello)
    println(s == :world)
end
f()
"#;
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "Symbol literal should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("hello") && rust_code.contains("world"),
        "interned symbol names must survive, got:\n{}",
        rust_code
    );
    // Regression: a symbol must NOT collapse to unit `()`.
    assert!(
        !rust_code.contains("println!(\"{}\", ())"),
        "symbol must not collapse to `()`, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7251: reachable-but-unused parametric Base structs (LogRange{T}) must
// not poison unrelated programs.
// ---------------------------------------------------------------------------

#[test]
fn test_aot_unused_parametric_base_struct_does_not_poison_7251() {
    // `sin` (and many Base functions) make Base's parametric `LogRange{T}`
    // reachable without ever constructing it. The compile must succeed; the
    // parametric struct definition is skipped rather than rejected.
    let source = "function f()\n    println(sin(1.0))\nend\nf()";
    let result = compile_to_rust_with_base_optimized(source);
    assert!(
        result.is_ok(),
        "a program that only makes a parametric Base struct reachable must compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    // The unused parametric struct must not be emitted.
    assert!(
        !rust_code.contains("struct LogRange"),
        "unused parametric struct must be skipped, got LogRange in output"
    );
}

#[test]
fn test_aot_parametric_struct_codegen_7040() {
    let source = r#"
struct Box{T}
    x::T
end

let b = Box{Int64}(41), c = Box(1.5)
    println(b.x + 1)
    println(c.x + 0.5)
end
"#;
    let result = compile_to_rust_with_base_optimized(source);
    assert!(
        result.is_ok(),
        "parametric struct constructors should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();

    assert!(
        rust_code.contains("pub struct Box<T>"),
        "parametric struct definition must use Rust generics, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("Box::<i64>::new(41i64)")
            && rust_code.contains("Box::<f64>::new(1.5_f64)"),
        "explicit and inferred constructors must instantiate concrete Rust types, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("let mut b: Box<i64>") && rust_code.contains("let mut c: Box<f64>"),
        "locals must carry instantiated parametric struct types, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_parametric_struct_7040");
}

// ---------------------------------------------------------------------------
// Issue #7038: InexactError-checked float→int / narrowing / Bool conversions
// ---------------------------------------------------------------------------

#[test]
fn test_aot_checked_float_to_int_conversion_7038() {
    // `Int64(x::Float64)` was gated; it now emits a round-trip check that
    // throws `InexactError` for non-integer / out-of-range values.
    let source = "function f(x::Float64)\n    Int64(x)\nend\nprintln(f(3.0))";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "checked Float64→Int64 should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("InexactError: Int64"),
        "must emit an InexactError check, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_checked_int_narrowing_conversion_7038() {
    // `Int8(x::Int64)` narrowing now emits a checked conversion with the
    // `trunc(Int8, …)` InexactError message.
    let source = "function f(x::Int64)\n    Int8(x)\nend\nprintln(f(100))";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "checked Int64→Int8 should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("InexactError: trunc(Int8"),
        "must emit a trunc InexactError check, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7050: top-level `@enum` codegen (Int32-backed integer semantics)
// ---------------------------------------------------------------------------

#[test]
fn test_aot_enum_codegen_7050() {
    // `@enum` at top level lowers to a `Stmt::EnumDef` in main; it must now be
    // collected into the AoT program, emit member constants, and type members
    // as Int32 so `c = member`, `Int(c)` and `c == member` work.
    let source = r#"
@enum Color red green blue
function f()
    c = green
    Int(c) + (c == red ? 1 : 0)
end
f()
"#;
    let result = compile_to_rust_with_base_optimized(source);
    assert!(
        result.is_ok(),
        "@enum should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("pub type Color")
            && rust_code.contains("green")
            && rust_code.contains("red"),
        "must emit the enum type and lowercase member constants, got:\n{}",
        rust_code
    );
    // Members keep their Julia names (no uppercasing) so references resolve.
    assert!(
        !rust_code.contains("const GREEN"),
        "enum members must not be uppercased, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7058: string functions lowered to Rust string methods
// ---------------------------------------------------------------------------

#[test]
fn test_aot_string_builtins_7058() {
    // `uppercase` / `lowercase` / `occursin` / `startswith` / `endswith` are
    // intercepted as Rust string methods; their pure-Julia Base bodies (which
    // reach `HasShape{1}`) are skipped as call-graph leaves.
    let source = r#"
function f(s::String)
    a = uppercase(s)
    b = occursin("ll", s)
    c = startswith(s, "he")
    d = endswith(s, "lo")
    (a, b, c, d)
end
f("hello")
"#;
    let result = compile_to_rust_with_base_optimized(source);
    assert!(
        result.is_ok(),
        "string builtins should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("to_uppercase")
            && rust_code.contains(".contains(")
            && rust_code.contains(".starts_with(")
            && rust_code.contains(".ends_with("),
        "string builtins must lower to Rust string methods, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7042: keyword arguments (kwargs) as trailing positional parameters
// ---------------------------------------------------------------------------

#[test]
fn test_aot_keyword_arguments_7042() {
    // Keyword params become trailing positional params; call sites fill them in
    // declaration order with the provided value or the default.
    let source = r#"
function scale(v::Float64; factor::Float64=2.0, offset::Float64=1.0)
    v * factor + offset
end
function f()
    a = scale(3.0)
    b = scale(3.0; factor=4.0)
    c = scale(3.0; factor=4.0, offset=5.0)
    a + b + c
end
f()
"#;
    // Before #7042 this failed with "no method matching scale(::Float64, …)"
    // because the keyword argument was passed positionally to a 1-param fn.
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "kwargs should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    // The omitted keyword defaults (`factor=2.0`, `offset=1.0`) must be filled
    // in — present whether `scale` is emitted as a function or inlined.
    assert!(
        rust_code.contains("2_f64") && rust_code.contains("1_f64"),
        "omitted keyword defaults must be materialized, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_keyword_arguments_string_return_7042() {
    // A keyword-taking function returning `string(...)` must type as String, so
    // the result prints with print (no quotes) — the `string(...)` variadic
    // return type fix that lands with kwargs.
    let source = r#"
function greet(name::String; greeting::String="Hello")
    string(greeting, ", ", name)
end
println(greet("World"; greeting="Hi"))
"#;
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "string-returning kwargs should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("greeting") && rust_code.contains("\"Hi\""),
        "keyword value must be threaded through, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7044: default positional arguments
// ---------------------------------------------------------------------------

#[test]
fn test_aot_default_positional_arguments_7044() {
    // `f(x, y=10, z=100)` lowers to forwarding stubs (`f(x) = f(x, 10, 100)`).
    // The stub must be matched to its own arity (not the full method's) and the
    // dynamic dispatcher must only carry max-arity arms, so every call site
    // compiles to the right mangled function.
    let source = r#"
function greet(x::Int, y::Int=10, z::Int=100)
    x + y + z
end
greet(1) + greet(1, 2) + greet(1, 2, 3)
"#;
    let result = compile_to_rust_with_base_optimized(source);
    assert!(
        result.is_ok(),
        "default positional args should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    // The arity-1 and arity-2 forwarding stubs are emitted as distinct mangled
    // functions, not collapsed onto the full method.
    assert!(
        rust_code.contains("greet_i64(") && rust_code.contains("greet_i64_i64("),
        "default-argument stubs must be emitted with their own arity, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7256: large/small whole-value floats print in Julia scientific
// notation. `println(1e30)` previously emitted Rust's decimal expansion
// (`1000000000000000000000000000000`); it must now route through the runtime
// formatter that yields `1.0e30`.
// ---------------------------------------------------------------------------

#[test]
fn test_aot_large_float_println_uses_runtime_formatter_7256() {
    let source = "function f()\n    println(1e30)\n    println(1.5e20)\nend\nf()";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "large-float println should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    // The float literal is rendered via the formatter, which delegates to the
    // runtime crate's Julia-faithful `format_float64_julia` (`1.0e30`).
    assert!(
        rust_code.contains("__sjulia_format_float64"),
        "float println must route through the formatter, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("subset_julia_vm_runtime::intrinsics::format_float64_julia(value)"),
        "the formatter must delegate to the runtime helper, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_inexact_error_embeds_runtime_formatted_float_7256() {
    // `Int64(1e30)` throws `InexactError: Int64(1.0e30)`; the embedded value is
    // rendered with the same runtime formatter, so the message matches upstream
    // Julia instead of showing the decimal expansion.
    let source = "function f(x::Float64)\n    Int64(x)\nend\nprintln(f(1e30))";
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "checked Float64->Int64 should compile, got: {:?}",
        result.err()
    );
    let rust_code = result.unwrap();
    assert!(
        rust_code.contains("InexactError: Int64"),
        "must emit an InexactError check, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("__sjulia_format_float64"),
        "the InexactError value must be rendered with the float formatter, got:\n{}",
        rust_code
    );
}

// ---------------------------------------------------------------------------
// Issue #7076: generated Rust must remain rustc `-D warnings` clean. This runs
// the emitted source as a real downstream crate instead of only checking header
// strings, so future codegen warning regressions fail under nextest.
// ---------------------------------------------------------------------------

#[test]
fn test_aot_generated_rust_checks_with_rustc_warnings_denied_7076() {
    // Float64 + Int64 emits the historically warning-prone `(a + (b as f64))`
    // expression as the sole println formatter argument, and the top-level
    // bindings exercise the generated global/static naming path.
    let source = "a = 3.0\nb = 2\nprintln(a + b)\n";
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("warning smoke program should compile to Rust");

    assert!(
        rust_code.contains("(a + (b as f64))"),
        "smoke source must exercise redundant parens, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(
        &rust_code,
        "sjulia_aot_generated_7076_rustc_warnings",
    );
}

#[test]
fn test_c_abi_export_signature_generated_rust_checks_7078() {
    let source = r#"
function add(x::Int64, y::Int64)
    x + y
end
"#;

    let rust_code = compile_to_rust_with_c_abi_exports(
        source,
        vec![CAbiExport::with_arg_types(
            "sjulia_add_i64",
            "add",
            vec![StaticType::I64, StaticType::I64],
        )],
    )
    .expect("signature-resolved C ABI export should compile");

    assert!(rust_code.contains(
        "#[no_mangle]\npub extern \"C\" fn sjulia_add_i64(x: i64, y: i64) -> i64 {\n    add(x, y)\n}"
    ));
    assert_generated_rust_checks_with_warnings_denied(
        &rust_code,
        "sjulia_generated_c_abi_issue_7078",
    );
}

#[test]
fn test_typed_overload_source_keeps_distinct_aot_signatures_7387() {
    let source = r#"
function add(x::Int64, y::Int64)
    x + y
end

function add(x::Float64, y::Float64)
    x + y
end

add(1, 2)
add(1.0, 2.0)
"#;
    let rust_code =
        compile_to_rust(source).expect("typed overload source should keep distinct AoT signatures");

    assert!(rust_code.contains("pub fn add_i64_i64(x: i64, y: i64) -> i64"));
    assert!(rust_code.contains("pub fn add_f64_f64(x: f64, y: f64) -> f64"));
    assert!(
        rust_code.contains("add_i64_i64(1i64, 2i64);"),
        "Int64 call should target the Int64 specialization, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("add_f64_f64(1_f64, 2_f64);"),
        "Float64 call should target the Float64 specialization, got:\n{}",
        rust_code
    );
}

#[test]
fn test_tuple_return_nested_destructuring_codegen_7048() {
    let source = r#"
function nested_pair(x)
    return (x, (x + 1, x + 2))
end

a, (b, c) = nested_pair(1)
println(a + b + c)
"#;
    let rust_code =
        compile_to_rust(source).expect("nested tuple return destructuring should compile to Rust");

    assert!(
        rust_code.contains("pub fn nested_pair(x: i64) -> (i64, (i64, i64))"),
        "nested_pair should keep a concrete tuple return type, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("a: i64 = __tuple_tmp_"),
        "top-level destructured binding should use tuple temp, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("b: i64 = __tuple_tmp_") && rust_code.contains(".1.0"),
        "nested binding b should index through the nested tuple, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("c: i64 = __tuple_tmp_") && rust_code.contains(".1.1"),
        "nested binding c should index through the nested tuple, got:\n{}",
        rust_code
    );
}

#[test]
fn test_tuple_return_rest_destructuring_codegen_7391() {
    let source = r#"
function triple(x)
    return (x, x + 1, x + 2)
end

a, rest... = triple(1)
println(a + rest[1] + rest[2])
"#;
    let rust_code =
        compile_to_rust(source).expect("tuple rest destructuring should compile to Rust");

    assert!(
        rust_code.contains("pub fn triple(x: i64) -> (i64, i64, i64)"),
        "triple should keep a concrete tuple return type, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("rest: (i64, i64) = (")
            && rust_code.contains("__tuple_tmp_")
            && rust_code.contains(".1")
            && rust_code.contains(".2"),
        "rest... should lower to a concrete tuple tail, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("rest.0") && rust_code.contains("rest.1"),
        "rest indexing should use Rust tuple field access, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_array_comprehension_codegen_7045() {
    let source = r#"
function twice(x::Int64)::Int64
    x * 2
end

xs = [x * 2 for x in 1:3]
ys = [x * 2 for x in 1:4 if x > 2]
zs = [i + j for i in 1:2, j in 1:2]
base = [1, 2, 3]
fs = [twice(x) for x in base]
println(xs[1] + xs[2] + xs[3] + ys[1] + ys[2] + zs[1] + zs[2] + zs[3] + zs[4] + fs[1] + fs[2] + fs[3])
"#;
    let rust_code = compile_to_rust(source).expect("array comprehensions should compile to Rust");

    assert!(
        rust_code.contains("let mut xs: Vec<i64> = { let mut __sjulia_comp: Vec<i64>"),
        "simple comprehension should build a concrete Vec, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("let mut ys: Vec<i64> = { let mut __sjulia_comp: Vec<i64>")
            && rust_code.contains("if (x > 2i64) { __sjulia_comp.push"),
        "filtered comprehension should guard push with the filter, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("let mut zs: Vec<i64> = { let mut __sjulia_comp: Vec<i64>")
            && rust_code.contains("for i in")
            && rust_code.contains("for j in")
            && rust_code.contains("__sjulia_comp.push((i).wrapping_add(j));"),
        "multi-clause comprehension should lower to nested loops, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("let mut fs: Vec<i64> = { let mut __sjulia_comp: Vec<i64>")
            && rust_code.contains("for x in base.iter().cloned()")
            && rust_code.contains("__sjulia_comp.push(twice(x));"),
        "function-call comprehension over an array variable should clone values into a Vec, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("Value::from(())"),
        "comprehensions must not fall through to the unsupported placeholder, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_lazy_range_and_char_range_codegen_7039() {
    let source = r#"
r = 1:3
xs = collect(r)
ys = collect(r)
zs = collect(6:-2:1)
cs = [c for c in 'a':'c']
println(xs[1] + xs[2] + xs[3] + ys[1] + ys[2] + ys[3] + zs[1] + zs[2] + zs[3] + length(cs) + length(r))
"#;
    let rust_code = compile_to_rust(source).expect("lazy ranges should compile to Rust");

    assert!(
        rust_code.contains("let mut r: SjuliaRange<i64>"),
        "numeric range should bind as a lazy SjuliaRange, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("SjuliaRange::new(1i64, 3i64, _sjulia_range_step)")
            && rust_code.contains("SjuliaRange::new(6i64, 1i64, _sjulia_range_step)"),
        "range expressions should construct lazy SjuliaRange carriers, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("r.clone().into_iter().collect::<Vec<_>>()"),
        "collect(r) should iterate a reusable lazy range without consuming the binding, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("SjuliaCharRange::new('a', 'c')"),
        "Char ranges should lower to the Char range carrier, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("_sjulia_range_values"),
        "range literals must not materialize an intermediate Vec, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_lazy_range_7039");
}

#[test]
fn test_aot_generator_expression_codegen_7046() {
    let source = r#"
xs = collect(x * 2 for x in 1:3)
ys = collect(x * 2 for x in 1:4 if x > 2)
total = sum(x * 2 for x in 1:3)
base = [1, 2, 3]
zs = collect(x + 1 for x in base)
println(xs[1] + xs[2] + xs[3] + ys[1] + ys[2] + total + zs[1] + zs[2] + zs[3])
"#;
    let rust_code = compile_to_rust(source).expect("generator expressions should compile to Rust");

    assert!(
        rust_code.contains("Box::new(")
            && rust_code.contains(".map(move |x|")
            && rust_code.contains("as Box<dyn Iterator<Item = i64>>"),
        "simple generator should lower to a boxed typed Rust iterator, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".filter_map(move |x|")
            && rust_code.contains("if (x > 2i64)")
            && rust_code.contains("Some((x).wrapping_mul(2i64))"),
        "filtered generator should lower to filter_map, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".sum::<i64>()"),
        "sum(generator) should consume the generated iterator directly, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("base.iter().cloned().map(move |x|"),
        "generator over an array variable should iterate cloned elements, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".collect::<Vec<_>>()") && !rust_code.contains("_sjulia_range_values"),
        "collect(generator) should not materialize range operands before collection, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_generator_7046");
}

#[test]
fn test_aot_namedtuple_construction_field_access_codegen_7049() {
    let source = r#"
nt = (a=1, b=2)
one = (only=41,)
println(nt.a + nt.b + one.only)
"#;
    let rust_code =
        compile_to_rust(source).expect("NamedTuple construction and field access should compile");

    assert!(
        rust_code.contains("let mut nt: (i64, i64) = (1i64, 2i64);"),
        "NamedTuple should lower to a field-ordered Rust tuple, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("(nt.0).wrapping_add(nt.1)"),
        "NamedTuple field access should lower to Rust tuple field access, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("let mut one: (i64,) = (41i64,);")
            && rust_code.contains(".wrapping_add(one.0)"),
        "single-field NamedTuple should use Rust one-tuple syntax, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("dynamic_binop"),
        "NamedTuple field arithmetic should stay fully static, got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_set_construction_membership_iteration_codegen_7035() {
    let source = r#"
s = Set([1, 2, 2])
push!(s, 3)
ok2 = 2 in s
ok4 = 4 in s
for x in s
end
xs = collect(s)
println(ok2)
println(ok4)
println(length(xs))
println(length(s))
println(isempty(s))
"#;
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("Set construction, membership, push!, and iteration should compile to Rust");

    assert!(
        rust_code.contains("std::collections::HashSet<i64>"),
        "Set{{Int64}} should lower to HashSet<i64>, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".contains(&"),
        "`in` on Set should lower to HashSet::contains, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".insert("),
        "push! on Set should lower to HashSet::insert, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".iter().cloned()"),
        "Set iteration should clone values out of HashSet iteration, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_set_7035");
}

#[test]
fn test_aot_dict_construction_lookup_haskey_iteration_codegen_7034() {
    let source = r#"
d = Dict("a" => 1, "b" => 2)
d["c"] = get(d, "a", 0) + 2
ok = haskey(d, "b")
miss = haskey(d, "z")
total = 0
for kv in d
    total += kv[2]
end
xs = collect(d)
typed = Dict{String, Int64}()
typed["len"] = length(xs)
println(d["a"])
println(ok)
println(miss)
println(total)
println(length(d))
println(isempty(d))
println(typed["len"])
"#;
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("Dict construction, lookup, haskey, assignment, and iteration should compile");

    assert!(
        rust_code.contains("std::collections::HashMap<String, i64>"),
        "Dict{{String, Int64}} should lower to HashMap<String, i64>, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".contains_key(&"),
        "haskey on Dict should lower to HashMap::contains_key, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".get(&"),
        "Dict lookup should lower to HashMap::get, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".insert("),
        "Dict assignment/construction should lower to HashMap::insert, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".iter().map(|(_sjulia_k, _sjulia_v)|"),
        "Dict iteration should clone key-value tuples from HashMap iteration, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_dict_7034");
}

// ---------------------------------------------------------------------------
// Issue #7311: the binary-op emitter wraps every operation in parentheses to
// preserve precedence in nested positions; at the top level (and as a sole
// function argument) these are redundant and trip rustc's `unused_parens` and
// clippy's `double_parens` lints. The generated header must allow both (plus
// `unused_braces`) so `clippy -D warnings` on the emitted crate stays clean.
// `--emit-binary` and execution are unaffected; this became observable only
// after #7242 let top-level globals compile far enough to reach the emitter.
// ---------------------------------------------------------------------------

#[test]
fn test_aot_header_allows_unused_parens_7311() {
    // A float + int binop is exactly the construct that makes the emitter produce
    // a redundant `(a + (b as f64))` passed as the sole `println` formatter
    // argument — the case clippy flags as both `unused_parens` and
    // `clippy::double_parens`. (This helper localizes the main block, so `a`/`b`
    // stay bare locals rather than `__sjulia_global_*` statics; the redundant
    // parens are identical either way.)
    let source = "a = 3.0\nb = 2\nprintln(a + b)\n";
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("float + int binop program should compile to Rust");

    // The redundant top-level paren the emitter produces must actually be present;
    // otherwise this test would pass vacuously even if the construct changed.
    assert!(
        rust_code.contains("(a + (b as f64))"),
        "the binop emitter should still produce the redundant top-level paren, got:\n{}",
        rust_code
    );

    // The header must silence the lints the redundant parens/braces would trip,
    // so `clippy -D warnings` on the generated crate does not fail.
    assert!(
        rust_code.contains("#![allow(unused_parens)]"),
        "generated header must allow unused_parens (Issue #7311), got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("#![allow(unused_braces)]"),
        "generated header must allow unused_braces (Issue #7311), got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("#![allow(clippy::double_parens)]"),
        "generated header must allow clippy::double_parens (Issue #7311), got:\n{}",
        rust_code
    );
}

#[test]
fn test_aot_header_allows_unused_parens_for_local_binop_7311() {
    // A local `p + q` also lowers to a parenthesized `(p + q)`; the allow header
    // is emitted unconditionally so any binop-bearing program stays clippy-clean.
    let source = "function f(p, q)\n    p + q\nend\nprintln(f(1, 2))\n";
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("local binop program should compile to Rust");

    assert!(
        rust_code.contains("#![allow(unused_parens)]"),
        "generated header must allow unused_parens (Issue #7311), got:\n{}",
        rust_code
    );
}

// Issue #8181: a local first assigned inside `if`/`elseif`/`else` branches and
// referenced after the block must be hoisted to a deferred `let mut x: T;` at
// function scope. Previously the branch assignment emitted a block-scoped `let`,
// so the post-block reference failed to compile with `cannot find value`.
#[test]
fn aot_branch_assigned_local_used_after_if_compiles_8181() {
    let source = r#"
function classify(c)
    if c
        v = 1.0
    else
        v = 2.0
    end
    v
end
println(classify(true))
"#;
    let rust_code =
        compile_to_rust(source).expect("branch-assigned local program should compile to Rust");
    // The declaration must be hoisted (deferred `let mut v`), and the in-branch
    // assignments must not re-introduce a block-scoped `let v`.
    assert!(
        rust_code.contains("let mut v: f64;"),
        "branch-escaping local must get a deferred function-scope declaration, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("let mut v: f64 = 1"),
        "in-branch assignment must not emit a block-scoped `let`, got:\n{}",
        rust_code
    );
    // And the whole crate must compile cleanly under `-D warnings`.
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_branch_local_8181");
}

// Issue #8181: a local assigned in a 4-way if/elseif/elseif/else chain and used
// afterwards (the IFS-fractal / Barnsley-fern shape) must compile.
#[test]
fn aot_multi_branch_assigned_local_compiles_8181() {
    let source = r#"
function pick(r, x)
    if r < 0.25
        nx = 0.1 * x
    elseif r < 0.5
        nx = 0.2 * x
    elseif r < 0.75
        nx = 0.3 * x
    else
        nx = 0.4 * x
    end
    nx
end
println(pick(0.6, 2.0))
"#;
    let rust_code =
        compile_to_rust(source).expect("multi-branch local program should compile to Rust");
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_multi_branch_local_8181");
}

// Issue #8180: chained `a + b + c` lowers to an n-ary `+(a, b, c)` call that the
// converter unfolds into nested binary ops. It must compile to Rust without
// pulling the variadic prelude `+`/`afoldl` path (which AoT cannot lower).
#[test]
fn aot_nary_addition_compiles_without_afoldl_8180() {
    let source = r#"
function add3(a, b, c)
    a + b + c
end
println(add3(1.0, 2.0, 3.0))
"#;
    let rust_code =
        compile_to_rust(source).expect("n-ary `+` program should compile to Rust (Issue #8180)");
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_nary_add_8180");
}

// Scalar Mandelbrot AoT codegen smoke: typed Float64/Int64 arithmetic, nested
// while loops with early return, no Complex or broadcasting. Verifies that the
// AoT pipeline emits clean Rust for the core escape-time pattern.
#[test]
fn test_aot_mandelbrot_scalar_codegen() {
    let source = include_str!("fixtures/aot/mandelbrot_scalar_aot.jl");
    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("scalar Mandelbrot should compile to Rust via AoT pipeline");

    assert!(
        rust_code.contains("fn mandel_point"),
        "AoT should emit a `mandel_point` function, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("fn mandel_count"),
        "AoT should emit a `mandel_count` function, got:\n{}",
        rust_code
    );
    // Both functions must carry concrete Float64/Int64 signatures (no Value/dynamic dispatch).
    assert!(
        rust_code.contains("f64") && rust_code.contains("i64"),
        "AoT scalar Mandelbrot must use concrete f64/i64 types, got:\n{}",
        rust_code
    );

    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_mandelbrot_scalar");
}
