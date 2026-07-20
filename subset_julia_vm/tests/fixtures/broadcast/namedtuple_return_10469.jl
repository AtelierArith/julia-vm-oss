using Test

@testset "NamedTuple-returning broadcast materializes typed arrays (Issue #10469)" begin
    h(i) = (a = i, b = 3 * i)
    r = h.(1:3)

    @test r == [(a = 1, b = 3), (a = 2, b = 6), (a = 3, b = 9)]
    @test typeof(r) == Vector{@NamedTuple{a::Int64, b::Int64}}
    @test eltype(r) == @NamedTuple{a::Int64, b::Int64}
    @test typeof(r[1]) == @NamedTuple{a::Int64, b::Int64}

    mixed(i) = (a = i, b = string(i))
    mixed_result = mixed.(1:2)

    @test mixed_result == [(a = 1, b = "1"), (a = 2, b = "2")]
    @test typeof(mixed_result) == Vector{@NamedTuple{a::Int64, b::String}}
    @test eltype(mixed_result) == @NamedTuple{a::Int64, b::String}

    matrix_result = h.(reshape([1, 2, 3, 4], 2, 2))
    @test matrix_result == [(a = 1, b = 3) (a = 3, b = 9); (a = 2, b = 6) (a = 4, b = 12)]
    @test typeof(matrix_result) == Matrix{@NamedTuple{a::Int64, b::Int64}}
    @test eltype(matrix_result) == @NamedTuple{a::Int64, b::Int64}
    @test size(matrix_result) == (2, 2)
end

true
