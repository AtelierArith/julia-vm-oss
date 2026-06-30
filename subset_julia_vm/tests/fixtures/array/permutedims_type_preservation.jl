using Test

# Regression test for Issue #3590:
# `permutedims([true, false])` previously returned `Matrix{Float64}`
# because the implementation only special-cased `T == Int64` and fell
# back to `zeros(...)` (Float64) for everything else. Per the #3590
# acceptance criteria, the result must preserve the input element type.
#
# Result allocation now uses `similar(arr, ...)`, so narrow element types
# are preserved without a flat `Any[]` fallback.

@testset "permutedims preserves Matrix{Bool} (#3590)" begin
    x = permutedims([true, false])
    @test size(x) == (1, 2)
    @test x[1, 1] == true
    @test x[1, 2] == false
    @test typeof(x) === Matrix{Bool}
end

@testset "permutedims preserves Matrix{Int64} (1D)" begin
    x = permutedims([1, 2, 3])
    @test size(x) == (1, 3)
    @test x == [1 2 3]
    @test typeof(x) === Matrix{Int64}
end

@testset "permutedims preserves Matrix{Float64} (regression)" begin
    x = permutedims([1.0, 2.0])
    @test size(x) == (1, 2)
    @test x == [1.0 2.0]
    @test typeof(x) === Matrix{Float64}
end

@testset "permutedims preserves String values" begin
    x = permutedims(["a", "b"])
    @test size(x) == (1, 2)
    @test x[1, 1] == "a"
    @test x[1, 2] == "b"
    @test typeof(x) === Matrix{String}
end

@testset "permutedims 2D transpose preserves element type" begin
    m = [1 2; 3 4]
    t = permutedims(m)
    @test size(t) == (2, 2)
    @test t == [1 3; 2 4]
    @test typeof(t) === Matrix{Int64}

    mb = [true false; false true]
    tb = permutedims(mb)
    @test tb == [true false; false true]
    @test typeof(tb) === Matrix{Bool}

    mf = [1.0 2.0; 3.0 4.0]
    tf = permutedims(mf)
    @test tf == [1.0 3.0; 2.0 4.0]
    @test typeof(tf) === Matrix{Float64}
end

@testset "permutedims preserves narrow and Float32 element types (#4018, #4656)" begin
    v8 = permutedims(Int8[1, 2])
    @test typeof(v8) === Matrix{Int8}
    @test eltype(v8) === Int8
    @test size(v8) == (1, 2)
    @test typeof(v8[1, 1]) === Int8
    @test v8[1, 1] == Int8(1)
    @test v8[1, 2] == Int8(2)

    m8 = permutedims(reshape(Int8[1, 2, 3, 4], 2, 2))
    @test typeof(m8) === Matrix{Int8}
    @test eltype(m8) === Int8
    @test size(m8) == (2, 2)
    @test typeof(m8[1, 1]) === Int8
    @test m8[1, 1] == Int8(1)
    @test m8[1, 2] == Int8(2)
    @test m8[2, 1] == Int8(3)
    @test m8[2, 2] == Int8(4)

    vf32 = permutedims(Float32[1, 2])
    @test typeof(vf32) === Matrix{Float32}
    @test eltype(vf32) === Float32
    @test size(vf32) == (1, 2)
    @test typeof(vf32[1, 1]) === Float32
    @test vf32[1, 1] == Float32(1)
    @test vf32[1, 2] == Float32(2)

    mf32 = permutedims(reshape(Float32[1, 2, 3, 4], 2, 2))
    @test typeof(mf32) === Matrix{Float32}
    @test eltype(mf32) === Float32
    @test size(mf32) == (2, 2)
    @test typeof(mf32[1, 1]) === Float32
    @test mf32[1, 1] == Float32(1)
    @test mf32[1, 2] == Float32(2)
    @test mf32[2, 1] == Float32(3)
    @test mf32[2, 2] == Float32(4)
end

true
