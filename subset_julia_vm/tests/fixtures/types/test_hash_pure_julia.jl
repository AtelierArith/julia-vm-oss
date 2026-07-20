# Test Pure Julia hash functions (Issues #2582 / #10237)
# Verifies hash(x) and hash(x, h) uphold the portable hash contract.
# Concrete hash VALUES are implementation-defined (upstream documents them as
# unstable across versions), so this fixture only asserts contract properties
# that hold under both upstream julia and sjulia. In particular it does NOT
# assert sjulia's internal mixing formula or the hash of -0.0 relative to 0.0
# (unspecified: isequal(0.0, -0.0) is false, so either relation is legal).
using Test

@testset "hash Pure Julia" begin
    # Basic hash - returns consistent values
    @test hash(42) == hash(42)
    @test hash(3.14) == hash(3.14)
    @test hash("hello") == hash("hello")
    @test hash('a') == hash('a')
    @test hash(true) == hash(true)
    @test hash(nothing) == hash(nothing)

    # Different values should (almost certainly) have different hashes
    @test hash(1) != hash(2)
    @test hash("hello") != hash("world")

    # isequal contract: isequal(x, y) => hash(x) == hash(y)
    @test hash(1) == hash(1)
    @test hash(0.0) == hash(0.0)
    @test hash(1) == hash(1.0)      # isequal(1, 1.0)
    @test hash(true) == hash(1)     # isequal(true, 1)

    # Two-argument hash for combining: deterministic, and mixing with a
    # non-default seed changes the result
    h1 = hash(1)
    h2 = hash(2, h1)
    @test h2 == hash(2, h1)
    @test h2 != hash(2)
end

true
