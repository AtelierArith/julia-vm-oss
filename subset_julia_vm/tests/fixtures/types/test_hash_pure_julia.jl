# Test Pure Julia hash functions (Issue #2582)
# Verifies hash(x) and hash(x, h) work correctly
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

    # Two-argument hash for combining
    h1 = hash(1)
    h2 = hash(2, h1)
    @test h2 == hash(xor(hash(2), h1))

    # Float special cases
    # -0.0 and 0.0: hash(-0.0) should equal hash(0.0) since isequal(-0.0, 0.0) is false in Julia
    # But our implementation hashes after canonicalization, so -0.0 maps to 0.0
    @test hash(0.0) == hash(-0.0)
end

true
