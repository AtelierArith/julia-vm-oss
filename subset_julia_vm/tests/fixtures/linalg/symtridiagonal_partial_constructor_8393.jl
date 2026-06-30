using LinearAlgebra
using Test

@testset "partial SymTridiagonal constructor (Issue #8393)" begin
    dv = [0.0, 0.0, 0.0]
    ev = [0.5, 0.25]
    H = SymTridiagonal{Float64}(dv, ev)

    @test size(H) == (3, 3)
    @test typeof(H) === SymTridiagonal{Float64, Vector{Float64}}
    @test H.dv === dv
    @test H.ev === ev
end

true
