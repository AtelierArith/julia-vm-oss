using Test

@testset "parenthesized UnionAll application (Issue #8430)" begin
    applied = (Vector{T} where T){Int}
    @test applied === Vector{Int}
    @test applied == Vector{Int}
    @test typeof(applied) === DataType
    @test string(applied) == "Vector{Int64}"
end

true
