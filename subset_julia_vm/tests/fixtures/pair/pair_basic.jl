using Test

p = Pair(1, 2)

@testset "Pair basic" begin
    @test p.first == 1
    @test p.second == 2
    @test isa(p, Pair)
end

p2 = Pair("hello", 42)

@testset "Pair mixed types" begin
    @test p2.first == "hello"
    @test p2.second == 42
end

true
