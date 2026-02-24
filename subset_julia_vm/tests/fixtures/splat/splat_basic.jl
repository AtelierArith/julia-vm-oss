# Test basic splat operator with function calls
# Expected result: 6.0

using Test

function add3(a, b, c)
    return a + b + c
end

@testset "Basic splat operator with function call" begin


    args = [1, 2, 3]
    result = add3(args...)

    @test (Float64(result)) == 6.0
end

true  # Test passed
