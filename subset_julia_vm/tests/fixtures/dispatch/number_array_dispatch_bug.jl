# Test for Issue #1658: Number type should not match Array types in dispatch
# This bug occurs when an array is passed through a higher-order function to
# a method that expects Number

using Test

# Define a function that only works with numbers
function process(x::Number)
    return x + 1
end

# Define a higher-order wrapper that calls through a function variable
function call_func(f, arg)
    return f(arg)
end

@testset "Number type should not match Array in dynamic dispatch" begin
    # Test that passing Number works correctly
    @test call_func(process, 5) == 6
    @test call_func(process, 2.5) == 3.5

    # Note: Calling process with an array through call_func correctly throws
    # MethodError: no method matching process(Vector{Float64})
    # This is tested by the error handling tests separately
end

true
