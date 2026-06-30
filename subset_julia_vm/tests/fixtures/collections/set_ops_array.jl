# Set operations on arrays: union, intersect, setdiff, symdiff (Issue #2007)
# In Julia, these functions work with any iterables, not just Sets.

using Test

@testset "set operations on arrays (Issue #2007)" begin
    # intersect: elements in both arrays
    @test intersect([1, 2, 3], [2, 3, 4]) == [2.0, 3.0]
    @test length(intersect([1, 2, 3], [4, 5])) == 0

    # setdiff: elements in first but not second
    @test setdiff([1, 2, 3], [2]) == [1.0, 3.0]
    @test length(setdiff([1, 2, 3], [1, 2, 3])) == 0

    # symdiff: elements in one but not both (symmetric difference)
    @test symdiff([1, 2, 3], [2, 3, 4]) == [1.0, 4.0]

    # union: all unique elements from both arrays
    @test union([1, 2, 3], [3, 4, 5]) == [1.0, 2.0, 3.0, 4.0, 5.0]

    # Duplicate handling
    @test union([1, 1, 2], [2, 3]) == [1.0, 2.0, 3.0]
    @test intersect([1, 1, 2, 3], [2, 3, 3, 4]) == [2.0, 3.0]
end

@testset "same-element vector set operations preserve result type (#4018)" begin
    u8 = union(Int8[1, 2], Int8[2, 3])
    @test u8 == Int8[1, 2, 3]
    @test typeof(u8) === Vector{Int8}
    @test eltype(u8) === Int8

    i16 = intersect(Int16[1, 2], Int16[2, 3])
    @test i16 == Int16[2]
    @test typeof(i16) === Vector{Int16}
    @test eltype(i16) === Int16

    f32 = setdiff(Float32[1, 2, 3], Float32[2])
    @test f32 == Float32[1, 3]
    @test typeof(f32) === Vector{Float32}
    @test eltype(f32) === Float32

    syms = symdiff([:a, :b], [:b, :c])
    @test syms == [:a, :c]
    @test typeof(syms) === Vector{Symbol}
    @test eltype(syms) === Symbol

    any_values = union(Any[1, 2], Any[2, 3])
    @test length(any_values) == 3
    @test any_values[1] == 1
    @test any_values[2] == 2
    @test any_values[3] == 3
    @test typeof(any_values) === Vector{Any}
    @test eltype(any_values) === Any
end

true
