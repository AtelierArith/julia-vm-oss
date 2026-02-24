# Test cld() - ceiling division (Issue #2088)

using Test

@testset "cld integer" begin
    @test cld(7, 3) == 3
    @test cld(6, 3) == 2
    @test cld(1, 3) == 1
    @test cld(0, 3) == 0
    @test cld(9, 3) == 3
    @test cld(10, 3) == 4

    # Negative numbers
    @test cld(-7, 2) == -3
    @test cld(-6, 3) == -2

    # Type correctness: cld(Int64, Int64) returns Int64
    @test isa(cld(7, 2), Int64)
end

@testset "cld float" begin
    @test cld(7.0, 3.0) == 3.0
    @test cld(6.0, 3.0) == 2.0
    @test cld(1.0, 3.0) == 1.0

    # Type correctness: cld(Float64, Float64) returns Float64
    @test isa(cld(7.0, 3.0), Float64)
end

true
