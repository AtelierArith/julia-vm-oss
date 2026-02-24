# Test Unicode identity operators (equivalent to === and !==)

using Test

@testset "Unicode identity operators: ≡ (===) and ≢ (!==)" begin

    # Test basic identity with integers
    @assert 1 === 1
    @assert 1 ≡ 1  # Unicode equivalent of ===

    # Test that === and ≡ are equivalent
    @assert (1 === 1) == (1 ≡ 1)
    @assert (1 === 2) == (1 ≡ 2)

    # Test with different types
    @assert !(1 === 1.0)
    @assert !(1 ≡ 1.0)

    # Test with strings
    @assert "hello" === "hello"
    @assert "hello" ≡ "hello"

    # Test non-identity operators
    @assert 1 !== 2
    @assert 1 ≢ 2  # Unicode equivalent of !==

    # Test equivalence of !== and ≢
    @assert (1 !== 2) == (1 ≢ 2)
    @assert (1 !== 1) == (1 ≢ 1)

    # Verify ≡ and ≢ are proper complements
    @assert (1 ≡ 1) && !(1 ≢ 1)
    @assert !(1 ≡ 2) && (1 ≢ 2)

    @test (true)
end

true  # Test passed
