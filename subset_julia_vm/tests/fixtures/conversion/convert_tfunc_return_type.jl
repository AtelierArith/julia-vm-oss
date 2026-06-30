# Test that convert(T, x) returns type T, not the source type (Issue #3475)

using Test

@testset "conversion_convert_tfunc_return_type: convert returns target type" begin
    @test typeof(convert(Float32, 1)) == Float32
    @test typeof(convert(Float32, 1.0)) == Float32
    @test typeof(convert(Float64, Float32(1.0))) == Float64
    @test typeof(convert(Int32, 1)) == Int32
    @test typeof(convert(Bool, 1)) == Bool
end

true
