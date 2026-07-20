# Test: two textually-identical Ref(x) constructors allocate independent,
# mutable cells and must NOT be CSE-aliased (Issue #9323 — sibling audit of the
# #9270 RNG-constructor bug; same class as #7176 zeros/ones and #5130 Ref).
#
# Before the fix, `BuiltinOp::Ref` fell into `infer_builtin_op_effects`'s
# `_ => pure_arithmetic()` default, so the straight-line CSE pass value-numbered
# the second `Ref(0)` into a reuse of the first cell. `r2[] = 20` then clobbered
# `r1[]` (sjulia printed 20 for r1[]; upstream prints 10).

using Test

@testset "Ref constructors are independent (Issue #9323)" begin
    r1 = Ref(0)
    r2 = Ref(0)
    r1[] = 10
    r2[] = 20
    @test r1[] == 10
    @test r2[] == 20
    @test r1 !== r2

    # Independence also holds inside a function body (the optimizer runs
    # per-function, so CSE there must likewise leave two allocations).
    function two_refs()
        a = Ref(1)
        b = Ref(1)
        a[] = 100
        b[] = 200
        (a[], b[])
    end
    @test two_refs() == (100, 200)
end

true  # Test passed
