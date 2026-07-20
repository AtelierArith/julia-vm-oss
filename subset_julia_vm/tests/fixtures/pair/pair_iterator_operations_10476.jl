using Test

@testset "Pair iterator-like operations" begin
    p = Pair(1, 2)

    @test iterate(p) == (1, 2)
    @test iterate(p, 2) == (2, 3)
    @test iterate(p, 3) === nothing
    @test length(p) == 2
    @test last(p) == 2
    @test collect(p) == [1, 2]
    @test typeof(collect(p)) === Vector{Int64}
    @test sum(p) == 3

    f10476(x, y) = x + y
    @test f10476(p...) == 3
end

@testset "Pair mixed iterator eltype" begin
    q = Pair(1, "two")
    @test collect(q) == [1, "two"]
end

true
