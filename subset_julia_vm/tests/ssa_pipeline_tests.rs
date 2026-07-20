//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod ssa_ir_8440_tests {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, Value};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{name}' not found"))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn ssa_phi_fold_collapses_identical_branch_assignments_issue_8440() {
        let source = r#"
    function same_branch_phi_8440(flag::Bool)::Int64
        x = 0
        if flag
            x = 41
        else
            x = 41
        end
        return x + 1
    end

    same_branch_phi_8440(true)
    "#;

        let compiled = compile_source(source);
        let func = get_function(&compiled, "same_branch_phi_8440");
        let body = function_body(&compiled, func);

        // With the SSA pipeline enabled (the default since Issue #8832), the phi
        // for `x` folds to a constant (both branches assign 41), eliminating the
        // slot entirely.  With the legacy path, the bridge in ir_opt folded the
        // stores down to ≤ 2.  Accept either shape: the slot missing means full
        // const-fold; the slot present means the joined-store fold ran.
        match func.slot_names.iter().position(|name| name == "x") {
            None => {
                // SSA const-folded `x` away entirely — no i64 stores expected.
                let any_i64_stores = body
                    .iter()
                    .filter(|instr| matches!(instr, Instr::StoreSlotI64(_)))
                    .count();
                assert!(
                    any_i64_stores == 0,
                    "SSA const-fold eliminated x slot but i64 stores remain; body={body:?}"
                );
            }
            Some(x_slot) => {
                let x_stores = body
                    .iter()
                    .filter(|instr| matches!(instr, Instr::StoreSlotI64(slot) if *slot == x_slot))
                    .count();
                assert!(
                    x_stores <= 2,
                    "SSA phi fold should keep only the initializer and one joined store to x; stores={x_stores}, body={body:?}"
                );
            }
        }

        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("vm run");
        assert!(matches!(result, Value::I64(42)));
    }
}

mod ssa_pipeline_parity_8552_tests {
    //! SSA pipeline ↔ legacy compiler round-trip parity harness (Issue #8552).
    //!
    //! Runs the same sources twice — once with `SJULIA_SSA_PIPELINE=0` (legacy
    //! `CoreCompiler` body emission, opt-out path) and once with the default (SSA
    //! pipeline ON: eligible user function bodies go Core IR → SSA build →
    //! `ssa_ir::opt` passes → `ssa_ir::lower` stack-bytecode lowering, everything
    //! else falls back to the legacy path per function) — and diffs the program
    //! result, the printed output, and any error string.
    //!
    //! Two layers:
    //!
    //! * Targeted lowering tests (straight-line, if/else phi, while-loop phi,
    //!   ternary, short-circuit conditions): the gated run must actually lower at
    //!   least one function (`take_ssa_pipeline_stats`), and results must match
    //!   the legacy run plus the upstream-Julia-verified expectation.
    //! * Whole fixture categories, mirroring the fixture runner: every fixture of
    //!   the covered categories is executed through both paths and diffed.
    //!   Covered categories: `const_prop`, `closures`, `dispatch` (Issue #8552
    //!   acceptance list; grow toward the full suite once the lowering covers
    //!   more shapes).

    use serde::Deserialize;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use subset_julia_vm::compile::host_support::{
        clear_compile_cache, clear_non_base_compile_cache, clear_program_compile_cache,
        compile_with_cache,
    };
    use subset_julia_vm::compile::ssa_ir::take_ssa_pipeline_stats;
    use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    /// Serializes process-env manipulation across tests: nextest runs one process
    /// per test, but plain `cargo test` shares the environment between threads.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const GATE_ENV: &str = "SJULIA_SSA_PIPELINE";

    /// Stack size for fixture-driven runs (matches `fixture_tests.rs`,
    /// Issue #2766).
    const PARITY_TEST_STACK_SIZE: usize = 16 * 1024 * 1024;

    struct RunOutcome {
        /// `Ok(final value debug repr)` or `Err(error display)`.
        result: Result<String, String>,
        output: String,
        lowered: u64,
        fallbacks: u64,
    }

    #[derive(Clone, Copy)]
    enum CacheReset {
        FreshProgram,
        AlternateGate,
    }

    fn run_source(src: &str, base_dir: Option<PathBuf>, cache_reset: CacheReset) -> RunOutcome {
        match cache_reset {
            CacheReset::FreshProgram => {
                // Preserve Base across fixture-category loops while still isolating
                // user program caches and user-defined promotion rules per fixture.
                clear_non_base_compile_cache();
            }
            CacheReset::AlternateGate => {
                // Avoid a full-program cache hit from the legacy compile when the
                // same source is recompiled with the SSA gate enabled, but keep the
                // Base registry replay from the first pass (Issue #9865).
                clear_program_compile_cache();
            }
        }
        subset_julia_vm::cancel::reset();
        let _ = take_ssa_pipeline_stats();
        let program = parse_and_lower_with_base_dir(src, base_dir)
            .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
        let compiled =
            compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
        let stats = take_ssa_pipeline_stats();
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = match vm.run() {
            Ok(value) => Ok(format!("{value:?}")),
            Err(err) => Err(err.to_string()),
        };
        RunOutcome {
            result,
            output: vm.get_output().to_string(),
            lowered: stats.lowered,
            fallbacks: stats.fallbacks,
        }
    }

    /// Run `src` with the SSA pipeline disabled (`SJULIA_SSA_PIPELINE=0`, legacy
    /// path) and enabled (default, no env override); program result and printed
    /// output must be identical, the legacy run must never touch the SSA pipeline,
    /// and the default run must lower at least `min_lowered` function bodies via SSA.
    fn assert_gate_parity(name: &str, src: &str, expected_output: &str, min_lowered: u64) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        std::env::set_var(GATE_ENV, "0");
        let off = run_source(src, None, CacheReset::FreshProgram);
        assert_eq!(
            off.lowered + off.fallbacks,
            0,
            "{name}: SJULIA_SSA_PIPELINE=0 must never enter the SSA pipeline"
        );

        std::env::remove_var(GATE_ENV);
        let on = run_source(src, None, CacheReset::AlternateGate);
        std::env::remove_var(GATE_ENV);

        assert_eq!(
            off.result, on.result,
            "{name}: SSA-lowered program result must match the legacy compiler"
        );
        assert_eq!(
            off.output, on.output,
            "{name}: SSA-lowered printed output must match the legacy compiler"
        );
        assert_eq!(
            on.output, expected_output,
            "{name}: output must match upstream Julia"
        );
        assert!(
            on.lowered >= min_lowered,
            "{name}: gate on must lower at least {min_lowered} function(s) via SSA \
             (lowered: {}, fallbacks: {})",
            on.lowered,
            on.fallbacks
        );
        eprintln!(
            "[ssa-pipeline parity] {name}: lowered={} fallbacks={}",
            on.lowered, on.fallbacks
        );
    }

    // ── Targeted lowering shapes (all outputs verified against upstream Julia) ──

    #[test]
    fn ssa_lowering_straight_line_issue_8552() {
        assert_gate_parity(
            "straight_line",
            r#"
    function poly(x::Int64)
        a = x * x
        b = 3 * a + 2 * x
        return b - 7
    end

    println(poly(11))
    "#,
            "378\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_if_else_phi_issue_8552() {
        assert_gate_parity(
            "if_else_phi",
            r#"
    function classify(n::Int64)
        if n > 100
            label = "big"
        else
            label = "small"
        end
        return label
    end

    println(classify(150))
    println(classify(3))
    "#,
            "big\nsmall\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_while_loop_phi_issue_8552() {
        assert_gate_parity(
            "while_loop_phi",
            r#"
    function sum_to(n::Int64)
        total = 0
        i = 1
        while i <= n
            total = total + i
            i = i + 1
        end
        return total
    end

    println(sum_to(100))
    "#,
            "5050\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_recursive_calls_issue_8552() {
        assert_gate_parity(
            "recursive_calls",
            r#"
    function fib(n::Int64)
        if n <= 1
            return n
        end
        return fib(n - 1) + fib(n - 2)
    end

    println(fib(20))
    "#,
            "6765\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_ternary_phi_issue_8552() {
        assert_gate_parity(
            "ternary_phi",
            r#"
    function clamp01(x::Float64)
        y = x < 0.0 ? 0.0 : (x > 1.0 ? 1.0 : x)
        return y
    end

    println(clamp01(-3.5))
    println(clamp01(0.25))
    println(clamp01(9.0))
    "#,
            "0.0\n0.25\n1.0\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_short_circuit_condition_issue_8552() {
        // `n != 0 && 10 % n == 0` must keep its short-circuit: the right side
        // divides by `n`, so it must not run when `n == 0`.
        assert_gate_parity(
            "short_circuit_condition",
            r#"
    function divides10(n::Int64)
        if n != 0 && 10 % n == 0
            return true
        end
        return false
    end

    println(divides10(0))
    println(divides10(5))
    println(divides10(3))
    "#,
            "false\ntrue\nfalse\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_implicit_return_conversion_issue_8552() {
        // Implicit tail expression with an F64-inferred return type: the legacy
        // path converts a trailing I64 via `emit_type_conversion`; the SSA
        // lowering must reproduce it (Issue #8552 implicit-tail mode).
        assert_gate_parity(
            "implicit_return",
            r#"
    half(x) = x / 2
    scaled(x) = x * 2

    println(half(7))
    println(scaled(21))
    "#,
            "3.5\n42\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_while_break_continue_issue_8552() {
        assert_gate_parity(
            "while_break_continue",
            r#"
    function count_odd_until(limit::Int64, stop::Int64)
        count = 0
        i = 0
        while i < limit
            i = i + 1
            if i == stop
                break
            end
            if i % 2 == 0
                continue
            end
            count = count + 1
        end
        return count
    end

    println(count_odd_until(10, 7))
    println(count_odd_until(10, 100))
    "#,
            "3\n5\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_global_read_issue_8552() {
        // The parameter is deliberately untyped: an `::Int64` annotation makes
        // the *legacy* path lose the module-level global (pre-existing bug,
        // Issue #8598) — both paths agree, but the output would not match
        // upstream Julia.
        assert_gate_parity(
            "global_read",
            r#"
    const FACTOR = 4

    function scale(x)
        return FACTOR * x + FACTOR
    end

    println(scale(10))
    "#,
            "44\n",
            1,
        );
    }

    // ── Phi-copy coalescing / effects wiring shapes (Issue #8440) ───────────────

    #[test]
    fn ssa_lowering_loop_carried_coalescing_issue_8440() {
        // Three loop-carried variables (F64, F64, I64): every latch value is
        // produced in the latch block and dies at its phi copy, so all three
        // copies coalesce into direct phi-slot stores (no `#ssatmp`, one store
        // per carried variable per iteration — the legacy store shape).
        // Typed parameter keeps the function off the runtime-specialization
        // fallback so the SSA path is actually exercised.
        assert_gate_parity(
            "loop_carried_coalescing",
            r#"
    function calc_pi_8440(n::Int64)
        s = 0.0
        sign = 1.0
        k = 1
        while k <= n
            s += sign / (2.0 * k - 1.0)
            sign = -sign
            k += 1
        end
        4.0 * s
    end
    println(calc_pi_8440(50))
    "#,
            "3.121594652591011\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_loop_swap_interference_issue_8440() {
        // `t = a; a = b; b = t` aliases both latch values to the header phis of
        // the *other* variable: the parallel copy interferes, coalescing must
        // stay away, and the two-round `#ssatmp` staging must still produce the
        // correct swap (adversarial case from Issue #8552).
        assert_gate_parity(
            "loop_swap_interference",
            r#"
    function swap_accum_8440(n::Int64)
        a = 1
        b = 2
        k = 0
        s = 0
        while k < n
            t = a
            a = b
            b = t
            s += a
            k += 1
        end
        s * 100 + a * 10 + b
    end
    println(swap_accum_8440(5))
    "#,
            "821\n",
            1,
        );
    }

    #[test]
    fn ssa_lowering_locally_rebound_call_side_effect_issue_8799() {
        // `abs = println; abs(-5)` — the callee name is rebound locally by plain
        // assignment. The passes must not misattribute the pure builtin summary
        // of `abs` and delete the call (Issue #8799): the surviving call then
        // forces the per-function legacy fallback, and the side effect prints on
        // both paths.
        assert_gate_parity(
            "locally_rebound_call_side_effect",
            r#"
    function rebound_call_8799()
        abs = println
        abs(-5)
        return 0
    end
    println(rebound_call_8799())
    "#,
            "-5\n0\n",
            0,
        );
    }

    #[test]
    fn ssa_lowering_pure_call_cse_across_branch_issue_8440() {
        // A dominating pure user call repeated in both branch arms: SSA CSE with
        // the body-derived effect summaries (Issue #8441 wiring) merges the arm
        // calls into the dominating one — the legacy user-scope CSE is
        // straight-line only and keeps all three. The callee body is
        // binary-operator-only on purpose: calling a multi-method Base name
        // (`sqrt`, n-ary `*` chains) inherits that name's conservative merged
        // summary and stays out of CSE.
        assert_gate_parity(
            "pure_call_cse_across_branch",
            r#"
    mydist_8440(a::Float64, b::Float64) = a * a + b * b

    function branchy_8440(a::Float64, b::Float64)
        base = mydist_8440(a, b)
        if a > b
            r = mydist_8440(a, b) + 1.0
        else
            r = mydist_8440(a, b) - 1.0
        end
        base + r
    end
    println(branchy_8440(3.0, 4.0))
    println(branchy_8440(4.0, 3.0))
    "#,
            "49.0\n51.0\n",
            1,
        );
    }

    /// Compile `src` with the gate on/off and return the per-gate instruction
    /// count of `func_name`'s body plus the number of call instructions
    /// targeting `callee` inside it (resolved through the function table).
    fn function_shape(
        src: &str,
        func_name: &str,
        callee: &str,
    ) -> ((usize, usize), (usize, usize)) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let shape = |gate: bool| {
            if gate {
                std::env::remove_var(GATE_ENV);
            } else {
                std::env::set_var(GATE_ENV, "0");
            }
            clear_compile_cache();
            subset_julia_vm::cancel::reset();
            let _ = take_ssa_pipeline_stats();
            let program = parse_and_lower_with_base_dir(src, None).expect("pipeline error");
            let compiled = compile_with_cache(&program).expect("compile error");
            std::env::remove_var(GATE_ENV);
            let info = compiled
                .functions
                .iter()
                .find(|f| f.name == func_name)
                .unwrap_or_else(|| panic!("function {func_name} not found"));
            let body = &compiled.code[info.code_start..info.code_end];
            let callee_indices: Vec<usize> = compiled
                .functions
                .iter()
                .enumerate()
                .filter(|(_, f)| f.name == callee)
                .map(|(i, _)| i)
                .collect();
            let calls = body
                .iter()
                .filter(|instr| match instr {
                    subset_julia_vm_bytecode::Instr::Call(idx, _)
                    | subset_julia_vm_bytecode::Instr::CallResolved(idx, _)
                    | subset_julia_vm_bytecode::Instr::CallInbounds(idx, _) => {
                        callee_indices.contains(idx)
                    }
                    _ => false,
                })
                .count();
            (body.len(), calls)
        };

        (shape(false), shape(true))
    }

    #[test]
    fn ssa_cse_reduces_branch_arm_calls_in_bytecode_issue_8440() {
        // The measurable optimization behind the Criterion benchmark
        // (`ssa_pipeline_cse_benchmark`): gate-on bytecode must contain exactly
        // one call to the pure callee where the legacy path keeps all three.
        let src = r#"
    mydist_8440(a::Float64, b::Float64) = a * a + b * b

    function branchy_8440(a::Float64, b::Float64)
        base = mydist_8440(a, b)
        if a > b
            r = mydist_8440(a, b) + 1.0
        else
            r = mydist_8440(a, b) - 1.0
        end
        base + r
    end
    println(branchy_8440(3.0, 4.0))
    "#;
        let ((_, legacy_calls), (_, ssa_calls)) =
            function_shape(src, "branchy_8440", "mydist_8440");
        assert_eq!(
            legacy_calls, 3,
            "legacy path should keep all three mydist_8440 calls"
        );
        assert_eq!(
            ssa_calls, 1,
            "SSA CSE should merge the branch-arm calls into the dominating one"
        );
    }

    #[test]
    fn ssa_coalescing_matches_legacy_loop_store_count_issue_8440() {
        // Loop-carried phi coalescing must not leave extra spill stores: the
        // gate-on body of a three-variable loop stays within the legacy
        // instruction count (same store shape, so the peephole loop fusions
        // apply on both paths).
        let src = r#"
    function calc_pi_8440(n::Int64)
        s = 0.0
        sign = 1.0
        k = 1
        while k <= n
            s += sign / (2.0 * k - 1.0)
            sign = -sign
            k += 1
        end
        4.0 * s
    end
    println(calc_pi_8440(50))
    "#;
        let ((legacy_len, _), (ssa_len, _)) = function_shape(src, "calc_pi_8440", "calc_pi_8440");
        assert!(
            ssa_len <= legacy_len + 1,
            "gate-on loop body must not carry spill-copy overhead \
             (legacy: {legacy_len} instrs, gate-on: {ssa_len} instrs)"
        );
    }

    // ── Fixture-category round trips (Issue #8552 acceptance list) ──────────────

    #[derive(Debug, Deserialize)]
    struct CategoryManifest {
        #[serde(default)]
        tests: Vec<CategoryTest>,
    }

    #[derive(Debug, Deserialize)]
    struct CategoryTest {
        name: String,
        file: String,
        #[serde(default)]
        skip: bool,
    }

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    /// Mask wall-clock timing lines (`@time`/`@elapsed` output like
    /// `  3.8e-5 seconds`) so nondeterministic durations don't fail the diff.
    /// Only whole lines whose content is `<float> seconds` are masked.
    fn normalize_output(output: &str) -> String {
        output
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if let Some(value) = trimmed.strip_suffix(" seconds") {
                    if value.parse::<f64>().is_ok() {
                        return "<elapsed> seconds".to_string();
                    }
                }
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Run every fixture of `category` through both compile paths and diff the
    /// final value, the printed output, and the error string.
    fn assert_category_parity(category: &str) {
        let category = category.to_string();
        let result = std::thread::Builder::new()
            .stack_size(PARITY_TEST_STACK_SIZE)
            .spawn(move || assert_category_parity_inner(&category))
            .expect("failed to spawn parity test thread")
            .join();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    fn assert_category_parity_inner(category: &str) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let dir = fixtures_dir().join(category);
        let manifest_path = dir.join("manifest.toml");
        let content = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", manifest_path.display()));
        let manifest: CategoryManifest = toml::from_str(&content)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", manifest_path.display()));
        assert!(
            !manifest.tests.is_empty(),
            "category {category} has no fixtures"
        );

        let mut total_lowered = 0u64;
        let mut total_fallbacks = 0u64;
        let mut compared = 0usize;
        for test in &manifest.tests {
            if test.skip {
                continue;
            }
            // Category manifests list files relative to their directory (the
            // fixture runner prefixes them the same way).
            let rel = if test.file.contains('/') {
                test.file.clone()
            } else {
                format!("{category}/{}", test.file)
            };
            let file_path = fixtures_dir().join(&rel);
            let source = fs::read_to_string(&file_path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", file_path.display()));
            let base_dir = file_path.parent().map(PathBuf::from);

            std::env::set_var(GATE_ENV, "0");
            let off = run_source(&source, base_dir.clone(), CacheReset::FreshProgram);
            assert_eq!(
                off.lowered + off.fallbacks,
                0,
                "{category}/{}: SJULIA_SSA_PIPELINE=0 must never enter the SSA pipeline",
                test.name
            );

            std::env::remove_var(GATE_ENV);
            let on = run_source(&source, base_dir, CacheReset::AlternateGate);
            std::env::remove_var(GATE_ENV);

            assert_eq!(
                off.result, on.result,
                "{category}/{}: SSA-lowered result must match the legacy compiler",
                test.name
            );
            assert_eq!(
                normalize_output(&off.output),
                normalize_output(&on.output),
                "{category}/{}: SSA-lowered output must match the legacy compiler",
                test.name
            );
            total_lowered += on.lowered;
            total_fallbacks += on.fallbacks;
            compared += 1;
        }

        assert!(
            total_lowered > 0,
            "category {category}: the gated run must lower at least one function \
             body via SSA across the category (lowered: {total_lowered}, \
             fallbacks: {total_fallbacks})"
        );
        eprintln!(
            "[ssa-pipeline parity] category {category}: fixtures={compared} \
             lowered={total_lowered} fallbacks={total_fallbacks}"
        );
    }

    // ── Expanded eligibility: if/else-always-returns tail (Issue #8832) ─────────

    #[test]
    fn ssa_lowering_if_else_always_returns_tail_issue_8832() {
        // Functions whose last statement is an `if`/`else` where both branches
        // return explicitly should be lowered via SSA. Previously they fell back
        // with "unsupported tail statement (implicit default return)".
        assert_gate_parity(
            "if_else_always_returns_tail",
            r#"
    function abs_val(x::Int64)::Int64
        if x >= 0
            return x
        else
            return -x
        end
    end

    function clamp_val(x::Int64, lo::Int64, hi::Int64)::Int64
        if x < lo
            return lo
        else
            if x > hi
                return hi
            else
                return x
            end
        end
    end

    println(abs_val(5))
    println(abs_val(-3))
    println(clamp_val(5, 0, 10))
    println(clamp_val(-3, 0, 10))
    println(clamp_val(15, 0, 10))
    "#,
            "5\n3\n5\n0\n10\n",
            2, // abs_val and clamp_val both lowered
        );
    }

    // ── Regression: &&/|| in statement position (Issue #8832) ──────────────────

    #[test]
    fn ssa_pipeline_and_or_statement_position_fallback_issue_8832() {
        // `&&`/`||` in statement position (result discarded) must fall back to
        // the legacy path. The SSA builder evaluates both operands unconditionally;
        // DCE would then remove the condition guard while keeping the side-effectful
        // right operand — `x <= 0 && throw(...)` would always throw regardless of
        // `x`. The fix is a per-function fallback before SSA construction.
        //
        // The helper `add_one` (no statement-position &&/||) must still be lowered
        // via SSA (min_lowered: 1).
        assert_gate_parity(
            "and_or_statement_fallback",
            r#"
    function add_one(x::Int64)
        return x + 1
    end

    function guard_positive(x::Int64)
        x <= 0 && throw(ArgumentError("must be positive"))
        return add_one(x)
    end

    function guard_nonempty(s::String)
        isempty(s) && throw(ArgumentError("must be non-empty"))
        return s
    end

    println(guard_positive(5))
    println(guard_positive(1))
    println(guard_nonempty("hello"))
    "#,
            "6\n2\nhello\n",
            1,
        );
    }

    #[test]
    fn ssa_pipeline_parity_const_prop_issue_8552() {
        assert_category_parity("const_prop");
    }

    #[test]
    fn ssa_pipeline_parity_closures_issue_8552() {
        assert_category_parity("closures");
    }

    #[test]
    fn ssa_pipeline_parity_dispatch_issue_8552() {
        assert_category_parity("dispatch");
    }

    // Issue #8832: expand parity coverage to additional fixture categories.
    // Each category is independently green with `SJULIA_SSA_PIPELINE=1`.

    #[test]
    fn ssa_pipeline_parity_control_flow_issue_8832() {
        assert_category_parity("control_flow");
    }

    // Issue #9671 Phase 2: the `function` category was merged into `functions`,
    // so its former parity coverage now lives under the `functions` test below.
    #[test]
    fn ssa_pipeline_parity_functions_issue_8832() {
        assert_category_parity("functions");
    }
}
