# Test mixed-type integer bitwise operators & / | / ⊻ (xor)
# Issue #8221: a bitwise op on two different integer types (e.g. 0x05 & 5,
#   UInt8 & Int64) had no exact same-type method and errored
#   `MethodError: no method matching &(::UInt8, ::Int64)`. Upstream promotes the
#   operands to a common type (`&(x::Integer, y::Integer) = &(promote(x,y)...)`).

using Test

@testset "mixed-type bitwise operators (Issue #8221)" begin
    @testset "& promotes mixed integer types" begin
        @test (0x05 & 5) === 5
        @test (UInt16(3) & 5) === 1
        @test (Int8(6) & Int16(3)) === Int16(2)
        @test (true & 5) === 1
        @test typeof(0x05 & 5) === Int64
    end

    @testset "| promotes mixed integer types" begin
        @test (0x05 | 5) === 5
        @test (true | 5) === 5
        @test (UInt8(8) | 1) === 9
        @test typeof(0x05 | 5) === Int64
    end

    @testset "⊻ / xor promote mixed integer types" begin
        @test (0x06 ⊻ 3) === 5
        @test xor(0x06, 3) === 5
        @test (true ⊻ 5) === 4
        @test typeof(0x06 ⊻ 3) === Int64
    end

    @testset "same-type calls keep their concrete type (exact methods win)" begin
        @test (0x0f & 0x05) === 0x05
        @test typeof(0x0f & 0x05) === UInt8
        @test (5 & 3) === 1
        @test (0x05 ⊻ 0x03) === 0x06
        @test (true & false) === false
        @test typeof(true & false) === Bool
    end

    @testset "works through a generic function (untyped args)" begin
        band(a, b) = a & b
        @test band(0x05, 5) === 5
        @test band(0x0f, 0x05) === 0x05
        @test band(true, false) === false
    end
end

true
