# max/min between a Rational and a wide/unsigned integer promote to the common
# Rational type instead of throwing MethodError (Issue #9526). The promote
# fallback max(x::Real, y::Real) = max(promote(x,y)...) widens to
# Rational{promote_type(T,S)}, which needs the Rational-from-Rational conversion
# Rational{S}(::Rational) for S in {Int128, UInt64, UInt128, ...}.

using Test

@testset "max/min Rational vs wide/unsigned integers (Issue #9526)" begin
    r = 3//4

    # Int128 operand -> Rational{Int128}
    @test max(r, Int128(5)) === Rational{Int128}(5, 1)
    @test min(r, Int128(5)) === Rational{Int128}(3, 4)
    @test max(r, Int128(-2)) === Rational{Int128}(3, 4)
    @test min(r, Int128(-2)) === Rational{Int128}(-2, 1)

    # UInt64 operand (either argument order) -> Rational{UInt64}
    @test max(r, UInt64(5)) === Rational{UInt64}(5, 1)
    @test min(UInt64(5), r) === Rational{UInt64}(3, 4)
    @test typeof(min(UInt64(5), r)) === Rational{UInt64}

    # UInt128 operand -> Rational{UInt128}
    @test max(r, UInt128(5)) === Rational{UInt128}(5, 1)
    @test min(r, UInt128(5)) === Rational{UInt128}(3, 4)
    @test typeof(max(r, UInt128(5))) === Rational{UInt128}

    # Narrow unsigned operands stay Rational{Int64} (promote_type widens to Int64)
    @test max(r, UInt8(5)) === Rational{Int64}(5, 1)
    @test min(r, UInt16(5)) === Rational{Int64}(3, 4)
    @test max(r, UInt32(5)) === Rational{Int64}(5, 1)

    # minmax mirrors min/max
    @test minmax(r, Int128(5)) === (Rational{Int128}(3, 4), Rational{Int128}(5, 1))
end

true
