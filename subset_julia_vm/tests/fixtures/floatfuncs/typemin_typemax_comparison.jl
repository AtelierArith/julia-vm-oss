using Test

@testset "typemin/typemax comparison (Issue #2231)" begin
    # typemin(Float64) should return Float64, not Bool
    @test typemin(Float64) < 0
    @test typemin(Float64) == -Inf
    @test typeof(typemin(Float64)) == Float64

    # typemax(Float64) should return Float64
    @test typemax(Float64) > 0
    @test typemax(Float64) == Inf
    @test typeof(typemax(Float64)) == Float64

    # typemin/typemax with Float32
    @test typemin(Float32) < 0
    @test typeof(typemin(Float32)) == Float32
    @test typemax(Float32) > 0
    @test typeof(typemax(Float32)) == Float32

    # typemin/typemax with integer types
    @test typemin(Int64) < 0
    @test typeof(typemin(Int64)) == Int64
    @test typemax(Int64) > 0
    @test typeof(typemax(Int64)) == Int64

    # Arithmetic with typemin/typemax results
    @test typemin(Float64) + 1.0 == -Inf
    @test typemax(Float64) - 1.0 == Inf

    # Variable assignment preserves type
    x = typemin(Float64)
    @test x < 0
    @test typeof(x) == Float64
end

true
