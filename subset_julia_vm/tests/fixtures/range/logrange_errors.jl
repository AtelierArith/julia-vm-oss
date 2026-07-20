# Test LogRange error cases (Issue #1835)
# Verify that invalid inputs produce the exception classes UPSTREAM produces.
#
# Issue #11146 (#10813 Phase 2a): every assertion here used to say
# `@test_throws ErrorException`, which was WRONG for all six cases — upstream
# raises `DomainError` for a zero/negative/non-finite endpoint and `ArgumentError`
# for a bad length, and neither is an `ErrorException` (they are siblings under
# `Exception`, not subtypes). The fixture "passed" only because `@test_throws`
# ignored its expected type (Issue #10354) AND sjulia's `LogRange` raised the
# equally-wrong `ErrorException` (it named the class inside an
# `error("DomainError: ...")` message, which throws an ErrorException whose text
# contradicts `typeof(e)`). Two wrongs cancelling out.
#
# With #10354's type-checking `@test_throws` and #11146's taxonomy funnel, both
# halves are corrected: `LogRange` throws the real classes, and the assertions
# name them. Verified against julia 1.12.6.

using Test

@testset "LogRange negative start" begin
    @test_throws DomainError logrange(-1.0, 10.0, 3)
end

@testset "LogRange zero start" begin
    @test_throws DomainError logrange(0.0, 10.0, 3)
end

@testset "LogRange negative stop" begin
    @test_throws DomainError logrange(1.0, -10.0, 3)
end

@testset "LogRange zero stop" begin
    @test_throws DomainError logrange(1.0, 0.0, 3)
end

@testset "LogRange non-finite endpoint" begin
    @test_throws DomainError logrange(1.0, Inf, 3)
end

@testset "LogRange negative length" begin
    @test_throws ArgumentError logrange(1.0, 10.0, -1)
end

@testset "LogRange endpoints differ with length=1" begin
    @test_throws ArgumentError logrange(1.0, 10.0, 1)
end

true
