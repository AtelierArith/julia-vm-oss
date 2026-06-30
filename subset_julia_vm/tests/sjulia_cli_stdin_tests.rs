//! CLI integration tests for the `sjulia` binary.
//!
//! These tests spawn the compiled `sjulia` binary as a subprocess so they can
//! exercise the argv-and-stdin dispatch logic the same way an end-user would.
//!
//! Issue #3560: `echo 'println("hi")' | sjulia` must execute the piped code
//! as a script (matching official `julia`), not start the interactive REPL.

use std::io::Write;
use std::process::{Command, Stdio};

/// Path to the freshly-built `sjulia` binary. Cargo populates this env var for
/// integration tests of crates that declare a `[[bin]]` named `sjulia`.
fn sjulia_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sjulia")
}

/// Spawn `sjulia` with no arguments, write `code` to its stdin, close stdin,
/// and return the captured `(stdout, stderr, exit_status)`.
fn run_with_piped_stdin(args: &[&str], code: &str) -> (String, String, std::process::ExitStatus) {
    let mut child = Command::new(sjulia_bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn sjulia");

    {
        let stdin = child.stdin.as_mut().expect("child stdin not piped");
        stdin
            .write_all(code.as_bytes())
            .expect("failed to write to child stdin");
    }
    // Dropping stdin (by ending the scope) closes the pipe so sjulia sees EOF.

    let output = child
        .wait_with_output()
        .expect("failed to wait on child sjulia");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (stdout, stderr, output.status)
}

/// Strip ANSI color escape sequences so assertions are robust to the REPL
/// banner / highlight colors that sjulia emits.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip CSI sequence: ESC [ ... letter
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[test]
fn piped_stdin_executes_as_script_not_repl() {
    // Regression test for Issue #3560: when stdin is not a TTY (here: a pipe),
    // sjulia should execute the piped content as a Julia script and exit,
    // matching official `julia`'s behavior. Previously it printed the REPL
    // banner and dropped into interactive mode.
    let (stdout, stderr, status) =
        run_with_piped_stdin(&[], "println(\"hi\")\nprintln(\"world\")\n");

    let stdout_clean = strip_ansi(&stdout);
    let stderr_clean = strip_ansi(&stderr);

    assert!(
        status.success(),
        "sjulia should exit cleanly when fed a stdin script. \
         status={:?}\nstdout=<<<{}>>>\nstderr=<<<{}>>>",
        status,
        stdout_clean,
        stderr_clean
    );

    // Must execute the script: both prints reach stdout in order.
    assert!(
        stdout_clean.contains("hi"),
        "expected 'hi' from piped script in stdout, got: <<<{}>>>",
        stdout_clean
    );
    assert!(
        stdout_clean.contains("world"),
        "expected 'world' from piped script in stdout, got: <<<{}>>>",
        stdout_clean
    );
    let hi_idx = stdout_clean.find("hi").expect("hi present");
    let world_idx = stdout_clean.find("world").expect("world present");
    assert!(
        hi_idx < world_idx,
        "expected 'hi' before 'world' in stdout, got: <<<{}>>>",
        stdout_clean
    );

    // Must NOT print the REPL banner / prompt artifacts.
    assert!(
        !stdout_clean.contains("Julia Subset REPL"),
        "REPL banner leaked into stdout for piped script: <<<{}>>>",
        stdout_clean
    );
    assert!(
        !stdout_clean.contains("julia>"),
        "REPL prompt leaked into stdout for piped script: <<<{}>>>",
        stdout_clean
    );
}

#[test]
fn dash_argument_reads_script_from_stdin() {
    // Julia parity: `sjulia -` reads a script from stdin even when stdin is
    // a TTY (and certainly when it's a pipe). This guards the explicit form.
    let (stdout, _stderr, status) = run_with_piped_stdin(&["-"], "println(1 + 2)\n");
    let stdout_clean = strip_ansi(&stdout);

    assert!(
        status.success(),
        "sjulia - should execute stdin script cleanly, got status={:?}",
        status
    );
    assert!(
        stdout_clean.trim().contains('3'),
        "expected '3' from `sjulia -` piped expression, got: <<<{}>>>",
        stdout_clean
    );
    assert!(
        !stdout_clean.contains("Julia Subset REPL"),
        "REPL banner leaked into `sjulia -` output: <<<{}>>>",
        stdout_clean
    );
}

#[test]
fn dash_e_flag_still_works() {
    // Sanity check that the existing `-e <code>` path is untouched by the
    // stdin-dispatch change.
    let output = Command::new(sjulia_bin())
        .args(["-e", "println(\"from -e\")"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run sjulia -e");

    assert!(output.status.success(), "sjulia -e should exit cleanly");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        strip_ansi(&stdout).contains("from -e"),
        "expected 'from -e' from sjulia -e, got: <<<{}>>>",
        stdout
    );
}
