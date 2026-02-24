# Test popat! with default - returns default if index out of bounds

using Test

@testset "popat!(arr, i, default): return default if index out of bounds (Issue #436)" begin
    arr = [1.0, 2.0, 3.0]
    x = popat!(arr, 10, -1.0)
    @test (x == -1.0 && length(arr) == 3)
end

true  # Test passed
