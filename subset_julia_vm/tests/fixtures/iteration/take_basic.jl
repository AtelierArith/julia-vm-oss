using Test
using Iterators

function test_take()
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = 0.0
    for x in take(arr, 3)
        result = result + x
    end
    return result  # 1 + 2 + 3 = 6.0
end

@testset "take: iterate first N elements of collection" begin

    @test (test_take()) == 6.0
end

true  # Test passed
