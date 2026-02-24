# Test for src/base/number.jl
# Based on Julia's test/numbers.jl
using Test

@testset "number predicates" begin
    # iszero and isone - from Julia's test/numbers.jl
    @test iszero(0) == true
    @test iszero(1) == false
    @test iszero(0.0) == true
    @test iszero(1.0) == false

    @test isone(1) == true
    @test isone(0) == false
    @test isone(1.0) == true
    @test isone(0.0) == false

    # ispositive and isnegative
    @test ispositive(1) == true
    @test ispositive(0) == false
    @test ispositive(-1) == false

    @test isnegative(-1) == true
    @test isnegative(0) == false
    @test isnegative(1) == false
end

@testset "identity" begin
    @test identity(1) == 1
    @test identity(3.14) == 3.14
    @test identity(-42) == -42
end

@testset "oneunit" begin
    @test oneunit(5) == 1
    @test oneunit(3.14) == 1
    @test oneunit(-7) == 1
end

@testset "abs2" begin
    @test abs2(3) == 9
    @test abs2(-4) == 16
    @test abs2(2.5) == 6.25
    @test abs2(-2.5) == 6.25
    @test abs2(0) == 0
end

@testset "isreal" begin
    @test isreal(1) == true
    @test isreal(3.14) == true
    @test isreal(0) == true
end

println("test_number.jl: All tests passed!")
