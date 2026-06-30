# Type-level ndims (Issue #5118): ndims(T) reads the dimension parameter N
# from an array type, and ndims(::Type{<:Number}) == 0. Value forms unchanged.

using Test

@testset "ndims type-level (Issue #5118)" begin
    # Array type forms: ndims(Array{T,N}) === N
    @test ndims(Vector{Int}) === 1
    @test ndims(Matrix{Int}) === 2
    @test ndims(Array{Int,3}) === 3
    @test ndims(Vector{Float64}) === 1
    @test ndims(Matrix{Float64}) === 2
    @test ndims(Array{Float64,4}) === 4
    @test ndims(Array{Bool,5}) === 5

    # Value forms still work and agree with their types
    @test ndims([1, 2, 3]) === 1
    @test ndims([1 2; 3 4]) === 2
    @test ndims(zeros(2, 3, 4)) === 3

    # Type form matches value form
    @test ndims(Vector{Int}) === ndims([1, 2, 3])
    @test ndims(Matrix{Int}) === ndims([1 2; 3 4])

    # ndims(::Type{<:Number}) === 0
    @test ndims(Int) === 0
    @test ndims(Float64) === 0
    @test ndims(Number) === 0
    @test ndims(Bool) === 0

    # Scalar value forms are 0-dimensional
    @test ndims(1) === 0
    @test ndims(3.14) === 0
end

true
