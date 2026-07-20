//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod sjulia_cli_cache_status_tests {
    //! CLI regression tests for cache-status observability (Issue #8718).

    use std::process::Command;

    fn sjulia_bin() -> &'static str {
        env!("CARGO_BIN_EXE_sjulia")
    }

    #[test]
    fn cache_status_reports_sources_and_fingerprints_8718() {
        let output = Command::new(sjulia_bin())
            .arg("--cache-status")
            .output()
            .expect("spawn sjulia --cache-status");
        assert!(
            output.status.success(),
            "sjulia --cache-status failed (status={:?})\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("cache-status should be JSON");
        assert_eq!(value["crate_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            value["compiler_build_fingerprint"]
                .as_str()
                .expect("compiler fingerprint"),
            env!("SJULIA_BASE_CACHE_BUILD_HASH")
        );

        for cache_name in ["base_cache", "prelude_program_cache"] {
            let cache = &value[cache_name];
            assert!(
                matches!(
                    cache["load_source"].as_str(),
                    Some("embedded" | "persistent" | "none")
                ),
                "{cache_name} should report load_source, got {cache}"
            );
            assert!(
                cache["fingerprints"].is_object(),
                "{cache_name} should report fingerprints, got {cache}"
            );
            assert!(
                cache["persistent"]["state"].is_string(),
                "{cache_name} should report persistent state, got {cache}"
            );
            assert!(
                cache["embedded"]["state"].is_string(),
                "{cache_name} should report embedded state, got {cache}"
            );
        }
    }

    #[test]
    fn version_flags_report_current_repl_package_version_11813() {
        let expected = format!(
            "sjulia — SubsetJuliaVM REPL v{}\n",
            env!("CARGO_PKG_VERSION")
        );

        for flag in ["-v", "--version"] {
            let output = Command::new(sjulia_bin())
                .arg(flag)
                .output()
                .unwrap_or_else(|err| panic!("spawn sjulia {flag}: {err}"));
            assert!(
                output.status.success(),
                "sjulia {flag} failed (status={:?})\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
            assert!(output.stderr.is_empty(), "sjulia {flag} wrote to stderr");
        }
    }
}

mod sjulia_cli_dump_bytecode_tests {
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
        // The typed local may be SSA-renamed, but it must stay specialized as an
        // I64 slot and store through the typed slot instruction, not collapse to Any.
        assert!(
            dump.lines().any(|line| line.contains("StoreSlotI64(2)")
                && line.contains("slot #2")
                && line.contains("::I64")),
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
    fn direct_memory_user_functions_track_memory_lattice_issue_9034() {
        // Issue #9034 (tracking option, PR #9052): `Memory{T}` is now a
        // `ConcreteType::Memory` lattice carrier mirroring `Array`. A
        // `m::Memory{Int64}` parameter annotation therefore types the slot as
        // `MemoryOf(I64)` instead of `Any`. The matching fixture pins runtime
        // behavior; this dump pins the improved parameter typing.
        //
        // Residual gap (shared with `Array{T}(undef, n)`): the `Memory{Int64}(...)`
        // constructor call and the indexed-load return type are still uninferred, so
        // `make_memory_9034() -> Any` and the `read_memory_9034` *return* stay `Any`
        // with `ReturnAny`. Only the parameter typing improved.
        let dump = dump_user_bytecode(
            "function make_memory_9034()\n\
                 m = Memory{Int64}(undef, 2)\n\
                 m[1] = 7\n\
                 m[2] = 11\n\
                 return m\n\
             end\n\
             function read_memory_9034(m::Memory{Int64})\n\
                 m[1] + m[2]\n\
             end\n\
             println(read_memory_9034(make_memory_9034()))\n",
        );
        assert!(
            dump.contains("make_memory_9034() -> Any"),
            "the Memory{{Int64}}(undef, n) constructor is still uninferred (shared Array/Memory gap), so this returns Any, got:\n{dump}"
        );
        assert!(
            dump.contains("read_memory_9034(m::MemoryOf(I64)) -> Any"),
            "Memory-typed parameters now type the slot as MemoryOf(I64) via the ConcreteType::Memory carrier; the return still widens to Any, got:\n{dump}"
        );
        assert!(
            dump.contains("ReturnAny"),
            "the indexed-load return type is still uninferred, so direct Memory returns use ReturnAny, got:\n{dump}"
        );
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

    #[test]
    fn dump_bytecode_resolves_package_struct_field_after_bare_name_collision_9964() {
        // Issue #9964: `--dump-bytecode` used a hand-rolled prelude/package
        // merge that never merged `prelude_program.modules` into
        // `program.modules`, then compiled the whole merged program from
        // scratch via `compile_core_program` (never reusing the precompiled
        // Base cache the way normal execution does via `compile_with_cache`).
        // Recompiling Base's own functions in that fresh, uncached pass
        // exposed a struct-table bug: a package module struct's bare-name
        // alias (registered so unqualified `Struct(...)` syntax works
        // inside/after `using Pkg`) unconditionally clobbered an
        // already-registered Base/prelude struct of the same bare name —
        // here, Base's own `struct Partition` (`xs`, `n`, backing
        // `Iterators.partition`) vs. `AbstractAlgebra.Partition` (`n`,
        // `part`, from the Young-tableau MVP). Recompiling Base's
        // `partition()` body then resolved its `Partition(...)` constructor
        // to the WRONG (AbstractAlgebra) struct, and a later `.xs` field
        // access failed to compile with "Unknown field 'xs' on struct
        // 'AbstractAlgebra.Partition'". Normal execution never recompiles
        // Base — it reuses the frozen precompiled Base cache — so it never
        // observed the clobbered entry. This pins that `--dump-bytecode` now
        // routes through the same shared pipeline + cache-aware compile path
        // as normal execution and succeeds on the MWE instead of failing.
        let output = Command::new(sjulia_bin())
            .args([
                "--dump-bytecode",
                "-e",
                "using AbstractAlgebra; println(base_ring_type(typeof(ZZ)))",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("failed to spawn sjulia --dump-bytecode");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "sjulia --dump-bytecode should compile the AbstractAlgebra MWE; status={:?}, stderr=<<<{}>>>",
            output.status,
            stderr
        );
        assert!(
            !stderr.contains("Unknown field"),
            "must not regress into the struct-table bare-name collision bug; stderr=<<<{}>>>",
            stderr
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("=== Bytecode Summary ==="),
            "sjulia --dump-bytecode should print a bytecode dump, got:\n{stdout}"
        );
    }
}

mod sjulia_cli_stderr_tests {
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
        let (stdout, stderr) = run_eval(
            r#"println("o1"); println(stderr, "e1"); println("o2"); println(stderr, "e2")"#,
        );
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
}

mod sjulia_cli_stdin_tests {
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
    fn run_with_piped_stdin(
        args: &[&str],
        code: &str,
    ) -> (String, String, std::process::ExitStatus) {
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
}

mod sjulia_cli_vm_bytecode_tests {
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

    #[test]
    fn run_vm_bytecode_rejects_tampered_version_loudly_10170() {
        // The .sjvmbc header stores the format version as a little-endian u32
        // at byte offset 4, right after the 4-byte "SJVM" magic (see
        // `subset_julia_vm/src/vm_bytecode_file.rs`). Flip it to an older
        // version and confirm `--run-vm-bytecode` fails loudly with the
        // exact-version-mismatch message instead of misdeserializing the
        // payload with current structs (Issue #10170).
        let dir = tempdir().expect("create temp dir");
        let source_path = dir.path().join("program.jl");
        let bytecode_path = dir.path().join("program.sjvmbc");

        fs::write(&source_path, "println(1 + 1)\n").expect("write source");

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

        let mut bytes = fs::read(&bytecode_path).expect("read bytecode file");
        assert_eq!(&bytes[..4], b"SJVM", "magic bytes should lead the header");
        bytes[4..8].copy_from_slice(&3u32.to_le_bytes()); // stale version 3
        fs::write(&bytecode_path, &bytes).expect("write tampered bytecode file");

        let run_output = Command::new(sjulia_bin())
            .args([
                "--run-vm-bytecode",
                bytecode_path.to_str().expect("utf-8 bytecode path"),
            ])
            .output()
            .expect("spawn sjulia --run-vm-bytecode on tampered file");
        assert!(
            !run_output.status.success(),
            "tampered version must not run\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&run_output.stdout),
            String::from_utf8_lossy(&run_output.stderr),
        );
        let stderr = String::from_utf8_lossy(&run_output.stderr);
        assert!(
            stderr.contains("version mismatch") && stderr.contains("--compile-vm"),
            "stderr should carry the actionable version-mismatch message, got:\n{stderr}"
        );
    }
}

mod sjulia_cli_soft_scope_9283_tests {
    //! CLI integration tests for the strict file-mode soft-scope diagnostic text
    //! (Issue #9283). These spawn the freshly-built `sjulia` binary so they exercise
    //! the real stderr the user sees — the soft-scope warning location and the
    //! `UndefVarError` suffix — end to end.
    //!
    //! Companion to `file_mode_soft_scope_9210_tests.rs` (behaviour via the Rust
    //! API) and `soft_scope_hosts_9283_tests.rs` (host strictness + error string).

    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn sjulia_bin() -> &'static str {
        env!("CARGO_BIN_EXE_sjulia")
    }

    /// A unique temp path so parallel test threads never collide.
    fn unique_jl_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "sjulia_softscope_9283_{tag}_{}_{n}.jl",
            std::process::id()
        ))
    }

    fn run_file_output(source: &str, tag: &str) -> (Output, String) {
        let path = unique_jl_path(tag);
        fs::write(&path, source).expect("write temp fixture");
        // Mirror the CLI's own `std::path::absolute` so the expected warning
        // location matches byte-for-byte (Issue #9283).
        let expected_loc = std::path::absolute(&path)
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .into_owned();
        let output = Command::new(sjulia_bin())
            .arg(&path)
            .output()
            .expect("failed to spawn sjulia");
        let _ = fs::remove_file(&path);
        (output, expected_loc)
    }

    /// Run `sjulia <file>` and return its stderr (the program is expected to fail
    /// with `UndefVarError`, so a non-zero exit is fine).
    fn run_file_stderr(source: &str, tag: &str) -> (String, String) {
        let (output, expected_loc) = run_file_output(source, tag);
        (
            String::from_utf8_lossy(&output.stderr).into_owned(),
            expected_loc,
        )
    }

    /// Warning-presence/absence regressions must also prove the program reached
    /// its expected output; an early compile/runtime error could otherwise make
    /// a no-warning assertion pass vacuously.
    fn run_successful_file(source: &str, tag: &str) -> (String, String) {
        let (output, _) = run_file_output(source, tag);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            output.status.success(),
            "sjulia file failed: status={}, stdout=<<<{stdout}>>>, stderr=<<<{stderr}>>>",
            output.status
        );
        (stderr, stdout)
    }

    /// `sjulia file.jl` locates the soft-scope warning at the script's absolute path
    /// and line, and raises `UndefVarError ... in local scope` + `Suggestion:`.
    #[test]
    fn file_mode_warning_locates_at_absolute_script_path() {
        let src = "total = 0\nfor i in 1:3\n    total += 1\nend\nprintln(total)\n";
        let (stderr, loc) = run_file_stderr(src, "single");
        assert!(
            stderr.contains("Assignment to `total` in soft scope is ambiguous"),
            "missing soft-scope warning, stderr=<<<{stderr}>>>"
        );
        assert!(
            stderr.contains(&format!("└ @ {loc}:3")),
            "warning must locate at the absolute script path + line `{loc}:3`, stderr=<<<{stderr}>>>"
        );
        assert!(
            stderr.contains("UndefVarError: `total` not defined in local scope"),
            "stderr=<<<{stderr}>>>"
        );
        assert!(
            stderr.contains("Suggestion: check for an assignment to a local variable"),
            "stderr=<<<{stderr}>>>"
        );
    }

    /// Multiple captured names are warned in SOURCE order (`zebra`, then `apple`,
    /// then `mango`) — not alphabetical order.
    #[test]
    fn file_mode_multiname_warnings_in_source_order() {
        let src = "zebra = 0\napple = 0\nmango = 0\nfor i in 1:3\n    zebra += 1\n    apple += 1\n    mango += 1\nend\n";
        let (stderr, _loc) = run_file_stderr(src, "multi");
        let zebra = stderr.find("Assignment to `zebra`").expect("zebra warning");
        let apple = stderr.find("Assignment to `apple`").expect("apple warning");
        let mango = stderr.find("Assignment to `mango`").expect("mango warning");
        assert!(
            zebra < apple && apple < mango,
            "warnings must appear in source order zebra<apple<mango, stderr=<<<{stderr}>>>"
        );
    }

    /// `sjulia -e <code>` has no backing file, so the warning locates at
    /// `none:<line>` (matching upstream `julia -e`), never a path.
    #[test]
    fn eval_mode_warning_locates_at_none() {
        let output = Command::new(sjulia_bin())
            .args([
                "-e",
                "total = 0\nfor i in 1:3\n    total += 1\nend\nprintln(total)\n",
            ])
            .output()
            .expect("failed to spawn sjulia -e");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("└ @ none:"),
            "`-e` must locate the warning at `none:<line>`, stderr=<<<{stderr}>>>"
        );
        assert!(
            !stderr.contains("└ @ /"),
            "`-e` must not synthesize an absolute path, stderr=<<<{stderr}>>>"
        );
    }

    /// A fresh top-level try-clause binding is not a global-before fact for a
    /// later loop, so strict file mode emits no phantom warning (#11322).
    #[test]
    fn later_loop_does_not_warn_for_fresh_try_clause_binding_11322() {
        let src = "try\n    ghost11322 = 1\ncatch\nend\nfor i in 1:1\n    ghost11322 = 2\nend\nprintln(@isdefined ghost11322)\n";
        let (stderr, stdout) = run_successful_file(src, "try_fresh_11322");
        assert_eq!(stdout, "false\n");
        assert!(
            !stderr.contains("Assignment to `ghost11322` in soft scope is ambiguous"),
            "fresh clause binding must not trigger a later phantom warning, stderr=<<<{stderr}>>>"
        );
    }

    /// The const-shadow shape from #11305 is already semantically green on
    /// current main; retain its no-warning behavior through the shared fix.
    #[test]
    fn loop_nested_try_const_shadow_emits_no_warning_11305() {
        let src = "const const11305 = 1\nfor i in 1:1\n    try\n        const11305 = 2\n    catch\n    end\nend\nprintln(const11305)\n";
        let (stderr, stdout) = run_successful_file(src, "try_const_11305");
        assert_eq!(stdout, "1\n");
        assert!(
            !stderr.contains("Assignment to `const11305` in soft scope is ambiguous"),
            "nested fresh clause binding must not warn against the outer const, stderr=<<<{stderr}>>>"
        );
    }

    /// The underlying const-provenance rule also suppresses the spurious
    /// warning for a direct top-level loop assignment.
    #[test]
    fn direct_loop_const_shadow_emits_no_warning_11305() {
        let src = "const direct_const11305 = 1\nfor i in 1:1\n    direct_const11305 = 2\nend\nprintln(direct_const11305)\n";
        let (stderr, stdout) = run_successful_file(src, "direct_const_11305");
        assert_eq!(stdout, "1\n");
        assert!(
            !stderr.contains("Assignment to `direct_const11305` in soft scope is ambiguous"),
            "const shadow must use a fresh local without warning, stderr=<<<{stderr}>>>"
        );
    }

    /// Conversely, an existing mutable global assigned directly by a top-level
    /// try clause is an ambiguous strict soft-scope binding (#11335).
    #[test]
    fn try_clause_existing_global_emits_warning_11335() {
        let src =
            "existing11335 = 1\ntry\n    existing11335 = 2\ncatch\nend\nprintln(existing11335)\n";
        let (stderr, stdout) = run_successful_file(src, "try_existing_11335");
        assert_eq!(stdout, "1\n");
        let warning = "Assignment to `existing11335` in soft scope is ambiguous";
        assert_eq!(
            stderr.matches(warning).count(),
            1,
            "existing global must trigger exactly one upstream warning, stderr=<<<{stderr}>>>"
        );
    }

    /// The nested assignment reuses the local created by the outer clause, so
    /// upstream emits exactly one ambiguity warning for the whole chain
    /// (#11159).
    #[test]
    fn nested_try_assignment_emits_one_enclosing_warning_11159() {
        let src = "nestedreuse11159 = 0\ntry\n    nestedreuse11159 = 1\n    try\n        nestedreuse11159 = 2\n    catch\n    end\n    println(nestedreuse11159)\ncatch\nend\n";
        let (stderr, stdout) = run_successful_file(src, "try_nested_reuse_11159");
        assert_eq!(stdout, "2\n");
        let warning = "Assignment to `nestedreuse11159` in soft scope is ambiguous";
        assert_eq!(
            stderr.matches(warning).count(),
            1,
            "nested clause must reuse the enclosing local without a second warning, stderr=<<<{stderr}>>>"
        );
    }

    /// A later real global declaration supersedes an earlier retired clause
    /// local, so the subsequent loop emits exactly one ambiguity warning.
    #[test]
    fn later_global_after_try_local_emits_warning() {
        let src = "try\n    mixedprov = 1\ncatch\nend\nmixedprov = 0\nfor i in 1:1\n    mixedprov = 2\nend\nprintln(mixedprov)\n";
        let (stderr, stdout) = run_successful_file(src, "try_then_global");
        assert_eq!(stdout, "0\n");
        let warning = "Assignment to `mixedprov` in soft scope is ambiguous";
        assert_eq!(
            stderr.matches(warning).count(),
            1,
            "the later mutable-global fact must trigger one warning, stderr=<<<{stderr}>>>"
        );
    }
}

mod sjulia_cli_malformed_source_survival_10908_tests {
    //! CLI process-survival corpus (Issue #10908, Phase 3 of the #10869
    //! panic-debt retirement epic): the `sjulia` binary is the CLI entrypoint
    //! Issue #10869 names; a malformed `.jl` file (or `-e` argument) must
    //! make the process exit non-zero with a typed, human-readable error —
    //! never a Rust panic backtrace (`thread '...' panicked at`) reaching the
    //! user's terminal. Reuses the same malformed-source shapes exercised at
    //! the parser (Issue #10904's `malformed_input_no_panic_tests.rs`),
    //! lowering/compile (Issue #10905's `lowering_compile_malformed_input_10905_tests`),
    //! and FFI (`compile_and_run_detailed`/`run_vm_bytecode_detailed`)
    //! boundaries above, applied one layer further out — the actual `sjulia`
    //! process a user runs.

    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn sjulia_bin() -> &'static str {
        env!("CARGO_BIN_EXE_sjulia")
    }

    fn unique_jl_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "sjulia_malformed_survival_10908_{tag}_{}_{n}.jl",
            std::process::id()
        ))
    }

    /// No Rust panic backtrace text of any kind reached stderr — the
    /// substring shared by every `panic!`/`.unwrap()`/`.expect()` default
    /// hook message, regardless of the specific panic payload.
    fn assert_no_panic_backtrace(stderr: &str, context: &str) {
        assert!(
            !stderr.contains("panicked at"),
            "{context}: a Rust panic backtrace reached stderr:\n{stderr}"
        );
    }

    const MALFORMED_SOURCE_SNIPPETS: &[&str] = &[
        "function f(x)\n    x + 1\n",      // unterminated function
        "struct S\n    x::Int\n",          // unterminated struct
        "for i in 1:10\n    println(i)\n", // unterminated for
        "if true\n    1\n",                // unterminated if
        "[1 2; 3 4",                       // unterminated matrix literal
        "Dict(:a => 1, :b => 2",           // unterminated call
        "x = ",                            // dangling assignment
        "::::",                            // malformed operator soup
        "let x = 1; x + 1",                // unterminated let
    ];

    #[test]
    fn malformed_source_file_exits_nonzero_without_panic_backtrace() {
        for (i, src) in MALFORMED_SOURCE_SNIPPETS.iter().enumerate() {
            let path = unique_jl_path(&format!("file{i}"));
            fs::write(&path, src).expect("write temp malformed fixture");
            let output = Command::new(sjulia_bin())
                .arg(&path)
                .output()
                .expect("failed to spawn sjulia on malformed file");
            let _ = fs::remove_file(&path);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_no_panic_backtrace(&stderr, &format!("file mode, snippet {i} ({src:?})"));
            // A malformed snippet is expected to fail (never silently
            // succeed), but the failure must be a typed diagnostic, not a
            // hung process or a crash — `output()` already proves the
            // process terminated; only the exit code and stderr shape are
            // asserted further.
            assert!(
                !output.status.success(),
                "malformed snippet {i} ({src:?}) unexpectedly succeeded"
            );
        }
    }

    #[test]
    fn malformed_source_eval_arg_exits_nonzero_without_panic_backtrace() {
        for (i, src) in MALFORMED_SOURCE_SNIPPETS.iter().enumerate() {
            let output = Command::new(sjulia_bin())
                .args(["-e", src])
                .output()
                .expect("failed to spawn sjulia -e on malformed code");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_no_panic_backtrace(&stderr, &format!("-e mode, snippet {i} ({src:?})"));
            assert!(
                !output.status.success(),
                "malformed -e snippet {i} ({src:?}) unexpectedly succeeded"
            );
        }
    }

    /// Every prefix-truncation of every snippet above, run through `-e`. Most
    /// truncations fail to parse even earlier than the full snippet; a few
    /// coincidentally parse a shorter valid prefix and succeed — this test
    /// only asserts the absence of a panic backtrace, not a specific exit
    /// code, mirroring the parser/lowering corpora's truncation sweeps.
    #[test]
    fn truncated_malformed_source_never_panics() {
        for src in MALFORMED_SOURCE_SNIPPETS {
            for end in 1..src.len() {
                if !src.is_char_boundary(end) {
                    continue;
                }
                let output = Command::new(sjulia_bin())
                    .args(["-e", &src[..end]])
                    .output()
                    .expect("failed to spawn sjulia -e on truncated code");
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert_no_panic_backtrace(&stderr, &format!("truncated -e {:?}", &src[..end]));
            }
        }
    }

    /// Invalid-UTF-8 file bytes must fail with a typed I/O error message, not
    /// a panic on the `fs::read_to_string` decode step.
    #[test]
    fn invalid_utf8_file_exits_nonzero_without_panic_backtrace() {
        let path = unique_jl_path("invalid_utf8");
        fs::write(&path, [0x78, 0x20, 0x3d, 0x20, 0xff, 0xfe, 0x0a])
            .expect("write invalid-UTF8 fixture");
        let output = Command::new(sjulia_bin())
            .arg(&path)
            .output()
            .expect("failed to spawn sjulia on invalid-UTF8 file");
        let _ = fs::remove_file(&path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_no_panic_backtrace(&stderr, "invalid-UTF8 file");
        assert!(
            !output.status.success(),
            "invalid-UTF8 file must not succeed"
        );
    }

    /// Empty file / empty `-e` argument must not panic or hang.
    #[test]
    fn empty_source_does_not_panic() {
        let path = unique_jl_path("empty");
        fs::write(&path, "").expect("write empty fixture");
        let file_output = Command::new(sjulia_bin())
            .arg(&path)
            .output()
            .expect("failed to spawn sjulia on empty file");
        let _ = fs::remove_file(&path);
        assert_no_panic_backtrace(&String::from_utf8_lossy(&file_output.stderr), "empty file");

        let eval_output = Command::new(sjulia_bin())
            .args(["-e", ""])
            .output()
            .expect("failed to spawn sjulia -e ''");
        assert_no_panic_backtrace(
            &String::from_utf8_lossy(&eval_output.stderr),
            "empty -e argument",
        );
    }
}

mod sjulia_cli_definition_order_11036_tests {
    use std::fs;
    use std::io::Write;
    use std::process::Stdio;
    use std::process::{Command, Output};

    fn sjulia_bin() -> &'static str {
        env!("CARGO_BIN_EXE_sjulia")
    }

    fn assert_success_with_matrix(output: Output, context: &str) {
        assert!(
            output.status.success(),
            "{context} failed (status={:?})\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "true\ntrue\ntrue\ntrue\n",
            "{context} changed constructor chronology"
        );
    }

    /// Issue #11036: loaded package Modules are independently lowered (or
    /// restored from `.ji.json`) and must be rebased after the surrounding
    /// Program before their definitions are transferred. Exercise both bare
    /// inner/ordinary definition orders, a later user extension of a loaded
    /// type, fresh/cache-restored execution, and the standalone Core-IR CLI.
    #[test]
    fn loaded_module_constructor_order_survives_cache_and_core_ir_11036() {
        let temp = tempfile::tempdir().expect("create temp package root");
        let package_root = temp.path().join("DefinitionOrderPkg11036");
        let source_dir = package_root.join("src");
        let cache_dir = temp.path().join("cache");
        fs::create_dir_all(&source_dir).expect("create temp package source dir");
        fs::create_dir_all(&cache_dir).expect("create temp loader cache dir");
        fs::write(
            package_root.join("Project.toml"),
            "name = \"DefinitionOrderPkg11036\"\n\
             uuid = \"11036000-0000-0000-0000-000000000000\"\n\
             version = \"0.1.0\"\n",
        )
        .expect("write temp Project.toml");
        fs::write(
            source_dir.join("DefinitionOrderPkg11036.jl"),
            r#"module DefinitionOrderPkg11036
export LoadedInnerThenOuter11036, LoadedOuterThenInner11036, LoadedThenUser11036

struct LoadedInnerThenOuter11036
    x::Int
    LoadedInnerThenOuter11036(x::Int) = new(x + 1)
end
LoadedInnerThenOuter11036(x::Int) = :outer

LoadedOuterThenInner11036(x::Int) = :outer
struct LoadedOuterThenInner11036
    x::Int
    LoadedOuterThenInner11036(x::Int) = new(x + 1)
end

struct LoadedThenUser11036
    x::Int
    LoadedThenUser11036(x::Int) = new(x + 1)
end
end
"#,
        )
        .expect("write temp package source");

        let packages = [
            (
                "DefinitionOrderShared11036",
                "11036000-0000-0000-0000-000000000001",
                "",
                r#"module DefinitionOrderShared11036
export SharedCtor11036
struct SharedCtor11036
    x::Int
    SharedCtor11036(x::Int) = new(x + 1)
end
end
"#,
            ),
            (
                "DefinitionOrderFirst11036",
                "11036000-0000-0000-0000-000000000002",
                "DefinitionOrderShared11036 = \"11036000-0000-0000-0000-000000000001\"\n",
                r#"module DefinitionOrderFirst11036
using DefinitionOrderShared11036
DefinitionOrderShared11036.SharedCtor11036(x::Int) = :first
end
"#,
            ),
            (
                "DefinitionOrderSecond11036",
                "11036000-0000-0000-0000-000000000003",
                "DefinitionOrderShared11036 = \"11036000-0000-0000-0000-000000000001\"\n",
                r#"module DefinitionOrderSecond11036
using DefinitionOrderShared11036
DefinitionOrderShared11036.SharedCtor11036(x::Int) = :second
end
"#,
            ),
            (
                "DefinitionOrderParent11036",
                "11036000-0000-0000-0000-000000000004",
                "DefinitionOrderShared11036 = \"11036000-0000-0000-0000-000000000001\"\nDefinitionOrderFirst11036 = \"11036000-0000-0000-0000-000000000002\"\nDefinitionOrderSecond11036 = \"11036000-0000-0000-0000-000000000003\"\n",
                r#"module DefinitionOrderParent11036
using DefinitionOrderShared11036
using DefinitionOrderFirst11036
DefinitionOrderShared11036.SharedCtor11036(x::Int) = :parent
using DefinitionOrderSecond11036
result() = DefinitionOrderShared11036.SharedCtor11036(10)
end
"#,
            ),
        ];
        for (name, uuid, deps, source) in packages {
            let root = temp.path().join(name);
            fs::create_dir_all(root.join("src")).expect("create dependency source dir");
            fs::write(
                root.join("Project.toml"),
                format!(
                    "name = \"{name}\"\nuuid = \"{uuid}\"\nversion = \"0.1.0\"\n[deps]\n{deps}"
                ),
            )
            .expect("write dependency Project.toml");
            fs::write(root.join("src").join(format!("{name}.jl")), source)
                .expect("write dependency source");
        }

        let source_path = temp.path().join("main.jl");
        fs::write(
            &source_path,
            r#"using DefinitionOrderPkg11036
DefinitionOrderPkg11036.LoadedThenUser11036(x::Int) = :user
println(LoadedInnerThenOuter11036(10) === :outer)
println(LoadedOuterThenInner11036(10).x == 11)
println(LoadedThenUser11036(10) === :user)
using DefinitionOrderParent11036
println(DefinitionOrderParent11036.result() === :second)
"#,
        )
        .expect("write temp main source");

        let run = |args: &[&std::ffi::OsStr]| {
            let mut command = Command::new(sjulia_bin());
            command
                .env("SUBSETJULIA_LOAD_PATH", temp.path())
                .env("SUBSETJULIA_CACHE_DIR", &cache_dir);
            command.args(args).output().expect("spawn sjulia")
        };

        assert_success_with_matrix(run(&[source_path.as_os_str()]), "fresh package load");
        assert_success_with_matrix(run(&[source_path.as_os_str()]), "restored package cache");

        let mut repl = Command::new(sjulia_bin())
            .env("SUBSETJULIA_LOAD_PATH", temp.path())
            .env("SUBSETJULIA_CACHE_DIR", &cache_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sjulia REPL");
        repl.stdin
            .take()
            .expect("REPL stdin")
            .write_all(
                b"using DefinitionOrderPkg11036\n\
                  println(LoadedInnerThenOuter11036(10) === :outer)\n\
                  println(LoadedOuterThenInner11036(10).x == 11)\n\
                  DefinitionOrderPkg11036.LoadedThenUser11036(x::Int) = :repl\n\
                  println(LoadedThenUser11036(10) === :repl)\n",
            )
            .expect("write REPL matrix");
        let repl_output = repl.wait_with_output().expect("wait for sjulia REPL");
        assert!(
            repl_output.status.success(),
            "warm-cache REPL failed:\n{}",
            String::from_utf8_lossy(&repl_output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&repl_output.stdout)
                .lines()
                .filter(|line| line.trim() == "true")
                .count(),
            3,
            "cross-eval package chronology changed:\n{}",
            String::from_utf8_lossy(&repl_output.stdout)
        );

        let ir_path = temp.path().join("main.sjir");
        let compile = run(&[
            std::ffi::OsStr::new("--compile"),
            source_path.as_os_str(),
            std::ffi::OsStr::new("-o"),
            ir_path.as_os_str(),
        ]);
        assert!(
            compile.status.success(),
            "Core-IR compile failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        assert_success_with_matrix(
            run(&[std::ffi::OsStr::new("--run-ir"), ir_path.as_os_str()]),
            "Core-IR cache-restored run",
        );
    }
}
