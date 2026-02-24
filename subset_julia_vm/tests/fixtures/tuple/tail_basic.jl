# Test tail for tuple - returns all but first element
# tail((1, 2, 3)) should return (2, 3)

using Test

@testset "tail((1,2,3)): returns (2,3) - all but first element (Issue #490)" begin
    t = tail((1, 2, 3))
    @test (t == (2, 3))
end

true  # Test passed
