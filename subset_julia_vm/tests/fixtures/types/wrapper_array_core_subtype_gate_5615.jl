using Test

@testset "wrapper array subtype CoreType gate (Issue #5615)" begin
    v = view([1, 2, 3], 1:2)
    vt = typeof(v)

    @test vt <: AbstractVector{Int64}
    @test vt <: AbstractArray{Int64,1}
    @test !(vt <: DenseVector{Int64})
    @test !(vt <: DenseArray{Int64,1})

    r = reshape(view([1, 2, 3, 4], 1:4), 2, 2)
    rt = typeof(r)

    @test rt <: AbstractMatrix{Int64}
    @test rt <: AbstractArray{Int64,2}
    @test !(rt <: DenseMatrix{Int64})
    @test !(rt <: DenseArray{Int64,2})
end

true
