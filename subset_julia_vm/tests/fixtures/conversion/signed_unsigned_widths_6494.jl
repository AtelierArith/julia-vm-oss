using Test

@testset "signed/unsigned integer widths (Issue #6494)" begin
    @test typeof(signed(Int8(-1))) === Int8
    @test typeof(signed(Int16(-1))) === Int16
    @test typeof(signed(Int32(-1))) === Int32
    @test typeof(signed(Int64(-1))) === Int64
    @test typeof(signed(Int128(-1))) === Int128

    @test signed(typemax(UInt8)) === Int8(-1)
    @test signed(typemax(UInt16)) === Int16(-1)
    @test signed(typemax(UInt32)) === Int32(-1)
    @test signed(typemax(UInt64)) === Int64(-1)
    @test signed(typemax(UInt128)) === Int128(-1)
    @test signed(true) === Int64(1)

    @test unsigned(Int8(-1)) === typemax(UInt8)
    @test unsigned(Int16(-1)) === typemax(UInt16)
    @test unsigned(Int32(-1)) === typemax(UInt32)
    @test unsigned(Int64(-1)) === typemax(UInt64)
    @test unsigned(Int128(-1)) === typemax(UInt128)

    @test typeof(unsigned(UInt8(1))) === UInt8
    @test typeof(unsigned(UInt16(1))) === UInt16
    @test typeof(unsigned(UInt32(1))) === UInt32
    @test typeof(unsigned(UInt64(1))) === UInt64
    @test typeof(unsigned(UInt128(1))) === UInt128
    @test unsigned(true) === UInt64(1)
end

true
