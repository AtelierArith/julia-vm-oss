using Test
using Iterators

function test_drop()
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = 0.0
    for x in drop(arr, 2)
        result = result + x
    end
    return result  # 3 + 4 + 5 = 12.0
end

@testset "drop: skip first N elements of collection" begin

    @test (test_drop()) == 12.0
end

true  # Test passed
