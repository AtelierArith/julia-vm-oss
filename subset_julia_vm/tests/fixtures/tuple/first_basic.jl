# Test first for tuple - returns first element
# first((1, 2, 3)) should return 1

using Test

@testset "first((1,2,3)): returns first element 1 (Issue #496)" begin
    t = first((1, 2, 3))
    @test (t == 1)
end

true  # Test passed
