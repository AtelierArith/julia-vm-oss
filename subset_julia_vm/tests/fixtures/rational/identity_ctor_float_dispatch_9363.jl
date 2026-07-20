# Issue #9363: the single-argument identity constructor `Rational(x::Rational)`
# (upstream base/rational.jl: `Rational(x::Rational) = x`) was missing, so a
# Rational argument was loose-matched to the concrete `Rational(num::Int64)`
# constructor and raised an InternalError (LoadSlotI64 on a StructRef).
#
# Issue #9362: `float(0x06 // 0x04)` (and Bool/unsigned Rational element types)
# mis-dispatched: the abstract `//(::Integer, ::Integer)` fallback's
# interprocedural return type inferred to `Union{}` (Bottom), and the static
# dispatcher treated `Union{} <: T` as matching every candidate — flakily
# picking `float(::Complex)` (InternalError) or raising a spurious ambiguity.
# Bottom-typed arguments now defer to runtime dispatch on the actual value.
# All expected values verified against upstream julia 1.12.6.

using Test

@testset "Rational identity constructor (Issue #9363)" begin
    @test Rational(3 // 4) === 3 // 4
    @test typeof(Rational(3 // 4)) === Rational{Int64}
    @test Rational(Int8(3) // Int8(4)) === Int8(3) // Int8(4)
    @test typeof(Rational(Int8(3) // Int8(4))) === Rational{Int8}
    @test typeof(Rational(0x06 // 0x04)) === Rational{UInt8}
    @test Rational(true // true) == 1
    @test Rational(big(3) // big(4)) == big(3) // big(4)
end

@testset "float of unsigned/Bool Rational element types (Issue #9362)" begin
    @test float(0x06 // 0x04) == 1.5
    @test float(true // true) == 1.0
    @test float(0x03 // 0x09) == 1 / 3
    # signed contrast cases (previously working) stay correct
    @test float(Int8(3) // Int8(4)) == 0.75
    @test float(3 // 4) == 0.75

    # direct dispatch on a Bottom-inferred `//` expression must reach the
    # Rational method, not an arbitrary candidate (Issue #9362)
    f9362(r::Rational) = :rational
    f9362(x::Int64) = :int
    f9362(z::Complex) = :complex
    @test f9362(0x06 // 0x04) === :rational
    @test f9362(true // true) === :rational
end

true
