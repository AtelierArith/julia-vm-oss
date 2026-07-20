//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod constructor_identity_lowering_11019 {
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;

    #[test]
    fn nested_dependent_bound_keeps_closing_brace() -> Result<(), String> {
        let source = "struct Dep{A,B}\n    Dep{A,B}() where {A,B<:Vector{A}} = :hit\nend";
        let mut parser = Parser::new().map_err(|error| error.to_string())?;
        let parse_outcome = parser.parse(source).map_err(|error| error.to_string())?;
        let mut lowering = Lowering::new(source);
        let program = lowering
            .lower(parse_outcome)
            .map_err(|error| error.to_string())?;
        let Some(struct_def) = program.structs.first() else {
            return Err("no struct definition found".to_string());
        };
        let params = &struct_def.inner_constructors[0].type_params;
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].name, "B");
        assert_eq!(
            params[1].get_upper_bound().map(String::as_str),
            Some("Vector{A}")
        );
        Ok(())
    }
}

mod test_try_debug {
    use subset_julia_vm::compile_and_run_str;

    #[test]
    fn test_simple_if_else() {
        let src = r#"
    x = -0.5
    result = 0
    if x < 0
        result = 1
    else
        result = 2
    end
    result
    "#;
        let result = compile_and_run_str(src, 0);
        println!("test_simple_if_else: Result for x=-0.5: {}", result);
        // x=-0.5 is < 0, so result = 1
        assert!((result - 1.0).abs() < 1e-10, "Expected 1, got {}", result);
    }

    #[test]
    fn test_if_elseif_else() {
        let src = r#"
    x = -0.5
    result = 0
    if x < -1
        result = 1
    elseif x < 0
        result = 2
    else
        result = 3
    end
    result
    "#;
        let result = compile_and_run_str(src, 0);
        println!("test_if_elseif_else: Result for x=-0.5: {}", result);
        // x=-0.5 is < 0 but not < -1, so result = 2
        assert!((result - 2.0).abs() < 1e-10, "Expected 2, got {}", result);
    }

    #[test]
    fn test_if_two_elseif_no_else() {
        let src = r#"
    x = -0.5
    result = 0
    if x < -2
        result = 1
    elseif x < -1
        result = 2
    elseif x < 0
        result = 3
    end
    result
    "#;
        let result = compile_and_run_str(src, 0);
        println!("test_if_two_elseif_no_else: Result for x=-0.5: {}", result);
        // x=-0.5: not < -2, not < -1, but < 0 => result = 3
        assert!((result - 3.0).abs() < 1e-10, "Expected 3, got {}", result);
    }

    #[test]
    fn test_if_two_elseif_with_else() {
        let src = r#"
    x = -0.5
    result = 0
    if x < -2
        result = 1
    elseif x < -1
        result = 2
    elseif x < 0
        result = 3
    else
        result = 4
    end
    result
    "#;
        let result = compile_and_run_str(src, 0);
        println!(
            "test_if_two_elseif_with_else: Result for x=-0.5: {}",
            result
        );
        // x=-0.5: not < -2, not < -1, but < 0 => result = 3
        assert!((result - 3.0).abs() < 1e-10, "Expected 3, got {}", result);
    }

    #[test]
    fn test_if_three_elseif_with_else() {
        let src = r#"
    x = 0.5
    result = 0
    if x < -1
        result = 1
    elseif x < 0
        result = 2
    elseif x < 1
        result = 3
    elseif x < 2
        result = 4
    else
        result = 5
    end
    result
    "#;
        let result = compile_and_run_str(src, 0);
        println!(
            "test_if_three_elseif_with_else: Result for x=0.5: {}",
            result
        );
        // x=0.5: not < -1, not < 0, but < 1 => result = 3
        assert!((result - 3.0).abs() < 1e-10, "Expected 3, got {}", result);
    }

    #[test]
    fn test_if_three_elseif_else_path() {
        let src = r#"
    x = 10.0
    result = 0
    if x < -1
        result = 1
    elseif x < 0
        result = 2
    elseif x < 1
        result = 3
    elseif x < 2
        result = 4
    else
        result = 5
    end
    result
    "#;
        let result = compile_and_run_str(src, 0);
        println!(
            "test_if_three_elseif_else_path: Result for x=10.0: {}",
            result
        );
        // x=10.0: not < -1, not < 0, not < 1, not < 2 => result = 5
        assert!((result - 5.0).abs() < 1e-10, "Expected 5, got {}", result);
    }
}

mod test_randn {
    use subset_julia_vm::compile_and_run_str;

    #[allow(dead_code)]
    fn test_randn_basic() {
        let src = r#"
    samples = randn(10)
    length(samples)
    "#;
        let result = compile_and_run_str(src, 42);
        println!("Length: {}", result);
        assert!((result - 10.0).abs() < 1e-10, "Expected 10, got {}", result);
    }

    #[allow(dead_code)]
    fn test_bins_access() {
        let src = r#"
    bins = zeros(6)
    bins[3] = 10.0
    bins[4] = 20.0
    bins[3] + bins[4]
    "#;
        let result = compile_and_run_str(src, 0);
        println!("bins[3] + bins[4] = {}", result);
        assert!((result - 30.0).abs() < 1e-10, "Expected 30, got {}", result);
    }

    #[allow(dead_code)]
    fn test_add_assign_to_bins() {
        let src = r#"
    bins = zeros(3)
    bins[1] += 1
    bins[1] += 1
    bins[1]
    "#;
        let result = compile_and_run_str(src, 0);
        println!("bins[1] after += 2: {}", result);
        assert!((result - 2.0).abs() < 1e-10, "Expected 2, got {}", result);
    }

    #[allow(dead_code)]
    fn test_randn_values_match_julia() {
        // Verify randn(n) produces the same first value as Julia's StableRNG(42)
        let src = r#"
    samples = randn(10)
    samples[1]
    "#;
        let result = compile_and_run_str(src, 42);
        // First randn value from Julia StableRNG(42) is -0.6702516921145671
        println!("VM result (first sample): {}", result);
        let expected = -0.6702516921145671;
        assert!(
            (result - expected).abs() < 1e-10,
            "Expected {}, got {}. Values should match Julia StableRNG(42).",
            expected,
            result
        );
    }

    #[allow(dead_code)]
    fn test_randn_first_10_values() {
        // Verify first 10 randn values match Julia exactly
        let src = r#"
    samples = randn(10)
    samples[10]
    "#;
        let result = compile_and_run_str(src, 42);
        // Julia's 10th value is 1.2973461452176338
        println!("VM 10th value: {}", result);
        let expected = 1.2973461452176338;
        assert!(
            (result - expected).abs() < 1e-10,
            "10th value mismatch: Expected {}, got {}",
            expected,
            result
        );
    }

    #[allow(dead_code)]
    fn test_minimal_bug() {
        // Minimal reproduction: intermediate variable breaks Float64 comparison
        let src = r#"
    arr = [1.5, -0.5, 0.5]
    x = arr[2]
    x < 0
    "#;
        let result = compile_and_run_str(src, 0);
        // -0.5 < 0 should be true
        assert!(
            (result - 1.0).abs() < 1e-10,
            "arr[2] stored in x: x < 0 should be true for -0.5, got {}",
            result
        );
    }

    #[allow(dead_code)]
    fn test_direct_index_comparison() {
        // Test comparison directly on array index (no intermediate variable)
        let src = r#"
    samples = randn(1)
    samples[1] < 0
    "#;
        let result = compile_and_run_str(src, 42);
        // -0.67 < 0 should be true
        assert!(
            (result - 1.0).abs() < 1e-10,
            "samples[1] < 0 should be true for -0.67, got {}",
            result
        );
    }

    #[allow(dead_code)]
    fn test_array_value_comparison() {
        // Test comparison with array-fetched value
        let src = r#"
    samples = randn(1)
    x = samples[1]
    result = 0
    if x < 0.0
        result = 1
    end
    x < 0.0
    "#;
        // First value is -0.6702516921145671 which is < 0
        let result = compile_and_run_str(src, 42);
        assert!(
            (result - 1.0).abs() < 1e-10,
            "samples[1] < 0.0 should be true for -0.67, got {}",
            result
        );
    }

    #[allow(dead_code)]
    fn test_negative_comparison() {
        // Test x < 0 comparison with negative value
        let src = r#"
    x = -0.5
    x < 0
    "#;
        let result = compile_and_run_str(src, 0);
        // -0.5 < 0 should be true (1.0)
        assert!(
            (result - 1.0).abs() < 1e-10,
            "-0.5 < 0 should be true, got {}",
            result
        );
    }

    #[allow(dead_code)]
    fn test_binning_with_array() {
        // Test binning first 10 randn values
        let src = r#"
    samples = randn(10)
    checksum = 0.0
    for i in 1:10
        x = samples[i]
        if x < -2
            checksum += 1.0
        elseif x < -1
            checksum += 2.0
        elseif x < 0
            checksum += 3.0
        elseif x < 1
            checksum += 4.0
        elseif x < 2
            checksum += 5.0
        else
            checksum += 6.0
        end
    end
    checksum
    "#;
        let result = compile_and_run_str(src, 42);
        assert!((result - 40.0).abs() < 1e-10, "Expected 40, got {}", result);
    }

    #[allow(dead_code)]
    fn test_elseif_negative_value() {
        // Test elseif with x = -0.5 (should go to bin 3)
        let src = r#"
    x = -0.5
    bin = 0
    if x < -2
        bin = 1
    elseif x < -1
        bin = 2
    elseif x < 0
        bin = 3
    elseif x < 1
        bin = 4
    else
        bin = 5
    end
    bin
    "#;
        let result = compile_and_run_str(src, 0);
        // x = -0.5 should be in bin 3 (-1 <= x < 0)
        assert!(
            (result - 3.0).abs() < 1e-10,
            "Expected bin 3, got {}",
            result
        );
    }

    #[allow(dead_code)]
    fn test_randn_bins_match_julia() {
        // Verify that bin counting matches Julia's result (668)
        let src = r#"
    n = 1000
    samples = randn(n)

    bins = zeros(6)

    for i in 1:n
        x = samples[i]
        if x < -2
            bins[1] += 1
        elseif x < -1
            bins[2] += 1
        elseif x < 0
            bins[3] += 1
        elseif x < 1
            bins[4] += 1
        elseif x < 2
            bins[5] += 1
        else
            bins[6] += 1
        end
    end

    bins[3] + bins[4]
    "#;
        let result = compile_and_run_str(src, 42);
        // Julia StableRNG(42) bins:
        // bins[1] = 31.0, bins[2] = 142.0, bins[3] = 339.0
        // bins[4] = 329.0, bins[5] = 137.0, bins[6] = 22.0
        // bins[3] + bins[4] = 668.0
        let expected = 668.0;
        assert!(
            (result - expected).abs() < 1e-10,
            "Expected {} (Julia), got {}.",
            expected,
            result
        );
    }

    // Generated aggregate chunks for nextest process amortization.
    #[test]
    fn chunk_000() {
        test_randn_basic();
        test_bins_access();
        test_add_assign_to_bins();
        test_randn_values_match_julia();
        test_randn_first_10_values();
        test_minimal_bug();
        test_direct_index_comparison();
        test_array_value_comparison();
        test_negative_comparison();
        test_binning_with_array();
        test_elseif_negative_value();
        test_randn_bins_match_julia();
    }
}

mod test_if_elseif_else {
    //! Test to verify if/elseif/else parsing and execution

    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    fn run_and_get_output(src: &str) -> String {
        let mut parser = Parser::new().expect("Parser initialization failed");
        let parsed = parser.parse(src).expect("Parse failed");
        let mut lowering = Lowering::new(src);
        let program = lowering.lower(parsed).expect("Lowering failed");
        let compiled = compile_core_program(&program).expect("Compilation failed");

        let rng = StableRng::new(12345);
        let mut vm = Vm::new_program(compiled, rng);
        let _result = vm.run();
        vm.get_output().to_string()
    }

    #[test]
    fn test_if_elseif_else_basic() {
        println!("\n=== Test if/elseif/else basic ===");

        let src = r#"
    n = 50
    if n == 50
        print("*")
    elseif n > 10
        print("+")
    else
        print(" ")
    end
    "#;

        let output = run_and_get_output(src);
        println!("Output: '{}'", output);
        assert_eq!(output.trim(), "*", "Expected '*' for n=50");
    }

    #[test]
    fn test_if_elseif_else_variations() {
        println!("\n=== Test if/elseif/else variations ===");

        // Test case 1: n == 50 (should print '*')
        let src1 = r#"
    n = 50
    if n == 50
        print("*")
    elseif n > 10
        print("+")
    else
        print(" ")
    end
    "#;
        let output1 = run_and_get_output(src1);
        println!("n=50: output='{}'", output1);
        assert_eq!(output1.trim(), "*", "Expected '*' for n=50");

        // Test case 2: n > 10 but != 50 (should print '+')
        let src2 = r#"
    n = 20
    if n == 50
        print("*")
    elseif n > 10
        print("+")
    else
        print(" ")
    end
    "#;
        let output2 = run_and_get_output(src2);
        println!("n=20: output='{}'", output2);
        assert_eq!(output2.trim(), "+", "Expected '+' for n=20");

        // Test case 3: n <= 10 (should print ' ')
        let src3 = r#"
    n = 5
    if n == 50
        print("*")
    elseif n > 10
        print("+")
    else
        print(" ")
    end
    "#;
        let output3 = run_and_get_output(src3);
        println!("n=5: output='{}'", output3);
        // Don't trim - we're comparing against a space which would be trimmed away
        assert_eq!(output3, " ", "Expected ' ' for n=5");
    }

    #[test]
    fn test_mandelbrot_style_condition() {
        println!("\n=== Test Mandelbrot-style condition ===");

        // This matches the exact structure from Mandelbrot code
        // n == 50 -> "*", n > 10 -> "+", else -> " "
        let test_cases = vec![
            (50, "*"),  // n == 50
            (20, "+"),  // n > 10
            (5, " "),   // else
            (100, "+"), // n > 10 (not n == 50!)
            (11, "+"),  // n > 10
            (10, " "),  // else (10 is not > 10)
        ];

        for (n, expected) in test_cases {
            let src = format!(
                r#"
    n = {}
    if n == 50
        print("*")
    elseif n > 10
        print("+")
    else
        print(" ")
    end
    "#,
                n
            );

            let output = run_and_get_output(&src);
            println!("n={}: output='{}', expected='{}'", n, output, expected);
            // Don't trim - we're comparing against a space which would be trimmed away
            assert_eq!(output, expected, "Mismatch for n={}", n);
        }
    }

    #[test]
    fn test_multiple_elseif() {
        println!("\n=== Test multiple elseif clauses ===");

        let src = r#"
    x = 3
    if x == 1
        print("one")
    elseif x == 2
        print("two")
    elseif x == 3
        print("three")
    else
        print("other")
    end
    "#;

        let output = run_and_get_output(src);
        println!("Output: '{}'", output);
        assert_eq!(output.trim(), "three", "Expected 'three' for x=3");
    }
}

/// Int128 / UInt128 string macros: upstream Base defines the lowercase
/// `@int128_str` / `@uint128_str` (int128"..." / uint128"..."). The capitalized
/// spellings Int128"..." / UInt128"..." are NOT upstream and must NOT silently
/// produce a value — they fall through to the generic `@Prefix_str` path and
/// error (Issues #10320, #10324 item 4). These live as a Rust test because the
/// capitalized forms error during macro expansion upstream, so they cannot be
/// exercised from an upstream-portable `.jl` fixture.
mod int128_uint128_string_macros_10320 {
    use subset_julia_vm::compile_and_run_value;
    use subset_julia_vm_bytecode::Value;

    #[test]
    fn int128_lowercase_yields_int128() {
        match compile_and_run_value("int128\"9223372036854775808\"", 0) {
            Ok(Value::I128(v)) => assert_eq!(v, 9223372036854775808i128),
            other => panic!("expected Int128 literal, got {:?}", other),
        }
    }

    #[test]
    fn uint128_lowercase_yields_uint128() {
        match compile_and_run_value("uint128\"123\"", 0) {
            Ok(Value::U128(v)) => assert_eq!(v, 123u128),
            other => panic!("expected UInt128 literal, got {:?}", other),
        }
    }

    #[test]
    fn uint128_lowercase_above_int128_max_does_not_overflow() {
        // 2^127 has a negative Int128 bit pattern; the lowering must route
        // through a range-checked BigInt so the UInt128 value is exact.
        match compile_and_run_value("uint128\"170141183460469231731687303715884105728\"", 0) {
            Ok(Value::U128(v)) => assert_eq!(v, 1u128 << 127),
            other => panic!("expected UInt128 literal, got {:?}", other),
        }
    }

    #[test]
    fn uint128_lowercase_typemax() {
        match compile_and_run_value("uint128\"340282366920938463463374607431768211455\"", 0) {
            Ok(Value::U128(v)) => assert_eq!(v, u128::MAX),
            other => panic!("expected UInt128 typemax literal, got {:?}", other),
        }
    }

    #[test]
    fn capitalized_int128_string_macro_errors() {
        // Must NOT return 123: the capitalized `Int128"..."` is not a defined
        // string macro (upstream: UndefVarError @Int128_str).
        let result = compile_and_run_value("Int128\"123\"", 0);
        assert!(
            result.is_err(),
            "capitalized Int128\"...\" must error, got {:?}",
            result
        );
    }

    #[test]
    fn capitalized_uint128_string_macro_errors() {
        let result = compile_and_run_value("UInt128\"123\"", 0);
        assert!(
            result.is_err(),
            "capitalized UInt128\"...\" must error, got {:?}",
            result
        );
    }
}

/// Malformed/adversarial-source differential fuzz corpus for the
/// lowering/compile front door (Issue #10905, Phase 1b of #10869).
///
/// Each snippet below stresses one of the proof-backed invariants converted
/// in this phase: comparison chains (`lowering/expr/binary.rs`), for/while
/// loop-frame push/pop (`compile/stmt.rs`), compound/broadcast assignment
/// (`lowering/stmt/assignment.rs`), N-D array literals
/// (`lowering/expr/collection.rs`'s `nd_cat`/`nd_fold`), generator clause
/// functions, numeric convert structural rewrite (`compile/ssa_ir/plan.rs`),
/// `eval`-produced function/macro-def reconstruction and `=>` pair
/// construction (`macro_runtime.rs`), and a `for` loop inside a quoted
/// expression (`lowering/expr/quote/cst_to_constructor.rs`).
///
/// Every prefix-truncation and single-character-deletion mutation of each
/// snippet is run through the bare `Parser` + `Lowering` + the production
/// cached compile entrypoint (`host_support::compile_with_cache`, the same
/// path the CLI/FFI/Web hosts use — Base is seeded from the compiled-Base
/// cache instead of re-inferred per call, which is what makes the exhaustive
/// sweep affordable in-suite) inside `catch_unwind`: most mutations fail to
/// parse (a different stage's job to reject cleanly) or fail
/// lowering/compile with a typed error, but the pipeline must never panic.
/// Same methodology the parser crate's Phase 1a
/// `malformed_input_no_panic_tests.rs` established (Issue #10904), applied
/// one stage further down the pipeline.
mod lowering_compile_malformed_input_10905_tests {
    use std::panic::{self, AssertUnwindSafe};
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;

    const SNIPPETS: &[&str] = &[
        // Chained scalar comparison (comparison-chain groups).
        "a = 1; b = 2; c = 3; a < b < c",
        // Chained dotted/broadcast comparison.
        "a = [1, 2]; b = [2, 3]; c = [3, 4]; a .< b .< c",
        // `for` loop with a positive const step (loop-frame push/pop).
        "s = 0; for i in 1:2:10; s += i; end; s",
        // `for` loop with a negative const step (checked_neg magnitude).
        "s = 0; for i in 10:-2:1; s += i; end; s",
        // `while` loop (loop-frame push/pop).
        "i = 0; while i < 5; i += 1; end; i",
        // `foreach`-shaped iteration over a collection.
        "s = 0; for x in [1, 2, 3]; s += x; end; s",
        // Tuple-destructuring `for` loop.
        "s = 0; for (a, b) in [(1, 2), (3, 4)]; s += a + b; end; s",
        // Broadcast compound assignment on a plain variable.
        "x = [1, 2, 3]; x .+= 1; x",
        // Non-broadcast compound assignment.
        "x = 1; x += 2; x",
        // N-D array literals (`nd_cat`/`nd_fold`).
        "[1 2; 3 4]",
        "[1 2 3; 4 5 6;; 7 8 9; 10 11 12]",
        // `let` binding.
        "let x = 1; x + 1 end",
        // Comma/product generator.
        "collect(x + y for x in 1:2, y in 1:2)",
        // Tuple-destructuring generator.
        "collect(a + b for (a, b) in [(1, 2), (3, 4)])",
        // Flatten (nested `for`) generator.
        "collect(x + y for x in 1:2 for y in 1:2)",
        // Numeric convert structural rewrite.
        "x = 1.5; Int64(x)",
        // `eval`-produced function/macro definitions. This `eval` is Julia
        // source data for the sandboxed sjulia interpreter under test (the
        // string is never passed to a Rust/host `eval`); it exercises
        // `macro_runtime.rs`'s `Expr(:function, ...)`/`Expr(:macro, ...)`
        // value reconstruction, not a host code-injection path.
        "eval(:(function f() 1 end)); f()",
        "eval(:(macro m() :(1) end))",
        // `for` loop inside a quoted expression.
        ":(for a = 1:2, b = 3:4; a + b; end)",
        // `=>` pair construction (Dict literal macro expansion).
        "Dict(:a => 1, :b => 2)",
    ];

    /// Runs `src` through the lowering/compile front door inside
    /// `catch_unwind`, asserting the pipeline never panics regardless of
    /// whether it succeeds or returns a typed error.
    fn assert_front_door_never_panics(src: &str) {
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let Ok(mut parser) = Parser::new() else {
                return;
            };
            let Ok(parse_outcome) = parser.parse(src) else {
                return;
            };
            let mut lowering = Lowering::new(src);
            if let Ok(program) = lowering.lower(parse_outcome) {
                let _ = compile_with_cache(&program);
            }
        }));
        assert!(
            result.is_ok(),
            "lowering/compile front door panicked on input: {src:?}"
        );
    }

    #[test]
    fn front_door_never_panics_on_full_snippets() {
        for src in SNIPPETS {
            assert_front_door_never_panics(src);
        }
    }

    #[test]
    fn front_door_never_panics_on_truncated_snippets() {
        for src in SNIPPETS {
            for end in 1..src.len() {
                if !src.is_char_boundary(end) {
                    continue;
                }
                assert_front_door_never_panics(&src[..end]);
            }
        }
    }

    #[test]
    fn front_door_never_panics_on_single_char_deletions() {
        for src in SNIPPETS {
            let chars: Vec<char> = src.chars().collect();
            for i in 0..chars.len() {
                let mutated: String = chars
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, c)| *c)
                    .collect();
                assert_front_door_never_panics(&mutated);
            }
        }
    }
}

/// Issues #10937/#10943/#10945/#10951: scoped-declaration lowering must
/// explicitly handle every normalized CST shape — delegate the valid forms
/// and emit the upstream-matching typed error for the invalid ones, never
/// silently dropping a wrapped child (the pre-fix failure mode was
/// `Stmt::Global { names: [] }` / a no-op const).
mod scoped_declaration_lowering_10943_10945 {
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;

    /// Lower `src` and return the typed lowering error's kind display, or
    /// None when lowering succeeds.
    fn lowering_error_message(src: &str) -> Option<String> {
        let mut parser = Parser::new().expect("parser must construct");
        let outcome = parser
            .parse(src)
            .unwrap_or_else(|e| panic!("must parse: {src:?}, error: {e:?}"));
        let mut lowering = Lowering::new(src);
        match lowering.lower(outcome) {
            Ok(_) => None,
            Err(error) => Some(error.kind.to_string()),
        }
    }

    #[test]
    fn invalid_scoped_forms_emit_upstream_global_declaration_error() {
        // Upstream julia 1.12.6: ERROR: syntax: invalid syntax in "global"
        // declaration (parse succeeds; lowering rejects).
        for src in [
            "global module M\nend",
            "global baremodule M\nend",
            "global while false\nend",
            "global for i in 1:1\nend",
            "global if true\n1\nend",
            "global begin\n1\nend",
            "global try\n1\ncatch\nend",
            "global struct S\nend",
            "global quote\n1\nend",
            "global return 1",
            "global break",
            "global using Foo",
            "global macro m()\n1\nend",
            "global 2 + 3",
            "global c + 1",
            "global c => 2",
            "global global x",
        ] {
            assert_eq!(
                lowering_error_message(src).as_deref(),
                Some("syntax: invalid syntax in \"global\" declaration"),
                "source: {src:?}"
            );
        }
    }

    #[test]
    fn invalid_scoped_forms_emit_upstream_local_declaration_error() {
        for src in [
            "local while false\nend",
            "local module M\nend",
            "local macro m()\n1\nend",
            "local x => 2",
            "local 2 + 3",
        ] {
            assert_eq!(
                lowering_error_message(src).as_deref(),
                Some("syntax: invalid syntax in \"local\" declaration"),
                "source: {src:?}"
            );
        }
    }

    #[test]
    fn const_local_orders_emit_upstream_const_assignment_error() {
        // Upstream julia 1.12.6: ERROR: syntax: expected assignment after
        // "const" (both orders; Issue #10943).
        for src in ["const local x = 1", "local const x = 1"] {
            assert_eq!(
                lowering_error_message(src).as_deref(),
                Some("syntax: expected assignment after \"const\""),
                "source: {src:?}"
            );
        }
    }

    #[test]
    fn global_method_definition_in_function_body_is_rejected() {
        // Upstream julia 1.12.6: ERROR: syntax: Global method definition
        // around <loc> needs to be placed at the top level, or use "eval".
        for src in [
            "function g()\n    global function h(x)\n        x\n    end\nend",
            "function g()\n    global h(x) = x\nend",
            "function g()\n    let\n        global function h(x)\n            x\n        end\n    end\nend",
            "g() = global h(x) = x",
        ] {
            let message = lowering_error_message(src)
                .unwrap_or_else(|| panic!("must be a lowering error: {src:?}"));
            assert!(
                message.starts_with("syntax: Global method definition around line ")
                    && message
                        .ends_with(" needs to be placed at the top level, or use \"eval\"."),
                "source: {src:?}, message: {message:?}"
            );
        }
    }

    #[test]
    fn valid_scoped_forms_lower_cleanly() {
        for src in [
            "const global c = 1",
            "global const c = 1",
            "global x",
            "global x, y",
            "global x, y = 1, 2",
            "global x::Int = 7",
            "global f(x) = 2x",
            "let\n    global function f(x)\n        x + 1\n    end\nend",
            "let\n    local function f(x)\n        x + 1\n    end\n    f(1)\nend",
            "global function f(x)\n    x\nend",
            "local a, b = 3, 4",
            // Quoted global method definitions stay quotable — the function
            // body pre-scan must not fire inside quote/macro arguments.
            "function g()\n    :(global function h()\n    end)\nend",
        ] {
            assert_eq!(
                lowering_error_message(src),
                None,
                "source: {src:?} must lower without error"
            );
        }
    }
}
