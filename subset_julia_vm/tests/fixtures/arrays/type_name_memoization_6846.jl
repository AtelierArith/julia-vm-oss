# Regression guard for Issue #6846 (type-name parse memoization in
# `CoreType::from_julia_name` + cheap array-wrapper type derivation in
# `array_wrapper_julia_type`). Exercises typeof / isa / eltype / dispatch across
# many distinct element types and ndims in one program, so a memoization cache
# that cross-contaminated entries, or a cheap base-name check that mis-classified
# a wrapper, would surface here. The repeated dispatch loop hits the warm cache.
using Test
using LinearAlgebra

@testset "array wrapper type identity stable under memoized parsing (#6846)" begin
    vf = [1.0, 2.0, 3.0]
    vi = [1, 2, 3]
    vb = [true, false]
    vc = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
    m = [1.0 2.0; 3.0 4.0]

    @test typeof(vf) == Vector{Float64}
    @test typeof(vi) == Vector{Int}
    @test typeof(vb) == Vector{Bool}
    @test typeof(vc) == Vector{Complex{Float64}}
    @test typeof(m) == Matrix{Float64}

    @test vf isa AbstractVector
    @test vf isa AbstractArray
    @test !(vf isa AbstractMatrix)
    @test m isa AbstractMatrix
    @test vi isa AbstractVector{Int}
    @test !(vf isa Vector{Int})

    # Repeated dynamic dispatch over the wrappers (warm cache path).
    s = 0.0
    for _ in 1:50
        s += norm(vf) + norm(vi)
    end
    @test s ≈ 50 * (norm(vf) + norm(vi))

    @test eltype(vf) == Float64
    @test eltype(vc) == Complex{Float64}
    @test ndims(m) == 2
    @test ndims(vf) == 1
end

true
