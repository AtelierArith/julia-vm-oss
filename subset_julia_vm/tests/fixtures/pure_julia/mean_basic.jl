# mean([2, 4, 6]) = 4

using Test
using Statistics

@testset "mean([2,4,6]) = 4 (Pure Julia implementation)" begin
    arr = [2.0, 4.0, 6.0]
    @test (mean(arr)) == 4.0
end

true  # Test passed
