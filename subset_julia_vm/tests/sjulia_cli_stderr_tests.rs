//! CLI integration tests for stderr routing (Issue #3573).
//!
//! `print(stderr, ...)` and `println(stderr, ...)` must reach the *real*
//! stderr stream of the `sjulia` process rather than being dumped on stdout
//! or discarded. These tests exercise the end-to-end pipeline by spawning
//! the freshly-built `sjulia` binary as a subprocess and asserting on the
//! captured stdout/stderr separately.
//!
//! Companion to `sjulia_cli_stdin_tests.rs`.

use std::process::Command;

/// Path to the freshly-built `sjulia` binary. Cargo populates this env var
/// for integration tests of crates that declare a `[[bin]]` named `sjulia`.
fn sjulia_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sjulia")
}

/// Run `sjulia -e <code>` and return `(stdout, stderr)` as owned `String`s.
fn run_eval(code: &str) -> (String, String) {
    let output = Command::new(sjulia_bin())
        .args(["-e", code])
        .output()
        .expect("failed to spawn sjulia for stderr test");
    assert!(
        output.status.success(),
        "sjulia -e exited with non-zero status for code {:?}\nstdout=<<<{}>>>\nstderr=<<<{}>>>",
        code,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (stdout, stderr)
}

#[test]
fn print_stderr_writes_to_actual_stderr() {
    // MWE from Issue #3573: `print(stderr, "warning")` must reach the real
    // stderr stream — not stdout, not /dev/null, not silently dropped.
    let (stdout, stderr) = run_eval(r#"print(stderr, "warning")"#);
    assert_eq!(
        stdout, "",
        "print(stderr, ...) must not leak onto stdout, got stdout=<<<{}>>>",
        stdout
    );
    assert!(
        stderr.contains("warning"),
        "print(stderr, \"warning\") should appear on stderr, got stderr=<<<{}>>>",
        stderr
    );
}

#[test]
fn println_stderr_writes_to_actual_stderr_with_newline() {
    // `println(stderr, "warning")` must write "warning\n" to the real stderr
    // and nothing to stdout. Before the fix this dumped `IOBuffer(...)warning`
    // onto stdout because the compiler's `println` form ignored the IO arg.
    let (stdout, stderr) = run_eval(r#"println(stderr, "warning")"#);
    assert_eq!(
        stdout, "",
        "println(stderr, ...) must not leak onto stdout, got stdout=<<<{}>>>",
        stdout
    );
    assert_eq!(
        stderr, "warning\n",
        "println(stderr, \"warning\") should write exactly \"warning\\n\" to stderr, got stderr=<<<{}>>>",
        stderr
    );
}

#[test]
fn println_stderr_no_extra_args_writes_only_newline() {
    // `println(stderr)` (no message) is the empty-line form: just "\n" to
    // stderr. Matches official Julia.
    let (stdout, stderr) = run_eval(r#"println(stderr)"#);
    assert_eq!(stdout, "", "println(stderr) must not write to stdout");
    assert_eq!(
        stderr, "\n",
        "println(stderr) should write a single newline to stderr, got stderr=<<<{}>>>",
        stderr
    );
}

#[test]
fn print_stderr_multiple_args_concatenate() {
    // `print(stderr, "a", 1, "b")` writes "a1b" to stderr (no separator,
    // no trailing newline) — same as Julia.
    let (stdout, stderr) = run_eval(r#"print(stderr, "a", 1, "b")"#);
    assert_eq!(stdout, "");
    assert_eq!(
        stderr, "a1b",
        "print(stderr, \"a\", 1, \"b\") should produce \"a1b\" on stderr, got stderr=<<<{}>>>",
        stderr
    );
}

#[test]
fn println_stderr_multiple_args_with_newline() {
    let (stdout, stderr) = run_eval(r#"println(stderr, "a", 1, "b")"#);
    assert_eq!(stdout, "");
    assert_eq!(
        stderr, "a1b\n",
        "println(stderr, \"a\", 1, \"b\") should produce \"a1b\\n\" on stderr, got stderr=<<<{}>>>",
        stderr
    );
}

#[test]
fn stdout_and_stderr_routes_independent() {
    // Mixed routing: stdout writes go to real stdout, stderr writes go to
    // real stderr, in the order issued.
    let (stdout, stderr) =
        run_eval(r#"println("o1"); println(stderr, "e1"); println("o2"); println(stderr, "e2")"#);
    assert_eq!(
        stdout, "o1\no2\n",
        "stdout should contain only the stdout writes, got stdout=<<<{}>>>",
        stdout
    );
    assert_eq!(
        stderr, "e1\ne2\n",
        "stderr should contain only the stderr writes, got stderr=<<<{}>>>",
        stderr
    );
}

#[test]
fn println_to_iobuffer_writes_into_buffer_not_stdout() {
    // The same compiler path that broke `println(stderr, ...)` also broke
    // `println(io::IOBuffer, ...)` — the IO value was being dumped onto
    // stdout instead of appended to the buffer. Verify the round-trip works.
    let (stdout, stderr) = run_eval(
        r#"
io = IOBuffer()
println(io, "first")
println(io, "second")
result = String(take!(io))
print(result == "first\nsecond\n" ? "OK" : "FAIL: $(repr(result))")
"#,
    );
    assert_eq!(
        stderr, "",
        "no stderr output expected, got <<<{}>>>",
        stderr
    );
    assert_eq!(
        stdout.trim_end(),
        "OK",
        "println(::IOBuffer, ...) should append to the buffer; got stdout=<<<{}>>>",
        stdout
    );
}

#[test]
fn println_to_devnull_discards_output() {
    // /dev/null still discards output through the IO-routed println path.
    let (stdout, stderr) = run_eval(r#"println(devnull, "should-not-appear"); print("after")"#);
    assert_eq!(stderr, "");
    assert_eq!(
        stdout, "after",
        "devnull writes must be discarded, got stdout=<<<{}>>>",
        stdout
    );
}
