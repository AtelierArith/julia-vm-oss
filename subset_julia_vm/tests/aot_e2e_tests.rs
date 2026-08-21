//! AoT End-to-End Tests for Phase 1
//!
//! These tests verify that the AoT compiler correctly compiles Julia source code
//! to Rust code and that the type inference produces correct results.

#![cfg(feature = "aot")]

use std::collections::HashSet;
use std::fs;
use std::process::Command;
use subset_julia_vm::aot::analyze::{program_to_aot_ir, reverse_generator_lifts_in_program};
use subset_julia_vm::aot::call_graph::CallGraph;
use subset_julia_vm::aot::codegen::aot_codegen::AotCodeGenerator;
use subset_julia_vm::aot::codegen::{CAbiExport, CodegenConfig};
use subset_julia_vm::aot::inference::TypeInferenceEngine;
use subset_julia_vm::aot::optimizer::optimize_aot_program_full;
use subset_julia_vm::aot::types::StaticType;
use subset_julia_vm::aot::{compile_program, CompileConfig};
use subset_julia_vm::base;
use subset_julia_vm::ir::core::{Block, Expr, LocalDeclKind, Program, Stmt};
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::span::Span;

fn lower_for_aot(source: &str) -> Result<Program, String> {
    let mut parser = Parser::new().map_err(|e| format!("Parser error: {:?}", e))?;
    let outcome = parser
        .parse(source)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Macro expansion seam (Issue #9178): idempotent install of the VM-backed
    // expander and Base/stdlib macro registry, mirroring the CLI `build_program`
    // path (subset_julia_vm/src/bin/aot.rs). Without this, Base-prelude macros
    // (`@time`, `@elapsed`, …) do not resolve during lowering and the user source
    // fails with `unknown macro @time`.
    subset_julia_vm::macro_runtime::install();

    // Lower to Core IR
    let mut lowering = Lowering::new(source);
    let mut program = lowering
        .lower(outcome)
        .map_err(|e| format!("Lowering error: {:?}", e))?;
    localize_main_block(&mut program);

    // Reverse the #9103 generator-body lift before inference (Issue #9179),
    // mirroring the CLI `prepare_aot_program` pipeline.
    reverse_generator_lifts_in_program(&mut program);
    Ok(program)
}

/// Helper function to compile Julia source to Rust code
fn compile_to_rust(source: &str) -> Result<String, String> {
    let program = lower_for_aot(source)?;

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

    // Macro expansion seam (Issue #9178): see `compile_to_rust`.
    subset_julia_vm::macro_runtime::install();

    let mut lowering = Lowering::new(source);
    let mut program = lowering
        .lower(outcome)
        .map_err(|e| format!("Lowering error: {:?}", e))?;
    localize_main_block(&mut program);

    // Reverse the #9103 generator-body lift before inference (Issue #9179).
    reverse_generator_lifts_in_program(&mut program);

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

    // Macro expansion seam (Issue #9178): idempotent install of the VM-backed
    // expander and Base/stdlib macro registry, mirroring the CLI `build_program`
    // path so Base-prelude macros (`@time`, `@elapsed`, …) resolve while lowering
    // the user source.
    subset_julia_vm::macro_runtime::install();

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
    let mut program = call_graph.filter_program(&program);

    // Reverse the #9103 generator-body lift before inference (Issue #9179),
    // mirroring the CLI `prepare_aot_program` pipeline.
    reverse_generator_lifts_in_program(&mut program);

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

/// Helper matching the public `juliars` compiler pipeline after parsing and
/// prelude merge. Use this for regressions where the canonical AoT pass sequence
/// matters, not just raw AoT IR conversion/codegen.
fn compile_to_rust_with_base_canonical(source: &str) -> Result<String, String> {
    let mut parser = Parser::new().map_err(|e| format!("Parser error: {:?}", e))?;

    subset_julia_vm::macro_runtime::install();

    let prelude_src = base::get_prelude();
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

    compile_program(program, &CompileConfig::default())
        .map(|result| result.output.rust_code)
        .map_err(|e| format!("Canonical AoT compile error: {:?}", e))
}

fn check_generated_rust(
    rust_code: &str,
    crate_name: &str,
    deny_warnings: bool,
) -> std::process::Output {
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
    let mut command = Command::new(cargo);
    command
        .arg("check")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", dir.path().join("target"));
    if deny_warnings {
        command.env("RUSTFLAGS", "-Dwarnings");
    }
    command
        .output()
        .expect("run cargo check for generated Rust")
}

/// Like [`assert_generated_rust_checks_with_warnings_denied`] but without
/// `-D warnings`, so it fails on hard compile *errors* only (not lint
/// warnings). Use to isolate a codegen bug that produces ill-formed Rust from
/// unrelated lint warts in the same generated program.
fn assert_generated_rust_compiles(rust_code: &str, crate_name: &str) {
    let output = check_generated_rust(rust_code, crate_name, false);

    assert!(
        output.status.success(),
        "generated Rust must compile (no hard errors)\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_generated_rust_checks_with_warnings_denied(rust_code: &str, crate_name: &str) {
    let output = check_generated_rust(rust_code, crate_name, true);

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

#[test]
fn test_aot_binding_provenance_matrix_11317() {
    #[derive(Clone, Copy)]
    struct DeclExpectation {
        name_prefix: &'static str,
        kind: LocalDeclKind,
    }

    struct ProvenanceCase {
        name: &'static str,
        source: &'static str,
        expected_stdout: Option<&'static str>,
        expected_slot: Option<&'static str>,
        decl: DeclExpectation,
    }

    fn collect_expr_decls<'a>(expr: &'a Expr, decls: &mut Vec<(&'a str, LocalDeclKind)>) {
        if let Expr::LetBlock { body, .. } = expr {
            collect_block_decls(body, decls);
        }
    }

    fn collect_block_decls<'a>(block: &'a Block, decls: &mut Vec<(&'a str, LocalDeclKind)>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::LocalDecl { var, kind, .. } => decls.push((var, *kind)),
                Stmt::Block(body)
                | Stmt::For { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::ForEachTuple { body, .. }
                | Stmt::While { body, .. }
                | Stmt::Timed { body, .. }
                | Stmt::TestSet { body, .. } => collect_block_decls(body, decls),
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_block_decls(then_branch, decls);
                    if let Some(branch) = else_branch {
                        collect_block_decls(branch, decls);
                    }
                }
                Stmt::Try {
                    try_block,
                    catch_block,
                    else_block,
                    finally_block,
                    ..
                } => {
                    collect_block_decls(try_block, decls);
                    for branch in [catch_block, else_block, finally_block]
                        .into_iter()
                        .flatten()
                    {
                        collect_block_decls(branch, decls);
                    }
                }
                Stmt::Assign { value, .. }
                | Stmt::AddAssign { value, .. }
                | Stmt::Expr { expr: value, .. } => collect_expr_decls(value, decls),
                Stmt::Return {
                    value: Some(value), ..
                } => collect_expr_decls(value, decls),
                _ => {}
            }
        }
    }

    let cases = [
        ProvenanceCase {
            name: "aot_binding_provenance_typed_11317",
            source: "function typed_11317()\n    total = 0\n    for i in 1:3\n        local step = i\n        total += step\n    end\n    println(total)\nend\ntyped_11317()",
            expected_stdout: Some("6"),
            expected_slot: Some("let mut step: i64"),
            decl: DeclExpectation {
                name_prefix: "step",
                kind: LocalDeclKind::Explicit,
            },
        },
        ProvenanceCase {
            name: "aot_binding_provenance_compiler_enclosing_11317",
            source: "function enclosing_11317(flag::Bool)\n    value = begin\n        if flag\n            8\n        else\n            9\n        end\n    end\n    println(value)\nend\nenclosing_11317(true)",
            // The provenance consumer is the Core IR -> AoT IR conversion below.
            // Downstream codegen for this valid Any -> Int64 let-result shape is
            // tracked separately by Issue #11352.
            expected_stdout: None,
            expected_slot: None,
            decl: DeclExpectation {
                name_prefix: "__sjvm_if_result_",
                kind: LocalDeclKind::CompilerEnclosing,
            },
        },
    ];

    for case in cases {
        let program = lower_for_aot(case.source)
            .unwrap_or_else(|error| panic!("{} failed to lower: {error}", case.name));
        let mut decls = Vec::new();
        for function in &program.functions {
            collect_block_decls(&function.body, &mut decls);
        }
        assert!(
            decls.iter().any(|(name, kind)| {
                name.starts_with(case.decl.name_prefix) && *kind == case.decl.kind
            }),
            "{} missing {:?} declaration with prefix `{}`: {decls:?}",
            case.name,
            case.decl.kind,
            case.decl.name_prefix
        );

        let mut type_engine = TypeInferenceEngine::new();
        let typed_program = type_engine
            .analyze_program(&program)
            .unwrap_or_else(|error| panic!("{} failed inference: {error}", case.name));
        program_to_aot_ir(&program, &typed_program)
            .unwrap_or_else(|error| panic!("{} failed AoT IR conversion: {error}", case.name));

        if let (Some(expected_slot), Some(expected_stdout)) =
            (case.expected_slot, case.expected_stdout)
        {
            let rust_code = compile_to_rust(case.source)
                .unwrap_or_else(|error| panic!("{} failed to compile: {error}", case.name));
            assert!(
                rust_code.contains(expected_slot),
                "{} must preserve its expected local representation `{}`:\n{}",
                case.name,
                expected_slot,
                rust_code
            );
            assert_generated_rust_runs_with_stdout(&rust_code, case.name, expected_stdout);
        }
    }
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
            // `prelude_program` was just lowered above, so each Arc here is
            // uniquely owned (refcount 1) — `make_mut` never clones.
            std::sync::Arc::make_mut(&mut f).is_base_extension = true;
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
fn aot_time_noncopy_assignment_avoids_dead_passthrough_move_issue_8499() {
    std::thread::Builder::new()
        .name("aot-time-noncopy-8499".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let source = r#"
@time xs = [41, 42]
println(xs[2])
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
                rust_code.contains("let mut xs: Vec<i64> = vec![41i64, 42i64];"),
                "Generated code should preserve the assignment inside @time, got:\n{}",
                rust_code
            );
            assert!(
                !rust_code.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with("let mut result__time_") && line.ends_with(" = xs;")
                }),
                "statement-position @time must not move xs into a dead result slot, got:\n{}",
                rust_code
            );
            assert!(
                rust_code.contains("let _sjulia_arr = &xs"),
                "Generated code should preserve downstream uses of the assigned value, got:\n{}",
                rust_code
            );
            assert_generated_rust_compiles(&rust_code, "aot_time_noncopy_assignment_8499");
        })
        .expect("spawn AoT regression with a large parser/lowering stack")
        .join()
        .expect("AoT regression thread panicked");
}

#[test]
fn test_aot_e2e_nested_assignment_materializes_every_store_11310() {
    let plain = compile_to_rust("result = (x = 42)\nprintln(result + x)\n")
        .expect("plain nested assignment should compile");
    assert!(plain.contains("let mut x: i64 = 42i64;"), "{plain}");
    assert!(plain.contains("let mut result: i64 = x;"), "{plain}");

    let timed_chain = compile_to_rust("@time a = (b = 42)\nprintln(a + b)\n")
        .expect("timed chained assignment should compile");
    assert!(
        timed_chain.contains("let mut b: i64 = 42i64;"),
        "{timed_chain}"
    );
    assert!(timed_chain.contains("let mut a: i64 = b;"), "{timed_chain}");

    let assigned_time = compile_to_rust("y = @time x = 42\nprintln(y + x)\n")
        .expect("assigned @time expression should compile");
    assert!(
        assigned_time.contains("let mut x: i64 = 42i64;"),
        "{assigned_time}"
    );
    assert!(
        assigned_time.lines().any(|line| line
            .trim_start()
            .starts_with("let mut y: i64 = result__time_")),
        "{assigned_time}"
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
    let source = r#"
function mandelbrot_escape(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandelbrot_grid(width::Int64, height::Int64, maxiter::Int64)
    xs = range(-2.0, 1.0; length=width)
    ys = range(1.2, -1.2; length=height)
    C = xs' .+ im .* ys
    counts = mandelbrot_escape.(C, maxiter)
    sum(counts)
end

println(mandelbrot_grid(50, 40, 50))
"#;
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
        rust_code.contains("fn op_add_f64_complex") && rust_code.contains("fn op_mul_complex_f64"),
        "broadcast operator references should have emitted Rust wrappers, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("__aot_broadcast_outer_product(op_add_f64_complex,")
            && !rust_code.contains("op_add_f64_complex_float64_"),
        "broadcast operator references should use emitted ComplexF64 wrappers, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("if let Some(x) = value.as_f64() { return x * x; }"),
        "boxed abs2 should handle real Value inputs before Complex-only fallback, got:\n{}",
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

    assert_generated_rust_compiles(&rust_code, "aot_mandelbrot_broadcast_8790");
}

#[test]
fn aot_mandelbrot_ref_broadcast_result_drives_bool_condition_issue_11812() {
    let source = r#"
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

function mandelbrot_grid(width, height, maxiter)
    xs = range(-2.0, 1.0; length=width)
    ys = range(1.2, -1.2; length=height)
    C = xs' .+ im .* ys
    mandelbrot_escape.(C, Ref(maxiter))
end

function main()
    grid = mandelbrot_grid(2, 2, 5)
    n = grid[1, 1]
    if n > 0
        println("inside")
    end
end

main()
"#;

    let rust_code = compile_to_rust_with_base_optimized(source)
        .expect("Ref-protected Mandelbrot broadcast should compile");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_mandelbrot_ref_result_11812", "inside");
}

#[test]
fn test_aot_e2e_matrix_sum_column_major_order_8790() {
    let source = r#"
A = [1.0e100 1.0; -1.0e100 2.0]
println(sum(A) == 3.0)
"#;
    let rust_code = compile_to_rust_with_base_optimized(source).expect("matrix sum should compile");

    assert!(
        rust_code.contains("__sjulia_sum_arr[__sjulia_i0][__sjulia_i1].clone()")
            && !rust_code.contains(".flatten().cloned().sum"),
        "sum(matrix) should iterate in Julia column-major order, got:\n{}",
        rust_code
    );
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_matrix_sum_column_major_8790", "true");
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
fn test_aot_complexf64_annotation_matches_im_arithmetic_result_issue_8795() {
    let source = r#"
function f(c::ComplexF64)::Float64
    abs2(c)
end

f(1.0 + 2.0im)
"#;
    let result = compile_to_rust(source);
    assert!(
        result.is_ok(),
        "ComplexF64 annotation should match the inferred im-arithmetic Complex result: {:?}",
        result.err()
    );
    assert_generated_rust_checks_with_warnings_denied(
        &result.unwrap(),
        "aot_complexf64_annotation_8795",
    );
}

#[test]
fn test_aot_e2e_abs2_any_return_stays_boxed_issue_8790() {
    let source = r#"
function f(x::Any)
    abs2(x)
end

x::Any = 3.0
f(x)
"#;
    let rust_code = compile_to_rust_with_base_optimized(source).expect("boxed abs2 should compile");
    assert!(
        rust_code.contains("Value::from(abs2_value"),
        "boxed abs2 should return a runtime Value for Any/Union callers, got:\n{}",
        rust_code
    );
    assert_generated_rust_compiles(&rust_code, "aot_abs2_any_boxed_8790");
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
// Closure Capture and Invocation Tests (Julia 1.12.4 Oracle Coverage)
// ============================================================================
// These tests verify AoT compilation and execution of closures with various
// capture patterns, following the Julia 1.12.4 oracle baseline. Each test
// compiles to Rust and verifies the generated binary produces the expected
// output matching upstream Julia 1.12.4.

#[test]
fn test_aot_closure_execution_noncapturing_lambda() {
    // Execution: noncapturing lambda returns correct result.
    let source = r#"
function noncapturing_lambda_exec()::Int64
    f = x -> x + 1
    f(41)
end
println(noncapturing_lambda_exec())
"#;
    let rust_code = compile_to_rust(source).expect("noncapturing lambda execution must compile");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_closure_noncapturing", "42");
}

#[test]
fn test_aot_closure_execution_immutable_scalar() {
    // Execution: immutable scalar capture returns correct value.
    let source = r#"
function immutable_scalar_exec()::Int64
    x = 10
    f = () -> x
    f()
end
println(immutable_scalar_exec())
"#;
    let rust_code = compile_to_rust(source).expect("immutable scalar execution must compile");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_closure_immutable_scalar", "10");
}

#[test]
fn test_aot_closure_execution_distinct_captures() {
    // Execution: two closures with distinct captures return correct values.
    let source = r#"
function distinct_captures_exec()::Tuple{Int64, Int64}
    x = 5
    y = 10
    f1 = () -> x
    f2 = () -> y
    (f1(), f2())
end
r1, r2 = distinct_captures_exec()
println("$r1 $r2")
"#;
    let rust_code = compile_to_rust(source).expect("distinct captures execution must compile");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_closure_distinct_captures", "5 10");
}

#[test]
fn test_aot_closure_execution_curried() {
    // Execution: curried closure returns correct result.
    let source = r#"
function curried_exec()::Int64
    make_adder = x -> (y -> x + y)
    add5 = make_adder(5)
    add5(37)
end
println(curried_exec())
"#;
    let rust_code = compile_to_rust(source).expect("curried closure execution must compile");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_closure_curried", "42");
}

#[test]
fn test_aot_closure_execution_nested_return() {
    // Execution: nested closure return and invoke returns correct result.
    let source = r#"
function nested_return_exec()::Int64
    outer = x -> begin
        inner = y -> x + y
        inner
    end
    f = outer(100)
    f(23)
end
println(nested_return_exec())
"#;
    let rust_code = compile_to_rust(source).expect("nested closure return execution must compile");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_closure_nested_return", "123");
}

#[test]
fn test_aot_closure_execution_calling_helper() {
    // Execution: closure calling helper returns correct result.
    let source = r#"
function helper_call_exec()::Int64
    helper(a, b) = a * b
    x = 7
    f = () -> helper(x, 6)
    f()
end
println(helper_call_exec())
"#;
    let rust_code = compile_to_rust(source).expect("closure calling helper execution must compile");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_closure_helper_call", "42");
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
fn test_aot_generated_rust_ownership_gate_detects_reused_value_11202() {
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
    let rust_code = compile_to_rust(source).expect("ownership probe should generate Rust");
    assert!(
        rust_code.contains("dynamic_call(\"iterate\", &[itr])")
            && rust_code.contains("dynamic_call(\"iterate\", &[itr, st])"),
        "probe must contain both uses of the same generated binding, got:\n{rust_code}"
    );

    let output = check_generated_rust(&rust_code, "sjulia_aot_ownership_negative_11202", false);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() && stderr.contains("E0382") && stderr.contains("itr"),
        "known #10663 output must be rejected as a moved-value use until that issue flips this to a positive compile gate\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        stderr
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
fn test_flat_nonliteral_destructuring_codegen_10464() {
    let source = r#"
function pair_10464(x)
    (x, x + 1)
end

function destructure_tail_10464(x)
    (a, b) = pair_10464(x)
end

destructure_tail_10464(10)
"#;
    let rust_code = compile_to_rust(source)
        .expect("flat nonliteral destructuring should compile through explicit AoT IR");

    assert!(
        rust_code.contains("let _destructure_value_aot_")
            && rust_code.contains("let mut a: i64 = _destructure_value_aot_")
            && rust_code.contains("let mut b: i64 = _destructure_value_aot_"),
        "AoT should evaluate the RHS into one tuple temp and bind both indexed elements:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("_destructure_value_aot_") && rust_code.contains(".1"),
        "tail position should yield the original tuple temp:\n{}",
        rust_code
    );
}

#[test]
fn test_indexable_and_arity_destructuring_codegen_10464() {
    let source = r#"
function from_array_10464()
    (a, b) = [1, 2, 3]
    a + b
end
function from_range_10464()
    (a, b) = 4:7
    a + b
end
function short_10464()
    (a, b) = (8,)
    a + b
end
from_array_10464()
from_range_10464()
"#;
    let rust_code = compile_to_rust(source)
        .expect("Array/Range destructuring and short tuple runtime checks should codegen");
    assert!(rust_code.contains("BoundsError"), "{rust_code}");
    assert!(rust_code.contains("SjuliaRange"), "{rust_code}");
    assert!(
        rust_code.contains("destructure_cursor_aot") && rust_code.contains(".as_mut().next()"),
        "{rust_code}"
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
fn test_aot_generator_filtered_and_tuple_binding_lift_reversal_9292() {
    // Regression (Issue #9292): PR #9274 (Issue #9127) extended the generator
    // lowering to lift a filtered generator's predicate into `__gen_pred_N` and
    // to inject a tuple-destructuring prologue (`a = arg[1]; b = arg[2]`) into the
    // lifted `__gen_body_N` / `__gen_pred_N` functions. The AoT lift-reversal
    // pre-pass (Issue #9179) originally inlined only the scalar body, so:
    //   * a filtered generator left a dangling `__gen_pred_N(x)` filter whose
    //     dropped definition made the AoT control-flow condition `Any` (the
    //     reported failure), and
    //   * a tuple-binding generator's prologue defeated the single-`return`
    //     inliner and tripped the #7014 diagnostic.
    // The pre-pass now inlines the predicate and substitutes the destructuring
    // prologue, so both shapes reverse to the eager/inline generator AoT supports.

    // Filtered scalar generators: the predicate must inline to an inline Bool
    // condition (not a dangling `__gen_pred_N` call), across `collect` and `sum`.
    let filtered = r#"
ys = collect(x * 2 for x in 1:6 if x > 2)
total = sum(x + 1 for x in 1:5 if x < 3)
println(ys[1] + ys[2] + ys[3] + ys[4] + total)
"#;
    let rust = compile_to_rust(filtered).expect("filtered generator should compile to Rust");
    assert!(
        rust.contains(".filter_map(move |x|") && rust.contains("if (x > 2i64)"),
        "filtered generator should lower to filter_map with an inline predicate, got:\n{}",
        rust
    );
    assert!(
        !rust.contains("__gen_pred") && !rust.contains("__gen_body"),
        "lifted body/predicate must be inlined, not left as helper calls, got:\n{}",
        rust
    );
    assert_generated_rust_checks_with_warnings_denied(&rust, "aot_generator_filtered_9292");

    // Tuple-destructuring generators, plain and filtered: the prologue
    // (`a = arg[1]; b = arg[2]`) must be substituted into inline index
    // expressions over the loop element, leaving no synthetic helper/param.
    let tuple = r#"
pairs = [(1, 5), (7, 2), (3, 4)]
sums = collect(a + b for (a, b) in pairs)
picks = collect(a * b for (a, b) in pairs if a < b)
println(sums[1] + sums[2] + sums[3] + picks[1] + picks[2])
"#;
    let rust = compile_to_rust(tuple).expect("tuple-binding generator should compile to Rust");
    // The lifted body/predicate helpers must be inlined away; the synthetic
    // single tuple parameter (`__gen_arg_N`) legitimately survives as the
    // reversed generator's loop variable / closure parameter, which the body
    // indexes by field (`__gen_arg_N.0`, `__gen_arg_N.1`).
    assert!(
        !rust.contains("__gen_body") && !rust.contains("__gen_pred"),
        "tuple-binding lift must inline the body/predicate helpers, got:\n{}",
        rust
    );
    assert!(
        rust.contains(".map(move |__gen_arg") && rust.contains(".filter_map(move |__gen_arg"),
        "tuple-binding generator should reverse to inline map / filter_map over the loop element, got:\n{}",
        rust
    );
    assert_generated_rust_checks_with_warnings_denied(&rust, "aot_generator_tuple_9292");
}

#[test]
fn test_aot_generator_lifted_body_expression_position_9179() {
    // Regression (Issue #9179): the #9103 generator-body lift wraps a non-trivial
    // generator body (anything other than a plain unary `f(var)` call) in a
    // bindings-free `let` block of the shape
    //     let function __gen_body_N(x); return x * x + 1; end
    //         (__gen_body_N(x) for x in 1:3) end
    // In expression position (a `sum(...)` / `collect(...)` argument) the AoT IR
    // converter rejected this with the #7014 "expression-position begin/let block"
    // diagnostic. The converter now reverses the lift (inlining the trivial call
    // into the generator body), so the program compiles and the body is emitted
    // inline rather than through a standalone (dead) helper function.
    let source = r#"
total = sum(x * x + 1 for x in 1:3)
doubled = collect(x + x for x in 1:3)
println(total + doubled[1] + doubled[2] + doubled[3])
"#;
    let rust_code = compile_to_rust(source).expect(
        "lifted generator body in expression position should compile to Rust (Issue #9179)",
    );

    // The lifted body must be inlined into the generator, not routed through a
    // standalone `__gen_body_*` helper (which would also be dead code under
    // `-D warnings`).
    assert!(
        !rust_code.contains("__gen_body"),
        "lifted generator body should be inlined, not emitted as a helper function, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".sum::<i64>()"),
        "sum over a lifted generator should consume the iterator directly, got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains(".map(move |x|"),
        "lifted generator should still lower to a mapped iterator, got:\n{}",
        rust_code
    );
    assert_generated_rust_checks_with_warnings_denied(&rust_code, "aot_generator_lifted_9179");
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

// ─────────────────────────────────────────────────────────────────────────────
// ADR_BACKEND_STRATEGY.md acceptance programs (Issue #8639)
//
// The owner-decided acceptance bar for the AoT backend: the three benchmark
// kernels (coprime pi / Aizawa / Mandelbrot) must compile AND run under AoT
// with stdout identical to upstream Julia. Third-party package loading is
// explicitly OUT of the AoT guarantee. The same fixtures also run under the
// VM via fixtures/aot/manifest.toml, so VM/AoT/julia stay in three-way parity.
// ─────────────────────────────────────────────────────────────────────────────

/// Compile the generated Rust into a crate and RUN it, asserting exact stdout.
fn assert_generated_rust_runs_with_stdout(
    rust_code: &str,
    crate_name: &str,
    expected_stdout: &str,
) {
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
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .output()
        .expect("run generated Rust binary");

    assert!(
        output.status.success(),
        "generated Rust binary must run successfully\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim_end(),
        expected_stdout,
        "AoT binary stdout must match upstream Julia exactly (ADR #8639 acceptance)"
    );
}

/// Strip the trailing bare fixture-result expression (`result == <val>`) so the
/// acceptance program ends with its `println` — the AoT binary's stdout is then
/// exactly the parity payload.
fn acceptance_source(fixture: &str) -> String {
    let mut lines: Vec<&str> = fixture.lines().collect();
    while let Some(last) = lines.last() {
        if last.trim().is_empty() || last.trim_start().starts_with("result ==") {
            lines.pop();
        } else {
            break;
        }
    }
    lines.join("\n")
}

#[test]
fn test_aot_acceptance_coprime_pi_8639() {
    let source = acceptance_source(include_str!("fixtures/aot/coprime_pi_acceptance_aot.jl"));
    let rust_code = compile_to_rust_with_base_optimized(&source)
        .expect("ADR #8639 acceptance: coprime pi must compile via the AoT pipeline");
    assert_generated_rust_runs_with_stdout(
        &rust_code,
        "aot_accept_coprime_pi",
        "3.139597498005517",
    );
}

#[test]
fn test_aot_acceptance_aizawa_8639() {
    let source = acceptance_source(include_str!("fixtures/aot/aizawa_acceptance_aot.jl"));
    let rust_code = compile_to_rust_with_base_optimized(&source)
        .expect("ADR #8639 acceptance: Aizawa attractor must compile via the AoT pipeline");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_accept_aizawa", "6617.642224697513");
}

#[test]
fn test_aot_acceptance_mandelbrot_8639() {
    let source = acceptance_source(include_str!("fixtures/aot/mandelbrot_acceptance_aot.jl"));
    let rust_code = compile_to_rust_with_base_optimized(&source)
        .expect("ADR #8639 acceptance: Mandelbrot must compile via the AoT pipeline");
    assert_generated_rust_runs_with_stdout(&rust_code, "aot_accept_mandelbrot", "8278");
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #10251 / #10111: same-named locals in sibling `let` scopes must get
// independent, precisely-typed storage — NOT be unified under the first-seen
// static type. Two sibling top-level `let` blocks that each bind `r` to a
// different concrete numeric type previously collapsed onto one slot, emitting
// non-compiling Rust (`Value: From<i8>` unsatisfied — the runtime `Value` enum
// has no Int8/UInt8 variant) or a truncating `i8::try_from(144)` panic. The IR
// converter now enters/exits a lexical scope per `let` block, so each `r` is a
// fresh binding typed by its own initializer and the two `let mut r` shadow
// correctly in the flattened Rust scope.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_aot_sibling_let_same_name_distinct_types_10251() {
    // Mirrors the #10111 reproduction: `r` is bound to Int8 in the first `let`
    // and UInt8 in the second. Upstream Julia and the sjulia VM both print
    // `Int8 6` / `UInt8 144`; the AoT binary must match.
    let source = "\
let
    r = Int8(3) + Int8(3)
    println(typeof(r), \" \", r)
end
let
    r = UInt8(200) + UInt8(200)
    println(typeof(r), \" \", r)
end
";
    let rust_code = compile_to_rust(source)
        .expect("sibling let blocks with same-named distinct-typed locals must compile (#10251)");
    // Each sibling `let` must declare its own `r` with its own concrete type.
    assert!(
        rust_code.matches("let mut r: i8").count() == 1
            && rust_code.matches("let mut r: u8").count() == 1,
        "each sibling `let` must get an independently-typed `r` (i8 then u8), got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("let mut r: Value"),
        "same-named sibling locals must not be unified to a boxed `Value` slot, got:\n{}",
        rust_code
    );
    assert_generated_rust_runs_with_stdout(
        &rust_code,
        "aot_sibling_let_10251",
        "Int8 6\nUInt8 144",
    );
}

#[test]
fn test_aot_base_prelude_sibling_let_same_name_distinct_types_10111() {
    // The CLI path merges the Base prelude before conversion. That prelude path
    // must preserve the same lexical-scope precision as the minimal converter
    // above; otherwise Int8/UInt8 initializers are widened to `Value` slots and
    // the generated Rust fails to compile (`Value: From<i8/u8>`).
    let source = "\
let
    r = Int8(3) + Int8(3)
    println(typeof(r), \" \", r)
end
let
    r = UInt8(200) + UInt8(200)
    println(typeof(r), \" \", r)
end
";
    let rust_code = compile_to_rust_with_base_canonical(source)
        .expect("CLI/Base AoT path must compile sibling let blocks (#10111)");
    assert!(
        rust_code.matches("let mut r: i8").count() == 1
            && rust_code.matches("let mut r: u8").count() == 1,
        "CLI/Base path must keep each sibling `let` local precisely typed, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("let mut r: Value"),
        "CLI/Base path must not widen same-named sibling locals to `Value`, got:\n{}",
        rust_code
    );
    assert_generated_rust_runs_with_stdout(
        &rust_code,
        "aot_base_sibling_let_10111",
        "Int8 6\nUInt8 144",
    );
}

#[test]
fn test_aot_sibling_let_same_name_int_float_10251() {
    // A second concrete pair (Int64 then Float64) to guard the general case,
    // not just the Int8/UInt8 boundary that first surfaced the bug.
    let source = "\
let
    v = 7
    println(typeof(v), \" \", v)
end
let
    v = 2.5
    println(typeof(v), \" \", v)
end
";
    let rust_code = compile_to_rust(source)
        .expect("sibling let blocks (Int64 then Float64) must compile (#10251)");
    assert!(
        rust_code.contains("let mut v: i64") && rust_code.contains("let mut v: f64"),
        "each sibling `let` must get its own concrete slot type (i64 then f64), got:\n{}",
        rust_code
    );
    assert_generated_rust_runs_with_stdout(
        &rust_code,
        "aot_sibling_let_int_float_10251",
        "Int64 7\nFloat64 2.5",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #10537 (codex review of #10528): scope-local join of `StaticType::Any`
// (not only `Union`) must keep a boxed `Value` slot. A later assignment through
// an Any-returning call used to drop the env entry, declare `i64` from the first
// concrete assignment, then fail codegen on the Any store (#6978).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_aot_let_any_return_reassignment_uses_value_slot_10537() {
    // Source-level regression for the #10528 codex finding: before the fix,
    // compile_to_rust failed with "cannot store value of type Any in slot type
    // Int64" (#6978). After the fix, the IR converter emits a boxed Value slot.
    // Assert the generated Rust shape rather than running the binary: the
    // call-site still needs Value-wrapping of concrete args into `g(::Any)`,
    // which is a separate codegen concern from the scope-local env entry.
    let source = "\
function g(x::Any)::Any
    return x
end
let
    x = 1
    x = g(\"s\")
    println(typeof(x), \" \", x)
end
";
    let rust_code = compile_to_rust(source)
        .expect("let local reassigned via Any-returning call must compile (#10537)");
    assert!(
        rust_code.contains("let mut x: Value"),
        "scope-local Any join must box to Value, got:\n{}",
        rust_code
    );
    assert!(
        !rust_code.contains("let mut x: i64"),
        "must not declare a concrete i64 slot when a later Any store is in scope, got:\n{}",
        rust_code
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #10955: AoT codegen-template panic strings (`.pop().expect(...)`,
// `panic!("popfirst! ...")`, the `dynamic_binop`/`dynamic_call`/dispatcher
// `.unwrap()` fallbacks) emitted a raw Rust panic into the GENERATED program
// instead of routing through `subset_julia_vm_runtime::error::aot_throw` —
// the same diverging-error mechanism every other Julia-visible error site in
// this codegen module already uses for BoundsError/KeyError/InexactError/etc.
// `pop!`/`popfirst!` on an empty collection now emit the same
// `ArgumentError: array must be non-empty` message as the VM interpreter
// (`VmError::EmptyArrayPop`) and upstream Julia. This builds and RUNS the
// generated Rust binary (not just `cargo check`) to prove the failure path
// itself — not just successful programs — stays free of the old raw text.
// ─────────────────────────────────────────────────────────────────────────────

/// Compile the generated Rust into a crate and RUN it, asserting the process
/// exits non-zero and stderr contains the expected Julia-shaped error message
/// routed through `aot_throw` (Issue #10955). Returns captured stderr so
/// callers can assert on its full content (e.g. that stale pre-conversion
/// panic text is gone).
fn assert_generated_rust_run_fails_with_stderr(
    rust_code: &str,
    crate_name: &str,
    expected_stderr_substring: &str,
) -> String {
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
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .output()
        .expect("run generated Rust binary");

    assert!(
        !output.status.success(),
        "generated Rust binary must fail on this input but exited successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains(expected_stderr_substring),
        "generated Rust binary stderr must contain the Julia-shaped error {:?} (Issue #10955), got:\n{}",
        expected_stderr_substring,
        stderr
    );
    stderr
}

#[test]
fn test_aot_pop_empty_collection_throws_julia_argument_error_10955() {
    // pop!(arr) on an empty Vector must route through `aot_throw` with an
    // upstream-shaped `ArgumentError: array must be non-empty` message (both
    // upstream `julia` and the sjulia VM interpreter raise this exact
    // ArgumentError for `pop!`/`popfirst!` on an empty collection), not the
    // old raw `.expect("pop! from empty collection")` panic text.
    let source = "\
arr = Int64[]
pop!(arr)
";
    let rust_code = compile_to_rust(source)
        .expect("pop! on an empty array literal must still compile (Issue #10955)");
    assert!(
        !rust_code.contains("pop! from empty collection"),
        "pop! codegen must no longer emit the raw pre-conversion panic text (Issue #10955), got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("subset_julia_vm_runtime::error::aot_throw"),
        "pop! codegen must route the empty-collection failure through aot_throw (Issue #10955), got:\n{}",
        rust_code
    );
    let stderr = assert_generated_rust_run_fails_with_stderr(
        &rust_code,
        "aot_pop_empty_10955",
        "ArgumentError: array must be non-empty",
    );
    assert!(
        !stderr.contains("pop! from empty collection"),
        "runtime failure must surface the Julia-style ArgumentError message, not the old raw panic text, got:\n{}",
        stderr
    );
}

#[test]
fn test_aot_popfirst_empty_collection_throws_julia_argument_error_10955() {
    // Same contract as pop! above, for popfirst!'s separate codegen template
    // (previously a bare `panic!("popfirst! from empty collection")`).
    let source = "\
arr = Int64[]
popfirst!(arr)
";
    let rust_code = compile_to_rust(source)
        .expect("popfirst! on an empty array literal must still compile (Issue #10955)");
    assert!(
        !rust_code.contains("popfirst! from empty collection"),
        "popfirst! codegen must no longer emit the raw pre-conversion panic text (Issue #10955), got:\n{}",
        rust_code
    );
    assert!(
        rust_code.contains("subset_julia_vm_runtime::error::aot_throw"),
        "popfirst! codegen must route the empty-collection failure through aot_throw (Issue #10955), got:\n{}",
        rust_code
    );
    let stderr = assert_generated_rust_run_fails_with_stderr(
        &rust_code,
        "aot_popfirst_empty_10955",
        "ArgumentError: array must be non-empty",
    );
    assert!(
        !stderr.contains("popfirst! from empty collection"),
        "runtime failure must surface the Julia-style ArgumentError message, not the old raw panic text, got:\n{}",
        stderr
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #10131: isless / mixed-type min-max / non-Int64 div-family in AoT.
// Each program's expected stdout is the verbatim upstream `julia` output.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_aot_isless_total_order_10131() {
    // isless follows Julia's canonical total order: `<` for integers, and for
    // floats NaN sorts after everything and -0.0 before 0.0 (the upstream
    // `_fpint` bit-pattern order).
    let source = "\
println(isless(3, 3))
println(isless(2, 3))
println(isless(2.5, 3))
println(isless(NaN, Inf))
println(isless(Inf, NaN))
println(isless(-0.0, 0.0))
println(isless(Int32(1), Int32(2)))
";
    let rust_code =
        compile_to_rust(source).expect("isless must compile in AoT (Issue #10131 gap 1)");
    assert_generated_rust_runs_with_stdout(
        &rust_code,
        "aot_isless_total_order_10131",
        "false\ntrue\ntrue\nfalse\ntrue\ntrue\ntrue",
    );
}

#[test]
fn test_aot_min_max_mixed_type_promotion_10131() {
    // min/max promote mixed numeric operands like upstream `promote`
    // (`min(3, 2.5) == 2.5`), instead of emitting an ill-typed Rust
    // `i64::min(f64)` call (Issue #10131 gap 2).
    let source = "\
println(min(3, 2.5))
println(max(3, 2.5))
println(min(Float32(2.5), 3))
m = min(Int64(3), 2.5)
println(typeof(m))
";
    let rust_code = compile_to_rust(source)
        .expect("mixed-type min/max must compile in AoT (Issue #10131 gap 2)");
    assert_generated_rust_runs_with_stdout(
        &rust_code,
        "aot_min_max_promotion_10131",
        "2.5\n3.0\n2.5\nFloat64",
    );
}

#[test]
fn test_aot_div_family_non_int64_widths_10131() {
    // div/fld/cld/rem/mod work on every integer width: the runtime `Value`
    // has boxing variants for the narrow/unsigned widths, and Value-typed
    // slots receive `Value::from(...)`-wrapped native results
    // (Issue #10131 gap 3). typeof must observe the exact width.
    let source = "\
x = div(Int8(7), Int8(3))
println(typeof(x))
println(x)
y = fld(UInt64(5), UInt64(2))
println(typeof(y))
println(y)
u = rem(UInt16(7), UInt16(3))
println(typeof(u))
println(u)
println(mod(UInt8(7), UInt8(3)))
println(cld(Int32(7), Int32(3)))
println(rem(Int16(7), Int16(3)))
";
    let rust_code = compile_to_rust(source)
        .expect("non-Int64 div-family must compile in AoT (Issue #10131 gap 3)");
    assert_generated_rust_runs_with_stdout(
        &rust_code,
        "aot_div_family_widths_10131",
        "Int8\n2\nUInt64\n2\nUInt16\n1\n1\n3\n1",
    );
}

#[path = "aot_e2e_tests/wasm_rng.rs"]
mod wasm_rng_tests;

mod wasm_backend_tests {
    use std::fs;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};
    use subset_julia_vm::aot::codegen::wasm::emit_module;
    use subset_julia_vm::aot::codegen::CAbiExport;
    use subset_julia_vm::aot::ir::{
        BasicBlock, BinOpKind, ConstValue, Instruction, IrFunction, IrModule, Terminator, VarRef,
    };
    use subset_julia_vm::aot::types::StaticType;
    use subset_julia_vm::aot::{compile_wasm_source, AotBackend, AotError, CompileConfig};

    fn run_wasm_bytes_node(wasm_bytes: &[u8], javascript: &str) -> String {
        let dir = tempfile::tempdir().expect("create Wasm test directory");
        let wasm_path = dir.path().join("module.wasm");
        let script_path = dir.path().join("run.mjs");
        fs::write(&wasm_path, wasm_bytes).expect("write Wasm module");
        fs::write(
            &script_path,
            format!(
                "const bytes = await import('node:fs').then(fs => fs.readFileSync({:?}));\nconst module = await WebAssembly.compile(bytes);\nconst instance = await WebAssembly.instantiate(module, {{}});\n{}",
                wasm_path, javascript
            ),
        )
        .expect("write Node Wasm runner");
        let mut child = Command::new("node")
            .arg(&script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("execute Wasm through Node");
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll Node Wasm runner") {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().expect("terminate hung Node Wasm runner");
                let _ = child.wait();
                panic!("Node Wasm runner exceeded five-second deadline");
            }
            thread::sleep(Duration::from_millis(10));
        };
        let result = child
            .wait_with_output()
            .expect("collect Node Wasm runner output");
        assert!(
            status.success(),
            "Node must validate and execute generated Wasm\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout)
            .expect("Node stdout should be UTF-8")
            .trim()
            .to_string()
    }

    fn compile_and_run_node(
        source: &str,
        function_name: &str,
        arg_types: Vec<StaticType>,
        javascript: &str,
    ) -> String {
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![CAbiExport::with_arg_types(
                function_name,
                function_name,
                arg_types,
            )],
            ..CompileConfig::default()
        };
        let output =
            compile_wasm_source(source, &config).expect("Wasm AoT compilation should succeed");
        run_wasm_bytes_node(&output.wasm_bytes, javascript)
    }

    fn wasm_text(wasm_bytes: &[u8]) -> String {
        let dir = tempfile::tempdir().expect("create Wasm text directory");
        let wasm_path = dir.path().join("module.wasm");
        fs::write(&wasm_path, wasm_bytes).expect("write Wasm text module");
        let output = Command::new("wasm-tools")
            .arg("print")
            .arg(&wasm_path)
            .output()
            .expect("print generated Wasm");
        assert!(
            output.status.success(),
            "wasm-tools print failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("generated WAT should be UTF-8")
    }

    #[test]
    fn wasm_backend_is_an_explicit_aot_backend() {
        // Given: the public AoT backend selector.
        // When: the Wasm backend is selected.
        let backend = AotBackend::Wasm;

        // Then: selection remains explicit rather than falling back to Rust.
        assert_eq!(backend, AotBackend::Wasm);
    }

    #[test]
    fn wasm_comprehensions_restore_shadowed_and_nested_bindings() {
        let source = r#"
function shadowed()::Int64
    i = 40
    xs = [i for i in 1:2]
    return i
end

function nested_same_name()::Int64
    i = 70
    [i for i in 1:2, i in 3:4]
    return i
end
"#;
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: ["shadowed", "nested_same_name"]
                .into_iter()
                .map(|name| CAbiExport::with_arg_types(name, name, Vec::new()))
                .collect(),
            ..CompileConfig::default()
        };
        let output = subset_julia_vm::aot::compile_wasm_source(source, &config)
            .expect("scoped comprehension bindings should compile");
        let shadowed = run_wasm_bytes_node(
            &output.wasm_bytes,
            "console.log(instance.exports.shadowed());",
        );
        assert_eq!(shadowed, "40n");
        let nested = run_wasm_bytes_node(
            &output.wasm_bytes,
            "console.log(instance.exports.nested_same_name());",
        );
        assert_eq!(nested, "70n");
    }

    #[test]
    fn wasm_comprehension_fresh_binding_is_unavailable_afterward() {
        let source = r#"
function fresh()::Int64
    [j for j in 1:2]
    return j
end
"#;
        let error = subset_julia_vm::aot::compile_wasm_source(
            source,
            &CompileConfig {
                backend: AotBackend::Wasm,
                c_abi_exports: vec![CAbiExport::with_arg_types("fresh", "fresh", Vec::new())],
                ..CompileConfig::default()
            },
        )
        .expect_err("a fresh comprehension binding must not escape its scope");
        assert!(
            format!("{error:?}").contains("could not resolve variable `j`"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn wasm_descriptor_v2_reports_generated_module_abi_two() {
        // Given: a generated scalar module with the ABI version export.
        let source = "answer()::Int64 = 42";

        // When: Node reads the generated-module ABI version.
        let value = compile_and_run_node(
            source,
            "answer",
            Vec::new(),
            "console.log(instance.exports.__sjulia_wasm_abi_version());",
        );

        // Then: descriptor ABI v2 is the only accepted generated-module contract.
        assert_eq!(value, "2");
    }

    #[test]
    fn wasm_descriptor_v2_exports_checked_allocation_lifetimes() {
        // Given: any pure generated Wasm module.
        let source = "answer()::Int64 = 42";

        // When: Node exercises allocation, reuse, malformed frees, and descriptor drops.
        let value = compile_and_run_node(
            source,
            "answer",
            Vec::new(),
            r#"
const { memory, __sjulia_alloc: alloc, __sjulia_free: free, __sjulia_drop: drop } = instance.exports;
if (![alloc, free, drop].every(value => typeof value === "function")) throw new Error("missing lifetime exports");
const traps = action => { try { action(); return 0; } catch (error) { return Number(error instanceof WebAssembly.RuntimeError); } };
const invalidAllocations = [alloc(0n, 8), traps(() => alloc(-1n, 8)), traps(() => alloc(1n, 0)), traps(() => alloc(1n, 3)), traps(() => alloc(1n, -8))];
const first = alloc(32n, 16);
let memoryBytes = new Uint8Array(memory.buffer);
memoryBytes[first] = 0x5a;
free(first);
const reused = alloc(16n, 16);
const badFreeTraps = [traps(() => free(reused + 1)), traps(() => free(32)), traps(() => free(memory.buffer.byteLength)), traps(() => free(0))];
free(reused);
const doubleFree = traps(() => free(reused));

const descriptor = alloc(56n, 8);
const data = alloc(4n, 1);
let view = new DataView(memory.buffer);
view.setUint32(descriptor, 2, true);
view.setUint32(descriptor + 4, 1, true);
view.setUint32(descriptor + 8, 1, true);
view.setUint32(descriptor + 12, 1, true);
view.setUint32(descriptor + 16, 0, true);
view.setUint32(descriptor + 20, 1, true);
view.setUint32(descriptor + 24, data, true);
view.setUint32(descriptor + 28, 0, true);
view.setBigUint64(descriptor + 32, 4n, true);
view.setBigUint64(descriptor + 40, 4n, true);
view.setBigInt64(descriptor + 48, 1n, true);
drop(descriptor);
view = new DataView(memory.buffer);
const cleared = view.getUint32(descriptor + 4, true) === 0 && view.getUint32(descriptor + 24, true) === 0;
const doubleDrop = traps(() => drop(descriptor));

const hostDescriptor = 32;
const hostData = 128;
view.setUint32(hostDescriptor, 2, true);
view.setUint32(hostDescriptor + 4, 0, true);
view.setUint32(hostDescriptor + 8, 1, true);
view.setUint32(hostDescriptor + 12, 1, true);
view.setUint32(hostDescriptor + 16, 0, true);
view.setUint32(hostDescriptor + 20, 1, true);
view.setUint32(hostDescriptor + 24, hostData, true);
view.setUint32(hostDescriptor + 28, 0, true);
view.setBigUint64(hostDescriptor + 32, 1n, true);
view.setBigUint64(hostDescriptor + 40, 1n, true);
view.setBigInt64(hostDescriptor + 48, 1n, true);
new Uint8Array(memory.buffer)[hostData] = 77;
drop(hostDescriptor);
const hostPreserved = new Uint8Array(memory.buffer)[hostData] === 77;

const zeroOwnedDescriptor = alloc(56n, 8);
view.setUint32(zeroOwnedDescriptor, 2, true);
view.setUint32(zeroOwnedDescriptor + 4, 1, true);
view.setUint32(zeroOwnedDescriptor + 8, 1, true);
view.setUint32(zeroOwnedDescriptor + 12, 1, true);
view.setUint32(zeroOwnedDescriptor + 16, 0, true);
view.setUint32(zeroOwnedDescriptor + 20, 1, true);
view.setUint32(zeroOwnedDescriptor + 24, 0, true);
view.setUint32(zeroOwnedDescriptor + 28, 0, true);
view.setBigUint64(zeroOwnedDescriptor + 32, 0n, true);
view.setBigUint64(zeroOwnedDescriptor + 40, 0n, true);
view.setBigInt64(zeroOwnedDescriptor + 48, 1n, true);
const zeroDropTrap = traps(() => drop(zeroOwnedDescriptor));
view = new DataView(memory.buffer);
const zeroOwnedCleared = view.getUint32(zeroOwnedDescriptor + 4, true) === 0 && view.getUint32(zeroOwnedDescriptor + 24, true) === 0;
const zeroSecondDrop = traps(() => drop(zeroOwnedDescriptor));

const zeroHostDescriptor = alloc(56n, 8);
view.setUint32(zeroHostDescriptor, 2, true);
view.setUint32(zeroHostDescriptor + 4, 0, true);
view.setUint32(zeroHostDescriptor + 8, 1, true);
view.setUint32(zeroHostDescriptor + 12, 1, true);
view.setUint32(zeroHostDescriptor + 16, 0, true);
view.setUint32(zeroHostDescriptor + 20, 1, true);
view.setUint32(zeroHostDescriptor + 24, hostData, true);
view.setUint32(zeroHostDescriptor + 28, 0, true);
view.setBigUint64(zeroHostDescriptor + 32, 0n, true);
view.setBigUint64(zeroHostDescriptor + 40, 0n, true);
view.setBigInt64(zeroHostDescriptor + 48, 1n, true);
const zeroHostDropTrap = traps(() => drop(zeroHostDescriptor));
const zeroHostPreserved = view.getUint32(zeroHostDescriptor + 4, true) === 0 && view.getUint32(zeroHostDescriptor + 24, true) === hostData;

const malformedZeroPointer = alloc(56n, 8);
view.setUint32(malformedZeroPointer, 2, true);
view.setUint32(malformedZeroPointer + 4, 1, true);
view.setUint32(malformedZeroPointer + 8, 1, true);
view.setUint32(malformedZeroPointer + 12, 1, true);
view.setUint32(malformedZeroPointer + 16, 0, true);
view.setUint32(malformedZeroPointer + 20, 1, true);
view.setUint32(malformedZeroPointer + 24, 127, true);
view.setUint32(malformedZeroPointer + 28, 1, true);
view.setBigUint64(malformedZeroPointer + 32, 0n, true);
view.setBigUint64(malformedZeroPointer + 40, 1n, true);
view.setBigInt64(malformedZeroPointer + 48, 1n, true);
const malformedZeroPointerTrap = traps(() => drop(malformedZeroPointer));

const malformedZeroShape = alloc(56n, 8);
view.setUint32(malformedZeroShape, 2, true);
view.setUint32(malformedZeroShape + 4, 0, true);
view.setUint32(malformedZeroShape + 8, 1, true);
view.setUint32(malformedZeroShape + 12, 1, true);
view.setUint32(malformedZeroShape + 16, 0, true);
view.setUint32(malformedZeroShape + 20, 1, true);
view.setUint32(malformedZeroShape + 24, 0, true);
view.setUint32(malformedZeroShape + 28, 0, true);
view.setBigUint64(malformedZeroShape + 32, 0n, true);
view.setBigUint64(malformedZeroShape + 40, 1n, true);
view.setBigInt64(malformedZeroShape + 48, 1n, true);
const malformedZeroShapeTrap = traps(() => drop(malformedZeroShape));

const beforeGrowth = memory.buffer.byteLength;
const large = alloc(BigInt(beforeGrowth), 8);
memoryBytes = new Uint8Array(memory.buffer);
memoryBytes[large + beforeGrowth - 1] = 0xa5;
const grew = memory.buffer.byteLength > beforeGrowth && memoryBytes[large + beforeGrowth - 1] === 0xa5;
free(large);
const exhaustion = [];
for (;;) {
  const pointer = alloc(1048576n, 8);
  if (pointer === 0) break;
  exhaustion.push(pointer);
}
const oomIsZero = exhaustion.length > 0 && alloc(1048576n, 8) === 0;
console.log(JSON.stringify({ invalidAllocations, aligned: first % 16 === 0, reused: reused === first, badFreeTraps, doubleFree, cleared, doubleDrop, hostPreserved, zeroDropTrap, zeroOwnedCleared, zeroSecondDrop, zeroHostDropTrap, zeroHostPreserved, malformedZeroPointerTrap, malformedZeroShapeTrap, grew, oomIsZero }));
"#,
        );

        // Then: OOM alone returns zero and every ownership violation traps.
        assert_eq!(
            value,
            r#"{"invalidAllocations":[0,1,1,1,1],"aligned":true,"reused":true,"badFreeTraps":[1,1,1,1],"doubleFree":1,"cleared":true,"doubleDrop":1,"hostPreserved":true,"zeroDropTrap":0,"zeroOwnedCleared":true,"zeroSecondDrop":1,"zeroHostDropTrap":0,"zeroHostPreserved":true,"malformedZeroPointerTrap":1,"malformedZeroShapeTrap":1,"grew":true,"oomIsZero":true}"#
        );
    }

    #[test]
    fn wasm_rejects_all_lifetime_helper_name_collisions() {
        // Given: each generated-module lifetime helper name used by Julia source.
        for name in ["__sjulia_alloc", "__sjulia_free", "__sjulia_drop"] {
            let source = format!("{name}()::Int64 = 1");

            // When: the canonical backend validates the generated namespace.
            let error = compile_wasm_source(
                &source,
                &CompileConfig {
                    backend: AotBackend::Wasm,
                    c_abi_exports: vec![CAbiExport::with_arg_types(name, name, Vec::new())],
                    ..CompileConfig::default()
                },
            )
            .expect_err("lifetime helper collisions must be rejected");

            // Then: collision is a typed unsupported diagnostic.
            assert!(matches!(error, AotError::UnsupportedInstruction(_)));
        }
    }

    #[test]
    fn wasm_descriptor_v2_reads_rank_two_uint8_with_inline_metadata() {
        // Given: a rank-2 UInt8 function and non-square column-major host storage.
        let source = "function update_matrix!(bytes::Matrix{UInt8}, row::Int64, column::Int64)::Int64\nbytes[row, column] = UInt8(99)\nreturn Int64(bytes[row, column]) + length(bytes)\nend";

        // When: Node supplies the v2 header followed by inline dimensions and strides.
        let value = compile_and_run_node(
            source,
            "update_matrix!",
            vec![
                StaticType::Array {
                    element: Box::new(StaticType::U8),
                    ndims: Some(2),
                },
                StaticType::I64,
                StaticType::I64,
            ],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const ptr = 128; const input = new Uint8Array(memory.buffer, ptr, 6); input.set([11, 12, 21, 22, 31, 32]); view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 0, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 2, true); view.setUint32(descriptor + 24, ptr, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 6n, true); view.setBigUint64(descriptor + 40, 2n, true); view.setBigInt64(descriptor + 48, 1n, true); view.setBigUint64(descriptor + 56, 3n, true); view.setBigInt64(descriptor + 64, 2n, true); const result = instance.exports[\"update_matrix!\"](descriptor, 2n, 3n); console.log(`${result}:${Array.from(input).join(',')}`);",
        );

        // Then: Julia one-based rank-aware addressing selects column three, row two.
        assert_eq!(value, "105:11,12,21,22,31,99");
    }

    #[test]
    fn wasm_primitive_array_assignment_converts_to_element_type() {
        // Given: Julia assignments whose RHS types differ from the array element types.
        let source = r#"
function write_u8!(value::Vector{UInt8}, input::Int64)::UInt8
    assigned = (value[1] = input)
    return assigned
end
function write_bool!(value::Vector{Bool}, input::Int64)::Bool
    assigned = (value[1] = input)
    return assigned
end
read_bool(value::Vector{Bool})::Bool = value[1]
"#;
        let oracle = Command::new("julia")
            .args([
                "--startup-file=no",
                "-e",
                &format!(
                    "{source}\nu=UInt8[0]; b=Bool[false]; println((write_u8!(u, 255), u[1], write_bool!(b, 1), b[1]))"
                ),
            ])
            .output()
            .expect("run upstream Julia assignment conversion oracle");
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout).trim(),
            "(0xff, 0xff, true, true)"
        );

        // When: generated Wasm stores through host-provided ABI v2 descriptors.
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types(
                    "write_u8!",
                    "write_u8!",
                    vec![
                        StaticType::Array {
                            element: Box::new(StaticType::U8),
                            ndims: Some(1),
                        },
                        StaticType::I64,
                    ],
                ),
                CAbiExport::with_arg_types(
                    "write_bool!",
                    "write_bool!",
                    vec![
                        StaticType::Array {
                            element: Box::new(StaticType::Bool),
                            ndims: Some(1),
                        },
                        StaticType::I64,
                    ],
                ),
                CAbiExport::with_arg_types(
                    "read_bool",
                    "read_bool",
                    vec![StaticType::Array {
                        element: Box::new(StaticType::Bool),
                        ndims: Some(1),
                    }],
                ),
            ],
            ..CompileConfig::default()
        };
        let output = compile_wasm_source(source, &config)
            .expect("primitive assignment conversions should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const { memory } = instance.exports;
const view = new DataView(memory.buffer);
const write = (descriptor, pointer, tag) => {
  view.setUint32(descriptor, 2, true);
  view.setUint32(descriptor + 4, 0, true);
  view.setUint32(descriptor + 8, tag, true);
  view.setUint32(descriptor + 12, 1, true);
  view.setUint32(descriptor + 16, 0, true);
  view.setUint32(descriptor + 20, 1, true);
  view.setUint32(descriptor + 24, pointer, true);
  view.setUint32(descriptor + 28, 0, true);
  view.setBigUint64(descriptor + 32, 1n, true);
  view.setBigUint64(descriptor + 40, 1n, true);
  view.setBigInt64(descriptor + 48, 1n, true);
};
write(32, 160, 1);
write(88, 168, 11);
const u8 = instance.exports["write_u8!"](32, 255n);
const bool = instance.exports["write_bool!"](88, 1n);
const dataBytes = new Uint8Array(memory.buffer);
dataBytes[168] = 255;
const normalizedBool = instance.exports.read_bool(88);
console.log(`${u8}:${dataBytes[160]}:${bool}:${normalizedBool}`);
"#,
        );

        // Then: stores use the static element type and assignments return converted values.
        assert_eq!(value, "255:255:1:1");
    }

    #[test]
    fn wasm_indexes_primitive_arrays_across_supported_ranks_and_strides() {
        // Given: every primitive element type and representative ranks through eight.
        let source = r#"
function u8_scalar()::Array{UInt8,0}
    value = zeros(UInt8)
    value[] = UInt8(0xa5)
    return value
end
function f32_rank3()::Array{Float32,3}
    value = zeros(Float32, 2, 1, 2)
    value[2, 1, 2] = Float32(3.5)
    return value
end
function f64_matrix()::Matrix{Float64}
    value = zeros(Float64, 2, 3)
    value[1, 3] = 6.25
    return value
end
function i32_vector()::Vector{Int32}
    value = zeros(Int32, 3)
    value[3] = Int32(-17)
    return value
end
function i64_rank5()::Array{Int64,5}
    value = zeros(Int64, 1, 2, 1, 2, 1)
    value[1, 2, 1, 2, 1] = Int64(72623859790382856)
    return value
end
function bool_rank8()::Array{Bool,8}
    value = zeros(Bool, 1, 1, 1, 1, 1, 1, 1, 2)
    value[1, 1, 1, 1, 1, 1, 1, 2] = true
    return value
end
u8_read(value::Array{UInt8,0})::UInt8 = value[]
function f32_write!(value::Array{Float32,3}, input::Float32)::Float32
    assigned = (value[2, 1, 2] = input)
    return assigned
end
function f64_write!(value::Matrix{Float64}, input::Float64)::Float64
    assigned = (value[2, 3] = input)
    return assigned
end
function i32_write!(value::Vector{Int32}, input::Int32)::Int32
    assigned = (value[3] = input)
    return assigned
end
function i64_write!(value::Array{Int64,5}, input::Int64)::Int64
    assigned = (value[1, 2, 1, 2, 1] = input)
    return assigned
end
function bool_write!(value::Array{Bool,8}, input::Bool)::Bool
    assigned = (value[1, 1, 1, 1, 1, 1, 1, 2] = input)
    return assigned
end
"#;
        let oracle = Command::new("julia")
            .args([
                "--startup-file=no",
                "-e",
                &format!(
                    "{source}\nprintln((u8_scalar()[], f32_rank3()[2,1,2], f64_matrix()[1,3], i32_vector()[3], i64_rank5()[1,2,1,2,1], bool_rank8()[1,1,1,1,1,1,1,2]))"
                ),
            ])
            .output()
            .expect("run upstream Julia arbitrary-rank indexing oracle");
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout).trim(),
            "(0xa5, 3.5f0, 6.25, -17, 72623859790382856, true)"
        );
        let array = |element, rank| StaticType::Array {
            element: Box::new(element),
            ndims: Some(rank),
        };
        let exports = [
            ("u8_scalar", vec![]),
            ("f32_rank3", vec![]),
            ("f64_matrix", vec![]),
            ("i32_vector", vec![]),
            ("i64_rank5", vec![]),
            ("bool_rank8", vec![]),
            ("u8_read", vec![array(StaticType::U8, 0)]),
            (
                "f32_write!",
                vec![array(StaticType::F32, 3), StaticType::F32],
            ),
            (
                "f64_write!",
                vec![array(StaticType::F64, 2), StaticType::F64],
            ),
            (
                "i32_write!",
                vec![array(StaticType::I32, 1), StaticType::I32],
            ),
            (
                "i64_write!",
                vec![array(StaticType::I64, 5), StaticType::I64],
            ),
            (
                "bool_write!",
                vec![array(StaticType::Bool, 8), StaticType::Bool],
            ),
        ];
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: exports
                .into_iter()
                .map(|(name, args)| CAbiExport::with_arg_types(name, name, args))
                .collect(),
            ..CompileConfig::default()
        };

        // When: Node reads module allocations, grows memory, and mutates strided host views.
        let output = compile_wasm_source(source, &config)
            .expect("arbitrary-rank primitive indexing should compile");
        let javascript = r#"
const e = instance.exports;
const imports = WebAssembly.Module.imports(module).length;
const decode = pointer => {
  const view = new DataView(e.memory.buffer);
  const rank = view.getUint32(pointer + 20, true);
  return {
    pointer,
    data: view.getUint32(pointer + 24, true),
    rank,
    dims: Array.from({ length: rank }, (_, axis) => view.getBigUint64(pointer + 40 + axis * 16, true)),
    strides: Array.from({ length: rank }, (_, axis) => view.getBigInt64(pointer + 48 + axis * 16, true)),
  };
};
const made = [e.u8_scalar(), e.f32_rank3(), e.f64_matrix(), e.i32_vector(), e.i64_rank5(), e.bool_rank8()];
const beforeGrowth = e.memory.buffer;
e.__sjulia_alloc(BigInt(beforeGrowth.byteLength), 8);
const refreshed = made.map(decode);
const moduleBytes = [
  new DataView(e.memory.buffer).getUint8(refreshed[0].data),
  new DataView(e.memory.buffer).getFloat32(refreshed[1].data + 12, true),
  new DataView(e.memory.buffer).getFloat64(refreshed[2].data + 32, true),
  new DataView(e.memory.buffer).getInt32(refreshed[3].data + 8, true),
  new DataView(e.memory.buffer).getBigInt64(refreshed[4].data + 24, true).toString(),
  new DataView(e.memory.buffer).getUint8(refreshed[5].data + 1),
];
let nextDescriptor = 32;
let nextData = 2048;
const host = (tag, bytes, dims, strides) => {
  const descriptor = nextDescriptor;
  const data = nextData;
  nextDescriptor += 176;
  nextData += 256;
  const view = new DataView(e.memory.buffer);
  view.setUint32(descriptor, 2, true);
  view.setUint32(descriptor + 4, 0, true);
  view.setUint32(descriptor + 8, tag, true);
  view.setUint32(descriptor + 12, bytes, true);
  view.setUint32(descriptor + 16, 0, true);
  view.setUint32(descriptor + 20, dims.length, true);
  view.setUint32(descriptor + 24, data, true);
  view.setUint32(descriptor + 28, 0, true);
  view.setBigUint64(descriptor + 32, dims.reduce((product, dim) => product * dim, 1n), true);
  dims.forEach((dim, axis) => {
    view.setBigUint64(descriptor + 40 + axis * 16, dim, true);
    view.setBigInt64(descriptor + 48 + axis * 16, strides[axis], true);
  });
  return { descriptor, data };
};
const f32 = host(9, 4, [2n, 1n, 2n], [1n, 2n, 3n]);
const f64 = host(10, 8, [2n, 3n], [1n, 3n]);
const i32 = host(6, 4, [3n], [2n]);
const i64 = host(8, 8, [1n, 2n, 1n, 2n, 1n], [0n, 1n, 0n, 2n, 0n]);
const bool = host(11, 1, [1n, 1n, 1n, 1n, 1n, 1n, 1n, 2n], [0n, 0n, 0n, 0n, 0n, 0n, 0n, 0n]);
e["f32_write!"](f32.descriptor, 9.5);
e["f64_write!"](f64.descriptor, -12.25);
e["i32_write!"](i32.descriptor, -123);
e["i64_write!"](i64.descriptor, 0x102030405060708n);
e["bool_write!"](bool.descriptor, 1);
const hostBytes = [
  new DataView(e.memory.buffer).getFloat32(f32.data + 16, true),
  new DataView(e.memory.buffer).getFloat64(f64.data + 56, true),
  new DataView(e.memory.buffer).getInt32(i32.data + 16, true),
  new DataView(e.memory.buffer).getBigInt64(i64.data + 24, true).toString(),
  new DataView(e.memory.buffer).getUint8(bool.data),
];
console.log(JSON.stringify({ imports, grew: beforeGrowth.byteLength === 0, moduleBytes, hostBytes, scalar: e.u8_read(made[0]) }));
"#;
        let values = (0..3)
            .map(|_| run_wasm_bytes_node(&output.wasm_bytes, javascript))
            .collect::<Vec<_>>();

        // Then: one-based checked strides select exact bytes for module and host arrays.
        let expected = r#"{"imports":0,"grew":true,"moduleBytes":[165,3.5,6.25,-17,"72623859790382856",1],"hostBytes":[9.5,-12.25,-123,"72623859790382856",1],"scalar":165}"#;
        assert_eq!(values, vec![expected, expected, expected]);
    }

    #[test]
    fn wasm_copies_inclusive_primitive_array_slices() {
        // Given: crop-like and mixed scalar/range indexing over a rank-two array.
        let source = r#"
crop(value::Matrix{Int32})::Matrix{Int32} = value[1:2, 2:3]
row(value::Matrix{Int32})::Vector{Int32} = value[2, 1:3]
empty(value::Matrix{Int32})::Matrix{Int32} = value[2:1, 1:3]
"#;
        let oracle = Command::new("julia")
            .args([
                "--startup-file=no",
                "-e",
                &format!(
                    "{source}\nA=reshape(Int32.(1:6), 2, 3); println((size(crop(A)), vec(crop(A)), size(row(A)), row(A), size(empty(A))))"
                ),
            ])
            .output()
            .expect("run upstream Julia slicing oracle");
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout).trim(),
            "((2, 2), Int32[3, 4, 5, 6], (3,), Int32[2, 4, 6], (0, 3))"
        );
        let matrix = StaticType::Array {
            element: Box::new(StaticType::I32),
            ndims: Some(2),
        };
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: ["crop", "row", "empty"]
                .into_iter()
                .map(|name| CAbiExport::with_arg_types(name, name, vec![matrix.clone()]))
                .collect(),
            ..CompileConfig::default()
        };

        // When: generated Wasm copies each result into module-owned ABI v2 storage.
        let output = compile_wasm_source(source, &config)
            .expect("inclusive primitive array slices should compile");
        let javascript = r#"
const e = instance.exports;
const view = new DataView(e.memory.buffer);
const descriptor = 32;
const data = 160;
view.setUint32(descriptor, 2, true);
view.setUint32(descriptor + 4, 0, true);
view.setUint32(descriptor + 8, 6, true);
view.setUint32(descriptor + 12, 4, true);
view.setUint32(descriptor + 16, 0, true);
view.setUint32(descriptor + 20, 2, true);
view.setUint32(descriptor + 24, data, true);
view.setUint32(descriptor + 28, 0, true);
view.setBigUint64(descriptor + 32, 6n, true);
view.setBigUint64(descriptor + 40, 2n, true);
view.setBigInt64(descriptor + 48, 1n, true);
view.setBigUint64(descriptor + 56, 3n, true);
view.setBigInt64(descriptor + 64, 2n, true);
new Int32Array(e.memory.buffer, data, 6).set([1, 2, 3, 4, 5, 6]);
const decode = pointer => {
  const current = new DataView(e.memory.buffer);
  const rank = current.getUint32(pointer + 20, true);
  const count = Number(current.getBigUint64(pointer + 32, true));
  const start = current.getUint32(pointer + 24, true);
  return {
    flags: current.getUint32(pointer + 4, true),
    rank,
    dims: Array.from({length: rank}, (_, axis) => Number(current.getBigUint64(pointer + 40 + axis * 16, true))),
    strides: Array.from({length: rank}, (_, axis) => Number(current.getBigInt64(pointer + 48 + axis * 16, true))),
    values: Array.from(new Int32Array(e.memory.buffer, start, count)),
  };
};
const results = [e.crop(descriptor), e.row(descriptor), e.empty(descriptor)];
const beforeGrowth = e.memory.buffer;
e.__sjulia_alloc(BigInt(beforeGrowth.byteLength), 8);
const decoded = results.map(decode);
results.forEach(e.__sjulia_drop);
console.log(JSON.stringify({ imports: WebAssembly.Module.imports(module).length, grew: beforeGrowth.byteLength === 0, decoded }));
"#;
        let values = (0..3)
            .map(|_| run_wasm_bytes_node(&output.wasm_bytes, javascript))
            .collect::<Vec<_>>();

        // Then: range axes are preserved, scalar axes drop, and empty ranges stay empty.
        assert_eq!(
            values,
            vec![
                r#"{"imports":0,"grew":true,"decoded":[{"flags":1,"rank":2,"dims":[2,2],"strides":[1,2],"values":[3,4,5,6]},{"flags":1,"rank":1,"dims":[3],"strides":[1],"values":[2,4,6]},{"flags":1,"rank":2,"dims":[0,3],"strides":[1,0],"values":[]}]}"#;
                3
            ]
        );
    }

    #[test]
    fn wasm_assigns_primitive_array_slices_transactionally() {
        // Given: scalar fill, overlapping array sources, exact aliasing, and invalid writes.
        let source = r#"
function fill_slice!(value::Vector{Int32}, input::Int32)::Int32
    value[2:4] = input
    return value[2]
end
function copy_forward!(value::Vector{Int32})::Int32
    value[2:5] = value[1:4]
    return value[4]
end
function copy_backward!(value::Vector{Int32})::Int32
    value[1:4] = value[2:5]
    return value[1]
end
function copy_alias!(value::Vector{Int32})::Int32
    value[1:5] = value[1:5]
    return value[5]
end
function shape_mismatch!(value::Matrix{Int32}, input::Vector{Int32})::Int32
    value[1:2, 1:2] = input
    return value[1,1]
end
function oob!(value::Vector{Int32}, input::Int32)::Int32
    value[0:2] = input
    return value[1]
end
"#;
        let vector = StaticType::Array {
            element: Box::new(StaticType::I32),
            ndims: Some(1),
        };
        let matrix = StaticType::Array {
            element: Box::new(StaticType::I32),
            ndims: Some(2),
        };
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types(
                    "fill_slice!",
                    "fill_slice!",
                    vec![vector.clone(), StaticType::I32],
                ),
                CAbiExport::with_arg_types("copy_forward!", "copy_forward!", vec![vector.clone()]),
                CAbiExport::with_arg_types(
                    "copy_backward!",
                    "copy_backward!",
                    vec![vector.clone()],
                ),
                CAbiExport::with_arg_types("copy_alias!", "copy_alias!", vec![vector.clone()]),
                CAbiExport::with_arg_types(
                    "shape_mismatch!",
                    "shape_mismatch!",
                    vec![matrix, vector.clone()],
                ),
                CAbiExport::with_arg_types("oob!", "oob!", vec![vector, StaticType::I32]),
            ],
            ..CompileConfig::default()
        };

        // When: Node executes valid and trapping assignments over host descriptors.
        let output = compile_wasm_source(source, &config)
            .expect("primitive slice assignment should compile");
        let javascript = r#"
const e = instance.exports;
const write = (descriptor, data, dims, flags = 0) => {
  const view = new DataView(e.memory.buffer);
  view.setUint32(descriptor, 2, true);
  view.setUint32(descriptor + 4, flags, true);
  view.setUint32(descriptor + 8, 6, true);
  view.setUint32(descriptor + 12, 4, true);
  view.setUint32(descriptor + 16, 0, true);
  view.setUint32(descriptor + 20, dims.length, true);
  view.setUint32(descriptor + 24, data, true);
  view.setUint32(descriptor + 28, 0, true);
  view.setBigUint64(descriptor + 32, dims.reduce((a, b) => a * b, 1n), true);
  let stride = 1n;
  dims.forEach((dim, axis) => {
    view.setBigUint64(descriptor + 40 + axis * 16, dim, true);
    view.setBigInt64(descriptor + 48 + axis * 16, stride, true);
    stride *= dim;
  });
};
const values = descriptor => {
  const view = new DataView(e.memory.buffer);
  return Array.from(new Int32Array(e.memory.buffer, view.getUint32(descriptor + 24, true), Number(view.getBigUint64(descriptor + 32, true))));
};
const reset = (descriptor, data) => { write(descriptor, data, [5n]); new Int32Array(e.memory.buffer, data, 5).set([1,2,3,4,5]); };
const traps = action => { try { action(); return 0; } catch (error) { return Number(error instanceof WebAssembly.RuntimeError); } };
reset(32, 1024); e["fill_slice!"](32, 9); const fill = values(32);
reset(32, 1024); e["copy_forward!"](32); const forward = values(32);
reset(32, 1024); e["copy_backward!"](32); const backward = values(32);
reset(32, 1024); e["copy_alias!"](32); const alias = values(32);
reset(32, 1024); write(96, 1100, [5n], 2); new Int32Array(e.memory.buffer, 1100, 5).set([1,2,3,4,5]);
const readonlyTrap = traps(() => e["fill_slice!"](96, 9)); const readonly = values(96);
reset(32, 1024); const oobTrap = traps(() => e["oob!"](32, 9)); const oob = values(32);
write(160, 1200, [2n,2n]); new Int32Array(e.memory.buffer, 1200, 4).set([7,8,9,10]);
write(232, 1300, [3n]); new Int32Array(e.memory.buffer, 1300, 3).set([1,2,3]);
const shapeTrap = traps(() => e["shape_mismatch!"](160, 232)); const shape = values(160);
reset(32, 1024);
for (;;) { if (e.__sjulia_alloc(1048576n, 8) === 0) break; }
for (;;) { if (e.__sjulia_alloc(16n, 8) === 0) break; }
const oomTrap = traps(() => e["copy_forward!"](32)); const oom = values(32);
console.log(JSON.stringify({ imports: WebAssembly.Module.imports(module).length, fill, forward, backward, alias, readonlyTrap, readonly, oobTrap, oob, shapeTrap, shape, oomTrap, oom }));
"#;
        let values = (0..3)
            .map(|_| run_wasm_bytes_node(&output.wasm_bytes, javascript))
            .collect::<Vec<_>>();

        // Then: all failures preserve sentinels and overlap behaves like a temporary copy.
        let expected = r#"{"imports":0,"fill":[1,9,9,9,5],"forward":[1,1,2,3,4],"backward":[2,3,4,5,5],"alias":[1,2,3,4,5],"readonlyTrap":1,"readonly":[1,2,3,4,5],"oobTrap":1,"oob":[1,2,3,4,5],"shapeTrap":1,"shape":[7,8,9,10],"oomTrap":1,"oom":[1,2,3,4,5]}"#;
        assert_eq!(values, vec![expected, expected, expected]);
    }

    #[test]
    fn wasm_allocates_primitive_arrays_and_reports_julia_shapes() {
        // Given: Julia's rank-0 through rank-8 primitive allocation and shape contract.
        let source = r#"
u8_scalar()::Array{UInt8,0} = ones(UInt8)
f32_empty()::Array{Float32,3} = zeros(Float32, 2, 0, 3)
f32_scalar()::Array{Float32,0} = ones(Float32)
f64_matrix()::Matrix{Float64} = ones(Float64, 2, 3)
i32_vector()::Vector{Int32} = zeros(Int32, 5)
i64_rank5()::Array{Int64,5} = ones(Int64, 1, 2, 1, 3, 1)
bool_rank8()::Array{Bool,8} = ones(Bool, 1, 1, 1, 1, 1, 1, 1, 2)
growth_array(n::Int64)::Vector{UInt8} = ones(UInt8, n)
dynamic_matrix(rows::Int64, columns::Int64)::Matrix{Float64} = zeros(Float64, rows, columns)
array_length(value::Array{Int64,5})::Int64 = length(value)
array_ndims(value::Array{Int64,5})::Int64 = ndims(value)
array_axis(value::Array{Int64,5}, axis::Int64)::Int64 = size(value, axis)
array_size(value::Array{Int64,5})::NTuple{5,Int64} = size(value)
"#;
        let oracle = Command::new("julia")
            .args([
                "--startup-file=no",
                "-e",
                &format!(
                    "{source}\nA=i64_rank5(); println((size(u8_scalar()), size(f32_empty()), size(f64_matrix()), size(i32_vector()), size(A), size(bool_rank8()), length(A), ndims(A), size(A, 4), size(A, 8)))"
                ),
            ])
            .output()
            .expect("run upstream Julia array oracle");
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout).trim(),
            "((), (2, 0, 3), (2, 3), (5,), (1, 2, 1, 3, 1), (1, 1, 1, 1, 1, 1, 1, 2), 6, 5, 3, 1)"
        );
        let exports = [
            ("u8_scalar", Vec::new()),
            ("f32_empty", Vec::new()),
            ("f32_scalar", Vec::new()),
            ("f64_matrix", Vec::new()),
            ("i32_vector", Vec::new()),
            ("i64_rank5", Vec::new()),
            ("bool_rank8", Vec::new()),
            ("growth_array", vec![StaticType::I64]),
            ("dynamic_matrix", vec![StaticType::I64, StaticType::I64]),
            (
                "array_length",
                vec![StaticType::Array {
                    element: Box::new(StaticType::I64),
                    ndims: Some(5),
                }],
            ),
            (
                "array_ndims",
                vec![StaticType::Array {
                    element: Box::new(StaticType::I64),
                    ndims: Some(5),
                }],
            ),
            (
                "array_axis",
                vec![
                    StaticType::Array {
                        element: Box::new(StaticType::I64),
                        ndims: Some(5),
                    },
                    StaticType::I64,
                ],
            ),
            (
                "array_size",
                vec![StaticType::Array {
                    element: Box::new(StaticType::I64),
                    ndims: Some(5),
                }],
            ),
        ];
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: exports
                .into_iter()
                .map(|(name, args)| CAbiExport::with_arg_types(name, name, args))
                .collect(),
            ..CompileConfig::default()
        };

        // When: generated Wasm allocates each array and Node decodes ABI v2 directly.
        let outputs = (0..3)
            .map(|_| {
                compile_wasm_source(source, &config)
                    .expect("primitive array allocation and shape queries should compile")
            })
            .collect::<Vec<_>>();
        assert_eq!(outputs[0].wasm_bytes, outputs[1].wasm_bytes);
        assert_eq!(outputs[1].wasm_bytes, outputs[2].wasm_bytes);
        let dir = tempfile::tempdir().expect("create array validation directory");
        let wasm_path = dir.path().join("arrays.wasm");
        fs::write(&wasm_path, &outputs[0].wasm_bytes).expect("write array Wasm");
        let validation = Command::new("wasm-tools")
            .arg("validate")
            .arg(&wasm_path)
            .output()
            .expect("validate array Wasm");
        assert!(
            validation.status.success(),
            "{}",
            String::from_utf8_lossy(&validation.stderr)
        );
        let value = run_wasm_bytes_node(
            &outputs[0].wasm_bytes,
            r#"
const e = instance.exports;
const decode = pointer => {
  const view = new DataView(e.memory.buffer);
  const rank = view.getUint32(pointer + 20, true);
  const dims = Array.from({ length: rank }, (_, axis) => Number(view.getBigUint64(pointer + 40 + axis * 16, true)));
  const strides = Array.from({ length: rank }, (_, axis) => Number(view.getBigInt64(pointer + 48 + axis * 16, true)));
  return { flags: view.getUint32(pointer + 4, true), tag: view.getUint32(pointer + 8, true), bytes: view.getUint32(pointer + 12, true), rank, data: view.getUint32(pointer + 24, true), count: Number(view.getBigUint64(pointer + 32, true)), dims, strides };
};
const arrays = [e.u8_scalar(), e.f32_empty(), e.f64_matrix(), e.i32_vector(), e.i64_rank5(), e.bool_rank8()];
const decoded = arrays.map(decode);
const f32Scalar = e.f32_scalar();
const f32Descriptor = decode(f32Scalar);
const f32Bits = new DataView(e.memory.buffer).getUint32(f32Descriptor.data, true).toString(16).padStart(8, "0");
const rank5 = arrays[4];
const sizeHandle = e.array_size(rank5);
let view = new DataView(e.memory.buffer);
const sizeTuple = Array.from({ length: 5 }, (_, axis) => Number(view.getBigInt64(sizeHandle + 4 + axis * 8, true)));
const initialBuffer = e.memory.buffer;
const grown = e.growth_array(5000000n);
const staleView = initialBuffer.byteLength === 0;
view = new DataView(e.memory.buffer);
const grownDecoded = decode(grown);
const first = new Uint8Array(e.memory.buffer, grownDecoded.data, grownDecoded.count)[0];
const last = new Uint8Array(e.memory.buffer, grownDecoded.data, grownDecoded.count)[grownDecoded.count - 1];
const traps = action => { try { action(); return false; } catch (error) { return error instanceof WebAssembly.RuntimeError; } };
const malformed = [traps(() => e.dynamic_matrix(-1n, 2n)), traps(() => e.dynamic_matrix(2147483648n, 2147483648n)), traps(() => e.array_axis(rank5, 0n)), traps(() => e.growth_array(20000000n))];
e.__sjulia_drop(grown);
const dropped = traps(() => e.__sjulia_drop(grown));
console.log(JSON.stringify({ imports: WebAssembly.Module.imports(module).length, decoded, f32Bits, queries: [Number(e.array_length(rank5)), Number(e.array_ndims(rank5)), Number(e.array_axis(rank5, 4n)), Number(e.array_axis(rank5, 8n))], sizeTuple, growth: [staleView, grownDecoded.count, first, last], malformed, dropped }));
"#,
        );

        // Then: tags, widths, canonical column-major strides, emptiness, and queries agree.
        assert_eq!(
            value,
            r#"{"imports":0,"decoded":[{"flags":1,"tag":1,"bytes":1,"rank":0,"data":4208,"count":1,"dims":[],"strides":[]},{"flags":1,"tag":9,"bytes":4,"rank":3,"data":0,"count":0,"dims":[2,0,3],"strides":[1,2,0]},{"flags":1,"tag":10,"bytes":8,"rank":2,"data":4440,"count":6,"dims":[2,3],"strides":[1,2]},{"flags":1,"tag":6,"bytes":4,"rank":1,"data":4624,"count":5,"dims":[5],"strides":[1]},{"flags":1,"tag":8,"bytes":8,"rank":5,"data":4768,"count":6,"dims":[1,2,1,3,1],"strides":[1,1,2,2,6]},{"flags":1,"tag":11,"bytes":1,"rank":8,"data":5000,"count":2,"dims":[1,1,1,1,1,1,1,2],"strides":[1,1,1,1,1,1,1,1]}],"f32Bits":"3f800000","queries":[6,5,3,1],"sizeTuple":[1,2,1,3,1],"growth":[true,5000000,1,1],"malformed":[true,true,true,true],"dropped":true}"#
        );
    }

    #[test]
    fn wasm_backend_emits_a_standalone_module_from_julia_source() {
        // Given: Julia source lowered through the real parser/lowering pipeline.
        let source = "add_i64(x::Int64, y::Int64) = x + y\nadd_i64(20, 22)";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            ..CompileConfig::default()
        };

        // When: the typed Wasm AoT entry point compiles the lowered program.
        let output =
            compile_wasm_source(source, &config).expect("Wasm AoT compilation should succeed");

        // Then: the result is a standalone core WebAssembly module.
        assert_eq!(&output.wasm_bytes[..4], b"\0asm");
    }

    #[test]
    fn wasm_returns_static_utf8_literal_through_direct_helper() {
        // Given: static Julia literals covering UTF-8, embedded NUL, and an empty value.
        let source = "string_identity(value::String)::String = value\nascii()::String = string_identity(\"hello\")\nempty()::String = \"\"\nnul()::String = \"a\\0b\"\nunicode()::String = \"café 漢字 🐱\"";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: ["ascii", "empty", "nul", "unicode"]
                .into_iter()
                .map(|name| CAbiExport::with_arg_types(name, name, Vec::new()))
                .collect(),
            ..CompileConfig::default()
        };

        // When: Node reads each returned static string view as {ptr, byte_len}.
        let output = compile_wasm_source(source, &config)
            .expect("static UTF-8 literals should compile for generated Wasm");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const memory = instance.exports.memory;
const decoder = new TextDecoder("utf-8", { fatal: true });
const read = name => {
  const descriptor = instance.exports[name]();
  const view = new DataView(memory.buffer);
  const pointer = view.getUint32(descriptor, true);
  const byteLength = view.getUint32(descriptor + 4, true);
  const bytes = Array.from(new Uint8Array(memory.buffer, pointer, byteLength));
  return { text: decoder.decode(Uint8Array.from(bytes)), byteLength, bytes };
};
console.log(JSON.stringify([read("ascii"), read("empty"), read("nul"), read("unicode")]));
"#,
        );

        // Then: lengths are UTF-8 byte lengths, never character counts or C-string lengths.
        assert_eq!(
            value,
            r#"[{"text":"hello","byteLength":5,"bytes":[104,101,108,108,111]},{"text":"","byteLength":0,"bytes":[]},{"text":"a\u0000b","byteLength":3,"bytes":[97,0,98]},{"text":"café 漢字 🐱","byteLength":17,"bytes":[99,97,102,195,169,32,230,188,162,229,173,151,32,240,159,144,177]}]"#
        );
    }

    #[test]
    fn wasm_interns_static_strings_deterministically_before_heap_allocations() {
        // Given: duplicate literals and a distinct multibyte literal.
        let source =
            "first()::String = \"repeat\"\nsecond()::String = \"repeat\"\nthird()::String = \"𓀀\"";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: ["first", "second", "third"]
                .into_iter()
                .map(|name| CAbiExport::with_arg_types(name, name, Vec::new()))
                .collect(),
            ..CompileConfig::default()
        };

        // When: the same source is compiled three times and then grows memory.
        let outputs = (0..3)
            .map(|_| {
                compile_wasm_source(source, &config)
                    .expect("static literals should compile")
                    .wasm_bytes
            })
            .collect::<Vec<_>>();
        let value = run_wasm_bytes_node(
            &outputs[0],
            r#"
const { memory, first, second, third, __sjulia_alloc: alloc } = instance.exports;
const imports = WebAssembly.Module.imports(module).length;
const firstView = first();
const secondView = second();
const thirdView = third();
const view = new DataView(memory.buffer);
const firstPtr = view.getUint32(firstView, true);
const firstLen = view.getUint32(firstView + 4, true);
const thirdPtr = view.getUint32(thirdView, true);
const thirdLen = view.getUint32(thirdView + 4, true);
const firstBytes = Array.from(new Uint8Array(memory.buffer, firstPtr, firstLen));
const thirdBytes = Array.from(new Uint8Array(memory.buffer, thirdPtr, thirdLen));
const allocated = alloc(BigInt(memory.buffer.byteLength), 8);
const survivedGrowth = Array.from(new Uint8Array(memory.buffer, firstPtr, firstLen));
console.log(JSON.stringify({ imports, interned: firstView === secondView, firstBytes, thirdBytes, allocationAfterData: allocated > thirdPtr + thirdLen, survivedGrowth }));
"#,
        );

        // Then: interning and bytes are stable, import-free, and outside allocator storage.
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(outputs[1], outputs[2]);
        assert_eq!(
            value,
            r#"{"imports":0,"interned":true,"firstBytes":[114,101,112,101,97,116],"thirdBytes":[240,147,128,128],"allocationAfterData":true,"survivedGrowth":[114,101,112,101,97,116]}"#
        );
    }

    #[test]
    fn wasm_aggregate_oracle_matches_structural_layout_handles() {
        // Given: immutable tuples and unrelated isbits structs accepted by Julia.
        let source = r#"
struct RGBLike
    r::Float32
    g::Float32
    b::Float32
end
struct Unrelated
    first::Float32
    second::Float32
    third::Float32
end
struct Mixed
    count::Int64
    weight::Float64
end
struct Nested
    color::RGBLike
    mixed::Mixed
end
tuple_helper()::Tuple{Int64,Float64} = (7, 2.5)
tuple_direct()::Tuple{Int64,Float64} = tuple_helper()
tuple_field()::Float64 = tuple_direct()[2]
nested_tuple()::Tuple{Int64,Tuple{Float32,Float64}} = (9, (Float32(1.25), 3.5))
rgb_value()::RGBLike = RGBLike(Float32(0.25), Float32(0.5), Float32(0.75))
unrelated_value()::Unrelated = Unrelated(Float32(1), Float32(2), Float32(3))
mixed_value()::Mixed = Mixed(11, 4.5)
nested_value()::Nested = Nested(rgb_value(), mixed_value())
rgb_green(value::RGBLike)::Float32 = value.g
nested_count(value::Nested)::Int64 = value.mixed.count
"#;
        let oracle = Command::new("julia")
            .args([
                "--startup-file=no",
                "-e",
                &format!(
                    "{source}\nprintln(tuple_direct()); println(tuple_field()); println(nested_tuple()); println(rgb_green(rgb_value())); println(nested_count(nested_value()))"
                ),
            ])
            .output()
            .expect("run upstream Julia aggregate oracle");
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout).trim(),
            "(7, 2.5)\n2.5\n(9, (1.25f0, 3.5))\n0.5\n11"
        );

        // When: the same program is compiled repeatedly and Node decodes only
        // the generated layout table plus each returned handle.
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: [
                "tuple_direct",
                "tuple_field",
                "nested_tuple",
                "rgb_value",
                "unrelated_value",
                "mixed_value",
                "nested_value",
            ]
            .into_iter()
            .map(|name| CAbiExport::with_arg_types(name, name, Vec::new()))
            .chain([
                CAbiExport::with_arg_types(
                    "rgb_green",
                    "rgb_green",
                    vec![StaticType::Struct {
                        type_id: 0,
                        name: "RGBLike".to_string(),
                    }],
                ),
                CAbiExport::with_arg_types(
                    "nested_count",
                    "nested_count",
                    vec![StaticType::Struct {
                        type_id: 0,
                        name: "Nested".to_string(),
                    }],
                ),
            ])
            .collect(),
            ..CompileConfig::default()
        };
        let outputs = (0..3)
            .map(|_| compile_wasm_source(source, &config).expect("aggregate Wasm should compile"))
            .collect::<Vec<_>>();
        assert_eq!(outputs[0].wasm_bytes, outputs[1].wasm_bytes);
        assert_eq!(outputs[1].wasm_bytes, outputs[2].wasm_bytes);
        let value = run_wasm_bytes_node(
            &outputs[0].wasm_bytes,
            r#"
const e = instance.exports;
const view = () => new DataView(e.memory.buffer);
const table = e.__sjulia_layout_table();
const count = e.__sjulia_layout_count();
const layouts = new Map();
let cursor = table;
for (let index = 0; index < count; index += 1) {
  const data = view();
  const id = data.getUint32(cursor, true);
  const size = data.getUint32(cursor + 4, true);
  const align = data.getUint32(cursor + 8, true);
  const fields = data.getUint32(cursor + 12, true);
  const entries = [];
  for (let field = 0; field < fields; field += 1) {
    const offset = cursor + 16 + field * 12;
    entries.push([data.getUint32(offset, true), data.getUint32(offset + 4, true), data.getUint32(offset + 8, true)]);
  }
  layouts.set(id, { size, align, fields: entries });
  cursor += 16 + fields * 12;
}
const decode = handle => {
  const data = view();
  const id = data.getUint32(handle, true);
  const layout = layouts.get(id);
  if (!layout || id === 0) throw new Error("forged aggregate handle");
  return { id, layout, handle };
};
const tuple = decode(e.tuple_direct());
const nestedTuple = decode(e.nested_tuple());
const rgb = decode(e.rgb_value());
const unrelated = decode(e.unrelated_value());
const mixed = decode(e.mixed_value());
const nested = decode(e.nested_value());
const data = view();
const readF32 = (aggregate, field) => data.getFloat32(aggregate.handle + 4 + aggregate.layout.fields[field][0], true);
const readI64 = (aggregate, field) => data.getBigInt64(aggregate.handle + 4 + aggregate.layout.fields[field][0], true).toString();
const tupleValues = [readI64(tuple, 0), data.getFloat64(tuple.handle + 4 + tuple.layout.fields[1][0], true)];
const traps = action => { try { action(); return false; } catch (error) { return error instanceof WebAssembly.RuntimeError; } };
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  count,
  tupleValues,
  tupleField: e.tuple_field(),
  rgb: [readF32(rgb, 0), readF32(rgb, 1), readF32(rgb, 2)],
  green: e.rgb_green(rgb.handle),
  nestedCount: e.nested_count(nested.handle).toString(),
  structuralDedup: rgb.id === unrelated.id,
  distinctMixed: rgb.id !== mixed.id,
  nestedLayouts: nestedTuple.layout.fields.some(field => field[2] !== 0) && nested.layout.fields.every(field => field[2] !== 0),
  forged: traps(() => e.rgb_green(mixed.handle)),
  misaligned: traps(() => e.rgb_green(rgb.handle + 1)),
}));
"#,
        );

        // Then: values, structural IDs, nested field IDs, and forged-handle
        // rejection are observable without type-name or color-name knowledge.
        assert_eq!(
            value,
            r#"{"imports":0,"count":5,"tupleValues":["7",2.5],"tupleField":2.5,"rgb":[0.25,0.5,0.75],"green":0.5,"nestedCount":"11","structuralDedup":true,"distinctMixed":true,"nestedLayouts":true,"forged":true,"misaligned":true}"#
        );
    }

    #[test]
    fn wasm_rejects_unsupported_aggregate_shapes_and_mutation() {
        // Given: mutable, reference-bearing, recursive, and mutation cases.
        let cases = [
            (
                "mutable struct MutableValue\nvalue::Int64\nend\nmake()::MutableValue = MutableValue(1)",
                "make",
                Vec::new(),
                "immutable non-parametric isbits struct",
            ),
            (
                "struct NamedValue\nname::String\nend\nmake()::NamedValue = NamedValue(\"x\")",
                "make",
                Vec::new(),
                "is not isbits",
            ),
            (
                "struct ImmutableValue\nvalue::Int64\nend\nfunction mutate(value::ImmutableValue)::Int64\nvalue.value = 2\nreturn value.value\nend",
                "mutate",
                vec![StaticType::Struct {
                    type_id: 0,
                    name: "ImmutableValue".to_string(),
                }],
                "values are immutable",
            ),
        ];

        for (source, name, arg_types, expected) in cases {
            // When: generated-Wasm compilation validates the aggregate graph.
            let error = compile_wasm_source(
                source,
                &CompileConfig {
                    backend: AotBackend::Wasm,
                    c_abi_exports: vec![CAbiExport::with_arg_types(name, name, arg_types)],
                    ..CompileConfig::default()
                },
            )
            .expect_err("unsupported aggregate must not compile");

            // Then: the typed diagnostic identifies the rejected contract.
            assert!(
                matches!(error, AotError::UnsupportedInstruction(_)),
                "{error}"
            );
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn wasm_rejects_dynamic_string_behavior_with_typed_diagnostics() {
        // Given: dynamic concatenation, interpolation, and mutation requests.
        let cases = [
            (
                "dynamic(value::String)::String = string(value, \"!\")",
                "dynamic",
                vec![StaticType::Str],
                "dynamic string concatenation or interpolation",
            ),
            (
                "interpolate(value::Int64)::String = \"value = $value\"",
                "interpolate",
                vec![StaticType::I64],
                "dynamic string concatenation or interpolation",
            ),
            (
                "function mutate(value::String)::String\nvalue[1] = 'x'\nreturn value\nend",
                "mutate",
                vec![StaticType::Str],
                "string literals are immutable",
            ),
        ];

        for (source, name, arg_types, diagnostic) in cases {
            // When: generated-Wasm lowering reaches unsupported dynamic behavior.
            let error = compile_wasm_source(
                source,
                &CompileConfig {
                    backend: AotBackend::Wasm,
                    c_abi_exports: vec![CAbiExport::with_arg_types(name, name, arg_types)],
                    ..CompileConfig::default()
                },
            )
            .expect_err("dynamic string behavior must not compile");

            // Then: a typed diagnostic names the unsupported behavior exactly.
            assert!(matches!(error, AotError::UnsupportedInstruction(_)));
            assert!(error.to_string().contains(diagnostic), "{error}");
        }
    }

    #[test]
    fn wasm_executes_integer_arithmetic_from_julia_source() {
        // Given: an independently compiled integer function.
        let source = "add_scale(x::Int64, y::Int64) = (x + y) * 2";

        // When: Node validates, instantiates, and calls the generated module.
        let value = compile_and_run_node(
            source,
            "add_scale",
            vec![StaticType::I64, StaticType::I64],
            "const f = Object.values(instance.exports).find(v => typeof v === 'function' && v.length === 2); console.log(f(10n, 11n).toString());",
        );

        // Then: Wasm matches Julia's Int64 result.
        assert_eq!(value, "42");
    }

    #[test]
    fn wasm_uint8_subtraction_wraps_before_unsigned_comparison() {
        // Given: subtraction that wraps from zero to UInt8(255).
        let source = "function wrapped_sub()::Bool\nx = UInt8(0) - UInt8(1)\nreturn x > 0x64\nend";

        // When: Node executes the exported function.
        let value = compile_and_run_node(
            source,
            "wrapped_sub",
            Vec::new(),
            "console.log(Number(instance.exports.wrapped_sub()));",
        );

        // Then: the wrapped byte compares as unsigned.
        assert_eq!(value, "1");
    }

    #[test]
    fn wasm_uint8_addition_wraps_before_comparison() {
        // Given: addition that wraps UInt8(250) + UInt8(10) to UInt8(4).
        let source =
            "function wrapped_add()::Bool\nx = UInt8(250) + UInt8(10)\nreturn x > UInt8(100)\nend";

        // When: Node executes the exported function.
        let value = compile_and_run_node(
            source,
            "wrapped_add",
            Vec::new(),
            "console.log(Number(instance.exports.wrapped_add()));",
        );

        // Then: comparison observes the normalized byte.
        assert_eq!(value, "0");
    }

    #[test]
    fn wasm_uint8_widens_to_int64_without_sign_extension() {
        // Given: a wrapped UInt8 value whose high bit is set.
        let source = "widen_wrapped()::Int64 = Int64(UInt8(0) - UInt8(1))";

        // When: Node executes the exported function.
        let value = compile_and_run_node(
            source,
            "widen_wrapped",
            Vec::new(),
            "console.log(instance.exports.widen_wrapped().toString());",
        );

        // Then: UInt8 widens as 255 rather than -1.
        assert_eq!(value, "255");
    }

    #[test]
    fn wasm_implicit_trailing_value_returns_without_hanging() {
        // Given: a typed function whose final statement is a ValueCarrier.
        let source = "function h(x::Int64)::Int64\ny = x * 2\ny\nend";

        // When: the backend reaches the currently unsupported trailing carrier.
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![CAbiExport::with_arg_types("h", "h", vec![StaticType::I64])],
            ..CompileConfig::default()
        };
        let error = compile_wasm_source(source, &config)
            .expect_err("ambiguous trailing carriers must be rejected instead of looping");

        // Then: compilation fails loudly and cannot emit a non-terminating module.
        assert!(matches!(error, AotError::UnsupportedInstruction(_)));
        assert!(error.to_string().contains("no unambiguous return value"));
    }

    #[test]
    fn wasm_rejects_duplicate_and_reserved_function_identities() {
        // Given: overloads that collapse to one Wasm symbol and reserved ABI names.
        let cases = [
            (
                "same(x::Int64)::Int64 = x\nsame(x::Float64)::Float64 = x",
                "same",
                vec![StaticType::I64],
            ),
            ("memory()::Int64 = 1", "memory", Vec::new()),
            (
                "__sjulia_wasm_abi_version()::Int64 = 1",
                "__sjulia_wasm_abi_version",
                Vec::new(),
            ),
        ];

        for (source, name, arg_types) in cases {
            // When: the canonical Wasm pipeline validates function identities.
            let error = compile_wasm_source(
                source,
                &CompileConfig {
                    backend: AotBackend::Wasm,
                    c_abi_exports: vec![CAbiExport::with_arg_types(name, name, arg_types)],
                    ..CompileConfig::default()
                },
            )
            .expect_err("duplicate or reserved Wasm identities must be rejected");

            // Then: callers receive a typed unsupported diagnostic, not invalid bytes.
            assert!(
                matches!(error, AotError::UnsupportedInstruction(_)),
                "`{name}` produced unexpected diagnostic: {error}"
            );
        }
    }

    #[test]
    fn wasm_honors_requested_export_alias_without_original_export() {
        // Given: an explicit alias that differs from the Julia function name.
        let source = "internal_add(x::Int64, y::Int64)::Int64 = x + y";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![CAbiExport::with_arg_types(
                "public_add",
                "internal_add",
                vec![StaticType::I64, StaticType::I64],
            )],
            ..CompileConfig::default()
        };

        // When: the module is compiled and inspected through Node.
        let output = compile_wasm_source(source, &config).expect("alias should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            "console.log(`${instance.exports.public_add(20n, 22n)}:${Object.hasOwn(instance.exports, 'internal_add')}`);",
        );

        // Then: only the requested public alias is required.
        assert_eq!(value, "42:false");
    }

    #[test]
    fn wasm_executes_float_comparison_and_conditional_from_julia_source() {
        // Given: Float64 comparison and conditional control flow.
        let source = "function choose(x::Float64, y::Float64)::Float64\nif x > y\nreturn x - y\nelse\nreturn y - x\nend\nend";

        // When: the compiled function runs in Node.
        let value = compile_and_run_node(
            source,
            "choose",
            vec![StaticType::F64, StaticType::F64],
            "const f = Object.values(instance.exports).find(v => typeof v === 'function' && v.length === 2); console.log(f(1.25, 6.75));",
        );

        // Then: branch selection and Float64 arithmetic agree with Julia.
        assert_eq!(value, "5.5");
    }

    #[test]
    fn wasm_preserves_float32_constant_return_and_direct_call() {
        // Given: Float32 constants flowing through a direct helper call.
        let source = "f32_twice(x::Float32)::Float32 = x + x\nf32_constant()::Float32 = f32_twice(Float32(-0.0))";

        // When: Node observes the exported scalar through its exact f32 bits.
        let value = compile_and_run_node(
            source,
            "f32_constant",
            Vec::new(),
            "const bits = new Uint32Array(new Float32Array([instance.exports.f32_constant()]).buffer)[0]; console.log(bits.toString(16).padStart(8, '0'));",
        );

        // Then: Float32 is neither widened nor stripped of its signed zero.
        assert_eq!(value, "80000000");
    }

    #[test]
    fn wasm_float32_native_operations_match_boundary_matrix() {
        // Given: Float32 arithmetic, unary negation, ordered comparisons, and overflow.
        let source = "function f32_ops(x::Float32, y::Float32)::Float32\nif x != x\nreturn x\nelseif x < y\nreturn -(x * y)\nelse\nreturn x / y\nend\nend";

        // When: Node drives finite values, signed zero, infinities, and distinct NaNs.
        let value = compile_and_run_node(
            source,
            "f32_ops",
            vec![StaticType::F32, StaticType::F32],
            r#"
const f = instance.exports.f32_ops;
const fromBits = bits => new Float32Array(new Uint32Array([bits]).buffer)[0];
const bits = value => new Uint32Array(new Float32Array([value]).buffer)[0].toString(16).padStart(8, "0");
const values = [
  bits(f(2, 0.5)),
  bits(f(-0, -0)),
  bits(f(3.4028234663852886e38, 0.5)),
  bits(f(fromBits(1), 2)),
  Number.isNaN(f(fromBits(0x7fc00001), 1)),
  Number.isNaN(f(fromBits(0xffc12345), 1)),
  f(-Infinity, Infinity) === Infinity,
];
console.log(JSON.stringify(values));
"#,
        );

        // Then: native Wasm f32 behavior agrees with Julia's Float32 contract.
        assert_eq!(
            value,
            r#"["40800000","7fc00000","7f800000","80000002",true,true,true]"#
        );
    }

    #[test]
    fn wasm_float32_conversions_round_and_reject_inexact_inputs() {
        // Given: conversions already represented by AoT Convert nodes.
        let source = "to_f32(x::Float64)::Float32 = Float32(x)\nto_f64(x::Float32)::Float64 = Float64(x)\nto_i32(x::Float32)::Int32 = Int32(x)\nto_i64(x::Float32)::Int64 = Int64(x)\nto_u8(x::Float32)::UInt8 = UInt8(x)\nto_bool(x::Float32)::Bool = Bool(x)\nfrom_i32(x::Int32)::Float32 = Float32(x)\nfrom_i64(x::Int64)::Float32 = Float32(x)\nfrom_u8(x::UInt8)::Float32 = Float32(x)\nfrom_bool(x::Bool)::Float32 = Float32(x)";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: [
                ("to_f32", vec![StaticType::F64]),
                ("to_f64", vec![StaticType::F32]),
                ("to_i32", vec![StaticType::F32]),
                ("to_i64", vec![StaticType::F32]),
                ("to_u8", vec![StaticType::F32]),
                ("to_bool", vec![StaticType::F32]),
                ("from_i32", vec![StaticType::I32]),
                ("from_i64", vec![StaticType::I64]),
                ("from_u8", vec![StaticType::U8]),
                ("from_bool", vec![StaticType::Bool]),
            ]
            .into_iter()
            .map(|(name, args)| CAbiExport::with_arg_types(name, name, args))
            .collect(),
            ..CompileConfig::default()
        };

        // When: Node probes exact rounding and malformed/inexact conversion inputs.
        let output = compile_wasm_source(source, &config).expect("F32 conversions should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const bits = value => new Uint32Array(new Float32Array([value]).buffer)[0].toString(16).padStart(8, "0");
const traps = action => { try { action(); return false; } catch (error) { return error instanceof WebAssembly.RuntimeError; } };
const values = {
  tie: bits(e.to_f32(1 + 2 ** -24)),
  aboveTie: bits(e.to_f32(1 + 3 * 2 ** -24)),
  promoted: e.to_f64(new Float32Array([1.0000001192092896])[0]),
  i32: e.to_i32(2147483520),
  i64: e.to_i64(2 ** 62).toString(),
  u8: e.to_u8(255),
  bools: [e.to_bool(-0), e.to_bool(1)],
  fromInts: [bits(e.from_i32(16777217)), bits(e.from_i64(9223372036854775807n)), bits(e.from_u8(255)), bits(e.from_bool(1))],
  traps: [traps(() => e.to_i32(1.5)), traps(() => e.to_i32(NaN)), traps(() => e.to_i32(Infinity)), traps(() => e.to_i32(2147483648)), traps(() => e.to_i64(2 ** 63)), traps(() => e.to_u8(-1)), traps(() => e.to_u8(256)), traps(() => e.to_bool(2))],
};
console.log(JSON.stringify(values));
"#,
        );

        // Then: representable values match Julia and every inexact case traps.
        assert_eq!(
            value,
            r#"{"tie":"3f800000","aboveTie":"3f800002","promoted":1.0000001192092896,"i32":2147483520,"i64":"4611686018427387904","u8":255,"bools":[0,1],"fromInts":["4b800000","5f000000","437f0000","3f800000"],"traps":[true,true,true,true,true,true,true,true]}"#
        );
    }

    #[test]
    fn wasm_scalar_math_and_float_predicates_match_julia_boundaries() {
        // Given: structurally recognized scalar math builtins over both float widths.
        let source = "f64_abs(x::Float64)::Float64 = abs(x)\nf64_floor(x::Float64)::Float64 = floor(x)\nf64_ceil(x::Float64)::Float64 = ceil(x)\nf64_trunc(x::Float64)::Float64 = trunc(x)\nf64_round(x::Float64)::Float64 = round(x)\nf64_sqrt(x::Float64)::Float64 = sqrt(x)\nf64_min(x::Float64, y::Float64)::Float64 = min(x, y)\nf64_max(x::Float64, y::Float64)::Float64 = max(x, y)\nf64_clamp(x::Float64, lo::Float64, hi::Float64)::Float64 = clamp(x, lo, hi)\nf64_isnan(x::Float64)::Bool = isnan(x)\nf64_isinf(x::Float64)::Bool = isinf(x)\nf64_isfinite(x::Float64)::Bool = isfinite(x)\nf32_abs(x::Float32)::Float32 = abs(x)\nf32_floor(x::Float32)::Float32 = floor(x)\nf32_ceil(x::Float32)::Float32 = ceil(x)\nf32_trunc(x::Float32)::Float32 = trunc(x)\nf32_round(x::Float32)::Float32 = round(x)\nf32_sqrt(x::Float32)::Float32 = sqrt(x)\nf32_min(x::Float32, y::Float32)::Float32 = min(x, y)\nf32_max(x::Float32, y::Float32)::Float32 = max(x, y)\nf32_clamp(x::Float32, lo::Float32, hi::Float32)::Float32 = clamp(x, lo, hi)\nf32_isnan(x::Float32)::Bool = isnan(x)\nf32_isinf(x::Float32)::Bool = isinf(x)\nf32_isfinite(x::Float32)::Bool = isfinite(x)";
        let mut exports = Vec::new();
        for name in [
            "abs", "floor", "ceil", "trunc", "round", "sqrt", "isnan", "isinf", "isfinite",
        ] {
            exports.push(CAbiExport::with_arg_types(
                format!("f64_{name}"),
                format!("f64_{name}"),
                vec![StaticType::F64],
            ));
            exports.push(CAbiExport::with_arg_types(
                format!("f32_{name}"),
                format!("f32_{name}"),
                vec![StaticType::F32],
            ));
        }
        for name in ["min", "max"] {
            exports.push(CAbiExport::with_arg_types(
                format!("f64_{name}"),
                format!("f64_{name}"),
                vec![StaticType::F64, StaticType::F64],
            ));
            exports.push(CAbiExport::with_arg_types(
                format!("f32_{name}"),
                format!("f32_{name}"),
                vec![StaticType::F32, StaticType::F32],
            ));
        }
        exports.push(CAbiExport::with_arg_types(
            "f64_clamp",
            "f64_clamp",
            vec![StaticType::F64; 3],
        ));
        exports.push(CAbiExport::with_arg_types(
            "f32_clamp",
            "f32_clamp",
            vec![StaticType::F32; 3],
        ));

        // When: Node executes Julia-oracle edge cases and reports exact IEEE bits.
        let output = compile_wasm_source(
            source,
            &CompileConfig {
                backend: AotBackend::Wasm,
                c_abi_exports: exports,
                ..CompileConfig::default()
            },
        )
        .expect("scalar math builtins should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const b64 = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString(16).padStart(16, "0");
const b32 = x => new Uint32Array(new Float32Array([x]).buffer)[0].toString(16).padStart(8, "0");
const sub64 = Number.MIN_VALUE;
const sub32 = new Float32Array(new Uint32Array([1]).buffer)[0];
const values = {
  f64: [b64(e.f64_abs(-0)), b64(e.f64_floor(-0)), b64(e.f64_ceil(sub64)), b64(e.f64_trunc(-2.75)), b64(e.f64_round(2.5)), b64(e.f64_round(3.5)), b64(e.f64_sqrt(-0)), b64(e.f64_sqrt(sub64)), b64(e.f64_min(-0, 0)), b64(e.f64_max(0, -0)), Number.isNaN(e.f64_min(NaN, 1)), Number.isNaN(e.f64_max(1, NaN)), b64(e.f64_clamp(-0, 0, 1)), e.f64_isnan(NaN), e.f64_isinf(Infinity), e.f64_isfinite(Number.MAX_VALUE)],
  f32: [b32(e.f32_abs(-0)), b32(e.f32_floor(-0)), b32(e.f32_ceil(sub32)), b32(e.f32_trunc(-2.75)), b32(e.f32_round(2.5)), b32(e.f32_round(3.5)), b32(e.f32_sqrt(-0)), b32(e.f32_sqrt(sub32)), b32(e.f32_min(-0, 0)), b32(e.f32_max(0, -0)), Number.isNaN(e.f32_min(NaN, 1)), Number.isNaN(e.f32_max(1, NaN)), b32(e.f32_clamp(-0, 0, 1)), e.f32_isnan(NaN), e.f32_isinf(Infinity), e.f32_isfinite(3.4028234663852886e38)],
};
console.log(JSON.stringify(values));
"#,
        );

        // Then: native Wasm instructions match Julia 1.12.4, including ties-to-even and signed zero.
        assert_eq!(
            value,
            r#"{"f64":["0000000000000000","8000000000000000","3ff0000000000000","c000000000000000","4000000000000000","4010000000000000","8000000000000000","1e60000000000000","8000000000000000","0000000000000000",true,true,"8000000000000000",1,1,1],"f32":["00000000","80000000","3f800000","c0000000","40000000","40800000","80000000","1a3504f3","80000000","00000000",true,true,"80000000",1,1,1]}"#
        );
    }

    #[test]
    fn wasm_import_free_pow_matches_julia_scalar_matrix() {
        // Given: direct and composed powers for both floating widths.
        let source = "f64_pow(x::Float64, y::Float64)::Float64 = x ^ y\nf64_gamma(x::Float64)::Float64 = clamp(x, 0.0, 1.0) ^ 2.2\nf32_pow(x::Float32, y::Float32)::Float32 = x ^ y";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types("f64_pow", "f64_pow", vec![StaticType::F64; 2]),
                CAbiExport::with_arg_types("f64_gamma", "f64_gamma", vec![StaticType::F64]),
                CAbiExport::with_arg_types("f32_pow", "f32_pow", vec![StaticType::F32; 2]),
            ],
            ..CompileConfig::default()
        };

        // When: the generated module is inspected and executed over representative domains.
        let output = compile_wasm_source(source, &config).expect("pow should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const close = (actual, expected, tolerance) => Math.abs(actual - expected) <= tolerance * Math.max(1, Math.abs(expected));
const traps = action => { try { action(); return false; } catch (error) { return error instanceof WebAssembly.RuntimeError; } };
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  f64: [close(e.f64_pow(2, 10), 1024, 1e-12), close(e.f64_pow(9, 0.5), 3, 1e-12), close(e.f64_pow(2, -3), 0.125, 1e-12), close(e.f64_pow(-2, 3), -8, 1e-12), close(e.f64_gamma(0.5), 0.217637640824031, 1e-12), e.f64_pow(0, 0) === 1, traps(() => e.f64_pow(-2, 0.5))],
  f32: [close(e.f32_pow(2, 10), 1024, 1e-5), close(e.f32_pow(9, 0.5), 3, 1e-5), close(e.f32_pow(2, -3), 0.125, 1e-5), close(e.f32_pow(-2, 3), -8, 1e-5), traps(() => e.f32_pow(-2, 0.5))],
}));
"#,
        );

        // Then: helpers remain import-free and values satisfy the explicit tolerance manifest.
        assert_eq!(
            value,
            r#"{"imports":0,"f64":[true,true,true,true,true,true,true],"f32":[true,true,true,true,true]}"#
        );
    }

    #[test]
    fn wasm_pow_special_values_match_julia() {
        // Given: Julia 1.12.4 power identities over zero, infinities, and NaN.
        let source = "f64_pow_edge(x::Float64, y::Float64)::Float64 = x ^ y\nf32_pow_edge(x::Float32, y::Float32)::Float32 = x ^ y\nf64_pow_repeat(x::Float64)::Float64 = (x ^ 0.5) + (x ^ 2.0) + (x ^ -1.0)";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types(
                    "f64_pow_edge",
                    "f64_pow_edge",
                    vec![StaticType::F64; 2],
                ),
                CAbiExport::with_arg_types(
                    "f32_pow_edge",
                    "f32_pow_edge",
                    vec![StaticType::F32; 2],
                ),
                CAbiExport::with_arg_types(
                    "f64_pow_repeat",
                    "f64_pow_repeat",
                    vec![StaticType::F64],
                ),
            ],
            ..CompileConfig::default()
        };

        // When: Node records exact result bits and typed domain traps.
        let output = compile_wasm_source(source, &config).expect("pow edges should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const b64 = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString(16).padStart(16, "0");
const b32 = x => new Uint32Array(new Float32Array([x]).buffer)[0].toString(16).padStart(8, "0");
const traps = action => { try { action(); return false; } catch (error) { return error instanceof WebAssembly.RuntimeError; } };
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  f64: [[0,3],[0,0.5],[0,-3],[0,0],[-0,3],[-0,2],[-0,-3],[-0,-2],[Infinity,3],[Infinity,-3],[-Infinity,3],[-Infinity,2],[-Infinity,-3],[-Infinity,-2],[NaN,3],[NaN,0]].map(([x,y]) => b64(e.f64_pow_edge(x,y))),
  f32: [[0,0.5],[0,-3],[-0,3],[-0,-3],[Infinity,-2],[-Infinity,3],[NaN,2],[NaN,0]].map(([x,y]) => b32(e.f32_pow_edge(x,y))),
  traps: [traps(() => e.f64_pow_edge(-2,0.5)), traps(() => e.f64_pow_edge(-Infinity,0.5)), traps(() => e.f32_pow_edge(-Infinity,0.5))],
  repeat: Math.abs(e.f64_pow_repeat(4) - 18.25) <= 1e-12,
}));
"#,
        );

        // Then: identities preserve Julia signed-zero/infinity bits and scratch isolation.
        assert_eq!(
            value,
            r#"{"imports":0,"f64":["0000000000000000","0000000000000000","7ff0000000000000","3ff0000000000000","8000000000000000","0000000000000000","fff0000000000000","7ff0000000000000","7ff0000000000000","0000000000000000","fff0000000000000","7ff0000000000000","0000000000000000","0000000000000000","7ff8000000000000","3ff0000000000000"],"f32":["00000000","7f800000","80000000","ff800000","00000000","ff800000","7fc00000","3f800000"],"traps":[true,true,true],"repeat":true}"#
        );
    }

    #[test]
    fn wasm_pow_infinite_exponents_and_extremes_match_julia() {
        // Given: Julia 1.12.4 identities for NaN, infinite exponents, and extreme finite bases.
        let source = "f64_pow_final(x::Float64, y::Float64)::Float64 = x ^ y\nf32_pow_final(x::Float32, y::Float32)::Float32 = x ^ y\nf64_pow_nested(x::Float64)::Float64 = ((x ^ 2.0) ^ -1.0) + (x ^ 3.0)";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types(
                    "f64_pow_final",
                    "f64_pow_final",
                    vec![StaticType::F64; 2],
                ),
                CAbiExport::with_arg_types(
                    "f32_pow_final",
                    "f32_pow_final",
                    vec![StaticType::F32; 2],
                ),
                CAbiExport::with_arg_types(
                    "f64_pow_nested",
                    "f64_pow_nested",
                    vec![StaticType::F64],
                ),
            ],
            ..CompileConfig::default()
        };

        // When: Node records exact classifications and repeated-call scratch behavior.
        let output = compile_wasm_source(source, &config).expect("final pow edges should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const b64 = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString(16).padStart(16, "0");
const b32 = x => new Uint32Array(new Float32Array([x]).buffer)[0].toString(16).padStart(8, "0");
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  f64: [[1,NaN],[NaN,0],[NaN,2],[2,Infinity],[2,-Infinity],[0.5,Infinity],[0.5,-Infinity],[1,Infinity],[1,-Infinity],[-2,Infinity],[-2,-Infinity],[-0.5,Infinity],[-0.5,-Infinity],[Number.MIN_VALUE,2],[Number.MIN_VALUE,-2],[Number.MAX_VALUE,2],[Number.MAX_VALUE,-2]].map(([x,y]) => b64(e.f64_pow_final(x,y))),
  f32: [[1,NaN],[NaN,0],[2,Infinity],[2,-Infinity],[0.5,Infinity],[0.5,-Infinity],[-2,Infinity],[-0.5,-Infinity],[1.401298464324817e-45,2],[3.4028234663852886e38,2]].map(([x,y]) => b32(e.f32_pow_final(x,y))),
  nested: Math.abs(e.f64_pow_nested(2) - 8.25) <= 1e-12,
}));
"#,
        );

        // Then: exponent classification and bounded exp preserve Julia results without imports.
        assert_eq!(
            value,
            r#"{"imports":0,"f64":["3ff0000000000000","3ff0000000000000","7ff8000000000000","7ff0000000000000","0000000000000000","0000000000000000","7ff0000000000000","3ff0000000000000","3ff0000000000000","7ff0000000000000","0000000000000000","0000000000000000","7ff0000000000000","0000000000000000","7ff0000000000000","7ff0000000000000","0000000000000000"],"f32":["3f800000","3f800000","7f800000","00000000","00000000","7f800000","7f800000","7f800000","00000000","7f800000"],"nested":true}"#
        );
    }

    #[test]
    fn wasm_negative_one_infinite_power_matches_julia() {
        // Given: Julia 1.12.4's unit-magnitude precedence over infinite exponents.
        let source = "f64_neg_one_pow(y::Float64)::Float64 = (-1.0) ^ y\nf32_neg_one_pow(y::Float32)::Float32 = Float32(-1.0) ^ y\nf64_one_pow(y::Float64)::Float64 = 1.0 ^ y\nf64_nan_pow(y::Float64)::Float64 = NaN ^ y";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types(
                    "f64_neg_one_pow",
                    "f64_neg_one_pow",
                    vec![StaticType::F64],
                ),
                CAbiExport::with_arg_types(
                    "f32_neg_one_pow",
                    "f32_neg_one_pow",
                    vec![StaticType::F32],
                ),
                CAbiExport::with_arg_types("f64_one_pow", "f64_one_pow", vec![StaticType::F64]),
                CAbiExport::with_arg_types("f64_nan_pow", "f64_nan_pow", vec![StaticType::F64]),
            ],
            ..CompileConfig::default()
        };

        // When: Node records exact special and finite parity bits.
        let output =
            compile_wasm_source(source, &config).expect("negative-one powers should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const b64 = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString(16).padStart(16, "0");
const b32 = x => new Uint32Array(new Float32Array([x]).buffer)[0].toString(16).padStart(8, "0");
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  neg64: [Infinity,-Infinity,NaN,0,3,2].map(x => b64(e.f64_neg_one_pow(x))),
  neg32: [Infinity,-Infinity,NaN,0,3,2].map(x => b32(e.f32_neg_one_pow(x))),
  precedence: [b64(e.f64_one_pow(NaN)),b64(e.f64_nan_pow(0)),b64(e.f64_nan_pow(2))],
}));
"#,
        );

        // Then: only ±infinite exponents use abs(base)==1; NaN and finite rules remain intact.
        assert_eq!(
            value,
            r#"{"imports":0,"neg64":["3ff0000000000000","3ff0000000000000","7ff8000000000000","3ff0000000000000","bff0000000000000","3ff0000000000000"],"neg32":["3f800000","3f800000","7fc00000","3f800000","bf800000","3f800000"],"precedence":["3ff0000000000000","3ff0000000000000","7ff8000000000000"]}"#
        );
    }

    #[test]
    fn wasm_import_free_exp_log_match_julia_scalar_matrix() {
        // Given: direct and composed exp/log functions for both floating widths.
        let source = "f64_exp(x::Float64)::Float64 = exp(x)\nf64_log(x::Float64)::Float64 = log(x)\nf64_roundtrip(x::Float64)::Float64 = exp(log(x))\nf32_exp(x::Float32)::Float32 = exp(x)\nf32_log(x::Float32)::Float32 = log(x)";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types("f64_exp", "f64_exp", vec![StaticType::F64]),
                CAbiExport::with_arg_types("f64_log", "f64_log", vec![StaticType::F64]),
                CAbiExport::with_arg_types("f64_roundtrip", "f64_roundtrip", vec![StaticType::F64]),
                CAbiExport::with_arg_types("f32_exp", "f32_exp", vec![StaticType::F32]),
                CAbiExport::with_arg_types("f32_log", "f32_log", vec![StaticType::F32]),
            ],
            ..CompileConfig::default()
        };

        // When: Node executes representative values, boundaries, and invalid domains.
        let output = compile_wasm_source(source, &config).expect("exp/log should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const close = (actual, expected, tolerance) => Math.abs(actual - expected) <= tolerance * Math.max(1, Math.abs(expected));
const traps = action => { try { action(); return false; } catch (error) { return error instanceof WebAssembly.RuntimeError; } };
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  f64: [close(e.f64_exp(-20), Math.exp(-20), 1e-12), close(e.f64_exp(0), 1, 1e-12), close(e.f64_exp(20), Math.exp(20), 1e-12), close(e.f64_log(Number.MIN_VALUE), Math.log(Number.MIN_VALUE), 1e-12), close(e.f64_log(1), 0, 1e-12), close(e.f64_log(Number.MAX_VALUE), Math.log(Number.MAX_VALUE), 1e-12), close(e.f64_roundtrip(0.125), 0.125, 1e-12), traps(() => e.f64_log(-1))],
  f32: [close(e.f32_exp(-10), Math.exp(-10), 1e-5), close(e.f32_exp(10), Math.exp(10), 1e-5), close(e.f32_log(0.125), Math.log(0.125), 1e-5), close(e.f32_log(3.4028234663852886e38), Math.log(3.4028234663852886e38), 1e-5), traps(() => e.f32_log(-1))],
}));
"#,
        );

        // Then: generated approximations remain import-free within manifest tolerances.
        assert_eq!(
            value,
            r#"{"imports":0,"f64":[true,true,true,true,true,true,true,true],"f32":[true,true,true,true,true]}"#
        );
    }

    #[test]
    fn wasm_exp_log_special_values_and_thresholds_match_julia() {
        // Given: Julia 1.12.4 special values and dense finite threshold probes.
        let source = "f64_exp_edge(x::Float64)::Float64 = exp(x)\nf64_log_edge(x::Float64)::Float64 = log(x)\nf32_exp_edge(x::Float32)::Float32 = exp(x)\nf32_log_edge(x::Float32)::Float32 = log(x)\nf64_exp_log_repeat(x::Float64)::Float64 = exp(log(x)) + log(exp(x))";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![
                CAbiExport::with_arg_types("f64_exp_edge", "f64_exp_edge", vec![StaticType::F64]),
                CAbiExport::with_arg_types("f64_log_edge", "f64_log_edge", vec![StaticType::F64]),
                CAbiExport::with_arg_types("f32_exp_edge", "f32_exp_edge", vec![StaticType::F32]),
                CAbiExport::with_arg_types("f32_log_edge", "f32_log_edge", vec![StaticType::F32]),
                CAbiExport::with_arg_types(
                    "f64_exp_log_repeat",
                    "f64_exp_log_repeat",
                    vec![StaticType::F64],
                ),
            ],
            ..CompileConfig::default()
        };

        // When: Node compares exact special bits and finite results to Julia oracles.
        let output = compile_wasm_source(source, &config).expect("exp/log edges should compile");
        let value = run_wasm_bytes_node(
            &output.wasm_bytes,
            r#"
const e = instance.exports;
const b64 = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString(16).padStart(16, "0");
const b32 = x => new Uint32Array(new Float32Array([x]).buffer)[0].toString(16).padStart(8, "0");
const close = (a,b,t) => Math.abs(a-b) <= t * Math.max(1,Math.abs(b));
const traps = action => { try { action(); return false; } catch (error) { return error instanceof WebAssembly.RuntimeError; } };
const exp64 = [[709,8.218407461554972e307],[709.5,1.3549863193146328e308],[-744,1e-323],[-745,5e-324]];
const log64 = [[Number.MIN_VALUE,-744.4400719213812],[Number.MAX_VALUE,709.782712893384]];
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  exp64Special: [Infinity,-Infinity,NaN,750,-750].map(x => b64(e.f64_exp_edge(x))),
  exp32Special: [Infinity,-Infinity,NaN,100,-110].map(x => b32(e.f32_exp_edge(x))),
  log64Special: [0,-0,Infinity,NaN].map(x => b64(e.f64_log_edge(x))),
  log32Special: [0,-0,Infinity,NaN].map(x => b32(e.f32_log_edge(x))),
  finite64: exp64.map(([x,y]) => close(e.f64_exp_edge(x),y,1e-12)).concat(log64.map(([x,y]) => close(e.f64_log_edge(x),y,1e-12))),
  finite32: [close(e.f32_exp_edge(80),5.5406225e34,1e-5),close(e.f32_exp_edge(-100),3.8e-44,1e-5),close(e.f32_log_edge(1.401298464324817e-45),-103.27893,1e-5)],
  traps: [traps(() => e.f64_log_edge(-1)),traps(() => e.f32_log_edge(-1))],
  repeat: close(e.f64_exp_log_repeat(4),8,1e-12),
}));
"#,
        );

        // Then: classification precedes approximation and exponent construction never wraps.
        assert_eq!(
            value,
            r#"{"imports":0,"exp64Special":["7ff0000000000000","0000000000000000","7ff8000000000000","7ff0000000000000","0000000000000000"],"exp32Special":["7f800000","00000000","7fc00000","7f800000","00000000"],"log64Special":["fff0000000000000","fff0000000000000","7ff0000000000000","7ff8000000000000"],"log32Special":["ff800000","ff800000","7f800000","7fc00000"],"finite64":[true,true,true,true,true,true],"finite32":[true,true,true],"traps":[true,true],"repeat":true}"#
        );
    }

    #[test]
    fn wasm_executes_counted_loop_from_julia_source() {
        // Given: a counted while loop with mutable scalar locals.
        let source = "function triangular(n::Int64)::Int64\ni = 1\ns = 0\nwhile i <= n\ns = s + i\ni = i + 1\nend\nreturn s\nend";

        // When: the loop executes in generated Wasm.
        let value = compile_and_run_node(
            source,
            "triangular",
            vec![StaticType::I64],
            "const f = Object.values(instance.exports).find(v => typeof v === 'function' && v.length === 1 && v !== instance.exports.__sjulia_wasm_abi_version); console.log(f(9n).toString());",
        );

        // Then: repeated block dispatch produces the expected sum.
        assert_eq!(value, "45");
    }

    #[test]
    fn wasm_executes_direct_helper_call_from_julia_source() {
        // Given: two typed Julia functions with a direct call edge.
        let source = "twice(x::Int64) = x * 2\nplus_twice(x::Int64, y::Int64) = x + twice(y)";

        // When: the caller is executed from Node.
        let value = compile_and_run_node(
            source,
            "plus_twice",
            vec![StaticType::I64, StaticType::I64],
            "const f = Object.entries(instance.exports).find(([name, v]) => name.includes('plus_twice') && typeof v === 'function')[1]; console.log(f(4n, 19n).toString());",
        );

        // Then: direct Wasm call resolution preserves the helper result.
        assert_eq!(value, "42");
    }

    #[test]
    fn wasm_mutates_uint8_memory_through_versioned_descriptor() {
        // Given: a generic UInt8 mutation loop using Julia's one-based indexing.
        let source = "function increment!(bytes::Vector{UInt8})\ni = 1\nwhile i <= length(bytes)\nbytes[i] = UInt8(bytes[i] + 1)\ni = i + 1\nend\nreturn length(bytes)\nend";

        // When: the host writes a v2 descriptor and invokes generated Wasm.
        let value = compile_and_run_node(
            source,
            "increment!",
            vec![StaticType::Array { element: Box::new(StaticType::U8), ndims: Some(1) }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const ptr = 128; const input = new Uint8Array(memory.buffer, ptr, 4); input.set([1, 2, 254, 0]); view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 0, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, ptr, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 4n, true); view.setBigUint64(descriptor + 40, 4n, true); view.setBigInt64(descriptor + 48, 1n, true); const f = Object.entries(instance.exports).find(([name, v]) => name.includes('increment') && typeof v === 'function')[1]; const len = f(descriptor); console.log(`${len}:${Array.from(input).join(',')}:${instance.exports.__sjulia_wasm_abi_version()}`);",
        );

        // Then: bytes mutate in place and ABI metadata remains observable.
        assert_eq!(value, "4:2,3,255,1:2");
    }

    #[test]
    fn wasm_rgba_loop_preserves_alpha() {
        // Given: an RGBA loop expressed only through generic UInt8 indexing.
        let source = "function invert_rgba!(bytes::Vector{UInt8})\ni = 1\nwhile i <= length(bytes)\nbytes[i] = UInt8(255 - bytes[i])\nbytes[i + 1] = UInt8(255 - bytes[i + 1])\nbytes[i + 2] = UInt8(255 - bytes[i + 2])\ni = i + 4\nend\nreturn length(bytes)\nend";

        // When: generated Wasm mutates host-owned linear memory.
        let value = compile_and_run_node(
            source,
            "invert_rgba!",
            vec![StaticType::Array { element: Box::new(StaticType::U8), ndims: Some(1) }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const ptr = 128; const input = new Uint8Array(memory.buffer, ptr, 8); input.set([10, 20, 30, 40, 100, 150, 200, 250]); view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 0, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, ptr, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 8n, true); view.setBigUint64(descriptor + 40, 8n, true); view.setBigInt64(descriptor + 48, 1n, true); const f = Object.entries(instance.exports).find(([name, v]) => name.includes('invert_rgba') && typeof v === 'function')[1]; f(descriptor); console.log(Array.from(input).join(','));",
        );

        // Then: RGB channels invert while both alpha bytes stay unchanged.
        assert_eq!(value, "245,235,225,40,155,105,55,250");
    }

    #[test]
    fn wasm_phi_edge_copies_are_parallel() {
        // Given: a low-level edge with cyclic phi assignments a <- b and b <- a.
        let mut function = IrFunction::new("phi_swap".to_string(), Vec::new(), StaticType::I64);
        let a = VarRef::new("a".to_string(), StaticType::I64);
        let b = VarRef::new("b".to_string(), StaticType::I64);
        let ten = VarRef::new("ten".to_string(), StaticType::I64);
        let scaled = VarRef::new("scaled".to_string(), StaticType::I64);
        let result = VarRef::new("result".to_string(), StaticType::I64);
        let entry = function
            .entry_block_mut()
            .expect("IR function has entry block");
        entry.push(Instruction::LoadConst {
            dest: a.clone(),
            value: ConstValue::Int64(1),
        });
        entry.push(Instruction::LoadConst {
            dest: b.clone(),
            value: ConstValue::Int64(2),
        });
        entry.set_terminator(Terminator::Jump("join".to_string()));
        let mut join = BasicBlock::new("join".to_string());
        join.push(Instruction::Phi {
            dest: a.clone(),
            incoming: vec![("entry".to_string(), b.clone())],
        });
        join.push(Instruction::Phi {
            dest: b.clone(),
            incoming: vec![("entry".to_string(), a.clone())],
        });
        join.push(Instruction::LoadConst {
            dest: ten.clone(),
            value: ConstValue::Int64(10),
        });
        join.push(Instruction::BinOp {
            dest: scaled.clone(),
            op: BinOpKind::Mul,
            left: a.clone(),
            right: ten,
        });
        join.push(Instruction::BinOp {
            dest: result.clone(),
            op: BinOpKind::Add,
            left: scaled,
            right: b,
        });
        join.set_terminator(Terminator::Return(Some(result)));
        function.add_block(join);
        let mut module = IrModule::new("phi_swap".to_string());
        module.add_function(function);

        // When: the backend emits and Node executes the cyclic phi edge.
        let bytes = emit_module(&module, &[]).expect("phi module should emit");
        let value = run_wasm_bytes_node(
            &bytes,
            "console.log(instance.exports.phi_swap().toString());",
        );

        // Then: both reads observe predecessor values before either destination changes.
        assert_eq!(value, "21");
    }

    #[test]
    fn wasm_uint8_descriptor_rejects_malformed_host_ranges() {
        // Given: a real Julia UInt8 reader and four malformed v2 host descriptors.
        let source = "first_byte(bytes::Vector{UInt8}) = bytes[1]";
        let javascript = "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const f = Object.entries(instance.exports).find(([name, v]) => name.includes('first_byte') && typeof v === 'function')[1]; const descriptor = 32; const ptr = 128; function valid() { view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 0, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, ptr, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 1n, true); view.setBigUint64(descriptor + 40, 1n, true); view.setBigInt64(descriptor + 48, 1n, true); } const cases = [() => view.setUint32(descriptor, 1, true), () => view.setBigUint64(descriptor + 32, 2n, true), () => view.setUint32(descriptor + 24, memory.buffer.byteLength, true), () => view.setBigInt64(descriptor + 48, -1n, true)]; let traps = 0; for (const corrupt of cases) { valid(); corrupt(); try { f(descriptor); } catch (error) { if (error instanceof WebAssembly.RuntimeError) traps += 1; } } console.log(traps);";

        // When: Node calls generated Wasm with each malformed descriptor.
        let value = compile_and_run_node(
            source,
            "first_byte",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            javascript,
        );

        // Then: every malformed descriptor traps before a memory access succeeds.
        assert_eq!(value, "4");
    }

    #[test]
    fn wasm_descriptor_v2_rejects_header_contract_violations_before_store() {
        // Given: a rank-1 writer and malformed fixed-header or expected-type fields.
        let source = "function set_first!(bytes::Vector{UInt8})::Int64\nbytes[1] = UInt8(99)\nreturn length(bytes)\nend";
        let javascript = r#"
const memory = instance.exports.memory;
const view = new DataView(memory.buffer);
const descriptor = 32;
const pointer = 128;
const input = new Uint8Array(memory.buffer, pointer, 1);
const target = instance.exports["set_first!"];
function valid() {
  view.setUint32(descriptor, 2, true);
  view.setUint32(descriptor + 4, 0, true);
  view.setUint32(descriptor + 8, 1, true);
  view.setUint32(descriptor + 12, 1, true);
  view.setUint32(descriptor + 16, 0, true);
  view.setUint32(descriptor + 20, 1, true);
  view.setUint32(descriptor + 24, pointer, true);
  view.setUint32(descriptor + 28, 0, true);
  view.setBigUint64(descriptor + 32, 1n, true);
  view.setBigUint64(descriptor + 40, 1n, true);
  view.setBigInt64(descriptor + 48, 1n, true);
}
const invalid = [
  () => view.setUint32(descriptor, 1, true),
  () => view.setUint32(descriptor, 3, true),
  () => view.setUint32(descriptor + 4, 4, true),
  () => view.setUint32(descriptor + 4, 2, true),
  () => view.setUint32(descriptor + 8, 2, true),
  () => view.setUint32(descriptor + 8, 99, true),
  () => view.setUint32(descriptor + 12, 2, true),
  () => view.setUint32(descriptor + 16, 1, true),
  () => view.setUint32(descriptor + 20, 2, true),
  () => view.setUint32(descriptor + 20, 9, true),
  () => view.setUint32(descriptor + 28, 1, true),
];
let traps = 0;
for (const corrupt of invalid) {
  valid();
  input[0] = 7;
  corrupt();
  try { target(descriptor); } catch (error) {
    if (error instanceof WebAssembly.RuntimeError && input[0] === 7) traps += 1;
  }
}
for (const invalidPointer of [0, -8, descriptor + 1, memory.buffer.byteLength - 32]) {
  valid();
  input[0] = 7;
  try { target(invalidPointer); } catch (error) {
    if (error instanceof WebAssembly.RuntimeError && input[0] === 7) traps += 1;
  }
}
console.log(traps);
"#;

        // When: each corrupt descriptor is passed through the real Wasm export.
        let value = compile_and_run_node(
            source,
            "set_first!",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            javascript,
        );

        // Then: all header violations trap before the sentinel can be stored.
        assert_eq!(value, "15");
    }

    #[test]
    fn wasm_descriptor_v2_rejects_shape_extent_and_index_violations_before_store() {
        // Given: a rank-2 writer and malformed inline shape, stride, extent, and index cases.
        let source = "function set_byte!(bytes::Matrix{UInt8}, row::Int64, column::Int64)::Int64\nbytes[row, column] = UInt8(99)\nreturn length(bytes)\nend";
        let javascript = r#"
const memory = instance.exports.memory;
const view = new DataView(memory.buffer);
const descriptor = 32;
const pointer = 160;
const input = new Uint8Array(memory.buffer, pointer, 6);
const target = instance.exports["set_byte!"];
function valid() {
  view.setUint32(descriptor, 2, true);
  view.setUint32(descriptor + 4, 0, true);
  view.setUint32(descriptor + 8, 1, true);
  view.setUint32(descriptor + 12, 1, true);
  view.setUint32(descriptor + 16, 0, true);
  view.setUint32(descriptor + 20, 2, true);
  view.setUint32(descriptor + 24, pointer, true);
  view.setUint32(descriptor + 28, 0, true);
  view.setBigUint64(descriptor + 32, 6n, true);
  view.setBigUint64(descriptor + 40, 2n, true);
  view.setBigInt64(descriptor + 48, 1n, true);
  view.setBigUint64(descriptor + 56, 3n, true);
  view.setBigInt64(descriptor + 64, 2n, true);
}
const invalid = [
  [() => view.setBigUint64(descriptor + 32, 5n, true), 1n, 1n],
  [() => { view.setBigUint64(descriptor + 40, 0xffffffffffffffffn, true); view.setBigUint64(descriptor + 56, 2n, true); }, 1n, 1n],
  [() => view.setBigInt64(descriptor + 48, -1n, true), 1n, 1n],
  [() => { view.setBigUint64(descriptor + 40, 3n, true); view.setBigInt64(descriptor + 48, 0x7fffffffffffffffn, true); }, 1n, 1n],
  [() => { view.setBigUint64(descriptor + 40, 2n, true); view.setBigUint64(descriptor + 56, 2n, true); view.setBigUint64(descriptor + 32, 4n, true); view.setBigInt64(descriptor + 48, 0x7fffffffffffffffn, true); view.setBigInt64(descriptor + 64, 0x7fffffffffffffffn, true); }, 1n, 1n],
  [() => view.setUint32(descriptor + 24, memory.buffer.byteLength - 1, true), 1n, 1n],
  [() => view.setUint32(descriptor + 24, 0, true), 1n, 1n],
  [() => view.setUint32(descriptor + 24, descriptor + 40, true), 1n, 1n],
  [() => {}, 3n, 1n],
  [() => {}, 1n, 4n],
  [() => { view.setBigUint64(descriptor + 40, 0n, true); view.setBigUint64(descriptor + 32, 0n, true); view.setUint32(descriptor + 24, 0, true); }, 1n, 1n],
];
let traps = 0;
for (const [corrupt, row, column] of invalid) {
  valid();
  input.fill(7);
  corrupt();
  try { target(descriptor, row, column); } catch (error) {
    if (error instanceof WebAssembly.RuntimeError && input.every((byte) => byte === 7)) traps += 1;
  }
}
console.log(traps);
"#;

        // When: Node invokes each malformed descriptor or out-of-bounds index pair.
        let value = compile_and_run_node(
            source,
            "set_byte!",
            vec![
                StaticType::Array {
                    element: Box::new(StaticType::U8),
                    ndims: Some(2),
                },
                StaticType::I64,
                StaticType::I64,
            ],
            javascript,
        );

        // Then: every case traps before any byte in the host buffer changes.
        assert_eq!(value, "11");
    }

    #[test]
    fn wasm_descriptor_v2_enforces_metadata_extent_and_rank_zero_rules() {
        // Given: rank-1 and rank-0 length exports with boundary descriptors.
        let rank_one = compile_and_run_node(
            "array_len(bytes::Vector{UInt8}) = length(bytes)",
            "array_len",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = memory.buffer.byteLength - 40; view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 0, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, 0, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 0n, true); let trapped = 0; try { instance.exports.array_len(descriptor); } catch (error) { if (error instanceof WebAssembly.RuntimeError) trapped = 1; } console.log(trapped);",
        );
        let rank_zero = compile_and_run_node(
            "array_len(bytes::Array{UInt8,0}) = length(bytes)",
            "array_len",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(0),
            }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const pointer = 128; function write(count) { view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 0, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 0, true); view.setUint32(descriptor + 24, pointer, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, count, true); } write(1n); const valid = instance.exports.array_len(descriptor); write(0n); let traps = 0; try { instance.exports.array_len(descriptor); } catch (error) { if (error instanceof WebAssembly.RuntimeError) traps += 1; } write(2n); try { instance.exports.array_len(descriptor); } catch (error) { if (error instanceof WebAssembly.RuntimeError) traps += 1; } console.log(`${valid}:${traps}`);",
        );

        // When: metadata truncation and invalid rank-0 counts cross the boundary.
        // Then: truncation traps, rank-0 count one passes, and other counts trap.
        assert_eq!(rank_one, "1");
        assert_eq!(rank_zero, "1:2");
    }

    #[test]
    fn wasm_descriptor_v2_enforces_readonly_and_zero_count_access_rules() {
        // Given: UInt8 read/write exports and valid readonly or empty descriptors.
        let readonly_read = compile_and_run_node(
            "read_first(bytes::Vector{UInt8}) = bytes[1]",
            "read_first",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const pointer = 128; new Uint8Array(memory.buffer, pointer, 1)[0] = 7; view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 2, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, pointer, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 1n, true); view.setBigUint64(descriptor + 40, 1n, true); view.setBigInt64(descriptor + 48, 1n, true); console.log(instance.exports.read_first(descriptor));",
        );
        let module_owned_read = compile_and_run_node(
            "read_first(bytes::Vector{UInt8}) = bytes[1]",
            "read_first",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const pointer = 128; new Uint8Array(memory.buffer, pointer, 1)[0] = 7; view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 1, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, pointer, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 1n, true); view.setBigUint64(descriptor + 40, 1n, true); view.setBigInt64(descriptor + 48, 1n, true); console.log(instance.exports.read_first(descriptor));",
        );
        let readonly_write = compile_and_run_node(
            "function write_first!(bytes::Vector{UInt8})::Int64\nbytes[1] = UInt8(99)\nreturn length(bytes)\nend",
            "write_first!",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const pointer = 128; const input = new Uint8Array(memory.buffer, pointer, 1); input[0] = 7; view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 2, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, pointer, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 1n, true); view.setBigUint64(descriptor + 40, 1n, true); view.setBigInt64(descriptor + 48, 1n, true); let trapped = 0; try { instance.exports[\"write_first!\"](descriptor); } catch (error) { if (error instanceof WebAssembly.RuntimeError) trapped = 1; } console.log(`${trapped}:${input[0]}`);",
        );
        let empty = compile_and_run_node(
            "read_first(bytes::Vector{UInt8}) = bytes[1]",
            "read_first",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            "const view = new DataView(instance.exports.memory.buffer); const descriptor = 32; view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 0, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 1, true); view.setUint32(descriptor + 24, 0, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 0n, true); view.setBigUint64(descriptor + 40, 0n, true); view.setBigInt64(descriptor + 48, 1n, true); let trapped = 0; try { instance.exports.read_first(descriptor); } catch (error) { if (error instanceof WebAssembly.RuntimeError) trapped = 1; } console.log(trapped);",
        );

        // When: a read and store use READONLY, and an index targets zero elements.
        // Then: only the read succeeds; both prohibited accesses trap without mutation.
        assert_eq!(readonly_read, "7");
        assert_eq!(module_owned_read, "7");
        assert_eq!(readonly_write, "1:7");
        assert_eq!(empty, "1");
    }

    #[test]
    fn wasm_descriptor_v2_rejects_static_rank_above_limit() {
        // Given: a statically typed UInt8 array above the ABI rank cap.
        let source = "array_len(bytes::Array{UInt8,9}) = length(bytes)";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![CAbiExport::with_arg_types(
                "array_len",
                "array_len",
                vec![StaticType::Array {
                    element: Box::new(StaticType::U8),
                    ndims: Some(9),
                }],
            )],
            ..CompileConfig::default()
        };

        // When: the canonical Wasm pipeline validates the static descriptor type.
        let error = compile_wasm_source(source, &config)
            .expect_err("rank above MAX_RANK must remain unsupported");

        // Then: rank overflow is a typed compile diagnostic, not a runtime ABI guess.
        assert!(matches!(error, AotError::UnsupportedInstruction(_)));
    }

    #[test]
    fn wasm_descriptor_v2_dimension_limit_is_inclusive_and_traps_before_store() {
        // Given: a zero-stride view whose one-byte extent isolates dimension validation.
        let source = "function write_and_length!(bytes::Vector{UInt8})::Int64\nbytes[1] = UInt8(99)\nreturn length(bytes)\nend";
        let javascript = r#"
const memory = instance.exports.memory;
const view = new DataView(memory.buffer);
const descriptor = 32;
const pointer = 128;
const input = new Uint8Array(memory.buffer, pointer, 1);
function write(dim) {
  view.setUint32(descriptor, 2, true);
  view.setUint32(descriptor + 4, 0, true);
  view.setUint32(descriptor + 8, 1, true);
  view.setUint32(descriptor + 12, 1, true);
  view.setUint32(descriptor + 16, 0, true);
  view.setUint32(descriptor + 20, 1, true);
  view.setUint32(descriptor + 24, pointer, true);
  view.setUint32(descriptor + 28, 0, true);
  view.setBigUint64(descriptor + 32, dim, true);
  view.setBigUint64(descriptor + 40, dim, true);
  view.setBigInt64(descriptor + 48, 0n, true);
}
write(0x80000000n);
input[0] = 7;
const boundary = instance.exports["write_and_length!"](descriptor);
write(0x80000001n);
input[0] = 7;
let trapped = 0;
try { instance.exports["write_and_length!"](descriptor); } catch (error) {
  if (error instanceof WebAssembly.RuntimeError && input[0] === 7) trapped = 1;
}
console.log(`${boundary}:${trapped}:${input[0]}`);
"#;

        // When: Node executes the inclusive boundary and then boundary plus one.
        let value = compile_and_run_node(
            source,
            "write_and_length!",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(1),
            }],
            javascript,
        );

        // Then: 2^31 is accepted and 2^31+1 traps before the sentinel changes.
        assert_eq!(value, "2147483648:1:7");
    }

    #[test]
    fn wasm_descriptor_v2_stride_zero_is_an_aliasing_view() {
        // Given: a module-owned 2x3 view with both strides zero and one backing byte.
        let source = "function alias_write!(bytes::Matrix{UInt8})::Int64\nbytes[2, 3] = UInt8(99)\nreturn Int64(bytes[1, 1]) + length(bytes)\nend";

        // When: Node calls the view through a valid ABI v2 descriptor.
        let value = compile_and_run_node(
            source,
            "alias_write!",
            vec![StaticType::Array {
                element: Box::new(StaticType::U8),
                ndims: Some(2),
            }],
            "const memory = instance.exports.memory; const view = new DataView(memory.buffer); const descriptor = 32; const pointer = 128; const input = new Uint8Array(memory.buffer, pointer, 1); input[0] = 7; view.setUint32(descriptor, 2, true); view.setUint32(descriptor + 4, 1, true); view.setUint32(descriptor + 8, 1, true); view.setUint32(descriptor + 12, 1, true); view.setUint32(descriptor + 16, 0, true); view.setUint32(descriptor + 20, 2, true); view.setUint32(descriptor + 24, pointer, true); view.setUint32(descriptor + 28, 0, true); view.setBigUint64(descriptor + 32, 6n, true); view.setBigUint64(descriptor + 40, 2n, true); view.setBigInt64(descriptor + 48, 0n, true); view.setBigUint64(descriptor + 56, 3n, true); view.setBigInt64(descriptor + 64, 0n, true); const result = instance.exports[\"alias_write!\"](descriptor); console.log(`${result}:${input[0]}`);",
        );

        // Then: every logical index aliases the same safe in-bounds byte.
        assert_eq!(value, "105:99");
    }

    #[test]
    fn wasm_u8_address_emission_has_no_unused_max_offset_operand() {
        // Given: a generated rank-1 load with checked descriptor addressing.
        let source = "read_first(bytes::Vector{UInt8}) = bytes[1]";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![CAbiExport::with_arg_types(
                "read_first",
                "read_first",
                vec![StaticType::Array {
                    element: Box::new(StaticType::U8),
                    ndims: Some(1),
                }],
            )],
            ..CompileConfig::default()
        };
        let output = compile_wasm_source(source, &config).expect("rank-1 load should compile");

        // When: the generated operator stream is rendered as canonical WAT.
        let wat = wasm_text(&output.wasm_bytes);
        let operators = wat.lines().map(str::trim).collect::<Vec<_>>().join("\n");

        // Then: address multiplication starts from the zero-based index, not max_offset.
        assert!(
            !operators
                .contains("local.get 9\nlocal.get 10\nlocal.get 0\ni64.load offset=48 align=1"),
            "address emission retained an unused max-offset operand:\n{wat}"
        );
    }

    #[test]
    fn wasm_rgba_node_benchmark_meets_warm_loop_gate() {
        // Given: the same general RGBA Julia function compiled by the full pipeline.
        let source = "function invert_rgba!(bytes::Vector{UInt8})\ni = 1\nwhile i <= length(bytes)\nbytes[i] = UInt8(255 - bytes[i])\nbytes[i + 1] = UInt8(255 - bytes[i + 1])\nbytes[i + 2] = UInt8(255 - bytes[i + 2])\ni = i + 4\nend\nreturn length(bytes)\nend";
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![CAbiExport::with_arg_types(
                "invert_rgba!",
                "invert_rgba!",
                vec![StaticType::Array {
                    element: Box::new(StaticType::U8),
                    ndims: Some(1),
                }],
            )],
            ..CompileConfig::default()
        };
        let output =
            compile_wasm_source(source, &config).expect("RGBA Wasm compilation should succeed");
        let dir = tempfile::tempdir().expect("create benchmark directory");
        let wasm_path = dir.path().join("rgba.wasm");
        fs::write(&wasm_path, output.wasm_bytes).expect("write benchmark Wasm");

        // When: Node benchmarks 20 warm 888x862 RGBA iterations.
        let result = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../benchmarks/wasm_aot_rgba.mjs"
            ))
            .arg(&wasm_path)
            .output()
            .expect("run Node RGBA benchmark");

        // Then: Node enforces the latency gate and emits all requested phases.
        assert!(
            result.status.success(),
            "RGBA benchmark must meet p95 <100ms\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let node_timings = String::from_utf8(result.stdout).expect("benchmark output is UTF-8");
        assert!(node_timings.contains("compile_ms="));
        assert!(node_timings.contains("instantiate_ms="));
        assert!(node_timings.contains("median_ms="));
        assert!(node_timings.contains("p95_ms="));
        for (phase, duration) in output.timings {
            eprintln!("{phase}_ms={:.3}", duration.as_secs_f64() * 1_000.0);
        }
        eprintln!("{}", node_timings.trim());
    }

    #[test]
    fn wasm_builds_primitive_rectangular_comprehensions() {
        // Given: one-dimensional, nested rectangular, and empty-axis comprehensions.
        let source = r#"
line()::Vector{Int64} = [i * i for i in 1:4]
grid()::Matrix{Int64} = [i + j for i in 1:2, j in 1:3]
empty_axis()::Vector{Int64} = [i for i in 1:0]
"#;
        let oracle = Command::new("julia")
            .args([
                "--startup-file=no",
                "-e",
                &format!(
                    "{source}\nprintln((size(line()), line(), size(grid()), vec(grid()), size(empty_axis()), empty_axis()))"
                ),
            ])
            .output()
            .expect("run upstream Julia comprehension oracle");
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout).trim(),
            "((4,), [1, 4, 9, 16], (2, 3), [2, 3, 3, 4, 4, 5], (0,), Int64[])"
        );
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: ["line", "grid", "empty_axis"]
                .into_iter()
                .map(|name| CAbiExport::with_arg_types(name, name, Vec::new()))
                .collect(),
            ..CompileConfig::default()
        };

        // When: generated Wasm allocates each result once and fills it in place.
        let output = compile_wasm_source(source, &config)
            .expect("primitive rectangular comprehensions should compile");
        let javascript = r#"
const e = instance.exports;
const decode = pointer => {
  const view = new DataView(e.memory.buffer);
  const rank = view.getUint32(pointer + 20, true);
  const count = Number(view.getBigUint64(pointer + 32, true));
  const start = view.getUint32(pointer + 24, true);
  return {
    flags: view.getUint32(pointer + 4, true),
    rank,
    dims: Array.from({length: rank}, (_, axis) => Number(view.getBigUint64(pointer + 40 + axis * 16, true))),
    strides: Array.from({length: rank}, (_, axis) => Number(view.getBigInt64(pointer + 48 + axis * 16, true))),
    values: Array.from(new BigInt64Array(e.memory.buffer, start, count), Number),
  };
};
const results = [e.line(), e.grid(), e.empty_axis()];
const decoded = results.map(decode);
results.forEach(e.__sjulia_drop);
console.log(JSON.stringify({ imports: WebAssembly.Module.imports(module).length, decoded }));
"#;
        let values = (0..3)
            .map(|_| run_wasm_bytes_node(&output.wasm_bytes, javascript))
            .collect::<Vec<_>>();

        // Then: results are module-owned, column-major, and match Julia exactly.
        assert_eq!(
            values,
            vec![
                r#"{"imports":0,"decoded":[{"flags":1,"rank":1,"dims":[4],"strides":[1],"values":[1,4,9,16]},{"flags":1,"rank":2,"dims":[2,3],"strides":[1,2],"values":[2,3,3,4,4,5]},{"flags":1,"rank":1,"dims":[0],"strides":[1],"values":[]}]}"#;
                3
            ]
        );
    }

    #[test]
    fn wasm_broadcasts_scalar_array_trees_over_one_array_operand() {
        // Given: scalar-array arithmetic, a comparison, a promoting division, a fused
        // chain and a two-scalar tree over a single rank-one array operand.
        let source = r#"
shift(v::Vector{Int32})::Vector{Int32} = v .+ Int32(10)
compare(v::Vector{Int32})::Vector{Bool} = v .< Int32(3)
halve(v::Vector{Int32})::Vector{Float64} = v ./ 2
fused(v::Vector{Int32})::Vector{Float64} = clamp.(v ./ 2 .+ 1.0, 1.5, 3.0)
two(v::Vector{Int32})::Vector{Int32} = v .+ Int32(2) .* Int32(3)
"#;
        let oracle = Command::new("julia")
            .args([
                "--startup-file=no",
                "-e",
                &format!(
                    "{source}\nv = Int32[1,2,3,4]\nprintln(join([string(eltype(x), \" \", x) for x in (shift(v), compare(v), halve(v), fused(v), two(v))], \" | \"))"
                ),
            ])
            .output()
            .expect("run upstream Julia broadcast oracle");
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&oracle.stdout).trim(),
            "Int32 Int32[11, 12, 13, 14] | Bool Bool[1, 1, 0, 0] | Float64 [0.5, 1.0, 1.5, 2.0] | Float64 [1.5, 2.0, 2.5, 3.0] | Int32 Int32[7, 8, 9, 10]"
        );
        let vector = StaticType::Array {
            element: Box::new(StaticType::I32),
            ndims: Some(1),
        };
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: ["shift", "compare", "halve", "fused", "two"]
                .into_iter()
                .map(|name| CAbiExport::with_arg_types(name, name, vec![vector.clone()]))
                .collect(),
            ..CompileConfig::default()
        };

        // When: Node drives contiguous, empty, and noncontiguous read-only inputs.
        let output = compile_wasm_source(source, &config)
            .expect("single-array broadcast trees should compile");
        let javascript = r#"
const e = instance.exports;
const view = new DataView(e.memory.buffer);
const describe = (at, flags, data, count, dim, stride) => {
  view.setUint32(at, 2, true);
  view.setUint32(at + 4, flags, true);
  view.setUint32(at + 8, 6, true);
  view.setUint32(at + 12, 4, true);
  view.setUint32(at + 16, 0, true);
  view.setUint32(at + 20, 1, true);
  view.setUint32(at + 24, data, true);
  view.setUint32(at + 28, 0, true);
  view.setBigUint64(at + 32, BigInt(count), true);
  view.setBigUint64(at + 40, BigInt(dim), true);
  view.setBigInt64(at + 48, BigInt(stride), true);
  return at;
};
const dense = describe(32, 0, 256, 4, 4, 1);
const empty = describe(96, 0, 0, 0, 0, 1);
const strided = describe(160, 2, 384, 3, 3, 2);
new Int32Array(e.memory.buffer, 256, 4).set([1, 2, 3, 4]);
new Int32Array(e.memory.buffer, 384, 6).set([1, 2, 3, 4, 5, 6]);
const decode = pointer => {
  const current = new DataView(e.memory.buffer);
  const tag = current.getUint32(pointer + 8, true);
  const rank = current.getUint32(pointer + 20, true);
  const count = Number(current.getBigUint64(pointer + 32, true));
  const start = current.getUint32(pointer + 24, true);
  const buffer = e.memory.buffer;
  const values = tag === 6 ? Array.from(new Int32Array(buffer, start, count))
    : tag === 10 ? Array.from(new Float64Array(buffer, start, count))
    : Array.from(new Uint8Array(buffer, start, count));
  return {
    tag,
    flags: current.getUint32(pointer + 4, true),
    dims: Array.from({length: rank}, (_, axis) => Number(current.getBigUint64(pointer + 40 + axis * 16, true))),
    strides: Array.from({length: rank}, (_, axis) => Number(current.getBigInt64(pointer + 48 + axis * 16, true))),
    values,
  };
};
const results = [
  e.shift(dense), e.compare(dense), e.halve(dense), e.fused(dense), e.two(dense),
  e.shift(empty), e.shift(strided),
];
const decoded = results.map(decode);
const inputs = [
  Array.from(new Int32Array(e.memory.buffer, 256, 4)),
  Array.from(new Int32Array(e.memory.buffer, 384, 6)),
];
results.forEach(e.__sjulia_drop);
console.log(JSON.stringify({ imports: WebAssembly.Module.imports(module).length, inputs, decoded }));
"#;
        let values = (0..3)
            .map(|_| run_wasm_bytes_node(&output.wasm_bytes, javascript))
            .collect::<Vec<_>>();

        // Then: every result is a module-owned contiguous array carrying Julia's
        // element type, empty and strided sources are honoured, and no input changes.
        assert_eq!(
            values,
            vec![
                r#"{"imports":0,"inputs":[[1,2,3,4],[1,2,3,4,5,6]],"decoded":[{"tag":6,"flags":1,"dims":[4],"strides":[1],"values":[11,12,13,14]},{"tag":11,"flags":1,"dims":[4],"strides":[1],"values":[1,1,0,0]},{"tag":10,"flags":1,"dims":[4],"strides":[1],"values":[0.5,1,1.5,2]},{"tag":10,"flags":1,"dims":[4],"strides":[1],"values":[1.5,2,2.5,3]},{"tag":6,"flags":1,"dims":[4],"strides":[1],"values":[7,8,9,10]},{"tag":6,"flags":1,"dims":[0],"strides":[1],"values":[]},{"tag":6,"flags":1,"dims":[3],"strides":[1],"values":[11,13,15]}]}"#;
                3
            ]
        );
    }

    #[test]
    fn wasm_rejects_multi_array_broadcast_before_codegen() {
        // Given: same-shape two-array broadcast, which shared AoT specialization
        // rewrites away before the Wasm backend can plan a shape for it.
        let source = "add(v::Vector{Int32}, w::Vector{Int32})::Vector{Int32} = v .+ w";
        let vector = StaticType::Array {
            element: Box::new(StaticType::I32),
            ndims: Some(1),
        };
        let config = CompileConfig {
            backend: AotBackend::Wasm,
            c_abi_exports: vec![CAbiExport::with_arg_types(
                "add",
                "add",
                vec![vector.clone(), vector],
            )],
            ..CompileConfig::default()
        };

        // When: the Wasm backend compiles it.
        let error = compile_wasm_source(source, &config)
            .expect_err("multi-array broadcast must not be miscompiled");

        // Then: it is a typed unsupported diagnostic rather than a wrong result.
        assert!(matches!(error, AotError::UnsupportedInstruction(_)));
    }
}
