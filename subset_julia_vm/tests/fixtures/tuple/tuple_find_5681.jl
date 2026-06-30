using Test

# Issue #5681: findfirst/findlast/findall over a Tuple. These return indices
# (Int / nothing / Vector{Int}), matching upstream — previously unsupported
# (NoMethodFound) on tuples.

@testset "findfirst/findlast/findall on tuples (Issue #5681)" begin
    @test findfirst(iseven, (1, 3, 4, 5)) == 3
    @test findfirst(iseven, (1, 3, 5)) === nothing
    @test findfirst(isodd, (2, 4, 6, 7)) == 4
    @test findfirst(x -> x > 10, (5, 15, 25)) == 2

    @test findlast(iseven, (1, 4, 3, 6)) == 4
    @test findlast(iseven, (1, 3, 5)) === nothing
    @test findlast(isodd, (2, 4, 5, 7)) == 4

    @test findall(iseven, (1, 2, 3, 4)) == [2, 4]
    @test findall(iseven, (1, 3, 5)) == Int[]
    @test findall(iseven, ()) == Int[]
    @test findall(x -> x > 2, (1, 2, 3, 4)) == [3, 4]

    # eltype of the index vector is Int.
    @test eltype(findall(iseven, (1, 2, 3, 4))) == Int
end

true
