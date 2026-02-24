# Test hasproperty function
# hasproperty(x, s::Symbol) returns true if object x has property s

using Test

struct Point
    x::Float64
    y::Float64
end

mutable struct Counter
    value::Int64
    name::String
end

struct Outer
    inner::Point
    id::Int64
end

@testset "hasproperty: check if object has property (Issue #449)" begin

    result = true

    # Test with struct

    p = Point(1.0, 2.0)
    result = result && hasproperty(p, :x) == true
    result = result && hasproperty(p, :y) == true
    result = result && hasproperty(p, :z) == false

    # Test with mutable struct

    c = Counter(0, "test")
    result = result && hasproperty(c, :value) == true
    result = result && hasproperty(c, :name) == true
    result = result && hasproperty(c, :count) == false

    # Test with nested struct

    o = Outer(Point(1.0, 2.0), 1)
    result = result && hasproperty(o, :inner) == true
    result = result && hasproperty(o, :id) == true
    result = result && hasproperty(o, :x) == false  # x is property of inner, not outer

    @test (result)
end

true  # Test passed
