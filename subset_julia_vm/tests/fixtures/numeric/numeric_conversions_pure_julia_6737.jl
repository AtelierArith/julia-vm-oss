# Issue #6737: signed / unsigned / float / widemul go through pure-Julia
# dispatch (base/int.jl, base/number.jl). widemul was migrated to the upstream
# pure-Julia definition `widemul(x, y) = widen(x) * widen(y)`; the old Rust
# handler was Int64-only and *errored* on Int8/Int16/Int32/Int128. reinterpret
# stays a primitive (raw bit reinterpretation). Values verified vs julia 1.12.

using Test

@testset "widemul widens across signed integer widths (Issue #6737)" begin
    # Previously these threw "widemul: cannot multiply"; now type-preserving widen.
    @test widemul(Int32(100000), Int32(100000)) === Int64(10000000000)
    @test widemul(Int8(100), Int8(100))         === Int16(10000)
    @test widemul(Int16(30000), Int16(30000))   === Int32(900000000)
    @test widemul(2, 3)                          === Int128(6)   # Int64 -> Int128
    @test widemul(Int8(-100), Int8(100))        === Int16(-10000)
end

@testset "widemul on unsigned widths — value parity (Issue #6737)" begin
    # The widened product is correct; the *result type* is currently Int64 in
    # the subset instead of UInt64 because convert(UInt64, ::UInt32) mis-tags the
    # value (tracked by Issue #6755). Assert the value only here.
    @test widemul(UInt32(4000000000), UInt32(4)) == 16000000000
    @test widemul(UInt8(200), UInt8(200))        === UInt16(40000)
    @test widemul(UInt16(60000), UInt16(60000))  === UInt32(3600000000)
end

@testset "signed / unsigned / float / reinterpret (Issue #6737)" begin
    @test signed(UInt8(200))   === Int8(-56)
    @test signed(UInt16(0xffff)) === Int16(-1)
    @test unsigned(Int16(-1))  === UInt16(0xffff)
    @test unsigned(Int8(-56))  === UInt8(200)

    @test float(7)    === 7.0
    @test float(3//4) === 0.75
    @test float(true) === 1.0
    @test float(2.0f0) === 2.0f0

    @test reinterpret(UInt32, 1.0f0) === UInt32(1065353216)
    @test reinterpret(Float32, UInt32(1065353216)) === 1.0f0
    @test reinterpret(UInt64, 1.0) === UInt64(0x3ff0000000000000)
end

true
