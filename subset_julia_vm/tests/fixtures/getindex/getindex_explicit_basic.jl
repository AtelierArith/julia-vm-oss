# Test: getindex explicit call returns same as indexing syntax

using Test

@testset "getindex(arr, i) is equivalent to arr[i]" begin
    arr = [10, 20, 30]

    # getindex(arr, i) should be equivalent to arr[i]
    @assert getindex(arr, 1) == 10
    @assert getindex(arr, 2) == 20
    @assert getindex(arr, 3) == 30

    # Compare with indexing syntax
    @assert getindex(arr, 2) == arr[2]

    @test (true)
end

true  # Test passed
