using Test

@testset "parametric Set subtypes bare Set (Issue #4660)" begin
    @test Set{Int64} <: Set

    s = Set([1, 2])
    @test typeof(s) === Set{Int64}
    @test typeof(s) <: Set

    empty!(s)
    @test length(s) == 0
    @test typeof(s) === Set{Int64}
end

true
