using Test

# Issue #6851: an annotation-free array literal whose elements are imaginary
# arithmetic expressions (`[1.0 + 2.0im, 3.0 + 4.0im]`) inferred `Vector{Any}`
# instead of `Vector{ComplexF64}`. The constructor form (`[Complex(1.0, 2.0),
# ...]`) and the typed-literal form (`ComplexF64[...]`) already worked.
#
# Root cause: the ValueType inference of `1.0 + 2.0im` (i.e. `1.0 + 2.0 * im`)
# dispatched the `*`/`+` Base operators, whose declared return type is `Any`, so
# the Complex element type was lost. The JuliaType inference path already applies
# Julia's `Real op Complex{T} -> Complex{promote(...)}` promotion as a fallback;
# the fix mirrors that recovery in the ValueType path so the literal element type
# folds to `ComplexF64`/`ComplexF32`.

@testset "imaginary-arithmetic array literal eltype (Issue #6851)" begin
    v = [1.0 + 2.0im, 3.0 + 4.0im]
    @test typeof(v) == Vector{ComplexF64}
    @test eltype(v) == ComplexF64
    @test v[1] == 1.0 + 2.0im
    @test v[2] == 3.0 + 4.0im
    @test sum(v) == 4.0 + 6.0im

    # ComplexF32 variant
    w = [1.0f0 + 2.0f0im, 3.0f0 + 4.0f0im]
    @test typeof(w) == Vector{ComplexF32}
    @test eltype(w) == ComplexF32
    @test w[1] == 1.0f0 + 2.0f0im

    # The constructor form and the typed-literal form still agree.
    @test typeof([Complex(1.0, 2.0), Complex(3.0, 4.0)]) == Vector{ComplexF64}
    @test typeof(ComplexF64[1.0 + 2.0im, 3.0 + 4.0im]) == Vector{ComplexF64}
    @test [1.0 + 2.0im, 3.0 + 4.0im] == ComplexF64[1.0 + 2.0im, 3.0 + 4.0im]
end

true
