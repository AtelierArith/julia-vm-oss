# Test that view modifications affect parent array

using Test

@testset "View modification affects parent" begin
    # Create an array
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Create a view of elements 2 to 4
    v = view(arr, 2:4)

    # Modify through the view
    v[2] = 100.0

    # Check that parent array was modified
    @test arr[3] == 100.0
    @test v[2] == 100.0
end

true
