# Test powermod() and invmod() (Issue #1865)

using Test

@testset "powermod basic" begin
    @test powermod(2, 10, 1000) == 24
    @test powermod(3, 4, 5) == 1
    @test powermod(2, 0, 7) == 1
    @test powermod(5, 3, 13) == 8
end

@testset "invmod basic" begin
    @test invmod(3, 7) == 5
    @test invmod(2, 5) == 3
    @test mod(3 * invmod(3, 7), 7) == 1
    @test mod(2 * invmod(2, 5), 5) == 1
end

true
