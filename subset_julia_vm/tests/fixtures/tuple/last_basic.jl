# Test last for tuple - returns last element
# last((1, 2, 3)) should return 3

using Test

@testset "last((1,2,3)): returns last element 3 (Issue #496)" begin
    t = last((1, 2, 3))
    @test (t == 3)
end

true  # Test passed
