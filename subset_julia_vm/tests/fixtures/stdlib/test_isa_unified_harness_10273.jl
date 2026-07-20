# Unified @test-family recording harness — @test isa fast path (Issue #10273)
#
# `@test x isa T` / `@test isa(x, T)` used to be lowered to a dedicated
# `Stmt::Test` → `Instr::Test` fast path that evaluated the condition OUTSIDE
# any try/catch, so a throwing isa-test escaped the enclosing `@testset` as a
# runtime error instead of being recorded as an "errored" outcome (Issue
# #10093 left this hole in the fast path even after `macro test` gained its
# try/catch). #10273 reroutes the isa fast path through the same recording IR
# as the macro path (`_test_record!` in a try, `_test_record_error!` in the
# catch), so both `isa` forms route through the unified recorder.
#
# This is the GREEN half: it verifies the rerouted isa lowering preserves the
# semantics of PASSING isa-tests (call form, infix form, negation, and inside
# a loop). The RED half — a THROWING isa-test recorded as errored + exit 1 —
# cannot be a fixture (the harness @testset gate rejects any fixture that
# records a failure, Issue #9360); it is covered by the Rust integration test
# `test_harness_entry_point_coverage_10273` in
# tests/testset_exit_code_8191_tests.rs.

using Test

function isa_catches_internally()
    try
        error("internal boom")
    catch
        42
    end
end

# Bare (no-@testset) isa test still records cleanly.
@test isa(0x01, UInt8)

@testset "isa call form records through the unified harness" begin
    @test isa(1, Int)
    @test isa("s", String)
    @test !isa(1, String)
    # An exception raised and caught INSIDE the isa expression is invisible to
    # the wrapper: the test records its Bool result.
    @test isa(isa_catches_internally(), Int)
    # Infix form goes through the same fast path.
    @test 1.5 isa Float64
    for i in 1:3
        @test isa(i, Int)
    end
end

true
