# Test pop! on Set (Issue #1832)
# pop!(set) removes and returns an arbitrary element from the Set

using Test

@testset "pop! on Set" begin
    # pop! removes one element and returns it
    s = Set([1, 2, 3])
    val = pop!(s)
    @test length(s) == 2

    # pop! until empty
    s2 = Set([10, 20])
    v1 = pop!(s2)
    v2 = pop!(s2)
    @test length(s2) == 0
    # The two popped values should sum to 30
    @test v1 + v2 == 30
end

true
