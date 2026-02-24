# Test in operator: x in collection
# Also tests: x ∈ collection

using Test

@testset "in operator - check if element is in collection (Issue #488)" begin

    result = true

    # Test with arrays of integers
    result = result && (1 in [1, 2, 3]) == true
    result = result && (5 in [1, 2, 3]) == false
    result = result && (0 in Int64[]) == false

    # Test with arrays of floats
    result = result && (1.5 in [1.0, 1.5, 2.0]) == true
    result = result && (3.0 in [1.0, 1.5, 2.0]) == false

    # Test with tuples
    result = result && (2 in (1, 2, 3)) == true
    result = result && (4 in (1, 2, 3)) == false

    # Test with symbols in tuples
    result = result && (:x in (:x, :y, :z)) == true
    result = result && (:w in (:x, :y, :z)) == false

    # Test with characters in strings
    result = result && ('e' in "hello") == true
    result = result && ('z' in "hello") == false
    result = result && ('日' in "日本語") == true

    # Test with Set
    s = Set([1, 2, 3])
    result = result && (2 in s) == true
    result = result && (5 in s) == false

    # Test with Dict keys
    d = Dict(1 => "a", 2 => "b")
    result = result && (1 in keys(d)) == true
    result = result && (3 in keys(d)) == false

    # Test with unicode ∈ operator (same as in)
    result = result && (1 ∈ [1, 2, 3]) == true
    result = result && (5 ∈ [1, 2, 3]) == false

    # Test mixed numeric types (Int64/Float64)
    result = result && (1 in [1.0, 2.0, 3.0]) == true
    result = result && (2.0 in [1, 2, 3]) == true

    @test (result)
end

true  # Test passed
