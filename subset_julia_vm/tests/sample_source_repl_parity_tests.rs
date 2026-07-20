//! Source-vs-REPL parity for bundled iOS/Web code samples (Issue #9156).
//!
//! An iOS sample must behave identically whether the user runs it as one program
//! (Editor tab = a single `eval` of the whole file) or line-by-line (REPL tab =
//! one `eval` per top-level expression on a persistent session). The two paths
//! share the same VM but differ in how global state crosses evaluation
//! boundaries, which is exactly where Issue #9156 lived: `@variables x y` bound
//! `x`/`y` fine within one program, but the REPL failed to persist them to the
//! next line, so `A = [x y; x x]` raised `UndefVarError`.
//!
//! This harness runs each allow-listed sample BOTH ways through `REPLSession` and
//! asserts the accumulated stdout is byte-identical. It is a self-checking
//! prevention mechanism: any future regression that makes a global (or function,
//! struct, macro binding, …) visible in one mode but not the other re-breaks
//! parity and fails here — no hand-authored expected output to drift.
//!
//! To extend coverage, add a sample path to `PARITY_SAMPLES`. A sample only
//! belongs here if it is deterministic (no RNG/time) and does not rely on a
//! separately-tracked REPL divergence (e.g. top-level `begin`-block globals,
//! Issue #9157). Samples that legitimately differ must be fixed or tracked, never
//! silently dropped.

use std::path::PathBuf;

use subset_julia_vm::repl::REPLSession;

/// Samples asserted to produce identical output as a script and in the REPL.
/// Paths are relative to this crate's manifest dir (`subset_julia_vm/`).
const PARITY_SAMPLES: &[&str] = &[
    "../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_linear_algebra.jl",
    "../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_package.jl",
];

fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let handler = std::thread::Builder::new()
        .name("sample-parity".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .unwrap();
    if let Err(e) = handler.join() {
        std::panic::resume_unwind(e);
    }
}

fn sample_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Editor / script mode: the whole file is one evaluation, exactly like
/// `sjulia file.jl`.
fn run_as_script(src: &str) -> String {
    let mut session = REPLSession::new(0);
    let result = session.eval(src);
    assert!(
        result.success,
        "script-mode evaluation failed: {:?}",
        result.error
    );
    result.output
}

/// REPL mode: split into top-level expressions and evaluate each on the same
/// persistent session, mirroring how the iOS REPL tab drives the session.
fn run_as_repl(src: &str) -> String {
    let mut session = REPLSession::new(0);
    let pieces: Vec<String> = match session.split_expressions(src) {
        Some(exprs) => exprs.into_iter().map(|(_, _, text)| text).collect(),
        // A single top-level expression: run the whole thing as one step.
        None => vec![src.to_string()],
    };

    let mut output = String::new();
    for piece in pieces {
        if piece.trim().is_empty() {
            continue;
        }
        let result = session.eval(&piece);
        assert!(
            result.success,
            "repl-mode evaluation failed on step {piece:?}: {:?}",
            result.error
        );
        output.push_str(&result.output);
    }
    output
}

#[test]
fn samples_produce_identical_output_as_script_and_repl_9156() {
    run_with_large_stack(|| {
        for rel in PARITY_SAMPLES {
            let path = sample_path(rel);
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read sample {}: {e}", path.display()));

            let script_output = run_as_script(&src);
            let repl_output = run_as_repl(&src);

            assert_eq!(
                script_output, repl_output,
                "sample {rel} diverged between script mode and REPL mode; \
                 a binding created in one mode is not visible in the other \
                 (Issue #9156 class of bug)"
            );
        }
    });
}
