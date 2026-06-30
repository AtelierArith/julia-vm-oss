# Test that min/max return promoted type, not Union (Issue #3479)

using Test

@testset "type_inference_min_max_promotion: min/max use Julia promotion" begin
    @test typeof(max(1, 2.0)) == Float64
    @test typeof(min(1, 2.0)) == Float64
    @test typeof(max(Int8(1), Int16(2))) == Int16
    @test typeof(min(Int32(1), Int64(2))) == Int64
    @test typeof(max(Int32(2), Int64(1))) == Int64
    @test typeof(min(Int32(2), Int64(1))) == Int64
    @test typeof(max(1, 2)) == Int64
    @test typeof(min(Int8(1), Int8(2))) == Int8
    @test typeof(max(UInt8(1), UInt8(2))) == UInt8
    @test typeof(min(Float16(1), Float16(2))) == Float16
    @test typeof(max(Float32(1), Float32(2))) == Float32
    @test typeof(max(false, true)) == Bool
    lo, hi = minmax(Int32(2), Int64(1))
    @test typeof(lo) == Int64
    @test typeof(hi) == Int64
end

true
