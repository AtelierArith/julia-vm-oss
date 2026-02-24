using Test

@testset "Pair arrow syntax" begin
    p = Pair(:a, 1)
    @test p.first == :a
    @test p.second == 1
end

@testset "Pair equality" begin
    @test Pair(1, 2) == Pair(1, 2)
    @test Pair(1, 2) != Pair(2, 1)
    @test Pair("a", 1) == Pair("a", 1)
end

@testset "Pair nested" begin
    p = Pair(Pair(1, 2), 3)
    @test p.first == Pair(1, 2)
    @test p.second == 3
end

@testset "Pair with different types" begin
    p1 = Pair(1, "one")
    p2 = Pair(true, 3.14)
    @test p1.first == 1
    @test p1.second == "one"
    @test p2.first == true
    @test p2.second == 3.14
end

true
