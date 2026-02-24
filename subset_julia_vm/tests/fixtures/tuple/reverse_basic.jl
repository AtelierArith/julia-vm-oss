# Test reverse for tuple returns a tuple (not array)
# reverse((1, 2, 3)) should return (3, 2, 1)

using Test

@testset "reverse((1,2,3)): returns tuple (3,2,1) not array (Issue #496)" begin
    t = reverse((1, 2, 3))
    @test (t == (3, 2, 1))
end

true  # Test passed
