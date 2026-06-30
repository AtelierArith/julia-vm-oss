# Issue #8042: a bare `sqrt` / `Base.sqrt` call must dispatch to the Base.sqrt
# builtin even when a *foreign* module defines its own `sqrt` generic function
# (e.g. NaNMath's `sqrt(x) = ... Base.sqrt(float(x))`).
#
# sjulia merges such a module-local `sqrt(x::Any)` into the global bare `sqrt`
# method table. Before the fix, a bare `sqrt` applied to an `Any`-typed Float64
# (one produced through a package helper chain rather than a literal) dispatched
# to the foreign `sqrt(::Any)`, whose `Base.sqrt(float(x))` body re-resolved back
# to itself — an unbounded recursion / stack overflow. Literals worked because
# their concrete Float64 type routes straight to the builtin. The recursion also
# bit *inside* the foreign method itself (its own `Base.sqrt(...)` body), so a
# qualified `ShadowMath.sqrt(...)` call must terminate too.
using Test

# Mirror of NaNMath.sqrt: a NEW generic function, NOT an `import Base: sqrt`
# extension. It is intentionally not `using`-imported so a bare `sqrt` elsewhere
# must resolve to Base.sqrt, exactly as in upstream Julia.
module ShadowMath
sqrt(x) = x < zero(x) ? oftype(float(x), NaN) : Base.sqrt(float(x))
end

# A package-style objective wrapper: `value` routes the Float64 through a struct
# field call so the result is `Any`-typed at the `sqrt` call site.
mutable struct Objective
    f
end
value(obj::Objective, x) = obj.f(x)

# Sample variance (divides by N-1), accumulating Any-typed helper values.
function myvar(y)
    n = length(y)
    mu = 0.0
    for yi in y
        mu += yi
    end
    mu = mu / n
    s = 0.0
    for yi in y
        d = yi - mu
        s += d * d
    end
    return s / (n - 1)
end

# √(var(f) · n/m): the Nelder-Mead stopping objective from Optim that triggered
# the crash. The argument to `sqrt` is `Any`-typed.
nmobjective(y, nx, mverts) = sqrt(myvar(y) * (nx / mverts))

@testset "Issue #8042: bare sqrt ignores a foreign module's sqrt" begin
    obj = Objective(x -> sum(abs2, x))
    simplex = [[3.0, -1.0], [3.1, -1.0], [3.0, -0.9]]
    m = length(simplex)
    fvals = Float64[value(obj, simplex[i]) for i in 1:m]

    pval = myvar(fvals) * (2 / 3)
    @test typeof(pval) === Float64

    # The literal already worked; the helper-chain value used to stack-overflow.
    r = sqrt(pval)
    @test abs(r * r - pval) < 1e-12
    @test r == nmobjective(fvals, 2, 3)

    # The foreign module's own sqrt stays reachable when qualified, and its
    # internal `Base.sqrt(...)` must terminate rather than recurse.
    @test ShadowMath.sqrt(16.0) == 4.0
    # A bare `sqrt` still hits the Base builtin for concrete arguments.
    @test sqrt(16.0) == 4.0
end

true
