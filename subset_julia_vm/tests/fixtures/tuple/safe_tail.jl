# Test safe_tail for tuple - returns all but first element without throwing on empty
# Unlike tail, safe_tail(()) returns () instead of throwing

using Test

@testset "safe_tail: non-throwing version of tail (Issue #490)" begin
    # Normal case - same as tail
    t1 = safe_tail((1, 2, 3))
    @test (t1 == (2, 3))

    # Single element - returns empty tuple
    t2 = safe_tail((42,))
    @test (length(t2) == 0)

    # Empty tuple - returns empty tuple (doesn't throw!)
    t3 = safe_tail(())
    @test (length(t3) == 0)
end

true  # Test passed
