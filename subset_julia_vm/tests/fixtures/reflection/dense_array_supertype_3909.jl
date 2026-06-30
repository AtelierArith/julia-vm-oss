using Test

@testset "Dense array supertype aliases (Issue #3909)" begin
    @test string(DenseArray) == "DenseArray"
    @test string(DenseVector) == "DenseVector"
    @test string(DenseMatrix) == "DenseMatrix"
    @test string(supertype(Vector{Int})) == "DenseVector{Int64}"
    @test string(supertype(Matrix{Int})) == "DenseMatrix{Int64}"
    @test string(supertype(Array{Int,1})) == "DenseVector{Int64}"
    @test string(supertype(Array{Int,2})) == "DenseMatrix{Int64}"
    @test string(supertype(Array{Int,3})) == "DenseArray{Int64, 3}"
    @test string(supertype(DenseVector{Int})) == "AbstractVector{Int64}"
    @test string(supertype(DenseMatrix{Int})) == "AbstractMatrix{Int64}"
    @test string(supertype(DenseArray{Int,1})) == "AbstractVector{Int64}"
    @test string(supertype(DenseArray{Int,2})) == "AbstractMatrix{Int64}"
    @test string(supertype(DenseArray{Int,3})) == "AbstractArray{Int64, 3}"
    @test Vector{Int} <: DenseVector{Int}
    @test Matrix{Int} <: DenseMatrix{Int}
    @test Array{Int,2} <: DenseMatrix{Int}
    @test Array{Int,3} <: DenseArray{Int,3}
    @test !(Vector{Int} <: DenseVector{Float64})
    @test !(Array{Int,2} <: DenseArray{Int,1})
end

true
