using Test

@testset "RNG and range VM arrays stay Julia-visible (Issue #3908)" begin
    r = rand(2, 3)
    @test typeof(r) == Matrix{Float64}
    @test size(r) == (2, 3)
    @test length(r) == 6

    n = randn(4)
    @test typeof(n) == Vector{Float64}
    @test size(n) == (4,)
    @test length(n) == 4

    c = collect(1:3)
    @test typeof(c) == Vector{Int64}
    @test c == [1, 2, 3]
end

true
