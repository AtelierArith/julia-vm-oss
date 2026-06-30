# Issue #3548: typed empty array literals (Int32[], Float32[], UInt8[], …)
# must report `Vector{T}` from `typeof` at runtime, not `Vector{Int64}` /
# `Vector{Float64}`.
using Test

@testset "Issue #3548 typed empty array runtime types" begin
    @test typeof(Int32[]) === Vector{Int32}
    @test typeof(Int16[]) === Vector{Int16}
    @test typeof(Int8[]) === Vector{Int8}
    @test typeof(UInt8[]) === Vector{UInt8}
    @test typeof(UInt16[]) === Vector{UInt16}
    @test typeof(UInt32[]) === Vector{UInt32}
    @test typeof(UInt64[]) === Vector{UInt64}
    @test typeof(Float32[]) === Vector{Float32}
    @test typeof(Float64[]) === Vector{Float64}
    @test typeof(Bool[]) === Vector{Bool}
    @test typeof(Int64[]) === Vector{Int64}

    # Pushing values of the correct type works and preserves element type.
    xs = Int32[]
    push!(xs, Int32(7))
    @test eltype(xs) === Int32
    @test xs[1] === Int32(7)

    ys = UInt8[]
    push!(ys, 0x05)
    @test eltype(ys) === UInt8
    @test ys[1] === UInt8(5)
end

true
