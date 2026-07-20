# Test varargs hypot(x...) and 1-arg hypot (Issue #9410)
# Upstream: hypot(x::Number...) reduces over all args pivoting on the
# maximum-magnitude element (julia/base/math.jl _hypot(::NTuple)).

using Test

@testset "hypot three arguments" begin
    @test hypot(3, 4, 12) == 13.0
    @test hypot(3.0, 4.0, 12.0) == 13.0
    @test hypot(3, 4.0, 12) == 13.0
    @test hypot(0, 0, 0) == 0.0
end

@testset "hypot four+ arguments" begin
    @test hypot(1, 1, 1, 1) == 2.0
    @test hypot(2.0, 4.0, 4.0, 8.0) == 10.0
end

@testset "hypot one argument" begin
    @test hypot(-5.7) == 5.7
    @test hypot(3) == 3.0
end

@testset "hypot overflow and underflow safety" begin
    @test hypot(1e300, 1e300, 1e300) == 1.7320508075688774e300
    @test hypot(1e-300, 1e-300, 1e-300) == 1.7320508075688775e-300
end

@testset "hypot special values" begin
    @test hypot(Inf, 1.0, 2.0) == Inf
    @test hypot(Inf, 1.0, NaN) == Inf
    @test isnan(hypot(NaN, 1.0, 2.0))
end

@testset "hypot complex arguments" begin
    @test hypot(3, 4im, 12.0) == 13.0
end

true
