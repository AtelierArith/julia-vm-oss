# Basic kwargs splatting in function call
# Tests: f(; opts...) where opts is a NamedTuple

using Test

function f(; x=0, y=0)
    return x + y
end

@testset "Basic kwargs splatting in function call: f(; opts...)" begin


    # Create a NamedTuple
    opts = (x=1, y=2)

    # Call with kwargs splat
    result = f(; opts...)

    # Return result for test comparison
    @test (Float64(result)) == 3.0
end

true  # Test passed
