# Test last for single element tuple
# last((42,)) should return 42

using Test

@testset "last((42,)): returns last element 42 (Issue #496)" begin
    t = last((42,))
    @test (t == 42)
end

true  # Test passed
