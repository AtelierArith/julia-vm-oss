//! CLI integration tests for persisted VM bytecode execution.

use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn sjulia_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sjulia")
}

fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed (status={:?})\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn help_distinguishes_core_ir_and_vm_bytecode_files() {
    let output = Command::new(sjulia_bin())
        .arg("--help")
        .output()
        .expect("spawn sjulia --help");
    assert_success(&output, "sjulia --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Compile to Core IR file (.sjir)"),
        "help should describe .sjir as Core IR, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Execute Core IR file"),
        "help should describe --run-ir as Core IR execution, got:\n{stdout}"
    );
    assert!(
        stdout.contains("--run-ir") && !stdout.contains("--run-bytecode"),
        "help should expose --run-ir without the old --run-bytecode spelling, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Compile to VM bytecode file (.sjvmbc)"),
        "help should describe .sjvmbc as VM bytecode, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("Compile to bytecode file (.sjir)")
            && !stdout.contains("Execute bytecode file")
            && !stdout.contains(".sjbc"),
        "help should not call .sjir a generic bytecode file, got:\n{stdout}"
    );
}

#[test]
fn compile_core_ir_runs_explicitly_and_by_extension() {
    let dir = tempdir().expect("create temp dir");
    let source_path = dir.path().join("program.jl");
    let ir_path = dir.path().join("program.sjir");
    let old_ir_path = dir.path().join("program.sjbc");

    fs::write(
        &source_path,
        r#"
function add_two(x)
    x + 2
end

println(add_two(40))
"#,
    )
    .expect("write source");

    let compile_output = Command::new(sjulia_bin())
        .current_dir(dir.path())
        .args(["--compile", "program.jl"])
        .output()
        .expect("spawn sjulia --compile");
    assert_success(&compile_output, "sjulia --compile");
    assert!(ir_path.exists(), "Core IR file should be created");
    assert!(
        !old_ir_path.exists(),
        "old .sjbc Core IR file should not be created"
    );

    let explicit_output = Command::new(sjulia_bin())
        .current_dir(dir.path())
        .args(["--run-ir", "program.sjir"])
        .output()
        .expect("spawn sjulia --run-ir");
    assert_success(&explicit_output, "sjulia --run-ir");
    assert_eq!(
        String::from_utf8_lossy(&explicit_output.stdout).trim(),
        "42"
    );

    let extension_output = Command::new(sjulia_bin())
        .current_dir(dir.path())
        .arg("program.sjir")
        .output()
        .expect("spawn sjulia program.sjir");
    assert_success(&extension_output, "sjulia program.sjir");
    assert_eq!(
        String::from_utf8_lossy(&extension_output.stdout).trim(),
        "42"
    );
}

#[test]
fn compile_vm_bytecode_runs_explicitly_and_by_extension() {
    let dir = tempdir().expect("create temp dir");
    let source_path = dir.path().join("program.jl");
    let bytecode_path = dir.path().join("program.sjvmbc");

    fs::write(
        &source_path,
        r#"
function add_two(x)
    x + 2
end

println(add_two(40))
"#,
    )
    .expect("write source");

    let compile_output = Command::new(sjulia_bin())
        .args([
            "--compile-vm",
            source_path.to_str().expect("utf-8 source path"),
            "-o",
            bytecode_path.to_str().expect("utf-8 bytecode path"),
        ])
        .output()
        .expect("spawn sjulia --compile-vm");
    assert_success(&compile_output, "sjulia --compile-vm");
    assert!(bytecode_path.exists(), "VM bytecode file should be created");

    let explicit_output = Command::new(sjulia_bin())
        .args([
            "--run-vm-bytecode",
            bytecode_path.to_str().expect("utf-8 bytecode path"),
        ])
        .output()
        .expect("spawn sjulia --run-vm-bytecode");
    assert_success(&explicit_output, "sjulia --run-vm-bytecode");
    assert_eq!(
        String::from_utf8_lossy(&explicit_output.stdout).trim(),
        "42"
    );

    let extension_output = Command::new(sjulia_bin())
        .arg(bytecode_path.to_str().expect("utf-8 bytecode path"))
        .output()
        .expect("spawn sjulia program.sjvmbc");
    assert_success(&extension_output, "sjulia program.sjvmbc");
    assert_eq!(
        String::from_utf8_lossy(&extension_output.stdout).trim(),
        "42"
    );
}
