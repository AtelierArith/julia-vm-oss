using LinearAlgebra
using Test

@testset "typed Matrix constructor materializes SymTridiagonal (Issue #8395)" begin
    S = SymTridiagonal([0.0, 0.0, 0.0], [0.1, 0.2])
    M = Matrix{Float64}(S)

    @test typeof(M) === Matrix{Float64}
    @test size(M) == (3, 3)
    @test M[1, 1] == 0.0
    @test M[1, 2] == 0.1
    @test M[2, 1] == 0.1
    @test M[2, 3] == 0.2
    @test M[3, 2] == 0.2
end

true
