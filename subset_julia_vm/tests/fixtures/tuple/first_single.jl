# Test first for single element tuple
# first((42,)) should return 42

using Test

@testset "first((42,)): returns first element 42 (Issue #496)" begin
    t = first((42,))
    @test (t == 42)
end

true  # Test passed
