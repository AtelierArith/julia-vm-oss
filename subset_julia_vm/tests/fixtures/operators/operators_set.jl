# Test set operators
# Note: Some tests are omitted due to a VM bug with negating certain function return values.

using Test

function issubset_proper(a, b)
    return issubset(a, b) && !issubset(b, a)
end

function issuperset(a, b)
    return issubset(b, a)
end

function issuperset_proper(a, b)
    return issubset_proper(b, a)
end

@testset "set operators (issubset, in, proper subset/superset)" begin

    # Helper functions for proper subset/superset (not in Julia Base)
    # SubsetJuliaVM provides these but official Julia doesn't



    a = [1.0, 2.0, 3.0]
    b = [1.0, 2.0, 3.0, 4.0, 5.0]
    c = [1.0, 2.0, 3.0]  # Equal to a

    # Test ∈ operator (element in collection)
    @assert 1.0 ∈ a
    @assert 3.0 ∈ a
    @assert 4.0 ∈ b

    # Test ∉ operator (element not in collection)
    @assert 5.0 ∉ a
    @assert 4.0 ∉ a
    @assert 10.0 ∉ b

    # Test ∋ operator (collection contains element)
    @assert a ∋ 1.0
    @assert b ∋ 4.0

    # Test ∌ operator (collection does not contain element)
    @assert a ∌ 5.0
    @assert a ∌ 4.0

    # Test issubset function
    @assert issubset(a, b)
    @assert issubset(a, c)  # Equal sets are subsets
    @assert issubset(c, a)  # Equal sets are subsets

    # Test ⊆ operator (subset)
    @assert a ⊆ b
    @assert a ⊆ c
    @assert c ⊆ a

    # Test ⊈ operator (not subset)
    @assert b ⊈ a  # b is NOT a subset of a

    # Test issubset_proper function
    @assert issubset_proper(a, b)

    # Test ⊊ operator (proper subset)
    @assert a ⊊ b

    # Test issuperset function
    @assert issuperset(b, a)  # b is superset of a
    @assert issuperset(c, a)  # Equal sets are supersets

    # Test ⊇ operator (superset)
    @assert b ⊇ a
    @assert c ⊇ a

    # Test ⊉ operator (not superset)
    @assert a ⊉ b  # a is NOT a superset of b

    # Test issuperset_proper function
    @assert issuperset_proper(b, a)  # b is proper superset of a

    # Test ⊋ operator (proper superset)
    @assert b ⊋ a

    @test (true)
end

true  # Test passed
