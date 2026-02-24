# Test Tuple constant index type inference (Issue #1638)
# (1, 2.0)[1] should infer as Int64, not a generic tuple element type

using Test

@testset "Tuple constant index inference" begin
    # Basic tuple indexing with constant index
    t1 = (1, 2.0, "hello")
    @test t1[1] == 1
    @test typeof(t1[1]) == Int64
    @test t1[2] == 2.0
    @test typeof(t1[2]) == Float64
    @test t1[3] == "hello"
    @test typeof(t1[3]) == String

    # Direct tuple literal indexing
    @test (10, 20.5)[1] == 10
    @test typeof((10, 20.5)[1]) == Int64
    @test (10, 20.5)[2] == 20.5
    @test typeof((10, 20.5)[2]) == Float64

    # Mixed type tuple
    mixed = (true, 42, 3.14, "test")
    @test mixed[1] == true
    @test typeof(mixed[1]) == Bool
    @test mixed[2] == 42
    @test typeof(mixed[2]) == Int64
    @test mixed[3] == 3.14
    @test typeof(mixed[3]) == Float64
    @test mixed[4] == "test"
    @test typeof(mixed[4]) == String
end

true
