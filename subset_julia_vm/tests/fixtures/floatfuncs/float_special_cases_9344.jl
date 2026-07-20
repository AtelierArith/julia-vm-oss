using Test

# Issue #9344: float special cases —
#   (1) isless total order: isless(-0.0, 0.0) == true, NaN sorts greatest,
#       and sort([0.0, -0.0]) == [-0.0, 0.0].
#   (2) literal_pow keeps Float32: typeof(0.5f0^2) == Float32 (regression guard).
#   (3) negative float base ^ non-integer exponent throws DomainError instead of NaN,
#       including a Rational exponent via ^(::AbstractFloat, ::Rational).

@testset "isless float total order" begin
    @test isless(-0.0, 0.0) == true
    @test isless(0.0, -0.0) == false
    @test isless(-0.0, -0.0) == false
    @test isless(0.0, 0.0) == false
    @test isless(-0.0f0, 0.0f0) == true
    @test isless(1.0, NaN) == true
    @test isless(NaN, 1.0) == false
    @test isless(NaN, NaN) == false
    @test sort([0.0, -0.0]) == [-0.0, 0.0]
    @test sort([1.0, NaN, -0.0, 0.0, -1.0])[end] === NaN
end

@testset "literal_pow preserves Float32" begin
    @test typeof(0.5f0^2) == Float32
    @test 0.5f0^2 == 0.25f0
    @test typeof(Float16(2)^2) == Float16
end

@testset "negative base non-integer exponent throws DomainError" begin
    @test_throws DomainError (-8.0)^(1//3)
    @test_throws DomainError (-8.0)^(1 / 3)
    @test_throws DomainError (-2.0)^0.5
    @test_throws DomainError (-8.0f0)^(1//3)
    # Valid / non-throwing cases stay correct.
    @test 8.0^(1//3) ≈ 2.0
    @test (-8.0)^2.0 == 64.0
    @test (-2.0)^NaN |> isnan
    @test 2^-1 == 0.5
end

true
