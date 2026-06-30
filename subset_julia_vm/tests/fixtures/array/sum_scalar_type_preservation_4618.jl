using Test

@testset "sum scalar preserves upstream reduction result types (#4019, #4618)" begin
    bools = sum(Bool[true, false, true])
    @test typeof(bools) == Int64
    @test bools == 2

    empty_bools = sum(Bool[])
    @test typeof(empty_bools) == Int64
    @test empty_bools == 0

    signed = sum(Int8[1, 2, 3])
    @test typeof(signed) == Int64
    @test signed == 6

    empty_signed = sum(Int8[])
    @test typeof(empty_signed) == Int64
    @test empty_signed == 0

    unsigned = sum(UInt8[1, 2, 3])
    @test typeof(unsigned) == UInt64
    @test unsigned == UInt64(6)

    empty_unsigned = sum(UInt8[])
    @test typeof(empty_unsigned) == UInt64
    @test empty_unsigned == UInt64(0)

    floats = sum(Float32[1, 2, 3])
    @test typeof(floats) == Float32
    @test floats == Float32(6)

    empty_floats = sum(Float32[])
    @test typeof(empty_floats) == Float32
    @test empty_floats == Float32(0)
end

true
