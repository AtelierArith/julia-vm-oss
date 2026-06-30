# Issue #3550: ranges constructed from typed integers must preserve their
# declared element type. Both `typeof(range)` and the loop variable must
# reflect the operand type (`UInt8`, `Int32`, …) instead of widening to
# `UnitRange{Int64}` / `Int64`.
using Test

@testset "Issue #3550 typed range element types" begin
    r_u8 = UInt8(1):UInt8(3)
    @test typeof(r_u8) === UnitRange{UInt8}
    @test first(r_u8) === UInt8(1)
    @test last(r_u8) === UInt8(3)

    seen_u8 = UInt8[]
    for x in r_u8
        @test typeof(x) === UInt8
        push!(seen_u8, x)
    end
    @test length(seen_u8) == 3
    @test seen_u8[1] === UInt8(1)
    @test seen_u8[2] === UInt8(2)
    @test seen_u8[3] === UInt8(3)

    # Inline range form must also preserve the operand type.
    saw = false
    for x in UInt8(1):UInt8(2)
        @test typeof(x) === UInt8
        saw = true
    end
    @test saw

    # Int32 range
    r_i32 = Int32(1):Int32(3)
    @test typeof(r_i32) === UnitRange{Int32}

    # Plain integer ranges still default to Int64.
    @test typeof(1:3) === UnitRange{Int64}
    @test first(1:3) === 1
end

true
