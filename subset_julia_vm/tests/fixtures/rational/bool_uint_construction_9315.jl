# Issue #9315: `//` / Rational construction for Bool, Unsigned and Int128
# element types used to hit the promote-fallback recursion trap (Issue #5966):
# `promote(true, true)` / `promote(0x01, 0x03)` returned the unchanged same-type
# pair, so `Rational(::Integer, ::Integer)` re-dispatched on itself until
# MAX_CALL_DEPTH. Upstream constructs Rational{Bool}, Rational{UInt8}, ... via a
# same-type outer constructor + gcd reduction. Values below were verified
# against upstream julia 1.12.

using Test

@testset "Rational construction for Bool / Unsigned / Int128 (Issue #9315)" begin
    # Bool: no recursion, element type preserved, no negation.
    @test true // true === Rational{Bool}(true, true)
    @test typeof(true // true) === Rational{Bool}
    @test (true // false).num === true
    @test (true // false).den === false      # den == 0 sentinel kept raw
    @test (false // true).num === false
    @test (false // true).den === true

    # UInt8: reduces via gcd, element type preserved.
    r8 = 0x06 // 0x04
    @test typeof(r8) === Rational{UInt8}
    @test r8.num === 0x03
    @test r8.den === 0x02
    @test (0x01 // 0x03) === Rational{UInt8}(0x01, 0x03)

    # Wider unsigned types.
    @test typeof(UInt16(1) // UInt16(3)) === Rational{UInt16}
    @test typeof(UInt32(1) // UInt32(3)) === Rational{UInt32}
    r64 = UInt64(12) // UInt64(8)
    @test typeof(r64) === Rational{UInt64}
    @test r64.num === UInt64(3)
    @test r64.den === UInt64(2)
    @test typeof(UInt128(1) // UInt128(3)) === Rational{UInt128}

    # Large unsigned value beyond Int64 range: generic gcd stays in-type.
    big_u = UInt64(18446744073709551614) // UInt64(6)
    @test typeof(big_u) === Rational{UInt64}
    @test big_u.num === UInt64(9223372036854775807)
    @test big_u.den === UInt64(3)

    # Int128: signed, so reduction AND sign normalization apply.
    ri = Int128(-6) // Int128(4)
    @test typeof(ri) === Rational{Int128}
    @test ri.num === Int128(-3)
    @test ri.den === Int128(2)
    @test (Int128(6) // Int128(-4)) === Int128(-6) // Int128(4)

    # gcd itself now works for these element types.
    @test gcd(0x06, 0x04) === 0x02
    @test gcd(UInt64(12), UInt64(8)) === UInt64(4)
    @test gcd(Int128(-6), Int128(4)) === Int128(2)
    @test gcd(true, true) === true
    @test gcd(false, false) === false

    # Explicit typed constructor coerces Int args and reduces for these types.
    @test Rational{UInt8}(6, 4) === 0x03 // 0x02
    @test Rational{Bool}(1, 1) === true // true

    # Mixed unsigned/signed promotes to the signed common type (unchanged path).
    @test typeof(0x01 // 3) === Rational{Int64}
    @test (0x01 // 3) === 1 // 3
end

true
