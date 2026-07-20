//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod register_vm_8448_tests {
    use subset_julia_vm::register_vm::{RegisterProgram, RegisterVm};
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm_bytecode::{CompiledProgram, Instr, Value};

    fn straight_line_i64_stack_program() -> CompiledProgram {
        CompiledProgram {
            code: vec![
                Instr::PushI64(40),
                Instr::StoreSlotI64(0),
                Instr::LoadSlotI64(0),
                Instr::PushI64(2),
                Instr::AddI64,
                Instr::ReturnI64,
            ],
            source_map: Vec::new(),
            functions: Vec::new(),
            struct_defs: Vec::new(),
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            enum_defs: Vec::new(),
            show_methods: Vec::new(),
            print_methods: Vec::new(),
            entry: 0,
            specializable_functions: Vec::new(),
            runtime_specialization_map: Vec::new(),
            inference_global_types_snapshot: Vec::new(),
            specialization_disable_flags: Default::default(),
            compile_context: None,
            base_function_count: 0,
            macro_bindings: Default::default(),
            module_registry: Default::default(),
            global_slot_names: vec!["x".to_string()],
            global_slot_types: Vec::new(),
            global_slot_count: 1,
            main_scope_names: Default::default(),
        }
    }

    #[test]
    fn register_vm_prototype_runs_straight_line_i64_fixture_issue_8448() {
        let compiled = straight_line_i64_stack_program();
        let register_program = RegisterProgram::from_stack_program(&compiled)
            .expect("lower stack bytecode to registers");

        let metrics = register_program.metrics();
        assert!(metrics.bytecode_bytes > 0);
        assert!(metrics.frame_registers >= 2);

        let mut vm = RegisterVm::new(register_program, StableRng::new(0));
        let result = vm.run().expect("run register VM prototype");
        assert!(
            matches!(result, Value::I64(42)),
            "expected Int64(42), got {result:?}"
        );
        assert_eq!(vm.dispatch_count(), metrics.dispatch_count);
    }
}

mod register_vm_parity_8558_tests {
    //! Register VM ↔ stack VM parity harness (Issue #8558).
    //!
    //! Runs real compiled fixtures twice — once with the `SJULIA_REGISTER_VM=1`
    //! gate off (production stack VM) and once with it on (eligible function
    //! bodies on the register VM prototype) — and diffs the printed output. The
    //! fixture sources were verified against upstream Julia
    //! (`julia --startup-file=no`): fib(20) = 6765, both calc_pi variants print
    //! 3.1415826535897198, countdown(500) = 0.
    //!
    //! Also records the per-function `RegisterVmMetrics` for the covered
    //! benchmark bodies; the numbers feed the Issue #8559 measurement matrix and
    //! are documented in `docs/vm/REGISTER_VM.md`.

    use std::sync::Mutex;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
    use subset_julia_vm::register_vm::RegisterProgram;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    /// Serializes process-env manipulation across tests: nextest runs one process
    /// per test, but plain `cargo test` shares the environment between threads.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const GATE_ENV: &str = "SJULIA_REGISTER_VM";

    /// Recursive benchmark (`fib`-class, Issue #8448 target list).
    const FIB_SRC: &str = r#"
    function fib(n::Int64)
        if n <= 1
            return n
        end
        return fib(n - 1) + fib(n - 2)
    end

    println(fib(20))
    "#;

    /// Loop benchmark (`calc_pi`-class), `while` form: F64 arithmetic, ToF64,
    /// NegFloat intrinsic, compare-and-branch fusions.
    const CALC_PI_WHILE_SRC: &str = r#"
    function calc_pi(n::Int64)
        acc = 0.0
        sign = 1.0
        k = 0
        while k < n
            acc = acc + sign / (2.0 * k + 1.0)
            sign = -sign
            k = k + 1
        end
        return 4.0 * acc
    end

    println(calc_pi(100000))
    "#;

    /// Loop benchmark, `for` form: exercises the counted-loop superinstructions
    /// (JumpIfGtI64Slots, AddConstI64SlotAndJumpIfLe, LoadSlotI64ToF64,
    /// LoadAddF64Slot) plus ModI64.
    const CALC_PI_FOR_SRC: &str = r#"
    function calc_pi_for(n::Int64)
        acc = 0.0
        for k in 0:(n - 1)
            term = (4.0 * (1.0 - 2.0 * (k % 2))) / (2.0 * Float64(k) + 1.0)
            acc += term
        end
        return acc
    end

    println(calc_pi_for(100000))
    "#;

    /// Deep recursion (depth 500 > the old MAX_REGISTER_VM_NESTING = 64):
    /// verifies recursive calls stay on explicit register frames instead of
    /// recursing through the host Rust stack or falling through the old nesting
    /// cap.
    const COUNTDOWN_SRC: &str = r#"
    function countdown(n::Int64)
        if n <= 0
            return 0
        end
        return countdown(n - 1)
    end

    println(countdown(500))
    "#;

    /// Explicit numeric type-constructor call in a hot loop (Issue #9803):
    /// `Float64(i)` on a statically I64 loop variable. Before the shared
    /// planned IR carried a structured conversion node, this call compiled
    /// to a generic `Expr::Call{"Float64", ...}` in the shared plan that the
    /// register backend could not resolve as a call target (there is no
    /// `Float64` function index — it is the `BuiltinId::Float64` builtin),
    /// so the whole function stayed on the stack VM.
    const FLOAT64_CTOR_WHILE_SRC: &str = r#"
    function sum_as_float(n::Int64)
        total = 0.0
        i = 1
        while i <= n
            total += Float64(i)
            i += 1
        end
        return total
    end

    println(sum_as_float(1000))
    "#;

    /// Unsupported by the current register subset: Float32 slot arithmetic must
    /// stay on the stack VM instead of being mis-lowered as Int64 or Float64.
    const F32_ARITH_SRC: &str = r#"
    function f32_store_load()
        x = Float32(1.5)
        y = Float32(2.5)
        x + y
    end

    println(f32_store_load())
    "#;

    /// Unsupported by the current register subset: BigInt arithmetic must stay
    /// on the stack VM instead of being mis-lowered as Int64 arithmetic.
    const BIGINT_ARITH_SRC: &str = r#"
    function add_big(a::BigInt, b::BigInt)
        return a + b
    end

    println(add_big(BigInt(10), BigInt(20)))
    "#;

    struct RunResult {
        output: String,
        register_calls: u64,
        fallback_calls: u64,
        dispatch_total: u64,
    }

    fn run_source(src: &str) -> RunResult {
        let program = parse_and_lower_with_base_dir(src, None)
            .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
        let compiled =
            compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
        RunResult {
            output: vm.get_output().to_string(),
            register_calls: vm.register_vm_executed_calls(),
            fallback_calls: vm.register_vm_fallback_calls(),
            dispatch_total: vm.register_vm_dispatch_total(),
        }
    }

    /// Run `src` with the gate off and on; the printed output must be identical.
    /// SSA-planned functions must actually execute on the register VM. Functions
    /// that still compile through the legacy Core-IR path have no shared plan under
    /// Issue #9089, so the gate must leave them on the stack VM rather than
    /// translating from stack bytecode.
    fn assert_gate_parity(
        name: &str,
        src: &str,
        expected_output: &str,
        expect_register_calls: bool,
    ) -> RunResult {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        std::env::remove_var(GATE_ENV);
        let off = run_source(src);
        assert_eq!(
            off.register_calls, 0,
            "{name}: gate off must never use the register VM"
        );

        std::env::set_var(GATE_ENV, "1");
        let on = run_source(src);
        std::env::remove_var(GATE_ENV);

        assert_eq!(
            off.output, on.output,
            "{name}: register VM output must match the stack VM"
        );
        assert_eq!(
            off.output, expected_output,
            "{name}: output must match upstream Julia"
        );
        if expect_register_calls {
            assert!(
                on.register_calls > 0,
                "{name}: gate on must execute at least one call on the register VM \
                 (fallbacks: {})",
                on.fallback_calls
            );
            assert!(
                on.dispatch_total > 0,
                "{name}: register VM dispatch count must be recorded"
            );
        } else {
            assert_eq!(
                on.register_calls, 0,
                "{name}: SSA-ineligible function must not translate from stack bytecode"
            );
        }
        eprintln!(
            "[register-vm parity] {name}: register_calls={} fallback_calls={} dispatch_total={}",
            on.register_calls, on.fallback_calls, on.dispatch_total
        );
        on
    }

    #[test]
    fn register_vm_parity_fib_recursion_issue_8558() {
        assert_gate_parity("fib", FIB_SRC, "6765\n", true);
    }

    #[test]
    fn register_vm_parity_calc_pi_while_loop_issue_8558() {
        let on = assert_gate_parity(
            "calc_pi_while",
            CALC_PI_WHILE_SRC,
            "3.1415826535897198\n",
            true,
        );
        assert!(
            on.dispatch_total <= 20,
            "P3 shared-plan loop blocks should keep calc_pi_while dispatch below 20, got {}",
            on.dispatch_total
        );
    }

    #[test]
    fn register_vm_parity_calc_pi_for_loop_issue_8558() {
        assert_gate_parity(
            "calc_pi_for",
            CALC_PI_FOR_SRC,
            "3.1415826535897198\n",
            false,
        );
    }

    #[test]
    fn register_vm_parity_deep_recursion_nesting_cap_issue_8558() {
        let on = assert_gate_parity("countdown", COUNTDOWN_SRC, "0\n", true);
        assert_eq!(
            on.register_calls, 501,
            "countdown(500) must execute every recursive invocation on register frames"
        );
    }

    #[test]
    fn register_vm_parity_float64_constructor_hot_loop_issue_9803() {
        assert_gate_parity(
            "float64_ctor_while",
            FLOAT64_CTOR_WHILE_SRC,
            "500500.0\n",
            true,
        );
    }

    #[test]
    fn register_vm_f32_arith_stays_on_stack_issue_10047() {
        assert_gate_parity("f32_arith", F32_ARITH_SRC, "4.0\n", false);
    }

    #[test]
    fn register_vm_bigint_arith_stays_on_stack_issue_10047() {
        assert_gate_parity("bigint_arith", BIGINT_ARITH_SRC, "30\n", false);
    }

    /// Static translation metrics for the covered benchmark bodies (Issue #8559
    /// measurement input; documented in docs/vm/REGISTER_VM.md).
    #[test]
    fn register_vm_metrics_for_covered_fixtures_issue_8558() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(GATE_ENV);

        for (name, src, func_name) in [
            ("fib", FIB_SRC, "fib"),
            ("calc_pi_while", CALC_PI_WHILE_SRC, "calc_pi"),
            ("calc_pi_for", CALC_PI_FOR_SRC, "calc_pi_for"),
        ] {
            let program = parse_and_lower_with_base_dir(src, None)
                .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
            let compiled =
                compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
            let func = compiled
                .functions
                .iter()
                .find(|f| f.name == func_name)
                .unwrap_or_else(|| panic!("{name}: function {func_name} not found"));
            let stack_len = func.code_end - func.entry;
            let register_program = RegisterProgram::from_stack_function(&compiled.code, func)
                .unwrap_or_else(|e| panic!("{name}: body must translate: {e}"));
            let metrics = register_program.metrics();
            assert!(metrics.bytecode_bytes > 0);
            assert!(metrics.frame_registers > 0);
            eprintln!(
                "[register-vm metrics] {name}: stack_instrs={stack_len} register_instrs={} \
                 bytecode_bytes={} frame_registers={} frame_slots={}",
                metrics.dispatch_count,
                metrics.bytecode_bytes,
                metrics.frame_registers,
                metrics.frame_slots
            );
        }
    }
}

mod register_vm_measurements_8559_tests {
    //! Issue #8559 measurement-infrastructure tests.
    //!
    //! Covers the pieces added for the register-vs-stack VM measurement matrix:
    //! the opt-in stack VM counters (`SJULIA_STACK_VM_METRICS` /
    //! `set_stack_vm_metrics_forced`), the environment-free register gate
    //! override (`set_register_vm_forced`, needed on wasm32 where no process
    //! environment exists), and the attractor-style Float64 benchmark body
    //! (translation + engine parity). The fixture source was verified against
    //! upstream Julia (`julia --startup-file=no`): lorenz_accum(1000000) prints
    //! -11779.830551874697.

    use std::sync::Mutex;

    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
    use subset_julia_vm::register_vm::RegisterProgram;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::{
        set_register_vm_forced, set_stack_vm_metrics_forced, StackVmMetrics, Vm,
    };

    /// Serializes process-global state (env vars, forced gates) across tests:
    /// nextest runs one process per test, but plain `cargo test` shares them.
    static GLOBAL_STATE_LOCK: Mutex<()> = Mutex::new(());

    /// Attractor-style Float64 loop (Lorenz step + accumulator), the third
    /// Issue #8448 benchmark next to fib/calc_pi.
    const LORENZ_SRC: &str = r#"
    function lorenz_accum(n::Int64)
        x = 1.0
        y = 1.0
        z = 1.0
        dt = 0.001
        acc = 0.0
        k = 0
        while k < n
            dx = 10.0 * (y - x)
            dy = x * (28.0 - z) - y
            dz = x * y - 2.6666666666666665 * z
            x = x + dt * dx
            y = y + dt * dy
            z = z + dt * dz
            acc = acc + x
            k = k + 1
        end
        return acc
    end

    println(lorenz_accum(1000000))
    "#;

    const LORENZ_EXPECTED: &str = "-11779.830551874697\n";

    fn run_source(src: &str) -> (String, Option<StackVmMetrics>, u64, u64) {
        let program = parse_and_lower_with_base_dir(src, None)
            .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
        let compiled =
            compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
        (
            vm.get_output().to_string(),
            vm.stack_vm_metrics(),
            vm.register_vm_executed_calls(),
            vm.register_vm_dispatch_total(),
        )
    }

    /// Default-off contract: without the env gate or the forced override, no
    /// metrics are collected and the accessor reports `None`.
    #[test]
    fn stack_vm_metrics_default_off_issue_8559() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("SJULIA_STACK_VM_METRICS");
        set_stack_vm_metrics_forced(false);

        let (output, metrics, _, _) = run_source("println(1 + 2)");
        assert_eq!(output, "3\n");
        assert!(
            metrics.is_none(),
            "stack VM metrics must be disabled by default"
        );
    }

    /// The counters are deterministic (load-independent): two identical runs
    /// record identical dispatch counts and high-water marks, and a real program
    /// records non-trivial values.
    #[test]
    fn stack_vm_metrics_deterministic_counters_issue_8559() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_register_vm_forced(false);
        set_stack_vm_metrics_forced(true);
        let (out_a, metrics_a, _, _) = run_source(LORENZ_SRC);
        let (out_b, metrics_b, _, _) = run_source(LORENZ_SRC);
        set_stack_vm_metrics_forced(false);

        assert_eq!(out_a, LORENZ_EXPECTED, "output must match upstream Julia");
        assert_eq!(out_a, out_b);
        let metrics_a = metrics_a.expect("metrics were forced on");
        let metrics_b = metrics_b.expect("metrics were forced on");
        assert_eq!(
            metrics_a, metrics_b,
            "stack VM counters must be deterministic across identical runs"
        );
        assert!(!metrics_a.is_empty(), "counters must record activity");
        assert!(metrics_a.dispatches > 0);
        assert!(metrics_a.operand_stack_high_water > 0);
        assert!(metrics_a.frames_high_water > 0);
    }

    /// `set_register_vm_forced` must gate calls onto the register VM without the
    /// `SJULIA_REGISTER_VM` env var (wasm32 has no environment), with output
    /// parity against both the stack VM and upstream Julia for the attractor
    /// benchmark body.
    #[test]
    fn register_vm_forced_gate_lorenz_parity_issue_8559() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("SJULIA_REGISTER_VM");

        set_register_vm_forced(false);
        let (off_output, _, off_register_calls, _) = run_source(LORENZ_SRC);
        assert_eq!(
            off_register_calls, 0,
            "gate off must never use the register VM"
        );

        set_register_vm_forced(true);
        let (on_output, _, on_register_calls, on_register_dispatches) = run_source(LORENZ_SRC);
        set_register_vm_forced(false);

        assert_eq!(
            off_output, on_output,
            "register VM output must match the stack VM"
        );
        assert_eq!(
            off_output, LORENZ_EXPECTED,
            "output must match upstream Julia"
        );
        assert!(
            on_register_calls > 0,
            "forced gate must execute at least one call on the register VM"
        );
        eprintln!(
            "[register-vm parity] lorenz: register_calls={on_register_calls} \
             dispatch_total={on_register_dispatches}"
        );
        assert!(
            on_register_dispatches <= 20,
            "P3 shared-plan loop blocks should keep lorenz register dispatch below 20, got {}",
            on_register_dispatches
        );
    }

    /// The attractor body must stay fully translatable (it feeds the static
    /// bytecode-size columns of the Issue #8559 matrix).
    #[test]
    fn lorenz_attractor_body_translates_issue_8559() {
        let _guard = GLOBAL_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let program = parse_and_lower_with_base_dir(LORENZ_SRC, None)
            .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
        let compiled =
            compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
        let func = compiled
            .functions
            .iter()
            .find(|f| f.name == "lorenz_accum")
            .expect("lorenz_accum not found");
        let register_program = RegisterProgram::from_stack_function(&compiled.code, func)
            .unwrap_or_else(|e| panic!("lorenz_accum body must translate: {e}"));
        let metrics = register_program.metrics();
        assert!(metrics.bytecode_bytes > 0);
        assert!(metrics.frame_registers > 0);
        assert!(metrics.frame_slots > 0);
    }
}
