# Test reverse for empty tuple
# reverse(()) should return ()

using Test

@testset "reverse(()): empty tuple returns empty tuple (Issue #496)" begin
    t = reverse(())
    @test (t == ())
end

true  # Test passed
