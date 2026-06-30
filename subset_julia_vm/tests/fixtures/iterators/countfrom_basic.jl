# Test countfrom basic usage with take
# countfrom(1) yields 1, 2, 3, ...
# take(countfrom(1), 5) yields first 5 elements
# sum should be 1+2+3+4+5 = 15

using Test
using Iterators

@testset "countfrom(): infinite counting from 1 with step 1 (Issue #530)" begin
    c = countfrom()
    @test IteratorSize(c) isa IsInfinite
    @test IteratorEltype(c) isa HasEltype
    @test eltype(c) == Int64
    @test length(take(c, 5)) == 5
    @test (sum(collect(take(countfrom(), 5)))) == 15
    @test collect(take(countfrom(1.5, 0.25), 3)) == [1.5, 1.75, 2.0]
end

true  # Test passed
