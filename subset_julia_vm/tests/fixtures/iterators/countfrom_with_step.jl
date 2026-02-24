# Test countfrom with start and step
# countfrom(5, 2) yields 5, 7, 9, ...
# take(countfrom(5, 2), 4) yields 5, 7, 9, 11
# sum should be 5+7+9+11 = 32

using Test
using Iterators

@testset "countfrom(start, step): counting with custom step (Issue #530)" begin
    @test (sum(collect(take(countfrom(5, 2), 4)))) == 32
end

true  # Test passed
