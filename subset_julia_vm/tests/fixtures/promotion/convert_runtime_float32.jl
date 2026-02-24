# Test convert(T, x) where T is a runtime DataType variable for Float32 (Issue #1786)

using Test

@testset "convert with runtime Float32 DataType variable" begin
    # Direct convert with literal type (baseline)
    c1 = convert(Float32, Int64(2))
    @test typeof(c1) == Float32
    @test c1 == Float32(2.0)

    # convert with runtime DataType from promote_type
    T = promote_type(Float32, Int64)
    @test T == Float32

    # convert with runtime DataType variable
    c2 = convert(T, Int64(2))
    @test typeof(c2) == Float32
    @test c2 == Float32(2.0)

    # convert Float64 to runtime Float32
    c3 = convert(T, Float64(3.14))
    @test typeof(c3) == Float32

    # convert Float32 to runtime Float32 (identity)
    c4 = convert(T, Float32(1.5))
    @test typeof(c4) == Float32
    @test c4 == Float32(1.5)
end

true
