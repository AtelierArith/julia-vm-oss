# gcd/lcm for Int128 and the Unsigned integer family (Issue #8812)
#
# Upstream defines gcd/lcm generically over `T<:Integer`; sjulia's concrete
# Int64/BigInt methods only covered part of the family. The generic same-type
# gcd landed with Issue #9315; this fixture pins the generic same-type lcm
# (checked_abs∘checked_mul, upstream julia/base/intfuncs.jl) and the full
# width/signedness matrix for both functions.

using Test

@testset "gcd wide integer types" begin
    @test gcd(Int128(12), Int128(8)) == Int128(4)
    @test gcd(Int128(12), Int128(8)) isa Int128
    @test gcd(UInt8(12), UInt8(8)) === UInt8(4)
    @test gcd(UInt16(12), UInt16(8)) === UInt16(4)
    @test gcd(UInt32(12), UInt32(8)) === UInt32(4)
    @test gcd(UInt64(12), UInt64(8)) === UInt64(4)
    @test gcd(UInt128(12), UInt128(8)) == UInt128(4)
    @test gcd(UInt128(12), UInt128(8)) isa UInt128
    @test gcd(Int128(0), Int128(0)) == Int128(0)
    @test gcd(UInt32(0), UInt32(6)) === UInt32(6)
end

@testset "lcm wide integer types" begin
    @test lcm(Int128(4), Int128(6)) == Int128(12)
    @test lcm(Int128(4), Int128(6)) isa Int128
    @test lcm(UInt8(4), UInt8(6)) === UInt8(12)
    @test lcm(UInt16(4), UInt16(6)) === UInt16(12)
    @test lcm(UInt32(4), UInt32(6)) === UInt32(12)
    @test lcm(UInt64(4), UInt64(6)) === UInt64(12)
    @test lcm(UInt128(4), UInt128(6)) == UInt128(12)
    @test lcm(UInt128(4), UInt128(6)) isa UInt128
    @test lcm(Int8(4), Int8(6)) === Int8(12)
    @test lcm(Int16(4), Int16(6)) === Int16(12)
    @test lcm(Int32(4), Int32(6)) === Int32(12)
    # zero handling: lcm(0, 0) == 0, lcm(0, x) == 0
    @test lcm(UInt32(0), UInt32(0)) === UInt32(0)
    @test lcm(Int128(0), Int128(5)) == Int128(0)
    # negative operands: result is non-negative
    @test lcm(Int32(-4), Int32(6)) === Int32(12)
    # overflow is checked, not wrapped (upstream OverflowError)
    @test_throws OverflowError lcm(typemax(UInt8), UInt8(2))
    @test_throws OverflowError Base.checked_abs(typemin(Int32))
end

@testset "gcd/lcm mixed signedness and width" begin
    @test gcd(UInt32(12), 8) == 4
    @test gcd(12, UInt32(8)) == 4
    @test lcm(UInt32(4), 6) == 12
    @test gcd(UInt64(12), Int64(-8)) === UInt64(4)
    @test lcm(UInt64(4), Int64(-6)) === UInt64(12)
    @test gcd(Int32(12), Int64(8)) === Int64(4)
    @test lcm(Int32(4), Int64(6)) === Int64(12)
    @test gcd(Int8(12), Int128(8)) == Int128(4)
    @test gcd(Int8(12), Int128(8)) isa Int128
end

@testset "checked_abs / checked_neg" begin
    @test Base.checked_abs(Int32(-5)) === Int32(5)
    @test Base.checked_abs(UInt16(5)) === UInt16(5)
    @test Base.checked_abs(true) === true
    @test Base.checked_neg(Int64(5)) === Int64(-5)
end

true
