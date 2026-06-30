using LinearAlgebra
using Test

@testset "hasmethod sees Diagonal SVD multiplication" begin
    A = rand(3, 4)
    F = svd(A)
    D = Diagonal(F.S)

    @test typeof(F.U) == Matrix{Float64}
    @test typeof(D) <: Diagonal
    @test hasmethod(*, Tuple{typeof(F.U), typeof(D)})
    @test !isempty(Base.return_types(*, Tuple{typeof(F.U), typeof(D)}))
    @test Base.infer_return_type(*, Tuple{typeof(F.U), typeof(D)}) !== Union{}
end

true
