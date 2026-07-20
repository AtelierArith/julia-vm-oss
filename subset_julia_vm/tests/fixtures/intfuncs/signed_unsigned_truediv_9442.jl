using Test

@testset "Signed×Unsigned true division / (Issue #9442)" begin
    @testset "mixed signed/unsigned across widths -> Float64, sign preserved" begin
        @test Int8(-1) / UInt8(5) === -0.2
        @test Int16(-1) / UInt16(5) === -0.2
        @test Int32(-1) / UInt32(5) === -0.2
        @test Int64(-1) / UInt64(5) === -0.2
        @test Int128(-1) / UInt128(5) === -0.2
        @test UInt8(5) / Int8(-1) === -5.0
        @test UInt64(5) / Int16(-1) === -5.0
        @test Int8(-6) / UInt16(4) === -1.5
    end
    @testset "result is always Float64" begin
        @test typeof(Int16(-1) / UInt16(5)) === Float64
        @test typeof(UInt8(3) / Int8(2)) === Float64
        @test typeof(Int8(3) / Int8(2)) === Float64
        @test typeof(UInt16(6) / UInt16(4)) === Float64
    end
    @testset "same-type and Bool integer division unchanged" begin
        @test Int16(-1) / Int16(5) === -0.2
        @test UInt16(6) / UInt16(4) === 1.5
        @test 6 / 4 === 1.5
        @test -6 / 4 === -1.5
        @test true / 2 === 0.5
        @test false / 2 === 0.0
    end
    @testset "division by zero yields Inf/-Inf/NaN, not InexactError" begin
        @test Int16(-1) / UInt16(0) === -Inf
        @test Int8(1) / UInt8(0) === Inf
        @test isnan(Int8(0) / UInt8(0))
    end
end

true
