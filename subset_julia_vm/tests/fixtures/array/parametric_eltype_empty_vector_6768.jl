# Issue #6768: an empty typed Vector whose element type is PARAMETRIC
# (e.g. UnitRange{Int64}, Vector{Int}, Complex{Float64}) must keep its eltype
# after push!, matching upstream — it previously widened to Vector{Any}.

using Test

@testset "parametric eltype preserved for empty typed vector (#6768)" begin
    # Vector{T}() constructor form
    v = Vector{UnitRange{Int64}}()
    push!(v, 1:2)
    @test typeof(v) == Vector{UnitRange{Int64}}
    @test eltype(v) == UnitRange{Int64}

    # T[] literal form
    u = UnitRange{Int64}[]
    push!(u, 1:2)
    @test typeof(u) == Vector{UnitRange{Int64}}

    # Empty constructor must already carry the eltype (before any push!)
    @test typeof(Vector{UnitRange{Int64}}()) == Vector{UnitRange{Int64}}
    @test eltype(UnitRange{Int64}[]) == UnitRange{Int64}

    # Nested parametric eltype: Vector{Vector{Int}} (Int normalizes to Int64)
    nv = Vector{Vector{Int}}()
    push!(nv, [1, 2])
    @test typeof(nv) == Vector{Vector{Int64}}

    # Regression: simple concrete eltype still preserved
    w = Int64[]
    push!(w, 1)
    @test typeof(w) == Vector{Int64}

    # Regression: Complex parametric eltype still preserved
    c = Vector{Complex{Float64}}()
    push!(c, 1.0 + 2.0im)
    @test typeof(c) == Vector{ComplexF64}

    # The element value side is fine (UnitRange struct round-trips)
    @test typeof(1:2) == UnitRange{Int64}
    @test (1:2) isa UnitRange{Int64}
end

true  # Test passed
