# Test enumerate: for (i, x) in enumerate(arr)
# Expected: 1*10 + 2*20 + 3*30 = 10 + 40 + 90 = 140

using Test

function test_enumerate()
    arr = [10.0, 20.0, 30.0]
    result = 0.0
    for (i, x) in enumerate(arr)
        result = result + i * x
    end
    return result
end

@testset "enumerate: for (i, x) in enumerate(arr) iterates with index" begin
    @test (test_enumerate()) == 140.0
end

true  # Test passed
