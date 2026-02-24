# Test Iterators.dropwhile - skip elements while predicate is true (Issue #1815)

using Test

less_than_3(x) = x < 3
is_negative(x) = x < 0

@testset "dropwhile" begin
    # Basic dropwhile: skip while true, then yield the rest
    result = collect(dropwhile(less_than_3, [1, 2, 3, 4, 5]))
    @test length(result) == 3
    @test result[1] == 3
    @test result[2] == 4
    @test result[3] == 5

    # dropwhile with no elements to skip
    result2 = collect(dropwhile(is_negative, [1, 2, 3]))
    @test length(result2) == 3
    @test result2[1] == 1

    # dropwhile with all elements skipped
    result3 = collect(dropwhile(less_than_3, [1, 2]))
    @test length(result3) == 0

    # dropwhile with empty collection
    result4 = collect(dropwhile(less_than_3, Int64[]))
    @test length(result4) == 0

    # dropwhile yields elements after first false, even if pred becomes true again
    result5 = collect(dropwhile(less_than_3, [1, 2, 3, 1, 2]))
    @test length(result5) == 3
    @test result5[1] == 3
    @test result5[2] == 1
    @test result5[3] == 2

    # dropwhile with range
    result6 = collect(dropwhile(less_than_3, 1:5))
    @test length(result6) == 3
    @test result6[1] == 3
end

true
