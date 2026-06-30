# Test: parametric Ref{T}(x) / Base.RefValue{T}(x) constructors (Issue #5130)

using Test

@testset "Ref{T} / Base.RefValue{T} constructors" begin
    rf = Ref{Float64}(0.0)
    @assert rf[] == 0.0
    @assert typeof(rf) === Base.RefValue{Float64}

    ri = Ref{Int}(7)
    @assert ri[] == 7
    @assert typeof(ri) === Base.RefValue{Int}

    rv = Base.RefValue{Int}(3)
    @assert rv[] == 3
    @assert typeof(rv) === Base.RefValue{Int}

    @test (true)
end

true  # Test passed
