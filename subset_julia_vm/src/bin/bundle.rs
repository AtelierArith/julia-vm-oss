#![deny(clippy::expect_used)]
//! Bundle Julia code into a standalone executable
//!
//! This tool generates a Rust source file that embeds the user's Julia code
//! and can be compiled into a standalone native executable.
//!
//! Usage:
//!   cargo run --bin bundle --features tempfile -- input.jl -o my_app
//!   # This generates: my_app (native executable)

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use tempfile::TempDir;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: bundle <input.jl> [-o output_name]");
        eprintln!("       bundle --ir <input.jl>  # Output IR JSON only");
        std::process::exit(1);
    }

    // Parse arguments
    let mut input_file = None;
    let mut output_name = String::from("julia_app");
    let mut ir_only = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                if i < args.len() {
                    output_name = args[i].clone();
                }
            }
            "--ir" => {
                ir_only = true;
            }
            arg if !arg.starts_with('-') => {
                input_file = Some(arg.to_string());
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let input_file = input_file.unwrap_or_else(|| {
        eprintln!("Error: input file required");
        std::process::exit(1);
    });

    // Read Julia source
    let source = fs::read_to_string(&input_file).unwrap_or_else(|e| {
        eprintln!("Error: failed to read input file '{}': {}", input_file, e);
        std::process::exit(1);
    });

    // Parse and lower to IR
    let mut parser = Parser::new().unwrap_or_else(|e| {
        eprintln!("Error: failed to create parser: {}", e);
        std::process::exit(1);
    });
    let outcome = parser.parse(&source).unwrap_or_else(|e| {
        eprintln!("Error: parse error: {}", e);
        std::process::exit(1);
    });

    let mut lowering = Lowering::new(&source);
    let program = lowering.lower(outcome).unwrap_or_else(|e| {
        eprintln!("Error: lowering error: {}", e);
        std::process::exit(1);
    });

    let ir_json = serde_json::to_string(&program).unwrap_or_else(|e| {
        eprintln!("Error: JSON serialization failed: {}", e);
        std::process::exit(1);
    });

    if ir_only {
        // Just output IR JSON
        println!("{}", ir_json);
        return;
    }

    // Generate standalone runner source
    let runner_source = generate_runner_source(&ir_json, &source);

    // Create temporary directory for build
    let temp_dir = TempDir::new().unwrap_or_else(|e| {
        eprintln!("Error: failed to create temp dir: {}", e);
        std::process::exit(1);
    });
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap_or_else(|e| {
        eprintln!("Error: failed to create src dir: {}", e);
        std::process::exit(1);
    });

    // Write Cargo.toml
    let cargo_toml = generate_cargo_toml(&output_name);
    fs::write(temp_dir.path().join("Cargo.toml"), cargo_toml).unwrap_or_else(|e| {
        eprintln!("Error: failed to write Cargo.toml: {}", e);
        std::process::exit(1);
    });

    // Write main.rs
    fs::write(src_dir.join("main.rs"), runner_source).unwrap_or_else(|e| {
        eprintln!("Error: failed to write main.rs: {}", e);
        std::process::exit(1);
    });

    // Build the executable
    println!("Building standalone executable...");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(temp_dir.path())
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to run cargo build: {}", e);
            std::process::exit(1);
        });

    if !status.success() {
        eprintln!("Build failed");
        std::process::exit(1);
    }

    // Copy the executable to output location
    let built_exe = temp_dir
        .path()
        .join("target")
        .join("release")
        .join(&output_name);

    let output_path = Path::new(&output_name);
    fs::copy(&built_exe, output_path).unwrap_or_else(|e| {
        eprintln!("Error: failed to copy executable: {}", e);
        std::process::exit(1);
    });

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(output_path)
            .unwrap_or_else(|e| {
                eprintln!("Error: failed to read metadata for '{}': {}", output_name, e);
                std::process::exit(1);
            })
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(output_path, perms).unwrap_or_else(|e| {
            eprintln!("Error: failed to set permissions for '{}': {}", output_name, e);
            std::process::exit(1);
        });
    }

    println!("Created: {}", output_name);
    println!("Run with: ./{}", output_name);
}

fn generate_runner_source(ir_json: &str, _original_source: &str) -> String {
    // Escape the IR JSON for embedding in Rust source
    let escaped_ir = ir_json
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");

    format!(
        r##"//! Auto-generated standalone Julia program runner
//! Generated by: subset_julia_vm bundle

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

const IR_JSON: &str = "{}";

fn main() {{
    // Parse embedded IR
    let program: Program = serde_json::from_str(IR_JSON)
        .unwrap_or_else(|e| {{
            eprintln!("Error: failed to parse embedded IR: {{}}", e);
            std::process::exit(1);
        }});

    // Compile
    let compiled = compile_core_program(&program)
        .unwrap_or_else(|e| {{
            eprintln!("Error: compilation failed: {{}}", e);
            std::process::exit(1);
        }});

    // Execute
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(42);

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {{
        Ok(_value) => {{
            // Print output
            let output = vm.get_output();
            if !output.is_empty() {{
                print!("{{}}", output);
            }}
        }}
        Err(e) => {{
            eprintln!("Runtime error: {{}}", e);
            std::process::exit(1);
        }}
    }}
}}
"##,
        escaped_ir
    )
}

fn generate_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
subset_julia_vm = {{ path = "{}" }}
serde_json = "1.0"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
"#,
        name,
        env!("CARGO_MANIFEST_DIR")
    )
}
