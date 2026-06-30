using Test

# Issue #6202 (part of #5926 / #5072): runtime dispatch from an imprecise
# container element must still rank bounded `where` methods by their actual
# runtime value type. The static path already picked the tighter bound; the
# `Any` container path previously fell back to the looser method.

type_bound(::Type{T}) where {T<:Real} = :real
type_bound(::Type{T}) where {T<:Integer} = :integer

vector_bound(::Vector{T}) where {T<:Real} = :real
vector_bound(::Vector{T}) where {T<:Integer} = :integer

@testset "bounded Type{T} dispatch from Any container" begin
    xs = Any[Int64, Float64]
    @test type_bound(Int64) === :integer
    @test type_bound(Float64) === :real
    @test type_bound(xs[1]) === :integer
    @test type_bound(xs[2]) === :real
end

@testset "bounded Vector{T} dispatch from Any container" begin
    xs = Any[[1, 2], Float64[1.0, 2.0]]
    @test vector_bound([1, 2]) === :integer
    @test vector_bound(Float64[1.0, 2.0]) === :real
    @test vector_bound(xs[1]) === :integer
    @test vector_bound(xs[2]) === :real
end

true
