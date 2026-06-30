//! Issue #8191: a failing `@test` / `@testset` must make the process exit
//! non-zero, matching upstream Julia (which throws a `TestSetException` → exit
//! 1). sjulia records failures without throwing, so the VM exposes a sticky
//! `any_test_failed()` flag that the CLI maps to a non-zero exit code.
//!
//! The fixture harness checks the final returned value, not the exit code, so a
//! `@testset`-only fixture ending in `true` cannot regression-test this. These
//! integration tests exercise the flag directly.

use subset_julia_vm::{
    compile::{cache::clear_cache, compile_with_cache},
    pipeline::parse_and_lower_with_base_dir,
    rng::StableRng,
    vm::Vm,
};

/// Run a source string through the full pipeline and return whether any test
/// failed (the value the CLI uses to pick its exit code).
fn any_test_failed(source: &str) -> bool {
    clear_cache();
    let program =
        parse_and_lower_with_base_dir(source, None).expect("source should parse and lower");
    let compiled = compile_with_cache(&program).expect("program should compile");
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    vm.run()
        .expect("program should run without a runtime error");
    vm.any_test_failed()
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
