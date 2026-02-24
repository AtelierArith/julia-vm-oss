# Test basic varargs function
# function sum_all(args...) collects all arguments into a Tuple

using Test

function sum_all(args...)
    total = 0
    for x in args
        total += x
    end
    total
end

@testset "Basic varargs function summing arguments" begin


    result1 = sum_all(1, 2, 3)
    check1 = result1 == 6

    result2 = sum_all(10, 20, 30, 40)
    check2 = result2 == 100

    @test (check1 && check2)
end

true  # Test passed
