//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod mandelbrot_6259_tests {
    #[cfg(feature = "profiling")]
    use std::collections::HashMap;
    use std::collections::HashSet;
    use subset_julia_vm::builtins::BuiltinId;
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    #[cfg(feature = "profiling")]
    use subset_julia_vm::vm::profiler;
    use subset_julia_vm::vm::specialize::specialize_function;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm_bytecode::{CompiledProgram, Instr, Value, ValueType};

    fn compile_source(source: &str) -> CompiledProgram {
        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).expect("lower source");
        compile_with_cache(&program).expect("compile source")
    }

    #[cfg(feature = "profiling")]
    fn run_with_profile(source: &str) -> HashMap<String, u64> {
        let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));

        profiler::clear();
        profiler::enable();
        let _ = vm.run().expect("run with VM profiler");
        profiler::disable();

        profiler::get_results().into_iter().collect()
    }

    // Match benchmarks/mandelbrot_bench_broadcast_untyped.jl: untyped params and
    // `z * z + c` (not `z^2`) so runtime specialization takes the same path as the
    // acceptance benchmark (Issue #10704).
    const MANDELBROT_ESCAPE_SOURCE: &str = r#"
    function mandelbrot_escape(c, maxiter)
        z = 0.0 + 0.0im
        for k in 1:maxiter
            if abs2(z) > 4.0
                return k
            end
            z = z * z + c
        end
        return maxiter
    end
    "#;

    const MANDELBROT_ESCAPE_K_MINUS_ONE_SOURCE: &str = r#"
    function mandelbrot_escape(c::Complex, maxiter::Int64)::Int64
        z = 0.0 + 0.0im
        for k in 1:maxiter
            if abs2(z) > 4.0
                return k - 1
            end
            z = z * z + c
        end
        return maxiter
    end
    "#;

    const VM_MANDELBROT_SOURCE: &str = include_str!("../../benchmarks/vm_mandelbrot.jl");

    fn function_body<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a [Instr] {
        let function = compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{name}' not found"));
        &compiled.code[function.code_start..function.code_end]
    }

    #[test]
    fn runtime_specialized_mandelbrot_escape_uses_concrete_complex_f64_ops_6259() {
        let compiled = compile_source(&format!(
            "{MANDELBROT_ESCAPE_SOURCE}\nmandelbrot_escape(0.0 + 0.0im, 10)"
        ));
        let specializable = compiled
            .specializable_functions
            .iter()
            .find(|f| f.name == "mandelbrot_escape")
            .expect("mandelbrot_escape should be registered for runtime specialization");
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            &specializable.ir,
            &[ValueType::ComplexF64, ValueType::I64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize mandelbrot_escape(::ComplexF64, ::Int64)");
        let code = &specialized.code;

        assert_eq!(specialized.return_type, ValueType::I64);
        assert!(
            !code.iter().any(|instr| matches!(instr, Instr::DynamicPow)),
            "ComplexF64 z^2 should not emit DynamicPow: {code:?}"
        );
        assert!(
            !code
                .iter()
                .any(|instr| matches!(instr, Instr::CallDynamicBinaryBoth(_, _))),
            "ComplexF64 z^2 + c should not emit generic binary dispatch: {code:?}"
        );
        assert!(
            !code
                .iter()
                .any(|instr| matches!(instr, Instr::CallResolved(_, _))),
            "abs2(::ComplexF64) should inline field arithmetic, not call a resolved method: {code:?}"
        );
        assert!(
            code.iter()
                .filter(|instr| matches!(instr, Instr::GetField(0) | Instr::GetField(1)))
                .count()
                >= 2,
            "ComplexF64 argument should be decomposed into real/imag field loads: {code:?}"
        );
        assert!(
            code.iter()
                .filter(|instr| matches!(
                    instr,
                    Instr::StoreF64(name) if name.starts_with("__sjulia_cx_re_") || name.starts_with("__sjulia_cx_im_")
                ))
                .count()
                >= 4,
            "ComplexF64 arithmetic should keep (re, im) in unboxed SROA slots: {code:?}"
        );

        assert!(
            code.iter()
                .filter(|instr| matches!(instr, Instr::MulF64))
                .count()
                >= 4,
            "ComplexF64 abs2/square should use typed Float64 multiplication: {code:?}"
        );
    }

    /// Regression guard for Issue #10799: the runtime specializer's
    /// ComplexF64 codegen for `z*z + c`/`abs2(z)` used to spill every
    /// operand to fresh temp locals unconditionally, producing far more raw
    /// instructions (and, after predecode fusion, far more `TypedLoopOp`s —
    /// 27 vs the static compiler's 8 for this same loop body, see the PR)
    /// than necessary. Pin an upper bound on the raw instruction count so a
    /// future change can't silently regress back toward the old shape.
    /// (Measured before the #10799 fix: 68 raw instructions.)
    #[test]
    fn runtime_specialized_mandelbrot_escape_op_count_10799() {
        let compiled = compile_source(&format!(
            "{MANDELBROT_ESCAPE_SOURCE}\nmandelbrot_escape(0.0 + 0.0im, 10)"
        ));
        let specializable = compiled
            .specializable_functions
            .iter()
            .find(|f| f.name == "mandelbrot_escape")
            .expect("mandelbrot_escape should be registered for runtime specialization");
        let type_object_names = HashSet::new();
        let specialized = specialize_function(
            &specializable.ir,
            &[ValueType::ComplexF64, ValueType::I64],
            &compiled.struct_defs,
            &type_object_names,
            None,
            false,
            false,
        )
        .expect("specialize mandelbrot_escape(::ComplexF64, ::Int64)");
        assert!(
            specialized.code.len() <= 60,
            "Issue #10799: expected <=60 raw instructions for the specialized \
             mandelbrot_escape body (was 68 before the fix, 58 after), got {}: {:#?}",
            specialized.code.len(),
            specialized.code
        );
    }

    #[test]
    fn mandelbrot_escape_original_source_preserves_results_6259() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    mandelbrot_escape(0.0 + 0.0im, 10) +
    mandelbrot_escape(-0.75 + 0.0im, 20) * 10 +
    mandelbrot_escape(1.0 + 1.0im, 20) * 100
    "#
        );
        let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
        let result = vm.run().expect("run Mandelbrot escape probes");
        match result {
            Value::I64(value) => assert_eq!(value, 510),
            other => panic!("expected Int64 Mandelbrot probe checksum, got {other:?}"),
        }
    }

    #[test]
    fn mandelbrot_broadcast_original_source_preserves_results_6259() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    grid = mandelbrot_escape.(C, Ref(10))
    grid[1,1] + grid[1,2] + grid[2,1] + grid[2,2]
    "#
        );
        let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
        let result = vm.run().expect("run broadcast Mandelbrot probes");
        match result {
            Value::I64(value) => assert_eq!(value, 25),
            other => panic!("expected Int64 broadcast Mandelbrot checksum, got {other:?}"),
        }
    }

    #[test]
    fn untyped_complex_broadcast_uses_typed_kernel_10704() {
        // Direct bulk kernel entry (what Base.Broadcast.materialize calls).
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    C = ComplexF64[0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    fast = _try_broadcast_typed_kernel(mandelbrot_escape, C, 10)
    result = -1
    if fast === nothing
        result = -1
    else
        result = length(fast)
    end
    result
    "#
        );
        let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
        let result = vm
            .run()
            .expect("run untyped ComplexF64 broadcast typed-kernel probe");
        match result {
            Value::I64(value) => assert_eq!(value, 4),
            other => panic!("expected typed-kernel Mandelbrot result length, got {other:?}"),
        }
    }

    #[test]
    fn untyped_complex_broadcast_dot_form_matches_typed_kernel_10704() {
        // Real call-site shape from mandelbrot_bench_broadcast_untyped.jl:
        // `mandelbrot_escape.(C, maxiter)` with a bare Int scalar (not Ref).
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    C = ComplexF64[0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    maxiter = 10
    counts = mandelbrot_escape.(C, maxiter)
    sum(counts)
    "#
        );
        let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
        let result = vm
            .run()
            .expect("run untyped ComplexF64 f.(C, maxiter) broadcast");
        match result {
            // Same escape checksum as mandelbrot_broadcast_original_source_preserves_results_6259
            // (return k, not k-1, maxiter=10 on the 2x2 probe grid).
            Value::I64(value) => assert_eq!(value, 25),
            other => panic!("expected Int64 broadcast checksum, got {other:?}"),
        }
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn untyped_complex_broadcast_hits_typed_kernel_event_10704() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    C = ComplexF64[0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    counts = mandelbrot_escape.(C, 10)
    sum(counts)
    "#
        );
        let counts = run_with_profile(&source);
        assert!(
            counts.get("BroadcastTypedKernelHit").copied().unwrap_or(0) > 0,
            "untyped mandelbrot_escape.(C, maxiter) must take the bulk typed \
             broadcast kernel (runtime-specialized ComplexF64 body): {counts:?}"
        );
        assert_eq!(
            counts.get("DynamicPow").copied().unwrap_or(0),
            0,
            "specialized ComplexF64 z*z must not fall back to DynamicPow: {counts:?}"
        );
    }

    #[test]
    fn untyped_complex_two_array_broadcast_matches_upstream_10704() {
        // Issue #10704: `_try_broadcast_typed_kernel` (Issues #8797/#9693) only
        // handles a broadcast with EXACTLY ONE array argument (`f.(A, scalar)`);
        // a two-array `f.(A, B)` broadcast bails out of that bulk kernel and
        // falls to `Base.Broadcast`'s generic elementwise loop, which calls `f`
        // through a *function value* per element (`Instr::CallFunctionVariable`).
        // That per-element call path is materially different from the one the
        // acceptance benchmark exercises, and it had no ComplexF64 coverage.
        // Pin its correctness here (values verified against upstream `julia`;
        // see the matching fixture at
        // `tests/fixtures/broadcast/untyped_complex_broadcast_call_variable_10704.jl`).
        let source = r#"
    function classify(a, b)
        if abs2(a) > abs2(b)
            return 1
        end
        return 0
    end
    A = ComplexF64[1.0 + 1.0im 2.0 + 0.5im; -1.0 + 0.5im 0.0 + 0.0im]
    B = ComplexF64[0.5 + 0.5im 1.0 + 1.0im; 0.5 - 0.5im 2.0 + 2.0im]
    counts = classify.(A, B)
    sum(counts)
    "#;
        let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));
        let result = vm
            .run()
            .expect("run untyped two-array ComplexF64 broadcast via CallFunctionVariable");
        match result {
            Value::I64(value) => assert_eq!(value, 3),
            other => panic!("expected Int64 checksum, got {other:?}"),
        }
    }

    #[test]
    fn mandelbrot_escape_k_minus_one_source_preserves_results_8796() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_K_MINUS_ONE_SOURCE}

    mandelbrot_escape(0.0 + 0.0im, 10) +
    mandelbrot_escape(-0.75 + 0.0im, 20) * 10 +
    mandelbrot_escape(1.0 + 1.0im, 20) * 100
    "#
        );
        let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
        let result = vm.run().expect("run k-1 Mandelbrot escape probes");
        match result {
            Value::I64(value) => assert_eq!(value, 410),
            other => panic!("expected Int64 Mandelbrot k-1 checksum, got {other:?}"),
        }
    }

    #[test]
    fn mandelbrot_broadcast_k_minus_one_source_preserves_results_8797() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_K_MINUS_ONE_SOURCE}

    C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    grid = mandelbrot_escape.(C, Ref(10))
    grid[1,1] + grid[1,2] + grid[2,1] + grid[2,2]
    "#
        );
        let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
        let result = vm.run().expect("run k-1 broadcast Mandelbrot probes");
        match result {
            Value::I64(value) => assert_eq!(value, 22),
            other => panic!("expected Int64 broadcast k-1 checksum, got {other:?}"),
        }
    }

    #[test]
    fn mandel_count_fuses_loop_branches_and_i64_float_conversions_6167() {
        let compiled = compile_source(VM_MANDELBROT_SOURCE);
        let body = function_body(&compiled, "mandel_count");

        assert!(
            body.iter()
                .filter(|instr| matches!(instr, Instr::JumpIfGtI64Slots(_, _, _)))
                .count()
                >= 2,
            "mandel_count should compare x/y loop slots directly: {body:?}"
        );
        assert!(
            body.iter()
                .filter(|instr| matches!(instr, Instr::LoadSlotI64ToF64(_)))
                .count()
                >= 4,
            "mandel_count should fuse Float64(slot) conversions: {body:?}"
        );
        assert!(
            !body.windows(2).any(|window| {
                matches!(
                    window,
                    [
                        Instr::LoadSlotI64(_),
                        Instr::CallBuiltin(BuiltinId::Float64, 1)
                    ]
                )
            }),
            "mandel_count should not leave LoadSlotI64 + CallBuiltin(Float64): {body:?}"
        );

        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let _ = vm.run().expect("run VM Mandelbrot benchmark source");
        assert_eq!(vm.get_output(), "166265\n");
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn broadcast_runtime_callable_escape_avoids_dynamic_pow_6259() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    grid = mandelbrot_escape.(C, Ref(10))
    grid[1,1]
    "#
        );
        let counts = run_with_profile(&source);

        assert!(
            counts.get("CallFunctionVariable").copied().unwrap_or(0) > 0,
            "probe should exercise broadcast _broadcast_apply's function-variable path: {counts:?}"
        );
        assert_eq!(
            counts.get("DynamicPow").copied().unwrap_or(0),
            0,
            "broadcasted mandelbrot_escape should not execute DynamicPow in the escape loop: {counts:?}"
        );
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn broadcast_runtime_callable_escape_uses_typed_loop_6253() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    grid = mandelbrot_escape.(C, Ref(10))
    grid[1,1] + grid[1,2] + grid[2,1] + grid[2,2]
    "#
        );
        let counts = run_with_profile(&source);

        assert!(
            counts
                .get("ExecutableBlock::TypedLoop")
                .copied()
                .unwrap_or(0)
                > 0,
            "broadcasted mandelbrot_escape should run through the typed-loop path: {counts:?}"
        );
        assert_eq!(
            counts
                .get("ExecutableBlock::ComplexF64MandelbrotEscapeLoop")
                .copied()
                .unwrap_or(0),
            0,
            "the ComplexF64MandelbrotEscapeLoopBlock fast path should be retired: {counts:?}"
        );
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn broadcast_runtime_callable_k_minus_one_escape_uses_typed_loop_8797() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_K_MINUS_ONE_SOURCE}

    C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
    grid = mandelbrot_escape.(C, Ref(10))
    grid[1,1] + grid[1,2] + grid[2,1] + grid[2,2]
    "#
        );
        let counts = run_with_profile(&source);

        assert!(
            counts
                .get("ExecutableBlock::TypedLoop")
                .copied()
                .unwrap_or(0)
                > 0,
            "broadcasted k-1 mandelbrot_escape should run through the typed-loop path: {counts:?}"
        );
        assert_eq!(
            counts
                .get("ExecutableBlock::ComplexF64MandelbrotEscapeLoop")
                .copied()
                .unwrap_or(0),
            0,
            "the ComplexF64MandelbrotEscapeLoopBlock fast path should be retired: {counts:?}"
        );
    }

    #[cfg(feature = "profiling")]
    #[test]
    fn function_variable_escape_avoids_dynamic_complex_ops_6259() {
        let source = format!(
            r#"{MANDELBROT_ESCAPE_SOURCE}

    f = mandelbrot_escape
    f(0.0 + 0.0im, 10)
    "#
        );
        let counts = run_with_profile(&source);

        assert!(
            counts.get("CallFunctionVariable").copied().unwrap_or(0) > 0,
            "probe should exercise function-variable dispatch: {counts:?}"
        );
        assert_eq!(
            counts.get("DynamicPow").copied().unwrap_or(0),
            0,
            "function-variable mandelbrot_escape should not execute DynamicPow: {counts:?}"
        );
    }
}

mod mandelbrot_coordinate_comparison_test {
    //! Regression coverage for scalar Mandelbrot arithmetic.

    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    fn run_and_get_f64(src: &str) -> f64 {
        match compile_and_run_value(src, 12345).expect("Execution failed") {
            Value::F64(v) => v,
            Value::I64(v) => v as f64,
            other => panic!("Expected numeric value, got {other:?}"),
        }
    }

    fn run_and_get_i64(src: &str) -> i64 {
        match compile_and_run_value(src, 12345).expect("Execution failed") {
            Value::I64(v) => v,
            Value::F64(v) => v as i64,
            other => panic!("Expected numeric value, got {other:?}"),
        }
    }

    fn rust_mandelbrot_escape(cr: f64, ci: f64, maxiter: i64) -> i64 {
        let mut zr = 0.0;
        let mut zi = 0.0;
        for k in 1..=maxiter {
            let zr2 = zr * zr;
            let zi2 = zi * zi;
            if zr2 + zi2 > 4.0 {
                return k;
            }
            zi = 2.0 * zr * zi + ci;
            zr = zr2 - zi2 + cr;
        }
        maxiter
    }

    #[test]
    fn test_coordinate_calculations_scalar() {
        let max_diff = run_and_get_f64(
            r#"
    maxdiff = 0.0
    for row in 0:10
        for col in 0:20
            ci = 1.0 - row * 0.2
            cr = -2.0 + col * 0.15
            expected_ci = 1.0 - row * 0.2
            expected_cr = -2.0 + col * 0.15
            ci_diff = ci - expected_ci
            cr_diff = cr - expected_cr
            if ci_diff < 0.0
                ci_diff = -ci_diff
            end
            if cr_diff < 0.0
                cr_diff = -cr_diff
            end
            if ci_diff > maxdiff
                maxdiff = ci_diff
            end
            if cr_diff > maxdiff
                maxdiff = cr_diff
            end
        end
    end
    maxdiff
    "#,
        );

        assert!(
            max_diff < 1e-9,
            "coordinate difference too large: {max_diff:.2e}"
        );
    }

    #[test]
    fn test_mandelbrot_escape_times() {
        let vm_checksum = run_and_get_i64(
            r#"
    function mandelbrot_escape(cr, ci, maxiter)
        zr = 0.0
        zi = 0.0
        for k in 1:maxiter
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0
                return k
            end
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
        end
        return maxiter
    end

    mandelbrot_escape(0.0, 0.0, 100) +
    mandelbrot_escape(-0.75, 0.0, 100) * 10 +
    mandelbrot_escape(1.0, 1.0, 100) * 100 +
    mandelbrot_escape(-0.1, 0.65, 100) * 1000
    "#,
        );

        let expected = rust_mandelbrot_escape(0.0, 0.0, 100)
            + rust_mandelbrot_escape(-0.75, 0.0, 100) * 10
            + rust_mandelbrot_escape(1.0, 1.0, 100) * 100
            + rust_mandelbrot_escape(-0.1, 0.65, 100) * 1000;
        assert_eq!(vm_checksum, expected);
    }

    #[test]
    fn test_intermediate_calculations() {
        let max_diff = run_and_get_f64(
            r#"
    maxdiff = 0.0
    row = 5
    d = row * 0.2 - 1.0
    if d < 0.0
        d = -d
    end
    if d > maxdiff
        maxdiff = d
    end
    col = 10
    d = col * 0.15 - 1.5
    if d < 0.0
        d = -d
    end
    if d > maxdiff
        maxdiff = d
    end
    row = 5
    d = 1.0 - row * 0.2 - 0.0
    if d < 0.0
        d = -d
    end
    if d > maxdiff
        maxdiff = d
    end
    col = 10
    d = -2.0 + col * 0.15 - -0.5
    if d < 0.0
        d = -d
    end
    if d > maxdiff
        maxdiff = d
    end
    zr = 0.5
    zi = 0.3
    d = 2.0 * zr * zi - 0.3
    if d < 0.0
        d = -d
    end
    if d > maxdiff
        maxdiff = d
    end
    maxdiff
    "#,
        );

        assert!(
            max_diff < 1e-9,
            "intermediate difference too large: {max_diff:.2e}"
        );
    }

    #[test]
    fn test_specific_mandelbrot_coordinates() {
        let vm_checksum = run_and_get_i64(
            r#"
    function mandelbrot_escape(cr, ci, maxiter)
        zr = 0.0
        zi = 0.0
        for k in 1:maxiter
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0
                return k
            end
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
        end
        return maxiter
    end

    checksum = 0
    for row in 0:5
        ci = 1.0 - row * 0.2
        for col in 0:5
            cr = -2.0 + col * 0.15
            checksum = checksum + mandelbrot_escape(cr, ci, 50)
        end
    end
    checksum
    "#,
        );

        let expected: i64 = (0..=5)
            .flat_map(|row| (0..=5).map(move |col| (row, col)))
            .map(|(row, col)| {
                let ci = 1.0 - row as f64 * 0.2;
                let cr = -2.0 + col as f64 * 0.15;
                rust_mandelbrot_escape(cr, ci, 50)
            })
            .sum();
        assert_eq!(vm_checksum, expected);
    }

    #[test]
    fn test_mandelbrot_step_by_step() {
        let vm_escape = run_and_get_i64(
            r#"
    function mandelbrot_escape(cr, ci, maxiter)
        zr = 0.0
        zi = 0.0
        for k in 1:maxiter
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0
                return k
            end
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
        end
        return maxiter
    end

    mandelbrot_escape(-2.0, 0.4, 50)
    "#,
        );

        assert_eq!(vm_escape, rust_mandelbrot_escape(-2.0, 0.4, 50));
    }
}

mod mandelbrot_coordinate_test {
    //! Regression coverage for Mandelbrot coordinate calculations.

    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    fn run_and_get_i64(src: &str) -> i64 {
        match compile_and_run_value(src, 12345).expect("Execution failed") {
            Value::I64(v) => v,
            Value::F64(v) => v as i64,
            other => panic!("Expected numeric value, got {other:?}"),
        }
    }

    fn rust_mandelbrot_escape(cr: f64, ci: f64, maxiter: i64) -> i64 {
        let mut zr = 0.0;
        let mut zi = 0.0;
        for k in 1..=maxiter {
            let zr2 = zr * zr;
            let zi2 = zi * zi;
            if zr2 + zi2 > 4.0 {
                return k;
            }
            zi = 2.0 * zr * zi + ci;
            zr = zr2 - zi2 + cr;
        }
        maxiter
    }

    #[test]
    fn test_mandelbrot_coordinates_row0() {
        let vm_checksum = run_and_get_i64(
            r#"
    function mandelbrot_escape(cr, ci, maxiter)
        zr = 0.0
        zi = 0.0
        for k in 1:maxiter
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0
                return k
            end
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
        end
        return maxiter
    end

    checksum = 0
    ci = 1.0
    for col in 0:5
        cr = -2.0 + col * 0.15
        checksum = checksum + mandelbrot_escape(cr, ci, 50)
    end
    checksum
    "#,
        );

        let expected: i64 = (0..=5)
            .map(|col| rust_mandelbrot_escape(-2.0 + col as f64 * 0.15, 1.0, 50))
            .sum();
        assert_eq!(vm_checksum, expected);
    }

    #[test]
    fn test_mandelbrot_coordinates_row3() {
        let vm_checksum = run_and_get_i64(
            r#"
    function mandelbrot_escape(cr, ci, maxiter)
        zr = 0.0
        zi = 0.0
        for k in 1:maxiter
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0
                return k
            end
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
        end
        return maxiter
    end

    checksum = 0
    ci = 0.4
    for col in 0:20
        cr = -2.0 + col * 0.15
        checksum = checksum + mandelbrot_escape(cr, ci, 50)
    end
    checksum
    "#,
        );

        let expected: i64 = (0..=20)
            .map(|col| rust_mandelbrot_escape(-2.0 + col as f64 * 0.15, 0.4, 50))
            .sum();
        assert_eq!(vm_checksum, expected);
    }

    #[test]
    fn test_coordinate_calculation_precision() {
        for row in 0..=10 {
            let ci_julia: f64 = 1.0 - row as f64 * 0.2;
            for col in 0..=20 {
                let cr_julia: f64 = -2.0 + col as f64 * 0.15;
                let expected_ci: f64 = 1.0 - row as f64 * 0.2;
                let expected_cr: f64 = -2.0 + col as f64 * 0.15;
                assert!(
                    (ci_julia - expected_ci).abs() < 1e-10,
                    "ci calculation mismatch"
                );
                assert!(
                    (cr_julia - expected_cr).abs() < 1e-10,
                    "cr calculation mismatch"
                );
            }
        }
    }
}

mod mandelbrot_debug_test {
    //! Regression coverage for Mandelbrot coordinate arithmetic.

    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    fn run_and_get_f64(src: &str) -> f64 {
        match compile_and_run_value(src, 12345).expect("Execution failed") {
            Value::F64(v) => v,
            Value::I64(v) => v as f64,
            other => panic!("Expected numeric value, got {other:?}"),
        }
    }

    fn run_and_get_i64(src: &str) -> i64 {
        match compile_and_run_value(src, 12345).expect("Execution failed") {
            Value::I64(v) => v,
            Value::F64(v) => v as i64,
            other => panic!("Expected numeric value, got {other:?}"),
        }
    }

    fn rust_mandelbrot_escape(cr: f64, ci: f64, maxiter: i64) -> i64 {
        let mut zr = 0.0;
        let mut zi = 0.0;
        for k in 1..=maxiter {
            let zr2 = zr * zr;
            let zi2 = zi * zi;
            if zr2 + zi2 > 4.0 {
                return k;
            }
            zi = 2.0 * zr * zi + ci;
            zr = zr2 - zi2 + cr;
        }
        maxiter
    }

    #[test]
    fn test_coordinate_calculations() {
        let max_diff = run_and_get_f64(
            r#"
    maxdiff = 0.0
    for row in 0:5
        for col in 0:5
            ci = 1.0 - row * 0.2
            cr = -2.0 + col * 0.15
            expected_ci = 1.0 - row * 0.2
            expected_cr = -2.0 + col * 0.15
            ci_diff = ci - expected_ci
            cr_diff = cr - expected_cr
            if ci_diff < 0.0
                ci_diff = -ci_diff
            end
            if cr_diff < 0.0
                cr_diff = -cr_diff
            end
            if ci_diff > maxdiff
                maxdiff = ci_diff
            end
            if cr_diff > maxdiff
                maxdiff = cr_diff
            end
        end
    end
    maxdiff
    "#,
        );

        assert!(
            max_diff < 1e-9,
            "coordinate difference too large: {max_diff:.2e}"
        );
    }

    #[test]
    fn test_mandelbrot_escape_for_coordinates() {
        let vm_checksum = run_and_get_i64(
            r#"
    function mandelbrot_escape(cr, ci, maxiter)
        zr = 0.0
        zi = 0.0
        for k in 1:maxiter
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0
                return k
            end
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
        end
        return maxiter
    end

    checksum = 0
    for row in 0:5
        ci = 1.0 - row * 0.2
        for col in 0:5
            cr = -2.0 + col * 0.15
            checksum = checksum + mandelbrot_escape(cr, ci, 50)
        end
    end
    checksum
    "#,
        );

        let expected: i64 = (0..=5)
            .flat_map(|row| (0..=5).map(move |col| (row, col)))
            .map(|(row, col)| {
                let ci = 1.0 - row as f64 * 0.2;
                let cr = -2.0 + col as f64 * 0.15;
                rust_mandelbrot_escape(cr, ci, 50)
            })
            .sum();
        assert_eq!(vm_checksum, expected);
    }

    #[test]
    fn test_row3_specific_coordinates() {
        let vm_escape = run_and_get_i64(
            r#"
    function mandelbrot_escape(cr, ci, maxiter)
        zr = 0.0
        zi = 0.0
        for k in 1:maxiter
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0
                return k
            end
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
        end
        return maxiter
    end

    mandelbrot_escape(-1.25, 0.4, 50)
    "#,
        );

        assert_eq!(vm_escape, rust_mandelbrot_escape(-1.25, 0.4, 50));
    }
}

mod test_mandelbrot_grid_comparison {
    use subset_julia_vm::*;
    use subset_julia_vm_bytecode::value::array_wrapper_value_to_array_value;
    use subset_julia_vm_bytecode::Value;

    #[test]
    fn test_mandelbrot_grid_direct_index_api_regression() {
        let src = r#"
    width = 5
    height = 5
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    ((xs' .+ im .* ys)[1, 1]).re
    "#;

        let result = compile_and_run_value(src, 0).expect("Failed to run direct Mandelbrot index");
        match result {
            Value::F64(value) => assert!((value + 2.0).abs() < 1e-10),
            other => panic!("Expected F64, got {:?}", other),
        }
    }

    #[test]
    fn test_mandelbrot_grid_assignment_index_api_regression() {
        let src = r#"
    width = 5
    height = 5
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    grid = xs' .+ im .* ys
    grid[1, 1].re
    "#;

        let result =
            compile_and_run_value(src, 0).expect("Failed to run assigned Mandelbrot index");
        match result {
            Value::F64(value) => assert!((value + 2.0).abs() < 1e-10),
            other => panic!("Expected F64, got {:?}", other),
        }
    }

    /// Test that the Mandelbrot grid computation matches Julia's output exactly.
    ///
    /// Julia code:
    /// ```julia
    /// width = 5
    /// height = 5
    /// xmin = -2.0; xmax = 1.0
    /// ymin = -1.2; ymax = 1.2
    ///
    /// xs = range(xmin, xmax; length=width)
    /// ys = range(ymax, ymin; length=height)
    ///
    /// xs' .+ im .* ys
    /// ```
    ///
    /// Expected output (5×5 Matrix{ComplexF64}):
    /// ```
    ///  -2.0+1.2im  -1.25+1.2im  -0.5+1.2im  0.25+1.2im  1.0+1.2im
    ///  -2.0+0.6im  -1.25+0.6im  -0.5+0.6im  0.25+0.6im  1.0+0.6im
    ///  -2.0+0.0im  -1.25+0.0im  -0.5+0.0im  0.25+0.0im  1.0+0.0im
    ///  -2.0-0.6im  -1.25-0.6im  -0.5-0.6im  0.25-0.6im  1.0-0.6im
    ///  -2.0-1.2im  -1.25-1.2im  -0.5-1.2im  0.25-1.2im  1.0-1.2im
    /// ```
    #[test]
    fn test_mandelbrot_grid_comparison() {
        let src = r#"
    width = 5
    height = 5
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    # Create 2D complex grid via broadcasting
    xs' .+ im .* ys
    "#;

        let result = compile_and_run_value(src, 0).expect("Failed to run Mandelbrot grid test");

        // Expected values from Julia (row-major order for readability, but Julia uses column-major)
        // Julia output:
        //  -2.0+1.2im  -1.25+1.2im  -0.5+1.2im  0.25+1.2im  1.0+1.2im
        //  -2.0+0.6im  -1.25+0.6im  -0.5+0.6im  0.25+0.6im  1.0+0.6im
        //  -2.0+0.0im  -1.25+0.0im  -0.5+0.0im  0.25+0.0im  1.0+0.0im
        //  -2.0-0.6im  -1.25-0.6im  -0.5-0.6im  0.25-0.6im  1.0-0.6im
        //  -2.0-1.2im  -1.25-1.2im  -0.5-1.2im  0.25-1.2im  1.0-1.2im
        //
        // Real parts (columns): -2.0, -1.25, -0.5, 0.25, 1.0
        // Imag parts (rows):    1.2,  0.6,   0.0, -0.6, -1.2
        let expected_re = [-2.0, -1.25, -0.5, 0.25, 1.0];
        let expected_im = [1.2, 0.6, 0.0, -0.6, -1.2];

        // Issue #3908: route the native-array destructure through the shared
        // `native_array_value_ref` helper instead of pattern-matching
        // the legacy native-array variant directly. The early-return panic
        // preserves the original "Expected Array, got ..." diagnostic when
        // `result` is not the native array carrier.
        let arr_owned = array_wrapper_value_to_array_value(&result, &[])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("Expected Array, got {:?}", result));
        {
            let arr = &arr_owned;

            // Verify shape is 5×5
            assert_eq!(
                arr.shape,
                vec![5, 5],
                "Expected 5×5 array, got {:?}",
                arr.shape
            );

            // Print the grid for visual inspection
            println!("\n=== sjulia Output ===");
            println!("Array shape: {:?}", arr.shape);
            println!("\n5×5 Matrix{{ComplexF64}}:");
            for row in 1..=5 {
                print!(" ");
                for col in 1..=5 {
                    if let Ok(v) = arr.get(&[row as i64, col as i64]) {
                        if let Some((re, im)) = v.as_complex_parts() {
                            if im >= 0.0 {
                                print!("{:5.2}+{:.1}im  ", re, im);
                            } else {
                                print!("{:5.2}{:.1}im  ", re, im);
                            }
                        }
                    }
                }
                println!();
            }

            // Verify all 25 values
            println!("\n=== Verification ===");
            let eps = 1e-10;
            let mut all_passed = true;

            for row in 1..=5_usize {
                for col in 1..=5_usize {
                    let expected_real = expected_re[col - 1];
                    let expected_imag = expected_im[row - 1];

                    match arr.get(&[row as i64, col as i64]) {
                        Ok(v) => {
                            if let Some((re, im)) = v.as_complex_parts() {
                                let re_ok = (re - expected_real).abs() < eps;
                                let im_ok = (im - expected_imag).abs() < eps;

                                if !re_ok || !im_ok {
                                    println!(
                                        "FAIL: [{}, {}] expected {}+{}im, got {}+{}im",
                                        row, col, expected_real, expected_imag, re, im
                                    );
                                    all_passed = false;
                                }
                            } else {
                                println!(
                                    "FAIL: [{}, {}] is not a complex number: {:?}",
                                    row, col, v
                                );
                                all_passed = false;
                            }
                        }
                        Err(e) => {
                            println!("FAIL: [{}, {}] access error: {:?}", row, col, e);
                            all_passed = false;
                        }
                    }
                }
            }

            if all_passed {
                println!("All 25 values match Julia's output!");
            }

            // Assert all values match
            for row in 1..=5_usize {
                for col in 1..=5_usize {
                    let expected_real = expected_re[col - 1];
                    let expected_imag = expected_im[row - 1];

                    let v = arr
                        .get(&[row as i64, col as i64])
                        .unwrap_or_else(|e| panic!("Failed to get [{}, {}]: {:?}", row, col, e));
                    let (re, im) = v.as_complex_parts().unwrap_or_else(|| {
                        panic!("[{}, {}] is not a complex number: {:?}", row, col, v)
                    });

                    assert!(
                        (re - expected_real).abs() < eps,
                        "[{}, {}].re: expected {}, got {}",
                        row,
                        col,
                        expected_real,
                        re
                    );
                    assert!(
                        (im - expected_imag).abs() < eps,
                        "[{}, {}].im: expected {}, got {}",
                        row,
                        col,
                        expected_imag,
                        im
                    );
                }
            }
        }
    }
}

mod retire_complex_f64_resolved_call_fast_path_tests {
    //! Regression coverage for Issue #10530: the ComplexF64 resolved-call fast
    //! path was retired, so `abs2`/`*`/`+` must still behave correctly when
    //! `Complex` values flow through untyped/abstract signatures.

    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    fn run_and_get_bool(src: &str) -> bool {
        match compile_and_run_value(src, 0).expect("Execution failed") {
            Value::Bool(b) => b,
            other => panic!("Expected Bool, got {other:?}"),
        }
    }

    fn run_and_get_f64(src: &str) -> f64 {
        match compile_and_run_value(src, 0).expect("Execution failed") {
            Value::F64(v) => v,
            Value::F32(v) => v as f64,
            Value::I64(v) => v as f64,
            other => panic!("Expected numeric value, got {other:?}"),
        }
    }

    fn run_and_get_i64(src: &str) -> i64 {
        match compile_and_run_value(src, 0).expect("Execution failed") {
            Value::I64(v) => v,
            other => panic!("Expected I64, got {other:?}"),
        }
    }

    #[test]
    fn untyped_mandel_point_matches_complex_f64_typed_10530() {
        let ok = run_and_get_bool(
            r#"
    function mandel_point_typed(c::ComplexF64, maxiter::Int64)::Int64
        z = 0.0 + 0.0im
        for k in 1:maxiter
            if abs2(z) > 4.0
                return k - 1
            end
            z = z * z + c
        end
        return maxiter
    end

    function mandel_point_untyped(c, maxiter::Int64)::Int64
        z = 0.0 + 0.0im
        for k in 1:maxiter
            if abs2(z) > 4.0
                return k - 1
            end
            z = z * z + c
        end
        return maxiter
    end

    mandel_point_untyped(0.0 + 0.0im, 10) == mandel_point_typed(0.0 + 0.0im, 10) &&
    mandel_point_untyped(1.0 + 1.0im, 10) == mandel_point_typed(1.0 + 1.0im, 10) &&
    mandel_point_untyped(-0.75 + 0.0im, 20) == mandel_point_typed(-0.75 + 0.0im, 20)
    "#,
        );

        assert!(
            ok,
            "untyped mandel_point must match the ComplexF64 typed version"
        );
    }

    #[test]
    fn abs2_complex_f64_via_untyped_route_10530() {
        let result = run_and_get_f64(
            r#"
    function my_abs2(c)
        abs2(c)
    end
    my_abs2(3.0 + 4.0im)
    "#,
        );

        assert!(
            (result - 25.0).abs() < 1e-12,
            "abs2(3.0 + 4.0im) through abstract route should be 25.0, got {result}"
        );
    }

    #[test]
    fn plus_complex_f64_via_untyped_route_10530() {
        let ok = run_and_get_bool(
            r#"
    function my_add(a, b)
        a + b
    end
    my_add(1.0 + 2.0im, 3.0 + 4.0im) == 4.0 + 6.0im
    "#,
        );

        assert!(
            ok,
            "Base.:+ on ComplexF64 through abstract route should give 4.0 + 6.0im"
        );
    }

    #[test]
    fn abs2_complex_non_f64_types_still_work_10530() {
        // ComplexF32
        let f32_result = run_and_get_f64(
            r#"
    function my_abs2(c)
        abs2(c)
    end
    my_abs2(Complex{Float32}(3.0, 4.0))
    "#,
        );
        assert!(
            (f32_result - 25.0).abs() < 1e-6,
            "abs2(Complex{{Float32}}(3, 4)) should be 25.0, got {f32_result}"
        );

        // Complex{Int64}
        let i64_result = run_and_get_i64(
            r#"
    function my_abs2(c)
        abs2(c)
    end
    my_abs2(Complex{Int64}(3, 4))
    "#,
        );
        assert_eq!(
            i64_result, 25,
            "abs2(Complex{{Int64}}(3, 4)) should be 25, got {i64_result}"
        );
    }
}
