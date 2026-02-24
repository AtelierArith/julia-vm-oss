# Test parametric array type dispatch (Issue #1237)
# Tests that Vector{Int64} and Vector{Float64} are properly distinguished
# for multiple dispatch purposes.

using Test

# Define functions with parametric array types
function test_dispatch(a::Vector{Int64})
    return "Int64"
end

function test_dispatch(a::Vector{Float64})
    return "Float64"
end

@testset "Parametric array type dispatch" begin
    # Test 1: collect from range should produce Vector{Int64}
    a = collect(1:5)
    @test typeof(a) == Vector{Int64}

    # Test 2: dispatch should work correctly for Vector{Int64}
    result = test_dispatch(a)
    @test result == "Int64"

    # Test 3: Float64 array dispatch
    b = [1.0, 2.0, 3.0]
    @test test_dispatch(b) == "Float64"

    # Test 4: literal Int64 array dispatch
    c = [1, 2, 3]
    @test test_dispatch(c) == "Int64"
end

true
