# Test: map(f, arr) correctly dispatches when f is an untyped function parameter
# Bug: When f has compile-time type Any (untyped parameter), CallTypedDispatch
# failed to match abstract supertypes like "Any" against parametric actual types
# like "Vector{Int64}", causing wrong method selection (Issue #2119)

using Test

function expand_pair(n)
    return [n, n * 10]
end

# Top-level map - always worked
r1 = map(expand_pair, [1, 2, 3])

# Typed f::Function parameter - always worked
function test_typed(f::Function, arr)
    return map(f, arr)
end
r2 = test_typed(expand_pair, [1, 2, 3])

# Untyped f parameter - was buggy (Issue #2119)
function test_untyped(f, arr)
    return map(f, arr)
end
r3 = test_untyped(expand_pair, [1, 2, 3])

@testset "map dispatch with untyped function parameter (Issue #2119)" begin
    # All three should produce the same result: [[1,10], [2,20], [3,30]]
    @test length(r1) == 3
    @test length(r2) == 3
    @test length(r3) == 3

    # Check the structure of each inner array
    @test r1[1][1] == 1
    @test r1[1][2] == 10
    @test r1[2][1] == 2
    @test r1[2][2] == 20
    @test r1[3][1] == 3
    @test r1[3][2] == 30

    # Typed parameter should match top-level
    @test r2[1][1] == 1
    @test r2[1][2] == 10
    @test r2[2][1] == 2
    @test r2[2][2] == 20
    @test r2[3][1] == 3
    @test r2[3][2] == 30

    # Untyped parameter should also match top-level (the bug fix)
    @test r3[1][1] == 1
    @test r3[1][2] == 10
    @test r3[2][1] == 2
    @test r3[2][2] == 20
    @test r3[3][1] == 3
    @test r3[3][2] == 30
end

true
