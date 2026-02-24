# Test for src/base/operators.jl
# Based on Julia's test/operators.jl and test/numbers.jl
using Test

@testset "min and max" begin
    # min tests
    @test min(1, 2) == 1
    @test min(2, 1) == 1
    @test min(-1, 0) == -1
    @test min(0, -1) == -1
    @test min(1.0, 1) == 1

    # max tests
    @test max(1, 2) == 2
    @test max(2, 1) == 2
    @test max(-1, 0) == 0
    @test max(0, -1) == 0

    # Float tests
    @test min(1.5, 2.5) == 1.5
    @test max(1.5, 2.5) == 2.5
end

@testset "copysign and flipsign" begin
    # copysign: return x with the sign of y
    @test copysign(1, 2) == 1
    @test copysign(1, -2) == -1
    @test copysign(-1, 2) == 1
    @test copysign(-1, -2) == -1
    @test copysign(1, 0) == 1

    # flipsign: flip sign of x if y is negative
    @test flipsign(1, 2) == 1
    @test flipsign(1, -2) == -1
    @test flipsign(-1, 2) == -1
    @test flipsign(-1, -2) == 1
    @test flipsign(1, 0) == 1
end

@testset "cmp" begin
    @test cmp(1, 2) == -1
    @test cmp(2, 1) == 1
    @test cmp(1, 1) == 0
    @test cmp(-1, 0) == -1
    @test cmp(0, -1) == 1
    @test cmp(1.5, 2.5) == -1
    @test cmp(2.5, 1.5) == 1
end

@testset "isless" begin
    # From Julia's test/operators.jl
    @test isless(1, 2) == true
    @test isless(2, 1) == false
    @test isless(1, 1) == false
    @test isless(-1, 0) == true
    @test isless(0, -1) == false
end

@testset "isequal" begin
    @test isequal(1, 1) == true
    @test isequal(1, 2) == false
    @test isequal(1.0, 1.0) == true
    @test isequal(-0.0, 0.0) == true
end

@testset "isapprox" begin
    @test isapprox(1.0, 1.0) == true
    @test isapprox(1.0, 1.0 + 1e-10) == true
    @test isapprox(1.0, 2.0) == false
    @test isapprox(0.0, 0.0) == true
end

println("test_operators.jl: All tests passed!")
