using Test

@testset "HOF range type inference (Issue #5094)" begin
    mapped_float = map(x -> x * 2.0, 1:3)
    @test mapped_float == [2.0, 4.0, 6.0]
    @test typeof(mapped_float) == Vector{Float64}

    mapped_int = map(x -> x + 1, 1:3)
    @test mapped_int == [2, 3, 4]
    @test typeof(mapped_int) == Vector{Int64}

    reduced = reduce((acc, x) -> acc + x, 1:4)
    @test reduced == 10
    @test typeof(reduced) == Int64

    filtered = filter(x -> x > 2, 1:5)
    @test filtered == [3, 4, 5]
    @test typeof(filtered) == Vector{Int64}
end

true
