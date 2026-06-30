# Exact mixed Int64/Float64 comparisons (Issue #8187)
#
# `==` / `!=` / `<` / `<=` / `>` / `>=` between an Int64 and a Float64 must be
# value-based, NOT promote-the-integer-to-Float64-then-compare. For integers
# above 2^53 the promotion rounds and silently changed the answer. This must hold
# for statically-typed operands, dynamically-typed (array-element) operands, and
# the `isequal` / `in` / tuple-`==` surfaces.

using Test

@testset "exact mixed Int64/Float64 comparisons (Issue #8187)" begin
    x = 9007199254740993            # 2^53 + 1
    f = 9.007199254740992e15        # Float64(2^53)

    # The classic failure: 2^53+1 must NOT equal Float64(2^53).
    @test (x == f) == false
    @test (f == x) == false
    @test (x != f) == true
    @test (x <= f) == false
    @test (x >= f) == true
    @test (x < f) == false
    @test (x > f) == true
    @test (f < x) == true
    @test (f >= x) == false      # f == 2^53 < x == 2^53+1

    # A case where naive widening rounds UP (2^53+3 -> 2^53+4).
    a = 9007199254740995            # 2^53 + 3
    g = 9.007199254740996e15        # 2^53 + 4
    @test (a < g) == true
    @test (a <= g) == true
    @test (a == g) == false
    @test (g > a) == true

    # typemax(Int64) vs Float64(typemax) == 2.0^63 (rounds up, so int < float).
    @test (typemax(Int64) == 9.223372036854776e18) == false
    @test (typemax(Int64) < 9.223372036854776e18) == true

    # Small/exact values keep working.
    @test (1 == 1.0) == true
    @test (2 == 2.5) == false
    @test (2 < 2.5) == true

    # Dynamically-typed operands (Vector{Int64} / Vector{Float64} elements go
    # through the VM's dynamic binary-dispatch path, not the typed fast path).
    ints = [x, a, 1, 2]
    flts = [f, g, 1.0, 2.5]
    @test (ints[1] == flts[1]) == false
    @test (ints[1] <= flts[1]) == false
    @test (ints[1] > flts[1]) == true
    @test (ints[2] < flts[2]) == true
    @test (ints[3] == flts[3]) == true
    @test (flts[1] == ints[1]) == false

    # isequal: value-based AND signed-zero aware (an integer is +0).
    @test isequal(x, f) == false
    @test isequal(1, 1.0) == true
    @test isequal(0, 0.0) == true
    @test isequal(0, -0.0) == false

    # tuple `==` and membership share the same exact comparison.
    @test ((x,) == (f,)) == false
    @test ((1,) == (1.0,)) == true
    @test (x in [f]) == false
    @test (2 in [1.0, 2.0, 3.0]) == true
end

true  # Test passed
