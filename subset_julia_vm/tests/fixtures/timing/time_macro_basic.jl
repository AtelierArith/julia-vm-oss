# Test @time macro (Pure Julia implementation)
# Should measure execution time and return the result

using Test

function slow_sum(n)
    total = 0
    for i in 1:n
        total = total + i
    end
    total
end

@testset "@time macro measures time and returns result (Pure Julia)" begin


    # @time should return the result of the expression
    result = @time slow_sum(100)

    # Verify it returns the correct value (sum of 1 to 100 = 5050)
    @test (Float64(result)) == 5050.0
end

true  # Test passed
