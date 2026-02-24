# Test front for single element tuple
# front((42,)) should return ()

using Test

@testset "front((42,)): returns empty tuple (Issue #496)" begin
    t = front((42,))
    @test (t == ())
end

true  # Test passed
