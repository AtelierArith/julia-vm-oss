# Test getfield() for field access by name (Symbol) and index (Int)

using Test

# Define test structs outside @testset
struct Point
    x::Float64
    y::Float64
end

struct MyStruct
    a::Int64
    b::String
    c::Tuple{Int64, Int64}
end

@testset "getfield() basic functionality" begin
    # Test getfield with Symbol for struct
    p = Point(1.0, 2.0)
    @test getfield(p, :x) == 1.0
    @test getfield(p, :y) == 2.0

    # Test getfield with integer index for struct (1-based)
    @test getfield(p, 1) == 1.0
    @test getfield(p, 2) == 2.0

    # Test getfield with more complex struct
    s = MyStruct(42, "hello", (1, 2))
    @test getfield(s, :a) == 42
    @test getfield(s, :b) == "hello"
    @test getfield(s, :c) == (1, 2)

    # Test getfield with integer index for complex struct
    @test getfield(s, 1) == 42
    @test getfield(s, 2) == "hello"
    @test getfield(s, 3) == (1, 2)

    # Test getfield with tuple (integer index only)
    t = (10, 20, 30)
    @test getfield(t, 1) == 10
    @test getfield(t, 2) == 20
    @test getfield(t, 3) == 30
end

true
