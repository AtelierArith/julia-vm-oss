using Test

# Issue #4757 (prevention for fix family #4725 / #4727 / #4729 /
# #4735 / #4755): Rust `_ => format!("{:?}")` catch-alls in `Value`
# display paths leak the variant-name-with-paren form (e.g.
# `"I128(...)"`, `"StructRef(...)"`, `"Str(\"a\")"`) into
# user-visible output. This matrix asserts that the common
# user-visible display entry points (`string`, `repr`,
# interpolation) never produce those tokens for the value classes
# most prone to slipping through a catch-all.
#
# Runtime-agnostic: the bad tokens don't occur in upstream Julia's
# own display form either, so the fixture passes under both sjulia
# and julia.

function no_debug_leak(s)
    bad_tokens = ("I8(", "I16(", "I32(", "I128(",
                  "U8(", "U16(", "U32(", "U64(", "U128(",
                  "F16(", "F32(",
                  "Str(", "Char(", "Symbol(",
                  "StructRef(")
    for tok in bad_tokens
        if occursin(tok, s)
            return false
        end
    end
    return true
end

@testset "no Rust Debug leak — scalar primitives (Issue #4757)" begin
    @test no_debug_leak(string(Int8(1)))
    @test no_debug_leak(repr(Int8(1)))
    @test no_debug_leak("interp: $(Int8(1))")

    @test no_debug_leak(string(Int128(1) << 60))
    @test no_debug_leak(repr(Int128(1) << 60))
    @test no_debug_leak("interp: $(Int128(1) << 60)")

    @test no_debug_leak(string(UInt8(0xff)))
    @test no_debug_leak(repr(UInt8(0xff)))
    @test no_debug_leak("interp: $(UInt8(0xff))")

    @test no_debug_leak(string(Float32(1.5)))
    @test no_debug_leak(repr(Float32(1.5)))
    @test no_debug_leak("interp: $(Float32(1.5))")

    @test no_debug_leak(string(:foo))
    @test no_debug_leak(repr(:foo))
    @test no_debug_leak("interp: $(:foo)")

    @test no_debug_leak(string('a'))
    @test no_debug_leak(repr('a'))
    @test no_debug_leak("interp: $('a')")

    @test no_debug_leak(string(nothing))
    @test no_debug_leak(repr(nothing))
    @test no_debug_leak("interp: $(nothing)")

    @test no_debug_leak(string(missing))
    @test no_debug_leak(repr(missing))
    @test no_debug_leak("interp: $(missing)")
end

@testset "no Rust Debug leak — common containers (Issue #4757)" begin
    p = Pair(1, 2)
    @test no_debug_leak(string(p))
    @test no_debug_leak(repr(p))
    @test no_debug_leak("interp: $p")

    d = Dict("a" => 1)
    @test no_debug_leak(string(d))
    @test no_debug_leak(repr(d))
    @test no_debug_leak("interp: $d")

    s = Set([1])
    @test no_debug_leak(string(s))
    @test no_debug_leak(repr(s))
    @test no_debug_leak("interp: $s")

    v = [1, 2, 3]
    @test no_debug_leak(string(v))
    @test no_debug_leak(repr(v))
    @test no_debug_leak("interp: $v")

    t = (1, "two")
    @test no_debug_leak(string(t))
    @test no_debug_leak(repr(t))
    @test no_debug_leak("interp: $t")
end

true
