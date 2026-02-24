# Test Dict string-key indexing when Dict is passed as Any-typed parameter (Issue #1814)
# When a function receives a Dict without type annotation, the compiler infers
# the parameter as Any, emitting IndexLoad/IndexStore instead of DictGet/DictSet.
# The runtime must handle non-integer keys for Dict values.

using Test

# Function without type annotation - parameter type is Any
function get_value(d, key)
    return d[key]
end

# Function that sets a value on Any-typed Dict
function set_value(d, key, val)
    d[key] = val
    return d
end

# Function with explicit Dict type annotation (should always work)
function get_value_typed(d::Dict, key)
    return d[key]
end

@testset "Dict indexing with Any-typed parameters (Issue #1814)" begin
    d = Dict("a" => 1, "b" => 2, "c" => 3)

    # Reading Dict values through untyped function parameter
    @test get_value(d, "a") == 1
    @test get_value(d, "b") == 2
    @test get_value(d, "c") == 3

    # Reading Dict values through typed function parameter
    @test get_value_typed(d, "a") == 1

    # Writing Dict values through untyped function parameter
    d2 = Dict("x" => 10)
    result = set_value(d2, "y", 20)
    @test get(result, "x", 0) == 10
    @test get(result, "y", 0) == 20

    # Integer-keyed Dict through untyped function parameter
    d3 = Dict(1 => "one", 2 => "two")
    @test get_value(d3, 1) == "one"
    @test get_value(d3, 2) == "two"
end

true
