# LinearIndices basic test

using Test

@testset "LinearIndices iteration over array dimensions" begin

    li = LinearIndices(6)
    test1 = li.len == 6

    # Test iteration
    sum_val = 0
    for i in li
        sum_val = sum_val + i
    end
    test2 = sum_val == 21  # 1+2+3+4+5+6 = 21

    # Test smaller case
    li2 = LinearIndices(4)
    sum2 = 0
    for i in li2
        sum2 = sum2 + i
    end
    test3 = sum2 == 10  # 1+2+3+4 = 10

    @test (test1 && test2 && test3)
end

true  # Test passed
