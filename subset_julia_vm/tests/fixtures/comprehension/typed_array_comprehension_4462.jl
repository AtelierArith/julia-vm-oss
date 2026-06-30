# Typed array comprehension T[expr for var in iter] (Issue #4462)

using Test

@testset "Typed array comprehension parses and evaluates (Issue #4462)" begin
    ys = Float64[i / 10.0 for i in 1:10]
    @test ys == [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
    @test typeof(ys) == Vector{Float64}
end

true
