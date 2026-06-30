using Test

# Issue #3702: typemax/typemin for narrow integer types previously returned
# Int64 because the Pure Julia method bodies used bare integer literals
# (e.g. `return 255` instead of `return UInt8(255)`). And typemax/typemin
# for Int128/UInt128 raised NoMethodFound entirely.
@testset "typemax/typemin return correct type (Issue #3702)" begin
    # Signed narrow types
    @test typeof(typemax(Int8)) == Int8
    @test typeof(typemax(Int16)) == Int16
    @test typeof(typemax(Int32)) == Int32
    @test typeof(typemax(Int64)) == Int64
    @test typeof(typemax(Int128)) == Int128

    @test typeof(typemin(Int8)) == Int8
    @test typeof(typemin(Int16)) == Int16
    @test typeof(typemin(Int32)) == Int32
    @test typeof(typemin(Int64)) == Int64
    @test typeof(typemin(Int128)) == Int128

    # Unsigned narrow types
    @test typeof(typemax(UInt8)) == UInt8
    @test typeof(typemax(UInt16)) == UInt16
    @test typeof(typemax(UInt32)) == UInt32
    @test typeof(typemax(UInt64)) == UInt64
    @test typeof(typemax(UInt128)) == UInt128

    @test typeof(typemin(UInt8)) == UInt8
    @test typeof(typemin(UInt16)) == UInt16
    @test typeof(typemin(UInt32)) == UInt32
    @test typeof(typemin(UInt64)) == UInt64
    @test typeof(typemin(UInt128)) == UInt128

    # Numerical correctness — values must match the official ranges
    @test typemax(Int8)   == Int8(127)
    @test typemax(Int16)  == Int16(32767)
    @test typemax(Int32)  == Int32(2147483647)
    @test typemax(Int64)  == 9223372036854775807
    @test typemax(Int128) == Int128(170141183460469231731687303715884105727)

    @test typemin(Int8)   == Int8(-128)
    @test typemin(Int16)  == Int16(-32768)
    @test typemin(Int32)  == Int32(-2147483648)
    @test typemin(Int128) == Int128(0) - Int128(170141183460469231731687303715884105727) - Int128(1)

    @test typemax(UInt8)   == UInt8(255)
    @test typemax(UInt16)  == UInt16(65535)
    @test typemax(UInt32)  == UInt32(4294967295)
    @test typemax(UInt64)  == UInt64(0xffffffffffffffff)
    @test typemax(UInt128) == UInt128(0xffffffffffffffffffffffffffffffff)

    # Downstream: inline `typemax(UIntN) ÷ UIntN(...)` previously widened
    # to Float64 because typemax returned Int64. Pin the chain.
    @test typeof(typemax(UInt8) ÷ UInt8(3)) == UInt8
    @test typemax(UInt8) ÷ UInt8(3) == UInt8(85)
end

true
