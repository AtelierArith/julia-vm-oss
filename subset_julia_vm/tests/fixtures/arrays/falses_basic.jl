# Test falses - create array of false values

using Test

@testset "falses(n): create array of false values (Issue #669)" begin
    arr = falses(4)
    @test (length(arr) == 4 && arr[1] == false && arr[2] == false && arr[3] == false && arr[4] == false)
end

true  # Test passed
