# Test basic view creation and indexing

using Test

@testset "Basic view operations" begin
    # Create an array
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Create a view of elements 2 to 4
    v = view(arr, 2:4)

    # Check view indexing
    @test v[1] == 2.0
    @test v[2] == 3.0
    @test v[3] == 4.0
end

true
