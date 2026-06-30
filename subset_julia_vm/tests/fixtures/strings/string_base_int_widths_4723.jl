using Test

@testset "string(x; base=N) accepts all signed integer widths (Issue #4723)" begin
    @test string(Int8(-42), base=10) == "-42"
    @test string(Int8(-1), base=16) == "-1"
    @test string(Int16(255), base=16) == "ff"
    @test string(Int32(-1024), base=10) == "-1024"
    @test string(Int64(255), base=16) == "ff"
    @test string(Int128(1) << 100, base=16) == "10000000000000000000000000"
    @test string(Int128(-1), base=10) == "-1"
end

@testset "string(x; base=N) accepts all unsigned integer widths (Issue #4723)" begin
    @test string(UInt8(255), base=16) == "ff"
    @test string(UInt8(255), base=2) == "11111111"
    @test string(UInt16(1000), base=2) == "1111101000"
    @test string(UInt32(0xCAFEBABE), base=16) == "cafebabe"
    @test string(UInt64(1) << 60, base=16) == "1000000000000000"
    @test string(typemax(UInt128), base=16) == "ffffffffffffffffffffffffffffffff"
end

@testset "string(x; base=N) accepts Bool (Issue #4723)" begin
    @test string(true, base=10) == "1"
    @test string(true, base=2) == "1"
    @test string(false, base=10) == "0"
    @test string(false, base=16) == "0"
end

@testset "string(x; base=N) bases 2/8/10/16 and generic 2..36 (Issue #4723)" begin
    @test string(255, base=8) == "377"
    @test string(255, base=36) == "73"
    @test string(UInt8(255), base=36) == "73"
    @test string(Int32(-255), base=36) == "-73"
end

true
