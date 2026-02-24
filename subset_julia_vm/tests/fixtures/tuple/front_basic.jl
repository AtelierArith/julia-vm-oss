# Test front for tuple - returns all but last element
# front((1, 2, 3)) should return (1, 2)

using Test

@testset "front((1,2,3)): returns (1,2) - all but last element (Issue #496)" begin
    t = front((1, 2, 3))
    @test (t == (1, 2))
end

true  # Test passed
