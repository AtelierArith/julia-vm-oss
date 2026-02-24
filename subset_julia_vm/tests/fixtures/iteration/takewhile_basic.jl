# Test Iterators.takewhile - yield elements while predicate is true (Issue #1815)

using Test

less_than_4(x) = x < 4
is_positive(x) = x > 0

@testset "takewhile" begin
    # Basic takewhile: stop when predicate becomes false
    result = collect(takewhile(less_than_4, [1, 2, 3, 4, 5]))
    @test length(result) == 3
    @test result[1] == 1
    @test result[2] == 2
    @test result[3] == 3

    # takewhile with all elements passing
    result2 = collect(takewhile(is_positive, [1, 2, 3]))
    @test length(result2) == 3

    # takewhile with no elements passing
    result3 = collect(takewhile(less_than_4, [5, 6, 7]))
    @test length(result3) == 0

    # takewhile with empty collection
    result4 = collect(takewhile(less_than_4, Int64[]))
    @test length(result4) == 0

    # takewhile stops at first false even if later elements would pass
    result5 = collect(takewhile(less_than_4, [1, 2, 5, 3, 1]))
    @test length(result5) == 2
    @test result5[1] == 1
    @test result5[2] == 2

    # takewhile with range
    result6 = collect(takewhile(less_than_4, 1:10))
    @test length(result6) == 3
end

true
