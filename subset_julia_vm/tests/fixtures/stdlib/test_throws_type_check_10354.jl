# Green-half regression coverage for `@test_throws` checking the expected
# exception (Issue #10354): before this fix, the macro's `catch` branch
# recorded `_test_record!(true, ...)` unconditionally, so a WRONG-type
# exception (or a wrong-value / non-matching-message one) was recorded as a
# Pass exactly like a right one -- a detection blind spot that hid 13 genuine
# sjulia bugs in this repo's own fixture suite before it was fixed (see
# docs/vm/EXCEPTION_PARITY.md "@test_throws type-check impact").
#
# This fixture is the GREEN half: every `@test_throws` form implemented here
# (Type / exception value / String / Regex / Array-of-strings / Function,
# mirroring upstream `do_test_throws`, `julia/stdlib/Test/src/Test.jl`)
# genuinely matching its expected exception. The RED half -- the SAME forms
# given a deliberately WRONG expectation, proving the macro now records a
# Fail (and sets the sticky any_test_failed flag) instead of silently
# passing -- cannot be a fixture: the harness's @testset gate rejects any
# fixture that records a failure (Issue #9360), so that half lives in
# `tests/testset_exit_code_8191_tests.rs`
# (`test_throws_checks_expected_exception_10354`,
# `test_throws_fail_message_names_expected_and_thrown_10354`).

using Test

@testset "Type form (the common case)" begin
    @test_throws ArgumentError throw(ArgumentError("boom"))
    @test_throws BoundsError [1, 2, 3][10]
    @test_throws DivideError div(typemin(Int64), Int64(-1))
end

@testset "Type form matches a SUBTYPE of the expected abstract type" begin
    # Upstream `do_test_throws` checks `exc isa expected`, so an abstract
    # expected type accepts any concrete subtype -- not just an exact type
    # match.
    @test_throws Exception throw(ArgumentError("boom"))
end

@testset "Exception-value form: type AND every field must match" begin
    @test_throws UndefVarError(:zzz_10354_green) zzz_10354_green
    @test_throws ArgumentError("exact message") throw(ArgumentError("exact message"))
end

@testset "String form: message substring match" begin
    @test_throws "boom" error("boom time")
    @test_throws "time" error("boom time")
end

@testset "Regex form: message regex match" begin
    @test_throws r"bo+m" error("boom time")
    @test_throws r"^boom" error("boom time")
end

@testset "Array-of-strings form: every element must occur in the message" begin
    @test_throws ["boom", "time"] error("boom time")
end

@testset "Function form: called on the message, must return true" begin
    @test_throws (msg -> occursin("boom", msg)) error("boom time")
end

true
