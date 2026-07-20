using Test

function apply_constructor(T, x)
    return T(x)
end

@testset "fixed-width constructors are pure Julia wrappers" begin
    @test typeof(Int8(1)) === Int8
    @test typeof(Int16(1)) === Int16
    @test typeof(Int32(1)) === Int32
    @test typeof(Int128(1)) === Int128
    @test typeof(UInt8(1)) === UInt8
    @test typeof(UInt16(1)) === UInt16
    @test typeof(UInt32(1)) === UInt32
    @test typeof(UInt64(1)) === UInt64
    @test typeof(UInt128(1)) === UInt128
    @test typeof(Float16(1)) === Float16
    @test typeof(Float32(1)) === Float32

    @test Int8(-3) === Int8(-3)
    @test Int16(Int8(-3)) === Int16(-3)
    @test Int32(3.0) === Int32(3)
    @test Int128(4) === Int128(4)
    @test UInt8(0x05) === UInt8(5)
    @test UInt16(UInt8(6)) === UInt16(6)
    @test UInt32(7) === UInt32(7)
    @test UInt64(UInt32(8)) === UInt64(8)
    @test UInt128(9) === UInt128(9)
    @test Float16(3) == Float16(3.0)
    @test Float32(3) === Float32(3.0)

    @test_throws InexactError Int8(128)
    @test_throws InexactError UInt8(-1)
end

@testset "fixed-width constructors remain callable values" begin
    @test apply_constructor(Int8, 10) === Int8(10)
    @test apply_constructor(UInt16, 11) === UInt16(11)
    @test apply_constructor(Float32, 1.25) === Float32(1.25)
end

@testset "not-egal public operator survives builtin alias removal" begin
    @test (Int8(1) !== Int8(2)) === true
    @test (Int8(1) !== Int8(1)) === false
end

true
