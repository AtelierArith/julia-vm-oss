using Test

# Regression test for Issue #3591:
# `stack(arrays)` previously hard-coded `Matrix{Float64}` output via
# `zeros(m, n)`, widening any non-Float input. Homogeneous input now allocates
# through similar(first_arr, m, n), preserving container element type.

@testset "stack preserves Int values (#3591)" begin
    x = stack([[1, 2], [3, 4]])
    @test typeof(x) === Matrix{Int64}
    @test eltype(x) === Int64
    @test size(x) == (2, 2)
    @test x[1, 1] == 1
    @test x[2, 1] == 2
    @test x[1, 2] == 3
    @test x[2, 2] == 4
    # Element value equality holds (matrix-level == checks each element)
    @test x == [1 3; 2 4]
end

@testset "stack preserves narrow numeric container types (#4018, #4603)" begin
    f32 = stack([Float32[1, 2], Float32[3, 4]])
    @test typeof(f32) === Matrix{Float32}
    @test eltype(f32) === Float32
    @test size(f32) == (2, 2)
    @test typeof(f32[1, 1]) === Float32
    @test f32[1, 1] == Float32(1)
    @test f32[2, 2] == Float32(4)

    i8 = stack([Int8[1, 2], Int8[3, 4]])
    @test typeof(i8) === Matrix{Int8}
    @test eltype(i8) === Int8
    @test size(i8) == (2, 2)
    @test typeof(i8[1, 1]) === Int8
    @test i8[1, 1] == Int8(1)
    @test i8[2, 2] == Int8(4)
end

@testset "stack promotes mixed input eltypes (#4018, #4652)" begin
    narrow = stack((Int8[1, 2], Int16[3, 4]))
    @test typeof(narrow) === Matrix{Int16}
    @test eltype(narrow) === Int16
    @test size(narrow) == (2, 2)
    @test typeof(narrow[1, 1]) === Int16
    @test narrow[1, 1] == Int16(1)
    @test narrow[1, 2] == Int16(3)
    @test narrow[2, 1] == Int16(2)
    @test narrow[2, 2] == Int16(4)

    floating = stack((Int8[1, 2], Float32[3, 4]))
    @test typeof(floating) === Matrix{Float32}
    @test eltype(floating) === Float32
    @test size(floating) == (2, 2)
    @test typeof(floating[1, 1]) === Float32
    @test floating[1, 1] == Float32(1)
    @test floating[1, 2] == Float32(3)
    @test floating[2, 1] == Float32(2)
    @test floating[2, 2] == Float32(4)

    boxed = stack((String["a", "b"], Any["c", "d"]))
    @test typeof(boxed) === Matrix{Any}
    @test eltype(boxed) === Any
    @test size(boxed) == (2, 2)
    @test boxed[1, 1] == "a"
    @test boxed[1, 2] == "c"
    @test boxed[2, 1] == "b"
    @test boxed[2, 2] == "d"
end

@testset "stack preserves Bool values" begin
    x = stack([[true, false], [false, true]])
    @test typeof(x) === Matrix{Bool}
    @test eltype(x) === Bool
    @test size(x) == (2, 2)
    @test x[1, 1] == true
    @test x[2, 2] == true
    @test x[1, 2] == false
end

@testset "stack preserves String values" begin
    x = stack([["a", "b"], ["c", "d"]])
    @test typeof(x) === Matrix{String}
    @test eltype(x) === String
    @test size(x) == (2, 2)
    @test x[1, 1] == "a"
    @test x[2, 2] == "d"
end

@testset "stack regression for Float64" begin
    x = stack([[1.0, 2.0], [3.0, 4.0]])
    @test typeof(x) === Matrix{Float64}
    @test eltype(x) === Float64
    @test size(x) == (2, 2)
    @test x[1, 1] == 1.0
    @test x[2, 2] == 4.0
end

@testset "stack edge cases" begin
    # Single column
    x = stack([[1, 2, 3]])
    @test size(x) == (3, 1)
    @test x[1, 1] == 1
    @test x[3, 1] == 3

    # Single-element columns
    y = stack([[1], [2], [3]])
    @test size(y) == (1, 3)
    @test y[1, 1] == 1
    @test y[1, 3] == 3
end

true
