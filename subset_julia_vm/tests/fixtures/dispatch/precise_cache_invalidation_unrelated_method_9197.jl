# Issue #9197 S6: precise per-name dispatch-cache invalidation. Redefining one
# generic function at eval time must (a) update the warmed dynamic-dispatch site
# for THAT function and (b) leave an unrelated warmed dynamic-dispatch site
# untouched and correct. The observable output is identical under precise or
# whole-clear invalidation; this fixture guards against a precise-invalidation
# bug returning a stale target for the wrong generic function.
#
# Verified against upstream Julia (julia --startup-file=no): prints 40, 4, 40,
# 4, 220, 4, 1, 2.

using Test

pa9197(x::Int) = 10
pa9197(x::Float64) = 20
pb9197(x::Int) = 1
pb9197(x::Float64) = 2

function suma9197(xs)
    s = 0
    for x in xs
        s += pa9197(x)
    end
    s
end

function sumb9197(xs)
    s = 0
    for x in xs
        s += pb9197(x)
    end
    s
end

xs9197 = Any[1, 2.0, 1]

@testset "precise dispatch-cache invalidation of unrelated methods (Issue #9197 S6)" begin
    # Warm both dynamic-dispatch sites twice so their call-site caches are
    # certainly filled before the redefinition.
    @test suma9197(xs9197) == 40
    @test sumb9197(xs9197) == 4
    @test suma9197(xs9197) == 40
    @test sumb9197(xs9197) == 4

    # Redefine only pa9197(::Int). The warmed pa9197 site must observe the new
    # method; the warmed pb9197 site must be completely unaffected.
    @eval pa9197(x::Int) = 100

    @test suma9197(xs9197) == 220   # 100 + 20 + 100
    @test sumb9197(xs9197) == 4     # unchanged
    @test pb9197(1) == 1
    @test pb9197(2.0) == 2
end

true
