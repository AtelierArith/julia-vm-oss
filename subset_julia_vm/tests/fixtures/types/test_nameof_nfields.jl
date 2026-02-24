# Test nameof and nfields reflection functions

using Test

# Define a test struct
struct TestPoint
    x::Float64
    y::Float64
    z::Float64
end

@testset "nameof function" begin
    # Test nameof for types
    @test nameof(Int64) == :Int64
    @test nameof(Float64) == :Float64
    @test nameof(String) == :String
    @test nameof(TestPoint) == :TestPoint
    
    # Test nameof for functions
    @test nameof(sin) == :sin
    @test nameof(cos) == :cos
    @test nameof(sum) == :sum
end

@testset "nfields function" begin
    # Test nfields for struct instances
    p = TestPoint(1.0, 2.0, 3.0)
    @test nfields(p) == 3
    
    # Test nfields for simpler types - tuples have fields
    t = (1, 2, 3, 4)
    @test nfields(t) == 4
end

true  # Test passed
