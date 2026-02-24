#![deny(clippy::expect_used)]
//! AoT (Ahead-of-Time) Compiler CLI
//!
//! This binary compiles Julia source code to Rust code.
//!
//! Usage:
//!   cargo run --bin aot --features aot -- input.jl -o output.rs
//!   cargo run --bin aot --features aot -- input.jl --stats
//!   cargo run --bin aot --features aot -- -e "1 + 2" -o output.rs
//!   cargo run --bin aot --features aot -- --bytecode input.sjbc -o output.rs

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use std::process;

use subset_julia_vm::aot::analyze::program_to_aot_ir;
use subset_julia_vm::aot::call_graph::CallGraph;
use subset_julia_vm::aot::codegen::aot_codegen::AotCodeGenerator;
use subset_julia_vm::aot::codegen::CodegenConfig;
use subset_julia_vm::aot::inference::TypeInferenceEngine;
use subset_julia_vm::aot::ir::AotProgram;
use subset_julia_vm::aot::optimizer::optimize_aot_program_full;
use subset_julia_vm::aot::{AotError, AotOutput, AotStats};
use subset_julia_vm::base;
use subset_julia_vm::bytecode;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::loader;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Command-line arguments
#[derive(Debug)]
struct Args {
    /// Input file path (None if using -e)
    input_file: Option<String>,
    /// Code string (for -e option)
    code: Option<String>,
    /// Bytecode file path (for --bytecode option)
    bytecode_file: Option<String>,
    /// Output file path
    output_file: Option<String>,
    /// Show statistics
    show_stats: bool,
    /// Emit debug comments in generated code
    emit_comments: bool,
    /// Show help
    show_help: bool,
    /// Show version
    show_version: bool,
    /// Generate pure Rust code without Value type dependency
    /// Fails if any dynamic dispatch is required
    pure_rust: bool,
    /// Use minimal prelude for AoT compilation (fully-typed functions only)
    minimal_prelude: bool,
}

impl Args {
    fn parse() -> Self {
        let args: Vec<String> = env::args().collect();
        let mut parsed = Args {
            input_file: None,
            code: None,
            bytecode_file: None,
            output_file: None,
            show_stats: false,
            emit_comments: false,
            show_help: false,
            show_version: false,
            pure_rust: false,
            minimal_prelude: false,
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => parsed.show_help = true,
                "-v" | "--version" => parsed.show_version = true,
                "-o" | "--output" => {
                    i += 1;
                    if i < args.len() {
                        parsed.output_file = Some(args[i].clone());
                    }
                }
                "-e" | "--eval" => {
                    i += 1;
                    if i < args.len() {
                        parsed.code = Some(args[i].clone());
                    }
                }
                "-b" | "--bytecode" => {
                    i += 1;
                    if i < args.len() {
                        parsed.bytecode_file = Some(args[i].clone());
                    }
                }
                "--stats" => parsed.show_stats = true,
                "--comments" => parsed.emit_comments = true,
                "--pure-rust" => parsed.pure_rust = true,
                "--minimal-prelude" => parsed.minimal_prelude = true,
                arg if !arg.starts_with('-') => {
                    if parsed.input_file.is_none()
                        && parsed.code.is_none()
                        && parsed.bytecode_file.is_none()
                    {
                        parsed.input_file = Some(arg.to_string());
                    }
                }
                _ => {
                    eprintln!("Unknown option: {}", args[i]);
                }
            }
            i += 1;
        }

        parsed
    }
}

fn print_help() {
    println!(
        r#"SubsetJuliaVM AoT Compiler v{}

USAGE:
    aot [OPTIONS] <input.jl>
    aot -e <code> [OPTIONS]
    aot -b <bytecode.sjbc> [OPTIONS]

OPTIONS:
    -h, --help       Show this help message
    -v, --version    Show version information
    -o, --output     Output file path (default: <input>.rs or output.rs)
    -e, --eval       Compile code string instead of file
    -b, --bytecode   Compile from bytecode file (.sjbc) instead of source
    --stats          Show compilation statistics
    --comments       Emit debug comments in generated code
    --pure-rust      Generate pure Rust code without Value type dependency
                     (fails if dynamic dispatch is required)
    --minimal-prelude Use minimal typed prelude for pure Rust code generation

EXAMPLES:
    aot input.jl -o output.rs
    aot input.jl --stats
    aot input.jl --pure-rust -o output.rs    # Standalone Rust (no runtime)
    aot -e "function add(x, y) x + y end" -o add.rs
    aot --bytecode program.sjbc -o output.rs

GENERATED CODE:
    The output is a Rust source file that can be compiled with:

    rustc output.rs -o program
    ./program

    With --pure-rust flag, the output can be compiled standalone
    without the subset_julia_vm_runtime crate.
"#,
        VERSION
    );
}

fn print_version() {
    println!("SubsetJuliaVM AoT Compiler v{}", VERSION);
}

fn main() {
    let args = Args::parse();

    if args.show_help {
        print_help();
        return;
    }

    if args.show_version {
        print_version();
        return;
    }

    // Determine source name for output file
    let source_name = if let Some(file) = &args.bytecode_file {
        file.clone()
    } else if let Some(file) = &args.input_file {
        file.clone()
    } else if args.code.is_some() {
        "<eval>".to_string()
    } else {
        eprintln!("Error: No input file, code, or bytecode provided");
        eprintln!("Use --help for usage information");
        process::exit(1);
    };

    // Determine output file
    let output_file = args.output_file.unwrap_or_else(|| {
        if let Some(input) = &args.bytecode_file {
            let path = Path::new(input);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            format!("{}.rs", stem)
        } else if let Some(input) = &args.input_file {
            let path = Path::new(input);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            format!("{}.rs", stem)
        } else {
            "output.rs".to_string()
        }
    });

    // Compile
    let result = if let Some(bytecode_file) = &args.bytecode_file {
        // Load from bytecode file
        compile_bytecode_to_rust(
            bytecode_file,
            &source_name,
            args.emit_comments,
            args.pure_rust,
            args.minimal_prelude,
        )
    } else if let Some(code) = &args.code {
        // Compile from code string
        compile_julia_to_rust(
            code,
            "<eval>",
            args.emit_comments,
            args.pure_rust,
            args.minimal_prelude,
        )
    } else if let Some(file) = &args.input_file {
        // Compile from source file
        if !Path::new(file).exists() {
            eprintln!("Error: File '{}' not found", file);
            process::exit(1);
        }
        let source = fs::read_to_string(file).unwrap_or_else(|e| {
            eprintln!("Error reading file '{}': {}", file, e);
            process::exit(1);
        });
        compile_julia_to_rust(
            &source,
            file,
            args.emit_comments,
            args.pure_rust,
            args.minimal_prelude,
        )
    } else {
        eprintln!("Error: No input file, code, or bytecode provided");
        eprintln!("Use --help for usage information");
        process::exit(1);
    };

    match result {
        Ok(output) => {
            // Write output file
            if let Err(e) = fs::write(&output_file, &output.rust_code) {
                eprintln!("Error writing output file '{}': {}", output_file, e);
                process::exit(1);
            }

            println!("Generated: {}", output_file);

            // Show statistics if requested
            if args.show_stats {
                println!();
                println!("Statistics:");
                println!(
                    "  Functions total (before DCE): {}",
                    output.stats.functions_total
                );
                println!(
                    "  Functions compiled (after DCE): {}",
                    output.stats.functions_compiled
                );
                println!(
                    "  Functions eliminated by DCE: {}",
                    output.stats.functions_eliminated
                );
                println!(
                    "  Instructions processed: {}",
                    output.stats.instructions_processed
                );
                println!("  Type inferences: {}", output.stats.type_inferences);
                println!("  Dynamic fallbacks: {}", output.stats.dynamic_fallbacks);
                println!(
                    "  Optimizations applied: {}",
                    output.stats.optimizations_applied
                );
            }

            // Show warnings if any
            if !output.warnings.is_empty() {
                println!();
                println!("Warnings:");
                for warning in &output.warnings {
                    println!("  - {}", warning);
                }
            }
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            process::exit(1);
        }
    }
}

/// Compile Julia source code to Rust code
fn compile_julia_to_rust(
    source: &str,
    source_name: &str,
    emit_comments: bool,
    pure_rust: bool,
    minimal_prelude: bool,
) -> Result<AotOutput, AotError> {
    let mut stats = AotStats::new();

    // Parse source
    let mut parser = Parser::new()
        .map_err(|e| AotError::InternalError(format!("Failed to create parser: {:?}", e)))?;

    // Parse and lower prelude (base functions)
    // Use minimal prelude for pure Rust code generation
    let prelude_src = if minimal_prelude {
        base::get_aot_prelude()
    } else {
        base::get_prelude()
    };
    let prelude_outcome = parser
        .parse(&prelude_src)
        .map_err(|e| AotError::InternalError(format!("Failed to parse prelude: {:?}", e)))?;
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_outcome)
        .map_err(|e| AotError::InternalError(format!("Prelude lowering error: {:?}", e)))?;

    // Parse user source
    let outcome = parser
        .parse(source)
        .map_err(|e| AotError::InternalError(format!("Parse error: {:?}", e)))?;

    // Lower to Core IR
    let mut lowering = Lowering::new(source);
    let mut program = lowering
        .lower(outcome)
        .map_err(|e| AotError::InternalError(format!("Lowering error: {:?}", e)))?;

    // Merge prelude with user program
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();

    // Merge structs (prelude first, skip if user defines same name)
    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.append(&mut program.structs);
    program.structs = all_structs;

    // Merge abstract types
    let user_abstract_names: HashSet<_> = program
        .abstract_types
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    let mut all_abstract_types: Vec<_> = prelude_program
        .abstract_types
        .into_iter()
        .filter(|a| !user_abstract_names.contains(a.name.as_str()))
        .collect();
    all_abstract_types.append(&mut program.abstract_types);
    program.abstract_types = all_abstract_types;

    // Merge functions (prelude first, skip if user defines same name)
    // Mark prelude functions as base extensions so they can be filtered out later
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

    // Load external modules if needed
    let existing_modules: HashSet<String> =
        program.modules.iter().map(|m| m.name.clone()).collect();
    let usings_to_load: Vec<_> = program
        .usings
        .iter()
        .filter(|u| !u.is_relative && !existing_modules.contains(&u.module))
        .cloned()
        .collect();

    if !usings_to_load.is_empty() {
        let mut package_loader = loader::PackageLoader::new(loader::LoaderConfig::from_env());
        let loaded_modules = package_loader
            .load_for_usings(&usings_to_load)
            .map_err(|e| AotError::InternalError(format!("Load error: {:?}", e)))?;

        for module in loaded_modules {
            if !existing_modules.contains(&module.name) {
                program.modules.push(module);
            }
        }
    }

    // Dead Code Elimination: filter program to only include reachable functions
    stats.functions_total = program.functions.len();
    let call_graph = CallGraph::from_program(&program);
    let program = call_graph.filter_program(&program);
    stats.functions_eliminated = stats.functions_total - program.functions.len();

    // Type inference
    let mut type_engine = TypeInferenceEngine::new();
    let typed_program = type_engine.analyze_program(&program)?;

    stats.functions_compiled = program.functions.len();
    stats.type_inferences = typed_program.function_count();

    // Convert Core IR to AoT IR
    let mut aot_program = program_to_aot_ir(&program, &typed_program)?;

    stats.instructions_processed = aot_program.instruction_count();

    // Run optimizations
    stats.optimizations_applied = optimize_aot_program_full(&mut aot_program);

    // Count dynamic fallbacks before generating code
    let dynamic_count = aot_program.count_dynamic_calls();

    // In pure Rust mode, fail if any dynamic dispatch is needed
    if pure_rust && dynamic_count > 0 {
        return Err(generate_pure_rust_error(&aot_program, dynamic_count));
    }

    // Generate Rust code
    let config = CodegenConfig {
        emit_comments,
        pure_rust,
        ..CodegenConfig::default()
    };
    let mut codegen = AotCodeGenerator::new(config);
    let rust_code = codegen.generate_program(&aot_program)?;

    let mut output = AotOutput::new(rust_code, stats);

    // Add source information as a comment
    if emit_comments {
        let header = format!(
            "// Source: {}\n// Generated by SubsetJuliaVM AoT Compiler v{}\n\n",
            source_name, VERSION
        );
        output.rust_code = header + &output.rust_code;
    }

    // Count dynamic fallbacks
    output.stats.dynamic_fallbacks = dynamic_count;

    // Add warnings for dynamic fallbacks
    if output.stats.dynamic_fallbacks > 0 {
        output.add_warning(format!(
            "{} function calls will use dynamic dispatch at runtime",
            output.stats.dynamic_fallbacks
        ));
    }

    Ok(output)
}

/// Compile bytecode file (.sjbc) to Rust code
fn compile_bytecode_to_rust(
    bytecode_path: &str,
    source_name: &str,
    emit_comments: bool,
    pure_rust: bool,
    _minimal_prelude: bool,
) -> Result<AotOutput, AotError> {
    let mut stats = AotStats::new();

    // Check file exists
    if !Path::new(bytecode_path).exists() {
        return Err(AotError::InternalError(format!(
            "Bytecode file '{}' not found",
            bytecode_path
        )));
    }

    // Load bytecode file
    let program = bytecode::load(bytecode_path)
        .map_err(|e| AotError::InternalError(format!("Failed to load bytecode: {}", e)))?;

    // Compile the program
    compile_program_to_rust(program, source_name, emit_comments, pure_rust, &mut stats)
}

/// Internal: compile a Core IR Program to Rust code
fn compile_program_to_rust(
    program: Program,
    source_name: &str,
    emit_comments: bool,
    pure_rust: bool,
    stats: &mut AotStats,
) -> Result<AotOutput, AotError> {
    // Dead Code Elimination: filter program to only include reachable functions
    stats.functions_total = program.functions.len();
    let call_graph = CallGraph::from_program(&program);
    let program = call_graph.filter_program(&program);
    stats.functions_eliminated = stats.functions_total - program.functions.len();

    // Type inference
    let mut type_engine = TypeInferenceEngine::new();
    let typed_program = type_engine.analyze_program(&program)?;

    stats.functions_compiled = program.functions.len();
    stats.type_inferences = typed_program.function_count();

    // Convert Core IR to AoT IR
    let mut aot_program = program_to_aot_ir(&program, &typed_program)?;

    stats.instructions_processed = aot_program.instruction_count();

    // Run optimizations
    stats.optimizations_applied = optimize_aot_program_full(&mut aot_program);

    // Count dynamic fallbacks before generating code
    let dynamic_count = aot_program.count_dynamic_calls();

    // In pure Rust mode, fail if any dynamic dispatch is needed
    if pure_rust && dynamic_count > 0 {
        return Err(generate_pure_rust_error(&aot_program, dynamic_count));
    }

    // Generate Rust code
    let config = CodegenConfig {
        emit_comments,
        pure_rust,
        ..CodegenConfig::default()
    };
    let mut codegen = AotCodeGenerator::new(config);
    let rust_code = codegen.generate_program(&aot_program)?;

    let mut output = AotOutput::new(rust_code, stats.clone());

    // Add source information as a comment
    if emit_comments {
        let header = format!(
            "// Source: {}\n// Generated by SubsetJuliaVM AoT Compiler v{}\n\n",
            source_name, VERSION
        );
        output.rust_code = header + &output.rust_code;
    }

    // Count dynamic fallbacks
    output.stats.dynamic_fallbacks = dynamic_count;

    // Add warnings for dynamic fallbacks
    if output.stats.dynamic_fallbacks > 0 {
        output.add_warning(format!(
            "{} function calls will use dynamic dispatch at runtime",
            output.stats.dynamic_fallbacks
        ));
    }

    Ok(output)
}

/// Generate a detailed error message for pure Rust mode failures
fn generate_pure_rust_error(aot_program: &AotProgram, dynamic_count: usize) -> AotError {
    let diagnostics = aot_program.diagnose_dynamic_operations();
    let mut error_msg = format!(
        "Pure Rust mode requires fully static types, but {} dynamic operation(s) were found.\n\n",
        dynamic_count
    );

    if !diagnostics.is_empty() {
        error_msg.push_str("Dynamic operations detected:\n");
        error_msg.push_str("─".repeat(60).as_str());
        error_msg.push('\n');

        for (i, diag) in diagnostics.iter().enumerate() {
            if i > 0 {
                error_msg.push('\n');
            }
            error_msg.push_str(&format!("{}. {}\n", i + 1, diag));
        }

        error_msg.push_str("─".repeat(60).as_str());
        error_msg.push_str("\n\n");
    }

    error_msg.push_str("To fix this:\n");
    error_msg.push_str(
        "  1. Add explicit type annotations to all function parameters and return types\n",
    );
    error_msg.push_str("  2. Use typed local variables (e.g., x::Float64 = 1.0)\n");
    error_msg.push_str("  3. Replace broadcasting operators (.+, .*, etc.) with explicit loops\n");
    error_msg.push_str("  4. Avoid operations that require runtime type dispatch\n");

    AotError::CodegenError(error_msg)
}
