# Test reverse for single element tuple
# reverse((42,)) should return (42,)

using Test

@testset "reverse((42,)): single element tuple (Issue #496)" begin
    t = reverse((42,))
    @test (t[1] == 42)
end

true  # Test passed
