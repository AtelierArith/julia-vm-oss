# Test: @generated fallback with where clause
# Type-parameterized function with fallback

using Test

function add_one(x::T) where T
    result = 0
    if @generated
        # In full Julia, would specialize based on T
        result = -1
    else
        # Fallback works for any numeric type
        result = x + 1
    end
    result
end

@testset "@generated with where clause uses fallback" begin


    @assert add_one(5) == 6
    @assert add_one(10) == 11

    # Return true if all tests passed
    @test (true)
end

true  # Test passed
