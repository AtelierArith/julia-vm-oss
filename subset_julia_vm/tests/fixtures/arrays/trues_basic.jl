# Test trues - create array of true values

using Test

@testset "trues(n): create array of true values (Issue #668)" begin
    arr = trues(3)
    @test (length(arr) == 3 && arr[1] == true && arr[2] == true && arr[3] == true)
end

true  # Test passed
