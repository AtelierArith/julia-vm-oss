# Test zip: for (x, y) in zip(a, b)
# Expected: 1*10 + 2*20 + 3*30 = 10 + 40 + 90 = 140

using Test

function test_zip()
    a = [1.0, 2.0, 3.0]
    b = [10.0, 20.0, 30.0]
    result = 0.0
    for (x, y) in zip(a, b)
        result = result + x * y
    end
    return result
end

@testset "zip: for (x, y) in zip(a, b) iterates two arrays in parallel" begin
    @test (test_zip()) == 140.0
end

true  # Test passed
