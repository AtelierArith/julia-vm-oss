# xor() on integers - bitwise exclusive or (Issue #2042)

using Test

@testset "xor on Bool" begin
    @test xor(true, false) == true
    @test xor(false, true) == true
    @test xor(true, true) == false
    @test xor(false, false) == false
end

@testset "xor on Int64 - bitwise XOR" begin
    @test xor(5, 3) == 6       # 101 ^ 011 = 110
    @test xor(255, 15) == 240  # 11111111 ^ 00001111 = 11110000
    @test xor(0, 0) == 0
    @test xor(0, 42) == 42
    @test xor(42, 0) == 42
    @test xor(42, 42) == 0     # x ^ x == 0
end

true
