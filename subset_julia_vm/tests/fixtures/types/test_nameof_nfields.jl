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

@testset "reflection field metadata matrix (Issue #9540)" begin
    # Empty tuple types have no fields.
    @test fieldnames(Tuple{}) == ()
    @test fieldtypes(Tuple{}) == ()
    @test fieldcount(Tuple{}) == 0
    @test nfields(()) == 0

    # Tuple field names are integer positions, not symbols or strings.
    @test fieldnames(Tuple{Int64,String}) == (1, 2)
    @test fieldtypes(Tuple{Int64,String}) == (Int64, String)
    @test fieldcount(Tuple{Int64,String}) == 2
    @test nfields((1, "x")) == 2

    # User structs keep symbol field names and declared field types.
    p = TestPoint(1.0, 2.0, 3.0)
    @test fieldnames(TestPoint) == (:x, :y, :z)
    @test fieldtypes(TestPoint) == (Float64, Float64, Float64)
    @test fieldcount(TestPoint) == 3
    @test nfields(p) == 3

    # Named tuple type reflection must expose symbol names and value types.
    nt = (a = 1, b = "x")
    NT = typeof(nt)
    @test fieldnames(NT) == (:a, :b)
    @test fieldtypes(NT) == (Int64, String)
    @test fieldcount(NT) == 2
    @test nfields(nt) == 2
end

true  # Test passed
