# Invalid UTF-8 String iteration parity (Issue #8995, reopened scope)
#
# StrBytes carriers must iterate to the exact malformed Chars with upstream
# byte-offset states, in linear time. Valid strings share the same byte-offset
# state model (upstream julia/base/strings/string.jl iterate).

using Test

@testset "valid string byte-offset iterate states" begin
    @test iterate("ab") == ('a', 2)
    @test iterate("ab", 2) == ('b', 3)
    @test iterate("ab", 3) === nothing
    @test iterate("あb") == ('あ', 4)
    @test iterate("あb", 4) == ('b', 5)
    @test iterate("あb", 5) === nothing
end

@testset "invalid byte iterate yields exact malformed Chars" begin
    s = String(UInt8[0xff, 0x61])
    c1, i1 = iterate(s)
    @test i1 == 2
    @test c1 == '\xff'
    @test !isvalid(c1)
    c2, i2 = iterate(s, i1)
    @test c2 == 'a'
    @test i2 == 3
    @test iterate(s, 3) === nothing
    @test collect(s) == ['\xff', 'a']
    @test length(s) == 2

    # truncated multibyte sequence: one malformed char consuming both bytes
    t = String(UInt8[0xe3, 0x81])
    ct, it = iterate(t)
    @test ct == '\xe3\x81'
    @test it == 3
    @test length(t) == 1

    # overlong encoding: Julia consumes the continuation byte
    o = String(UInt8[0xc0, 0x80, 0x61])
    co, io = iterate(o)
    @test co == '\xc0\x80'
    @test io == 3
    @test length(o) == 2
end

@testset "malformed char splat and indexing" begin
    f(xs...) = xs
    s = String(UInt8[0xff, 0x61])
    @test f(s...) == ('\xff', 'a')
    @test s[1] == '\xff'
    @test s[2] == 'a'
    @test repr(s[1]) == "'\\xff'"
    @test repr(s) == "\"\\xffa\""
end

@testset "concat preserves raw bytes" begin
    s = String(UInt8[0xff, 0x61])
    u = s * "b"
    @test ncodeunits(u) == 3
    @test codeunit(u, 1) == 0xff
    # 0x62 == UInt8('b'); the UInt8(::Char) constructor itself is Issue #11406
    @test codeunit(u, 3) == 0x62
    v = "b" * s
    @test ncodeunits(v) == 3
    @test codeunit(v, 1) == 0x62
    @test string(s, "b") == u
end

true
