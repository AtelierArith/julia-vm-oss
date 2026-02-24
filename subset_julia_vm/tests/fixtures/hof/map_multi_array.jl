# map(f, A, B) - map with multiple array arguments (Issue #2019)
# Tests element-wise application of binary function to two collections.

using Test

@testset "map(f, A, B) multi-array (Issue #2019)" begin
    # Lambda function
    @test map((x,y) -> x + y, [1,2,3], [4,5,6]) == [5.0, 7.0, 9.0]
    @test map((x,y) -> x * y, [1,2,3], [4,5,6]) == [4.0, 10.0, 18.0]
    @test map((x,y) -> x - y, [10,20,30], [1,2,3]) == [9.0, 18.0, 27.0]

    # Bare operators as function arguments
    @test map(+, [1,2,3], [4,5,6]) == [5.0, 7.0, 9.0]
    @test map(*, [1,2,3], [4,5,6]) == [4.0, 10.0, 18.0]
    @test map(-, [10,20,30], [1,2,3]) == [9.0, 18.0, 27.0]

    # Single-element arrays
    @test map(+, [10], [20]) == [30.0]

    # Single-array map still works
    @test map(x -> x * 2, [1,2,3]) == [2.0, 4.0, 6.0]
end

true
