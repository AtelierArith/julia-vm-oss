# Set operations tests

using Test

@testset "Set operations: union, intersect, setdiff, symdiff, issubset, isdisjoint, push!, delete!" begin

    # Create sets
    a = Set([1, 2, 3])
    b = Set([2, 3, 4])

    # Union: a ∪ b = {1, 2, 3, 4}
    u = union(a, b)
    # length should be 4

    # Intersection: a ∩ b = {2, 3}
    i = intersect(a, b)
    # length should be 2

    # Difference: a \ b = {1}
    d = setdiff(a, b)
    # length should be 1

    # Symmetric difference: (a \ b) ∪ (b \ a) = {1, 4}
    s = symdiff(a, b)
    # length should be 2

    # Subset check
    c = Set([1, 2])
    sub = issubset(c, a)  # true

    # Disjoint check
    e = Set([5, 6])
    dis = isdisjoint(a, e)  # true

    # push! and delete!
    f = Set([10, 20])
    push!(f, 30)
    # length should be 3
    delete!(f, 10)
    # length should be 2

    # Verify results
    # Union: 4 + Intersect: 2 + Setdiff: 1 + Symdiff: 2 = 9
    # + (sub ? 10 : 0) = 19
    # + (dis ? 10 : 0) = 29
    # + length(f) = 31
    result = length(u) + length(i) + length(d) + length(s)
    result = result + (sub ? 10 : 0)
    result = result + (dis ? 10 : 0)
    result = result + length(f)
    @test (result) == 31.0
end

true  # Test passed
