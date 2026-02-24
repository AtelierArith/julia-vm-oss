# Collecting kwargs in function definition
# Tests: function f(; kwargs...) where kwargs is Base.Pairs of all passed kwargs

using Test

function g(; kwargs...)
    # kwargs should be a Base.Pairs (like in Julia)
    return length(kwargs)
end

@testset "Collecting kwargs in function definition: function g(; kwargs...)" begin


    # Pass multiple keyword arguments
    result = g(a=1, b=2, c=3)

    # Return result for test comparison
    @test (Float64(result)) == 3.0
end

true  # Test passed
