# Integer type combination tests
# Verifies that mixed-type integer arithmetic and comparisons work correctly.
# Prevention test for Issue #1614 / #2218 / #2267

using Test

@testset "Int8 mixed-type arithmetic" begin
    @test Int8(2) + Int64(3) == 5
    @test Int64(3) + Int8(2) == 5
    @test Int8(5) - Int64(2) == 3
    @test Int8(2) * Int64(3) == 6
end

@testset "Int16 mixed-type arithmetic" begin
    @test Int16(100) + Int64(200) == 300
    @test Int64(200) + Int16(100) == 300
    @test Int16(100) * Int64(3) == 300
end

@testset "Int32 mixed-type arithmetic" begin
    @test Int32(1000) + Int64(2000) == 3000
    @test Int64(2000) + Int32(1000) == 3000
    @test Int32(100) * Int64(30) == 3000
end

@testset "UInt8 mixed-type arithmetic" begin
    @test UInt8(200) + Int64(55) == 255
    @test Int64(55) + UInt8(200) == 255
    @test UInt8(10) * Int64(5) == 50
end

@testset "UInt16 mixed-type arithmetic" begin
    @test UInt16(1000) + Int64(2000) == 3000
    @test Int64(2000) + UInt16(1000) == 3000
end

@testset "UInt32 mixed-type arithmetic" begin
    @test UInt32(100000) + Int64(200000) == 300000
    @test Int64(200000) + UInt32(100000) == 300000
end

@testset "Integer-Float promotion" begin
    @test Int8(2) + 1.0 == 3.0
    @test typeof(Int8(2) + 1.0) == Float64
    @test Int16(2) + 1.0 == 3.0
    @test typeof(Int16(2) + 1.0) == Float64
    @test Int32(2) + 1.0 == 3.0
    @test typeof(Int32(2) + 1.0) == Float64
    @test UInt8(2) + 1.0 == 3.0
    @test typeof(UInt8(2) + 1.0) == Float64
end

@testset "Integer comparison across types" begin
    @test Int8(42) == Int64(42)
    @test Int16(42) == Int64(42)
    @test Int32(42) == Int64(42)
    @test UInt8(42) == Int64(42)
    @test UInt16(42) == Int64(42)
    @test UInt32(42) == Int64(42)
    @test Int8(1) < Int64(2)
    @test UInt8(1) < Int64(2)
end

true
