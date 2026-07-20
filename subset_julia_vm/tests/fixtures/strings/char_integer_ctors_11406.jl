# Non-Int64 integer constructors from Char (Issue #11406).
#
# Upstream `julia/base/char.jl`:
#   (::Type{T})(x::AbstractChar) where {T<:Union{Number,AbstractChar}} = T(codepoint(x))
# sjulia only wired Int/Int64('b'); every other fixed-width integer
# constructor (Int8, UInt8, Int16, UInt16, Int32, UInt32, UInt64, Int128,
# UInt128) raised a `convert` MethodError instead of converting via the
# character's Unicode codepoint. Verified row-by-row against upstream Julia
# 1.12.6.

using Test

@testset "char to fixed-width integer constructors" begin
    c = 'b'  # codepoint 98

    @test Int(c) === 98
    @test Int8(c) === Int8(98)
    @test Int16(c) === Int16(98)
    @test Int32(c) === Int32(98)
    @test Int64(c) === Int64(98)
    @test Int128(c) === Int128(98)

    @test UInt8(c) === 0x62
    @test UInt16(c) === 0x0062
    @test UInt32(c) === 0x00000062
    @test UInt64(c) === 0x0000000000000062
    @test UInt128(c) === UInt128(98)
end

@testset "char to integer constructors round-trip via codepoint" begin
    for c in ['a', 'Z', '0', ' ', '~']
        cp = codepoint(c)
        @test Int(c) == Int(cp)
        @test UInt8(c) == UInt8(cp)
        @test Int16(c) == Int16(cp)
        @test UInt32(c) == UInt32(cp)
    end
end

@testset "out-of-range Char to narrow integer raises InexactError" begin
    # 'あ' (U+3042 HIRAGANA LETTER A) has codepoint 12354, which does not fit
    # in an Int8/UInt8 but does fit in the wider integer types.
    wide = 'あ'
    @test_throws InexactError UInt8(wide)
    @test_throws InexactError Int8(wide)
    @test UInt16(wide) == 0x3042
    @test UInt32(wide) == 0x00003042
    @test Int32(wide) == 12354
end

true
