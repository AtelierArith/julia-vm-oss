using Test

@testset "integer parity" begin
    @test isodd(5) == true
    @test isodd(4) == false
    @test iseven(4) == true
    @test iseven(5) == false
end

@testset "integer abs and sign" begin
    @test abs(-42) == 42
    @test abs(42) == 42
    @test signbit(-5) == true
    @test signbit(5) == false
end

@testset "integer gcd" begin
    @test gcd(48, 18) == 6
    @test gcd(7, 5) == 1
    @test gcd(0, 5) == 5
    @test gcd(12, 12) == 12
end

@testset "integer divrem" begin
    @test div(10, 3) == 3
    @test div(7, 2) == 3
    r = divrem(10, 3)
    @test r[1] == 3
    @test r[2] == 1
end

@testset "integer bitwise" begin
    @test 5 & 3 == 1
    @test 5 | 3 == 7
    @test xor(5, 3) == 6
    @test 4 << 2 == 16
    @test 16 >> 2 == 4
end

true
