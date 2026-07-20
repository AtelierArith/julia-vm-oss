# isdefined field keys match upstream's builtin contract (Issue #10240):
# only Symbol or a machine `Int` index is accepted. Every other Integer
# width (Int8/Int16/Int32/Int128, UInt8..UInt128, Bool, BigInt) and every
# non-integer key (Float64, String) raises
# `TypeError: in isdefined, expected Symbol, got a value of type T` —
# NOT a Bool result and NOT a MethodError. Out-of-range `Int` indices
# return `false` (no error). Verified against julia 1.12.

using Test

struct IsdefKeyProbe10240
    a::Int
    b::Int
end

@testset "isdefined Integer key widths (Issue #10240)" begin
    global isdef_key_global_10240 = 42
    b = GlobalRef(Main, :isdef_key_global_10240).binding
    @test typeof(b) === Core.Binding

    # Machine Int keys: in-range -> field definedness, out-of-range -> false.
    @test isdefined(b, 1)          # :globalref is always set
    @test isdefined(b, 5)          # :flags is always set
    @test !isdefined(b, 0)
    @test !isdefined(b, -1)
    @test !isdefined(b, 6)
    @test !isdefined(b, typemax(Int))

    # Every non-Int Integer width raises TypeError, matching upstream's
    # jl_f_isdefined (which only accepts Symbol or Int).
    @test_throws TypeError isdefined(b, Int8(1))
    @test_throws TypeError isdefined(b, Int16(1))
    @test_throws TypeError isdefined(b, Int32(1))
    @test_throws TypeError isdefined(b, Int128(1))
    @test_throws TypeError isdefined(b, UInt8(1))
    @test_throws TypeError isdefined(b, UInt16(1))
    @test_throws TypeError isdefined(b, UInt32(1))
    @test_throws TypeError isdefined(b, UInt64(1))
    @test_throws TypeError isdefined(b, UInt128(1))
    @test_throws TypeError isdefined(b, true)
    @test_throws TypeError isdefined(b, false)
    @test_throws TypeError isdefined(b, big(1))
    @test_throws TypeError isdefined(b, big(2)^100)   # out-of-range AND wrong type
    @test_throws TypeError isdefined(b, Int8(-1))     # negative AND wrong type

    # Non-integer keys are TypeError too (upstream: not MethodError).
    @test_throws TypeError isdefined(b, 1.0)
    @test_throws TypeError isdefined(b, "globalref")

    # The TypeError payload matches upstream: expected Symbol, got the key.
    err = try
        isdefined(b, UInt8(1))
        nothing
    catch e
        e
    end
    @test err isa TypeError
    @test err.expected === Symbol
    @test err.got === UInt8(1)

    # Same contract on ordinary structs.
    p = IsdefKeyProbe10240(1, 2)
    @test isdefined(p, 1)
    @test isdefined(p, 2)
    @test !isdefined(p, 0)
    @test !isdefined(p, -5)
    @test !isdefined(p, 3)
    @test_throws TypeError isdefined(p, Int8(1))
    @test_throws TypeError isdefined(p, Int32(1))
    @test_throws TypeError isdefined(p, UInt8(1))
    @test_throws TypeError isdefined(p, UInt64(1))
    @test_throws TypeError isdefined(p, true)
    @test_throws TypeError isdefined(p, big(1))
    @test_throws TypeError isdefined(p, 1.0)
    @test_throws TypeError isdefined(p, "a")

    # Modules reject every non-Symbol key with TypeError, including Int.
    @test_throws TypeError isdefined(Main, 1)
    @test_throws TypeError isdefined(Main, UInt8(1))
    @test_throws TypeError isdefined(Main, 1.0)
end

true
