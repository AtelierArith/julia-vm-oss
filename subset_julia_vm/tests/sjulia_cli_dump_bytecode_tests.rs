//! CLI integration tests for `sjulia --dump-bytecode`.

use std::process::{Command, Stdio};

/// Path to the freshly-built `sjulia` binary. Cargo populates this env var for
/// integration tests of crates that declare a `[[bin]]` named `sjulia`.
fn sjulia_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sjulia")
}

/// Dump the user-function bytecode (no `--all`, so Base is excluded) for `code`.
fn dump_user_bytecode(code: &str) -> String {
    let output = Command::new(sjulia_bin())
        .args(["--dump-bytecode", "-e", code])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to spawn sjulia --dump-bytecode");
    assert!(
        output.status.success(),
        "sjulia --dump-bytecode failed; status={:?}, stderr=<<<{}>>>",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn return_and_local_type_annotations_elide_noop_convert_issue_8147() {
    // Issue #8147: `::Int` return annotations and `tmp::Int = b` locals lowered
    // to `convert(Int, x)` calls that emitted `CallBuiltin(Convert, 2)` on the
    // hot path even when `x` was already `Int64`, collapsing the slot type to
    // unknown and costing a method search + dispatch per execution. When the
    // value is already the concrete target type the convert must be elided.
    let dump = dump_user_bytecode(
        "function f(a::Int, b::Int)::Int\n    tmp::Int = b\n    a + tmp\nend\nprintln(f(2, 3))\n",
    );
    assert!(
        dump.contains("f(a::I64, b::I64) -> I64"),
        "function should compile with concrete I64 slots, got:\n{dump}"
    );
    assert!(
        !dump.contains("Convert"),
        "no-op convert(Int, x::Int) must be elided from the hot path, got:\n{dump}"
    );
    // The typed local must stay specialized as an I64 slot, not collapse to Any.
    assert!(
        dump.contains("tmp") && dump.contains("StoreSlotI64"),
        "typed local `tmp::Int` should remain an I64 slot, got:\n{dump}"
    );
}

#[test]
fn mixed_int_float_scalar_ops_specialize_to_typed_no_dispatch_issue_8183() {
    // Issue #8183: mixed-type primitive scalar arithmetic/comparison (e.g.
    // `Int64 / Float64`) compiled to a dynamic method `Call` to the Base
    // operator on every execution instead of promoting the integer operand to
    // Float64 and using the typed F64 intrinsic. Julia's promotion of an
    // integer-and-float pair is exactly `Float64(int) op float`, so the typed
    // `…ToF64; <op>F64` form is output-identical and avoids a per-execution
    // method search on the hot path (and unblocks native typed-loop recognition
    // for `executable.rs`, where an opaque `Call` aborts the recognizer).
    // `op_sym argc` is the dump comment a dynamic method Call to the operator
    // leaves behind (e.g. `; call #1005 / argc=2`); the typed path emits no such
    // call. The user-program `f(7, 2.0)` call shows up as `f argc=2`, so an
    // operator-specific needle keeps the negative assertion precise.
    for (expr, op_sym, typed_op) in [
        ("a / b", "/", "DivF64"),
        ("a * b", "*", "MulF64"),
        ("a + b", "+", "AddF64"),
        ("a - b", "-", "SubF64"),
    ] {
        let dump = dump_user_bytecode(&format!(
            "function f(a::Int64, b::Float64)\n    {expr}\nend\nprintln(f(7, 2.0))\n"
        ));
        assert!(
            dump.contains(typed_op),
            "mixed Int64/Float64 `{expr}` must use the typed {typed_op} op, got:\n{dump}"
        );
        assert!(
            dump.contains("ToF64"),
            "the Int64 operand of `{expr}` must be promoted to Float64 (…ToF64), got:\n{dump}"
        );
        assert!(
            !dump.contains(&format!("{op_sym} argc")),
            "mixed Int64/Float64 `{expr}` must not emit a dynamic method Call to `{op_sym}`, got:\n{dump}"
        );
    }

    // Comparisons are deliberately NOT specialized this way: `==`/`<` between an
    // integer and a float must keep Julia's exact semantics (e.g.
    // `9007199254740993 < 9.0e15`), which a naive promote-to-Float64 would break
    // for integers beyond 2^53. They stay on the exact dispatch path. The
    // benchmark loops only compare same-typed operands (I64/I64, F64/F64), which
    // already use typed ops, so excluding mixed comparison costs no perf here.
}

#[test]
fn dump_bytecode_tolerates_closed_stdout_issue_6254() {
    let mut child = Command::new(sjulia_bin())
        .args([
            "--dump-bytecode",
            "--all",
            "-e",
            "z = 1.0 + 2.0im; println(abs2(z)); println(z*z)",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn sjulia --dump-bytecode");

    drop(child.stdout.take());

    let output = child
        .wait_with_output()
        .expect("failed to wait on sjulia --dump-bytecode");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "sjulia --dump-bytecode should exit cleanly on closed stdout; status={:?}, stderr=<<<{}>>>",
        output.status,
        stderr
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("Broken pipe"),
        "sjulia --dump-bytecode must not panic on closed stdout; stderr=<<<{}>>>",
        stderr
    );
}
