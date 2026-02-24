# Test ispow2(), nextpow(), prevpow() (Issue #1865)

using Test

@testset "ispow2 basic" begin
    @test ispow2(1) == true
    @test ispow2(2) == true
    @test ispow2(4) == true
    @test ispow2(8) == true
    @test ispow2(16) == true
    @test ispow2(3) == false
    @test ispow2(5) == false
    @test ispow2(6) == false
    @test ispow2(0) == false
end

@testset "nextpow basic" begin
    @test nextpow(2, 1) == 1
    @test nextpow(2, 3) == 4
    @test nextpow(2, 4) == 4
    @test nextpow(2, 5) == 8
    @test nextpow(2, 7) == 8
    @test nextpow(2, 9) == 16
end

@testset "prevpow basic" begin
    @test prevpow(2, 4) == 4
    @test prevpow(2, 5) == 4
    @test prevpow(2, 7) == 4
    @test prevpow(2, 8) == 8
    @test prevpow(2, 9) == 8
end

true
