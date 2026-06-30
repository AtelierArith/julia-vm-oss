using Test

@testset "prod scalar preserves upstream reduction result types (#4019, #4615)" begin
    signed = prod(Int8[2, 3])
    @test typeof(signed) == Int64
    @test signed == 6

    unsigned = prod(UInt8[2, 3])
    @test typeof(unsigned) == UInt64
    @test unsigned == UInt64(6)

    floats = prod(Float32[2, 3])
    @test typeof(floats) == Float32
    @test floats == Float32(6)

    bools = prod(Bool[true, false])
    @test typeof(bools) == Bool
    @test bools == false

    words = prod(String["a", "b"])
    @test typeof(words) == String
    @test words == "ab"

    empty_unsigned = prod(UInt8[])
    @test typeof(empty_unsigned) == UInt64
    @test empty_unsigned == UInt64(1)

    empty_float = prod(Float32[])
    @test typeof(empty_float) == Float32
    @test empty_float == Float32(1)

    empty_bool = prod(Bool[])
    @test typeof(empty_bool) == Bool
    @test empty_bool == true

    empty_words = prod(String[])
    @test typeof(empty_words) == String
    @test empty_words == ""
end

true
