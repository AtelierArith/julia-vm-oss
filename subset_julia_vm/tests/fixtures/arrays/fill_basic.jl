# Test fill - create array filled with a value

using Test

@testset "fill(value, n): create array filled with value (Issue #690)" begin
    arr = fill(5.0, 4)
    @test (length(arr) == 4 && arr[1] == 5.0 && arr[2] == 5.0 && arr[3] == 5.0 && arr[4] == 5.0)
end

true  # Test passed
