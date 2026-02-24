# Test tail for single-element tuple
# tail((1,)) should return ()

using Test

@testset "tail((1,)): returns () - single element to empty (Issue #490)" begin
    t = tail((1,))
    @test (length(t) == 0)
end

true  # Test passed
