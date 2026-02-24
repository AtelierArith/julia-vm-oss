# Test countfrom basic usage with take
# countfrom(1) yields 1, 2, 3, ...
# take(countfrom(1), 5) yields first 5 elements
# sum should be 1+2+3+4+5 = 15

using Test
using Iterators

@testset "countfrom(): infinite counting from 1 with step 1 (Issue #530)" begin
    @test (sum(collect(take(countfrom(), 5)))) == 15
end

true  # Test passed
