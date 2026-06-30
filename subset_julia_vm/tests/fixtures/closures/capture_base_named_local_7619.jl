# Issue #7619: a closure that captures and mutates an enclosing local whose name
# collides with a `Base` function (e.g. `count`, `sum`, `length`) used to
# mis-resolve the name to the `Base` FunctionValue instead of the captured slot.
# The `Expr::Var` read path now resolves a captured closure variable (via
# `LoadCaptured`) BEFORE the Base-function / type-name / module-alias checks.
#
# Issue #7759: a NAMED inner-function closure must also *box* such a
# captured-and-reassigned local so the accumulator mutates the shared binding
# (Core.Box semantics), not a value snapshot. The boxing pass
# (lowering/closure_box.rs) previously excluded capture-on-assign names from its
# `assigned_captures` analysis, so `make_counter()` returned 0,0,0 instead of
# 1,2,3.
#
# NOTE: the final expression below is a *genuine* regression guard — it is `false`
# (so the fixture FAILS) if any accumulator returns a wrong value. The previous
# version ended in a bare `true`, which let `@testset` failures be swallowed and
# the fixture pass even when every assertion failed.

using Test

# Named inner-function accumulator over a `Base`-named local.
function make_counter()
    count = 0
    function pred()
        count = count + 1
        count
    end
    pred
end

# `sum` also collides with a Base function.
function make_sum()
    sum = 0
    function acc(x)
        sum = sum + x
        sum
    end
    acc
end

# The arrow-lambda form of the same capture-and-mutate.
function make_length()
    length = 100
    () -> (length = length - 1; length)
end

# A non-Base name — regression guard so the fix does not over-narrow.
function make_zzz()
    zzz = 0
    function pred()
        zzz = zzz + 1
        zzz
    end
    pred
end

p = make_counter()
count_ok = (p() == 1) && (p() == 2) && (p() == 3)

a = make_sum()
sum_ok = (a(10) == 10) && (a(5) == 15) && (a(100) == 115)

g = make_length()
length_ok = (g() == 99) && (g() == 98)

q = make_zzz()
zzz_ok = (q() == 1) && (q() == 2)

@testset "closure captures + mutates an enclosing local (Issues #7619 / #7759)" begin
    @test count_ok
    @test sum_ok
    @test length_ok
    @test zzz_ok
end

# Genuine regression guard: `false` (=> fixture fails) if any accumulator is wrong.
count_ok && sum_ok && length_ok && zzz_ok
