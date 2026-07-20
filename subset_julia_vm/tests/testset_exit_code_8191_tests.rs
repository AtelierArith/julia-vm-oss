//! Issue #8191: a failing `@test` / `@testset` must make the process exit
//! non-zero, matching upstream Julia (which throws a `TestSetException` → exit
//! 1). sjulia records failures without throwing, so the VM exposes a sticky
//! `any_test_failed()` flag that the CLI maps to a non-zero exit code.
//!
//! The fixture harness checks the final returned value, not the exit code, so a
//! `@testset`-only fixture ending in `true` cannot regression-test this. These
//! integration tests exercise the flag directly.

use subset_julia_vm::{
    compile::host_support::{clear_compile_cache, compile_with_cache},
    pipeline::{parse_and_lower_strict, parse_and_lower_with_base_dir},
    rng::StableRng,
    vm::Vm,
};

/// Run a source string through the full pipeline and return whether any test
/// failed (the value the CLI uses to pick its exit code).
fn any_test_failed(source: &str) -> bool {
    clear_compile_cache();
    let program =
        parse_and_lower_with_base_dir(source, None).expect("source should parse and lower");
    let compiled = compile_with_cache(&program).expect("program should compile");
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    vm.run()
        .expect("program should run without a runtime error");
    vm.any_test_failed()
}

/// Like [`any_test_failed`], but also returns the captured stdout so tests can
/// assert on the errored-outcome record and the testset summary (Issue #10093).
fn run_capturing_output(source: &str) -> (bool, String) {
    clear_compile_cache();
    let program =
        parse_and_lower_with_base_dir(source, None).expect("source should parse and lower");
    let compiled = compile_with_cache(&program).expect("program should compile");
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    vm.run()
        .expect("program should run without a runtime error");
    (vm.any_test_failed(), vm.get_output().to_string())
}

/// Strict file-mode variant used by the CLI. The static Test macro expansion
/// must retain explicit-local provenance before strict soft-scope rewriting.
fn run_strict_capturing_output(source: &str) -> (bool, String) {
    clear_compile_cache();
    let program = parse_and_lower_strict(source).expect("source should parse and lower strictly");
    let compiled = compile_with_cache(&program).expect("program should compile");
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    vm.run()
        .expect("program should run without a runtime error");
    (vm.any_test_failed(), vm.get_output().to_string())
}

#[test]
fn passing_bare_test_stays_passing_in_strict_file_mode_11415() {
    let (failed, output) = run_strict_capturing_output("using Test\n@test true\ntrue\n");
    assert!(
        !failed,
        "a passing strict-mode @test must not set the sticky failure flag; output:\n{output}"
    );
    assert!(
        output.contains("Test Passed") && !output.contains("Test Failed"),
        "strict-mode @test true must record a pass; output:\n{output}"
    );
}

#[test]
fn failing_testset_flags_failure_8191() {
    let src = r#"
using Test
@testset "demo" begin
    @test 1 + 1 == 3
end
true
"#;
    assert!(
        any_test_failed(src),
        "a failing @test inside @testset must set any_test_failed (→ exit 1)"
    );
}

#[test]
fn passing_testset_does_not_flag_failure_8191() {
    let src = r#"
using Test
@testset "demo" begin
    @test 1 + 1 == 2
    @test "a" == "a"
end
true
"#;
    assert!(
        !any_test_failed(src),
        "an all-passing @testset must NOT set any_test_failed (→ exit 0)"
    );
}

#[test]
fn bare_failing_test_flags_failure_8191() {
    let src = r#"
using Test
@test 1 == 2
"#;
    assert!(
        any_test_failed(src),
        "a bare failing @test (no @testset) must set any_test_failed (→ exit 1)"
    );
}

#[test]
fn no_tests_does_not_flag_failure_8191() {
    let src = r#"
println("hello")
1 + 1
"#;
    assert!(
        !any_test_failed(src),
        "a program with no tests must NOT set any_test_failed (→ exit 0)"
    );
}

#[test]
fn later_passing_testset_keeps_earlier_failure_8191() {
    // The sticky flag accumulates: an earlier failure is not cleared by a later
    // all-passing @testset (each @testset resets only its own pass/fail counts).
    let src = r#"
using Test
@testset "first" begin
    @test false
end
@testset "second" begin
    @test true
end
true
"#;
    assert!(
        any_test_failed(src),
        "an earlier failure must persist through a later passing @testset"
    );
}

// ─── Issue #10093: @test catches exceptions from its expression ────────────
//
// Upstream `Test.@test` evaluates its expression inside try/catch
// (`get_test_result` → `Returned`/`Threw`) and records a thrown exception as
// an *errored* outcome (`Error(:test_error, ...)`), letting the enclosing
// `@testset` run to its summary before the run ultimately exits non-zero.
// These tests pin the sjulia equivalent: the exception must NOT propagate out
// of `vm.run()`, later tests must still execute, the summary must include the
// errored count, and the sticky `any_test_failed` flag must be set.

#[test]
fn errored_test_does_not_propagate_and_flags_failure_10093() {
    let src = r#"
using Test
@testset "throws inside a @test expr" begin
    @test (error("boom_inside_test"); true)
    @test 1 + 1 == 2
end
true
"#;
    // `vm.run()` succeeding at all (inside the helper) is the core assertion:
    // before the fix the exception propagated out of the @testset as a
    // runtime error and the summary never printed.
    let (failed, output) = run_capturing_output(src);
    assert!(
        failed,
        "an errored @test must set any_test_failed (→ exit 1), like upstream's \
         TestSetException for '1 errored'"
    );
    let error_pos = output.find("Error During Test").unwrap_or_else(|| {
        panic!("errored @test must print an Error During Test record; got:\n{output}")
    });
    assert!(
        output.contains("boom_inside_test"),
        "the errored record must carry the thrown exception; got:\n{output}"
    );
    let pass_pos = output.find("Test Passed").unwrap_or_else(|| {
        panic!("the @test AFTER the errored one must still run; got:\n{output}")
    });
    let summary_pos = output
        .find("1 passed, 0 failed, 1 errored (2 total)")
        .unwrap_or_else(|| {
            panic!("summary must count the errored outcome separately; got:\n{output}")
        });
    assert!(
        error_pos < pass_pos && pass_pos < summary_pos,
        "expected order: errored record → later pass → summary; got:\n{output}"
    );
}

#[test]
fn nonbool_test_records_errored_outcome_10093() {
    // Upstream records a non-Boolean @test value as Error(:test_nonbool)
    // ("Expression evaluated to non-Boolean"), also an errored outcome.
    let src = r#"
using Test
@testset "non-Bool @test value" begin
    @test 1 + 1
    @test true
end
true
"#;
    let (failed, output) = run_capturing_output(src);
    assert!(failed, "a non-Bool @test value must set any_test_failed");
    assert!(
        output.contains("Error During Test") && output.contains("non-Boolean"),
        "non-Bool @test value must be recorded as an errored outcome; got:\n{output}"
    );
    assert!(
        output.contains("1 passed, 0 failed, 1 errored (2 total)"),
        "summary must count the non-Bool outcome as errored; got:\n{output}"
    );
}

#[test]
fn errored_count_resets_between_testsets_10093() {
    // The errored counter is per-testset (like pass/fail/broken); the sticky
    // flag still accumulates across testsets.
    let src = r#"
using Test
@testset "first (errors)" begin
    @test (error("boom"); true)
end
@testset "second (clean)" begin
    @test true
end
true
"#;
    let (failed, output) = run_capturing_output(src);
    assert!(
        failed,
        "the earlier errored test must keep the sticky flag set"
    );
    assert!(
        output.contains("0 passed, 0 failed, 1 errored (1 total)"),
        "first testset summary must show the errored count; got:\n{output}"
    );
    assert!(
        output.contains("1 passed, 0 failed (1 total)"),
        "second testset summary must NOT inherit the errored count; got:\n{output}"
    );
}

#[test]
fn exception_inside_test_expression_is_still_catchable_by_user_code_10093() {
    // The @test try/catch must wrap only the test expression itself: an
    // exception raised and handled INSIDE the expression never reaches the
    // recorder, and the test passes normally (green-half fixture
    // stdlib/test_errored_expr_10093.jl covers the same under the harness).
    let src = r#"
using Test
@testset "internal catch" begin
    @test (try
        error("handled internally")
        false
    catch e
        true
    end)
end
true
"#;
    let (failed, output) = run_capturing_output(src);
    assert!(
        !failed,
        "an internally handled exception must not error the test; got:\n{output}"
    );
    assert!(
        output.contains("1 passed, 0 failed (1 total)"),
        "summary must show a clean pass; got:\n{output}"
    );
}

#[test]
fn test_macro_catch_variable_does_not_shadow_user_e_10242() {
    // Issue #10242: the Test macros' quote-internal catch variable used to
    // leak into the enclosing @testset scope because the static
    // stdlib-macro quote expansion's hygiene pass
    // (`collect_introduced_vars` in
    // `lowering/expr/quote/handlers.rs`) never registered the `catch`
    // variable as an introduced local (unlike `local`/assignment targets,
    // which were already gensym-renamed). With the natural `catch e`
    // spelling, any expansion containing it (@test after #10093,
    // @test_broken/@test_throws always) made later references to `e` in the
    // same testset resolve to the caught exception — here
    // Base.MathConstants.e would compare unequal to ℯ. `collect_introduced_vars`
    // now has an `ExprHead::Try` arm that registers the catch variable for
    // hygiene renaming, so `Test.jl` uses the natural `catch e` spelling
    // again (the `__test_*`-prefixed workaround was removed; see
    // docs/vm/WORKAROUNDS.md, W-67 Resolved). The @test_broken probe lives
    // here rather than in the green fixture because upstream julia's Broken
    // summary column is not parseable by fixture_julia_parity.sh.
    let src = r#"
using Test
using Base.MathConstants: e
@testset "catch var must not shadow e" begin
    @test typeof(e) == Irrational{:ℯ}
    @test_broken (error("shadow probe"); true)
    @test e == ℯ
    @test typeof(e) == Irrational{:ℯ}
end
true
"#;
    let (failed, output) = run_capturing_output(src);
    assert!(
        !failed,
        "e must still resolve to Base.MathConstants.e both before and after a \
         @test_broken expansion in the same testset; got:\n{output}"
    );
    assert!(
        output.contains("3 passed, 0 failed, 1 broken (4 total)"),
        "summary must show 3 passes (including the pre-catch read of e) and \
         the expected broken test; got:\n{output}"
    );
}

// ─── Issue #10630: macro statement/value adapter tail-effect matrix ────────
//
// Prevention for the #10307/#10496/PR #10625 root cause: stdlib macro quotes
// lower through nested Block and LetBlock wrappers, and the statement-position
// adapter (`discard_macro_tail_value`) must retain the recorder control-flow
// subtree and remove ONLY the nested result tail, while the expression path
// must preserve the recorded value. These are the deliberately-FAILING halves
// of the matrix — a bare failing statement proves the sticky flag, i.e. that
// the recorder effect survived the tail discard. (Upstream julia THROWS for a
// bare failing @test, so this half cannot be a parity fixture; the green half
// is macros/test_macro_stmt_expr_value_matrix_10630.jl.)

#[test]
fn statement_position_matrix_10630() {
    // Bare @test in STATEMENT position: pass → no flag; fail/error → flag.
    // The failing cases prove the statement adapter kept the recorder
    // effects: had the expansion's control-flow subtree been discarded with
    // the tail value, no outcome would be recorded and the flag stayed clear.
    let (failed, output) = run_capturing_output(
        r#"
using Test
@test 1 + 1 == 2
println("after_pass")
true
"#,
    );
    assert!(!failed, "bare passing @test statement must not flag");
    assert!(
        output.contains("after_pass"),
        "statement expansion must not disturb the following statement; got:\n{output}"
    );

    let (failed, output) = run_capturing_output(
        r#"
using Test
@test 1 == 2
println("after_fail")
true
"#,
    );
    assert!(
        failed,
        "a bare failing @test STATEMENT must set the sticky flag — the \
         recorder subtree must survive the statement adapter's tail discard; \
         got:\n{output}"
    );
    assert!(
        output.contains("Test Failed") && output.contains("after_fail"),
        "the failure record must print and execution must continue; got:\n{output}"
    );

    let (failed, output) = run_capturing_output(
        r#"
using Test
@test (error("boom_stmt_10630"); true)
println("after_error")
true
"#,
    );
    assert!(
        failed,
        "a bare errored @test STATEMENT must set the sticky flag; got:\n{output}"
    );
    assert!(
        output.contains("Error During Test")
            && output.contains("boom_stmt_10630")
            && output.contains("after_error"),
        "the errored record must print and execution must continue; got:\n{output}"
    );
}

#[test]
fn expression_position_matrix_10630() {
    // Assignment-value half of the matrix: EXPRESSION position must both
    // record the outcome (sticky flag on fail/error) AND preserve the
    // Test.Result value for the assignment target.
    let (failed, output) = run_capturing_output(
        r#"
using Test
r = @test 1 + 1 == 2
println(r isa Test.Pass)
true
"#,
    );
    assert!(
        !failed,
        "assigned passing @test must not flag; got:\n{output}"
    );
    assert!(
        output.contains("true"),
        "expression expansion must preserve the Test.Pass value; got:\n{output}"
    );

    let (failed, output) = run_capturing_output(
        r#"
using Test
r = @test 1 == 2
println(r isa Test.Fail)
true
"#,
    );
    assert!(
        failed,
        "an assigned failing @test must still set the sticky flag; got:\n{output}"
    );
    assert!(
        output.contains("true"),
        "expression expansion must preserve the Test.Fail value; got:\n{output}"
    );

    let (failed, output) = run_capturing_output(
        r#"
using Test
r = @test (error("boom_expr_10630"); true)
println(r isa Test.Error)
true
"#,
    );
    assert!(
        failed,
        "an assigned errored @test must still set the sticky flag; got:\n{output}"
    );
    assert!(
        output.contains("boom_expr_10630") && output.contains("true"),
        "expression expansion must record and preserve the Test.Error value; \
         got:\n{output}"
    );

    // @testset in assignment position with a failing test inside: the
    // TestSet-shaped value is preserved AND the failure stays sticky.
    let (failed, output) = run_capturing_output(
        r#"
using Test
y = @testset "failing set in value position" begin
    @test false
end
println(y isa Test.DefaultTestSet)
true
"#,
    );
    assert!(
        failed,
        "a failing @testset in value position must set the sticky flag; got:\n{output}"
    );
    assert!(
        output.contains("true"),
        "the @testset value must remain a Test.DefaultTestSet; got:\n{output}"
    );
}

// ─── Issue #10273: unified @test-family recording harness ──────────────────
//
// Design invariant (docs/vm/TESTING_GUIDE.md "The unified @test-family
// recording harness"): NO @test-family entry point may propagate an
// evaluation exception past the enclosing @testset without recording an
// outcome. This coverage test enumerates every entry point and, for the forms
// whose expression can throw, asserts the exception is caught & recorded (the
// summary still prints, `vm.run()` succeeds, and the run is flagged) rather
// than unwinding out of the testset. It is the prevention mechanism for the
// #10093/#10273 fix round: re-introducing a bare `Instr::Test` fast path for
// any throwing entry point (as the `@test x isa T` path used to be) fails here.

/// One @test-family entry point: a Julia source whose `@testset` contains a
/// THROWING test expression, plus whether that entry point's errored/failed
/// outcome must set the sticky `any_test_failed()` flag.
struct EntryPoint {
    name: &'static str,
    src: &'static str,
    must_flag_failure: bool,
}

#[test]
fn test_harness_entry_point_coverage_10273() {
    let entry_points = [
        EntryPoint {
            name: "@test macro (throwing expr)",
            src: r#"
using Test
@testset "ep" begin
    @test (error("boom_macro"); true)
    @test true
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "@test isa(x, T) call form (throwing x)",
            src: r#"
using Test
@testset "ep" begin
    @test isa(error("boom_isacall"), Int)
    @test true
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "@test x isa T infix form (throwing x)",
            src: r#"
using Test
@testset "ep" begin
    @test error("boom_isainfix") isa Int
    @test true
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "@test_throws (throwing expr — pass)",
            src: r#"
using Test
@testset "ep" begin
    @test_throws ErrorException error("boom_throws")
    @test true
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "@test_broken (throwing expr — broken)",
            src: r#"
using Test
@testset "ep" begin
    @test_broken (error("boom_broken"); true)
    @test true
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "@test_skip (throwing expr NOT evaluated — broken)",
            src: r#"
using Test
@testset "ep" begin
    @test_skip error("boom_skip")
    @test true
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "nested @testset (throwing @test in inner set)",
            src: r#"
using Test
@testset "outer" begin
    @testset "inner" begin
        @test (error("boom_nested"); true)
    end
    @test true
end
true
"#,
            must_flag_failure: true,
        },
    ];

    for ep in entry_points {
        // Core invariant: the throwing expression must NOT propagate out of the
        // testset — `run_capturing_output` calls `vm.run().expect(...)`, so a
        // propagated exception panics the test with a clear message.
        let (failed, output) = run_capturing_output(ep.src);
        assert!(
            output.contains("total)"),
            "[{}] the @testset summary must still print (no exception may unwind \
             past the testset); got:\n{output}",
            ep.name
        );
        // The `@test true` after the throwing entry point must still execute —
        // proof the recorder returned control to the testset body.
        assert!(
            output.contains("Test Passed"),
            "[{}] the test AFTER the throwing entry point must still run; \
             got:\n{output}",
            ep.name
        );
        assert_eq!(
            failed, ep.must_flag_failure,
            "[{}] any_test_failed() mismatch (expected {}); got:\n{output}",
            ep.name, ep.must_flag_failure
        );
    }
}

/// Issue #10354: `@test_throws` must check the expected exception, not just
/// "something was thrown". Before this fix, `_test_record!(true, ...)` ran
/// unconditionally in the `catch` branch, so a WRONG-type exception recorded
/// a Pass — the same "must not flag failure" outcome as a right-type or
/// no-exception-at-all case. That blind spot hid 13 genuine sjulia bugs in
/// this repo's own fixture suite (docs/vm/EXCEPTION_PARITY.md).
///
/// A fixture (`.jl`) cannot cover the wrong-type/no-throw RED half directly:
/// the fixture harness's `@testset` gate rejects any fixture that records a
/// failure (Issue #9360), same constraint as `failing_testset_flags_failure_8191`
/// above. These entry points exercise every `@test_throws` form this PR
/// implements (Type / exception value / String / Regex / Array / Function,
/// mirroring upstream `do_test_throws`) on both a matching and a
/// deliberately-mismatched case, proving the blind spot cannot silently
/// return: a wrong type/value/message MUST flag failure, a right one must
/// NOT.
#[test]
fn test_throws_checks_expected_exception_10354() {
    let entry_points = [
        EntryPoint {
            name: "Type form — right type",
            src: r#"
using Test
@testset "ep" begin
    @test_throws ArgumentError throw(ArgumentError("boom"))
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "Type form — WRONG type (the blind-spot regression)",
            src: r#"
using Test
@testset "ep" begin
    @test_throws ArgumentError error("boom")
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "Type form — no exception thrown",
            src: r#"
using Test
@testset "ep" begin
    @test_throws ArgumentError (1 + 1)
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "Exception-value form — matching fields",
            src: r#"
using Test
@testset "ep" begin
    @test_throws UndefVarError(:zzz_10354) zzz_10354
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "Exception-value form — mismatched field",
            src: r#"
using Test
@testset "ep" begin
    @test_throws UndefVarError(:other_name_10354) zzz_10354
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "String form — message substring matches",
            src: r#"
using Test
@testset "ep" begin
    @test_throws "boom" error("boom time")
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "String form — message substring does not match",
            src: r#"
using Test
@testset "ep" begin
    @test_throws "xyz_10354" error("boom time")
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "Regex form — message matches",
            src: r#"
using Test
@testset "ep" begin
    @test_throws r"bo+m" error("boom time")
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "Regex form — message does not match",
            src: r#"
using Test
@testset "ep" begin
    @test_throws r"xyz_10354" error("boom time")
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "Array-of-strings form — every element matches",
            src: r#"
using Test
@testset "ep" begin
    @test_throws ["boom", "time"] error("boom time")
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "Array-of-strings form — one element does not match",
            src: r#"
using Test
@testset "ep" begin
    @test_throws ["boom", "xyz_10354"] error("boom time")
end
true
"#,
            must_flag_failure: true,
        },
        EntryPoint {
            name: "Function form — matcher returns true",
            src: r#"
using Test
@testset "ep" begin
    @test_throws (msg -> occursin("boom", msg)) error("boom time")
end
true
"#,
            must_flag_failure: false,
        },
        EntryPoint {
            name: "Function form — matcher returns false",
            src: r#"
using Test
@testset "ep" begin
    @test_throws (msg -> occursin("xyz_10354", msg)) error("boom time")
end
true
"#,
            must_flag_failure: true,
        },
    ];

    for ep in entry_points {
        let (failed, output) = run_capturing_output(ep.src);
        assert!(
            output.contains("total)"),
            "[{}] the @testset summary must still print; got:\n{output}",
            ep.name
        );
        assert_eq!(
            failed, ep.must_flag_failure,
            "[{}] any_test_failed() mismatch (expected {}); got:\n{output}",
            ep.name, ep.must_flag_failure
        );
    }
}

/// Issue #10354: a Fail message must name both the expected and the actual
/// (thrown) exception, matching upstream's `Expected: T / Thrown: U` shape —
/// not just "something threw", which gave no diagnostic signal at all.
#[test]
fn test_throws_fail_message_names_expected_and_thrown_10354() {
    let (failed, output) = run_capturing_output(
        r#"
using Test
@testset "ep" begin
    @test_throws ArgumentError error("boom_message_10354")
end
true
"#,
    );
    assert!(failed, "wrong-type @test_throws must flag failure");
    assert!(
        output.contains("Expected: ArgumentError"),
        "Fail message must name the expected exception; got:\n{output}"
    );
    assert!(
        output.contains("Thrown: ErrorException"),
        "Fail message must name the actually-thrown exception; got:\n{output}"
    );
}

/// Issue #10338: an outer `@testset` containing nested `@testset`s must print
/// an AGGREGATED summary (upstream `DefaultTestSet` parent aggregation), not
/// repeat the LAST inner set's counts. Before the fix, each nested
/// `_testset_begin!` clobbered the flat counters, so the outer summary line
/// duplicated the last inner line ("1 passed ... (1 total)" twice here).
#[test]
fn outer_testset_summary_aggregates_nested_counts_10338() {
    let (failed, output) = run_capturing_output(
        r#"
using Test
@testset "Outer10338" begin
    @testset "InnerA10338" begin
        @test 1 == 1
        @test 2 == 2
    end
    @testset "InnerB10338" begin
        @test 3 == 3
    end
end
true
"#,
    );
    assert!(!failed, "all tests pass; got:\n{output}");
    let count = |needle: &str| output.matches(needle).count();
    assert_eq!(
        count("  2 passed, 0 failed (2 total)"),
        1,
        "InnerA prints its own summary once; got:\n{output}"
    );
    assert_eq!(
        count("  1 passed, 0 failed (1 total)"),
        1,
        "InnerB's summary must appear exactly ONCE (the pre-fix bug echoed the \
         last inner set's counts again as the outer summary); got:\n{output}"
    );
    assert_eq!(
        count("  3 passed, 0 failed (3 total)"),
        1,
        "the outer testset must aggregate 2+1 nested tests; got:\n{output}"
    );
}

/// Issue #10338: nested failures/errored/broken outcomes aggregate into the
/// enclosing testset's summary in upstream `TestSetException` order, and
/// tests recorded directly by the outer set (after the nested set finished)
/// count on top of the folded-in nested results.
#[test]
fn outer_testset_aggregates_mixed_nested_outcomes_10338() {
    let (failed, output) = run_capturing_output(
        r#"
using Test
@testset "MixedOuter10338" begin
    @testset "MixedInner10338" begin
        @test 1 == 1
        @test 1 == 2
        @test_broken false
    end
    @test true
end
true
"#,
    );
    assert!(failed, "the nested failure must set the sticky flag");
    assert_eq!(
        output
            .matches("  1 passed, 1 failed, 1 broken (3 total)")
            .count(),
        1,
        "inner set reports its own mixed outcomes once; got:\n{output}"
    );
    assert_eq!(
        output
            .matches("  2 passed, 1 failed, 1 broken (4 total)")
            .count(),
        1,
        "outer set folds the nested counts into its own direct @test; got:\n{output}"
    );
}

/// Issue #10338: aggregation composes across THREE levels — each level's
/// summary covers everything beneath it.
#[test]
fn three_level_nested_testsets_aggregate_transitively_10338() {
    let (failed, output) = run_capturing_output(
        r#"
using Test
@testset "L1_10338" begin
    @testset "L2_10338" begin
        @testset "L3_10338" begin
            @test 1 == 1
        end
        @test 2 == 2
    end
    @test 3 == 3
end
true
"#,
    );
    assert!(!failed, "all tests pass; got:\n{output}");
    assert_eq!(
        output.matches("  1 passed, 0 failed (1 total)").count(),
        1,
        "L3 summary appears exactly once; got:\n{output}"
    );
    assert_eq!(
        output.matches("  2 passed, 0 failed (2 total)").count(),
        1,
        "L2 aggregates L3 + its own test; got:\n{output}"
    );
    assert_eq!(
        output.matches("  3 passed, 0 failed (3 total)").count(),
        1,
        "L1 aggregates the whole tree; got:\n{output}"
    );
}
