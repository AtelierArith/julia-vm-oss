# Tuple comprehension basic

using Test

@testset "Basic tuple comprehension: [(i, i^2) for i in 1:3]" begin
    arr = [(i, i^2) for i in 1:3]
    @test (length(arr)) == 3.0
end

true  # Test passed
