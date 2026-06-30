using Test

# Issue #6771: a typed array literal with a `ComplexF64` (= Complex{Float64})
# element type and *integer* elements stored the raw Int verbatim instead of
# converting each element to the declared element type. `Float32[1,2]` already
# coerces Int → Float32 at the storage layer, but the Complex element type is
# kept in boxed (`Any`) storage whose store does no coercion, so the literal
# construction path never invoked `convert(ComplexF64, x)`.
#
# The `==`-based comparison `ComplexF64[1,2] == [1.0+0.0im, ...]` hides the bug
# via numeric promotion, so this fixture asserts on the EXACT element type via
# `===`, `typeof`, and `eltype`.

@testset "ComplexF64 typed array literal converts Int elements (Issue #6771)" begin
    a = ComplexF64[1, 2]
    @test eltype(a) == ComplexF64
    @test typeof(a) == Vector{ComplexF64}
    # Each element must be the *converted* ComplexF64 value, not a raw Int.
    @test a[1] === 1.0 + 0.0im
    @test a[2] === 2.0 + 0.0im
    @test typeof(a[1]) === ComplexF64
    @test typeof(a[2]) === ComplexF64
    @test all(x -> x isa ComplexF64, a)

    # ComplexF32 literal with Int elements likewise converts to ComplexF32.
    af32 = ComplexF32[1, 2]
    @test eltype(af32) == ComplexF32
    @test af32[1] === 1.0f0 + 0.0f0im
    @test typeof(af32[1]) === ComplexF32

    # Parametric spelling Complex{Float64}[...] behaves identically.
    p = Complex{Float64}[1, 2]
    @test eltype(p) == ComplexF64
    @test p[1] === 1.0 + 0.0im
    @test typeof(p[1]) === ComplexF64

    # Regression guards: previously-working forms still convert to the eltype.
    b = ComplexF64[1.0, 2.0]
    @test b[1] === 1.0 + 0.0im
    @test typeof(b[1]) === ComplexF64
    @test b == [1.0 + 0.0im, 2.0 + 0.0im]

    c = ComplexF64[1 + 2im, 3 + 4im]
    @test c[1] === 1.0 + 2.0im
    @test c[2] === 3.0 + 4.0im
    @test typeof(c[1]) === ComplexF64
    @test c == [1.0 + 2.0im, 3.0 + 4.0im]

    # Float32 typed literal (the reference path that already worked) is unregressed.
    f = Float32[1, 2]
    @test f[1] === 1.0f0
    @test eltype(f) == Float32
end

true
