# Test that public `zero` and `sqrt` reach Pure Julia methods (Issue #3737).
#
# After the migration, lowering no longer maps `zero` / `sqrt` directly to
# `BuiltinOp::Zero` / `BuiltinOp::Sqrt`. Direct calls go through method
# dispatch first; the BuiltinOp fallback only fires for primitive numeric
# types when no Pure Julia method matches (e.g. `sqrt(::Float64)`).

using Test

@testset "zero / sqrt Pure Julia dispatch (Issue #3737)" begin
    # zero of BigInt — this used to be silently shadowed by Instr::Zero,
    # which returned Float64(0.0) for BigInt input. With Pure Julia
    # dispatch we now reach the `zero(x::BigInt)` method in base/gmp.jl.
    z_big = zero(BigInt(42))
    @test (Int64(z_big)) == 0

    # zero of a Complex value — Pure Julia method in base/complex.jl
    zc = zero(Complex(1.0, 2.0))
    @test (real(zc)) == 0.0
    @test (imag(zc)) == 0.0

    # sqrt of a Complex{Float64} — Pure Julia method in base/complex.jl.
    # `sqrt(3.0 + 4.0im) == 2.0 + 1.0im` (matches official Julia).
    s = sqrt(Complex(3.0, 4.0))
    @test (real(s)) == 2.0
    @test (imag(s)) == 1.0

    # Real-number forms still work via the BuiltinOp fallback.
    @test (sqrt(4.0)) == 2.0
    @test (sqrt(16)) == 4.0
    @test (zero(3.14)) == 0.0
    @test (zero(Int64)) == 0
    @test (zero(0)) == 0
end

true
