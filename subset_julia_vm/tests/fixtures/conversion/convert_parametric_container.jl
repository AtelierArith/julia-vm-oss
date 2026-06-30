# convert(::Type{Vector{T}}, x) recursively converts each element into a newly
# allocated T-element array, mirroring upstream Julia's
#   convert(::Type{T}, a::AbstractArray) where {T<:Array} = a isa T ? a : T(a)::T
# (julia/base/array.jl). When the source already has the exact target type the
# original object is returned unchanged (identity). A non-convertible element
# raises InexactError, exactly as upstream (Issue #5111).

using Test

@testset "convert(Vector{Float64}, x) widens each element" begin
    a = convert(Vector{Float64}, [1, 2, 3])
    @test a == [1.0, 2.0, 3.0]
    @test typeof(a) === Vector{Float64}
    @test eltype(a) === Float64
end

@testset "convert(Vector{Int}, x) narrows integral floats" begin
    b = convert(Vector{Int}, Float64[1.0, 2.0])
    @test b == [1, 2]
    @test typeof(b) === Vector{Int64}
    @test eltype(b) === Int64
end

@testset "convert returns the source unchanged when types match (identity)" begin
    d = [1.0, 2.0, 3.0]
    e = convert(Vector{Float64}, d)
    @test e === d

    s = [10, 20, 30]
    @test convert(Vector{Int}, s) === s
end

@testset "convert(Array{T}, x) without N dispatches to the Vector path" begin
    f = convert(Array{Float64}, [1, 2, 3])
    @test f == [1.0, 2.0, 3.0]
    @test typeof(f) === Vector{Float64}
end

@testset "convert(Vector{Float64}, range) materializes and converts" begin
    g = convert(Vector{Float64}, 1:3)
    @test g == [1.0, 2.0, 3.0]
    @test typeof(g) === Vector{Float64}
end

@testset "convert preserves emptiness and element type for empty input" begin
    h = convert(Vector{Float64}, Int[])
    @test typeof(h) === Vector{Float64}
    @test isempty(h)
end

@testset "convert(Vector{Int8}, non-integral / out-of-range floats) throws InexactError" begin
    # The recursive element conversion propagates the same InexactError that the
    # scalar `convert(Int8, e)` raises for a non-integral or out-of-range value.
    # (A narrow integer element type is used here because float->Int8 integrality
    # is range-checked by the VM; the float->Int64 path is not yet checked — a
    # pre-existing scalar-convert gap tracked separately, not part of #5111.)
    @test_throws InexactError convert(Vector{Int8}, [1.5, 2.5])
    @test_throws InexactError convert(Vector{Int8}, [300.0])
end

true
