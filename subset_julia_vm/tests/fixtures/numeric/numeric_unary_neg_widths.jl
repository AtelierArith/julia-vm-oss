using Test

# Issue #3705: unary `-` on Int8/Int16/Int32/Int128 and on every UIntN
# raised the compile error `Unsupported unary op: Neg`. The compile-time
# unary dispatch in `compile/expr/unary.rs` only knew I64/F64/F32/F16/Bool/Any/Struct.
# Now those arms route through `Intrinsic::NegAny`, which has been extended
# to handle every primitive integer width with two's-complement wrapping
# semantics on unsigned types (matching Julia).
@testset "Unary - on all primitive integer widths (Issue #3705)" begin
    # Signed narrow types
    @test typeof(-Int8(5)) == Int8
    @test typeof(-Int16(5)) == Int16
    @test typeof(-Int32(5)) == Int32
    @test typeof(-Int64(5)) == Int64
    @test typeof(-Int128(5)) == Int128

    @test -Int8(5) == Int8(-5)
    @test -Int16(5) == Int16(-5)
    @test -Int32(5) == Int32(-5)
    @test -Int64(5) == -5
    @test -Int128(5) == Int128(-5)

    # Unsigned types — two's-complement wrap (Julia: -UInt8(5) == 0xfb)
    @test typeof(-UInt8(5)) == UInt8
    @test typeof(-UInt16(5)) == UInt16
    @test typeof(-UInt32(5)) == UInt32
    @test typeof(-UInt64(5)) == UInt64
    @test typeof(-UInt128(5)) == UInt128

    @test -UInt8(5)   == UInt8(251)
    @test -UInt16(5)  == UInt16(65531)
    @test -UInt32(5)  == UInt32(4294967291)
    @test -UInt64(5)  == UInt64(0xfffffffffffffffb)
    @test -UInt128(5) == UInt128(0xfffffffffffffffffffffffffffffffb)

    # Edge cases
    @test -Int128(0) == Int128(0)
    @test -UInt8(0)  == UInt8(0)
    @test -typemax(Int8) == Int8(-127)
    @test -typemin(Int8) == typemin(Int8)  # wraps in two's complement

    # Bool widens to Int64 (Julia semantics: -true == -1)
    @test -true == -1
    @test typeof(-true) == Int64
    @test -false == 0

    # BigInt stays BigInt
    @test typeof(-big(5)) == BigInt
    @test -big(5) == big(-5)
end

true
