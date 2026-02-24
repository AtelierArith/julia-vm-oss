using Test

# Test convert with UInt/Bool to Float32/Float64/Int64 (Issue #2249).
# Previously, UInt and Bool types were missing from convert match arms.

@testset "convert UInt to Float32" begin
    @test convert(Float32, UInt8(2)) == Float32(2.0)
    @test convert(Float32, UInt16(300)) == Float32(300.0)
    @test convert(Float32, UInt32(100000)) == Float32(100000.0)
    @test convert(Float32, UInt64(42)) == Float32(42.0)
end

@testset "convert UInt to Float64" begin
    @test convert(Float64, UInt8(255)) == 255.0
    @test convert(Float64, UInt16(65535)) == 65535.0
    @test convert(Float64, UInt32(100)) == 100.0
    @test convert(Float64, UInt64(42)) == 42.0
end

@testset "convert UInt to Int64" begin
    @test convert(Int64, UInt8(128)) == 128
    @test convert(Int64, UInt16(500)) == 500
    @test convert(Int64, UInt32(100000)) == 100000
end

@testset "convert Bool to numeric" begin
    @test convert(Int64, true) == 1
    @test convert(Int64, false) == 0
    @test convert(Float64, true) == 1.0
    @test convert(Float64, false) == 0.0
    @test convert(Float32, true) == Float32(1.0)
    @test convert(Float32, false) == Float32(0.0)
end

true
