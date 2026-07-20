# Test popat! - remove and return element at index

using Test

@testset "popat!(arr, i): remove and return element at index (Issue #436)" begin
    arr = [1.0, 2.0, 3.0, 4.0]
    x = popat!(arr, 2)
    @test (x == 2.0 && length(arr) == 3 && arr[1] == 1.0 && arr[2] == 3.0)
end

true  # Test passed
