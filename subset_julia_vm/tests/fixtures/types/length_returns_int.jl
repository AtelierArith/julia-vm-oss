# Test that length() returns Int type for Array

using Test

@testset "length() returns Int64 type for Array" begin
    arr = [1, 2, 3, 4, 5]
    len = length(arr)
    @test (typeof(len) == Int64)
end

true  # Test passed
