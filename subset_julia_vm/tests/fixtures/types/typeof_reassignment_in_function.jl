# Test dynamic typing across reassignments inside a function
# Bug: x = 2 after x = 1.0 should have typeof(x) == Int64, not Float64
# This tests Julia's dynamic typing inside functions using isa()

using Test

function f(x)
    2x + 1
end

function g()
    x = 1
    t1_ok = isa(x, Int64)
    fx1_ok = isa(f(x), Int64)

    x = 1.0
    t2_ok = isa(x, Float64)
    fx2_ok = isa(f(x), Float64)

    x = 2
    t3_ok = isa(x, Int64)           # Should be true (was incorrectly Float64)
    fx3_ok = isa(f(x), Int64)       # Should be true

    # Return true if all tests pass
    t1_ok && t2_ok && t3_ok && fx1_ok && fx2_ok && fx3_ok
end

@testset "Function-level dynamic typing: x=1->x=1.0->x=2 with typeof(x)==Int64 and f(x) preserves type" begin



    @test (g())
end

true  # Test passed
