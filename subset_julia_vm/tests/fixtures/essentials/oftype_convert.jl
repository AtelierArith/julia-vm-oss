# Test oftype(x, y) = convert(typeof(x), y) matches upstream (Issue #5109)
#
# Note: out-of-range narrowing (e.g. oftype(Int8(1), 300)) should throw
# InexactError upstream, but sjulia's convert wraps instead of range-checking
# (separate pre-existing bug, Issue #5192). Those error cases are intentionally
# excluded here and will be added once #5192 is fixed.

using Test

@testset "oftype - convert to type of reference value (Issue #5109)" begin
    # float reference, integer value -> Float64
    @test oftype(1.0, 2) === 2.0
    @test typeof(oftype(1.0, 2)) === Float64

    # integer reference, float value -> Int
    @test oftype(1, 2.0) === 2
    @test typeof(oftype(1, 2.0)) === Int

    # narrow integer reference -> Int8
    @test oftype(Int8(1), 5) === Int8(5)
    @test typeof(oftype(Int8(1), 5)) === Int8

    # same type short-circuit: returns the value itself unchanged
    @test oftype(3.0, 4.0) === 4.0
    @test oftype(Int8(1), Int8(7)) === Int8(7)

    # other narrow types (in-range)
    @test oftype(UInt16(0), 5) === UInt16(5)
    @test oftype(0x1, 200) === UInt8(200)
end

true
