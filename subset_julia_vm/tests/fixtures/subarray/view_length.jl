# Test length and size operations on SubArray

using Test

@testset "View length and size" begin
    # Create an array
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Create a view of elements 2 to 4
    v = view(arr, 2:4)

    # Check length
    @test length(v) == 3

    # Check size
    @test size(v, 1) == 3

    # Check firstindex and lastindex
    @test firstindex(v) == 1
    @test lastindex(v) == 3
end

true
