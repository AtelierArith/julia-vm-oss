# convert(T, x) range-checks narrow integer / float->int targets and throws
# InexactError on out-of-range values, matching the type constructor and
# upstream Julia (Issue #5192).
#
# Before the fix, convert(Int8, 300) silently bit-wrapped to 44 while the
# Int8(300) constructor correctly threw InexactError.

using Test

@testset "convert narrowing out-of-range throws InexactError" begin
    # Signed narrow targets
    @test_throws InexactError convert(Int8, 300)
    @test_throws InexactError convert(Int8, 128)
    @test_throws InexactError convert(Int8, -129)
    @test_throws InexactError convert(Int16, 40000)
    @test_throws InexactError convert(Int16, -40000)
    @test_throws InexactError convert(Int32, 3_000_000_000)

    # Unsigned narrow targets
    @test_throws InexactError convert(UInt8, 300)
    @test_throws InexactError convert(UInt8, -1)
    @test_throws InexactError convert(UInt16, 70000)
    @test_throws InexactError convert(UInt32, -1)

    # Cross-width signed/unsigned mismatch
    @test_throws InexactError convert(UInt8, Int64(256))
    @test_throws InexactError convert(Int8, UInt8(200))
    @test_throws InexactError convert(Int64, typemax(UInt64))
end

@testset "convert float->int non-integral / out-of-range throws InexactError" begin
    @test_throws InexactError convert(Int8, 3.5)
    @test_throws InexactError convert(Int8, 300.0)
    @test_throws InexactError convert(UInt8, -1.0)
    @test_throws InexactError convert(Int64, 1.0e20)
end

@testset "convert in-range values still succeed (no regression)" begin
    # Boundary values that fit exactly
    @test convert(Int8, 127) === Int8(127)
    @test convert(Int8, -128) === Int8(-128)
    @test convert(UInt8, 255) === UInt8(255)
    @test convert(UInt8, 0) === UInt8(0)
    @test convert(Int16, 32767) === Int16(32767)
    @test convert(Int16, -32768) === Int16(-32768)
    @test convert(UInt16, 65535) === UInt16(65535)

    # Widening / same-type / Bool always valid
    @test convert(Int16, Int8(42)) === Int16(42)
    @test convert(Int64, Int8(-5)) === Int64(-5)
    @test convert(Int128, Int32(100)) === Int128(100)
    @test convert(Int8, Int8(5)) === Int8(5)
    @test convert(Int64, true) === 1
    @test convert(UInt8, true) === UInt8(1)

    # Integral floats in range
    @test convert(Int8, 1.0) === Int8(1)
    @test convert(UInt16, 2.0) === UInt16(2)
    @test convert(Int8, -1.0) === Int8(-1)

    # Float targets unaffected
    @test convert(Float64, 3) === 3.0
    @test convert(Float32, 3) === Float32(3)
end

@testset "convert matches Int8 constructor behavior" begin
    # Both convert and the constructor throw for the same out-of-range value
    @test_throws InexactError convert(Int8, 300)
    @test_throws InexactError Int8(300)
    # Both succeed for the same in-range value
    @test convert(Int8, 100) === Int8(100)
    @test Int8(100) === Int8(100)
end

true
