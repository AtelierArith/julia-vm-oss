# Test: only function - returns single element
# Expected: true

using Test

@testset "only(x) - returns single element from collection" begin

    @test (only([42]) == 42)
end

true  # Test passed
