# Test countfrom with start value
# countfrom(10) yields 10, 11, 12, ...
# take(countfrom(10), 3) yields 10, 11, 12
# sum should be 10+11+12 = 33

using Test
using Iterators

@testset "countfrom(n): counting from n with step 1 (Issue #530)" begin
    @test (sum(collect(take(countfrom(10), 3)))) == 33
end

true  # Test passed
