using Test

@testset "Array dimensional aliases in runtime type objects (Issue #5593)" begin
    @test string(Base.unwrap_unionall(Array)) == "Array{T, N}"
    @test string(Base.unwrap_unionall(Vector)) == "Array{T, 1}"
    @test string(Base.unwrap_unionall(Matrix)) == "Array{T, 2}"
    @test string(Base.unwrap_unionall(DenseArray)) == "DenseArray{T, N}"
    @test string(Base.unwrap_unionall(DenseVector)) == "DenseArray{T, 1}"
    @test string(Base.unwrap_unionall(DenseMatrix)) == "DenseArray{T, 2}"

    @test Base.unwrap_unionall(Vector).parameters[2] === 1
    @test Base.unwrap_unionall(Matrix).parameters[2] === 2
    @test Base.unwrap_unionall(DenseVector).parameters[2] === 1
    @test Base.unwrap_unionall(DenseMatrix).parameters[2] === 2

    @test typeof(DenseArray) === UnionAll
    @test typeof(DenseVector) === UnionAll
    @test nameof(Vector) === :Array
    @test nameof(DenseVector) === :DenseArray

    @test supertype(Vector{Int}) === DenseVector{Int64}
    @test supertype(Matrix{Int}) === DenseMatrix{Int64}
    @test supertype(Array{Int,3}) === DenseArray{Int64,3}
    @test supertype(DenseVector{Int}) === AbstractVector{Int64}

    @test Vector{Int} <: DenseVector{Int}
    @test DenseVector{Int} === DenseArray{Int,1}
    @test DenseMatrix{Float64} === DenseArray{Float64,2}

    @test Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector) === Vector
    @test Base.rewrap_unionall(Base.unwrap_unionall(Array), Array) === Array
    @test Base.rewrap_unionall(Base.unwrap_unionall(DenseVector), DenseVector) === DenseVector
    @test Base.rewrap_unionall(Base.unwrap_unionall(DenseArray), DenseArray) === DenseArray
end

true
