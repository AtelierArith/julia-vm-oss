# Closure-factory captured variable vs. parameter/global name collision (Issue #8080).
#
# Regression for the capture bug behind Optim's W-42: an objective bound to a
# variable literally named `f` and threaded through a call chain that ALSO binds
# `f` as a parameter, where a module-private closure factory returns a closure
# capturing `f` (built from inside an inner-constructor body) and that closure is
# invoked deep inside a nested line-search-style loop.
#
# Three distinct `f` bindings coexist: the global `f`, the `optimize`/factory
# parameter `f`, and the closure's captured `f`. The captured value must resolve
# to the objective each binding actually refers to — never the wrong frame's `f`.
# Pure Julia, no Optim dependency; identical under upstream julia and sjulia.

using Test

module CaptureProbe

# Module-private closure factory: returns a closure that captures `f` and `h`.
function _grad(f)
    h = 0.5
    return function (G, x)
        for i in eachindex(x)
            G[i] = (f(x .+ h) - f(x .- h)) / (2 * h)
        end
        return G
    end
end

# Objective wrapper whose INNER CONSTRUCTOR body calls the module-private factory.
mutable struct Obj
    f
    g!
    calls::Int
    function Obj(f)
        return new(f, _grad(f), 0)
    end
end

value(o::Obj, x) = (o.calls += 1; o.f(x))
grad!(o::Obj, G, x) = o.g!(G, x)

# A line-search-style driver that invokes a captured closure through try/catch
# inside a nested loop (mirrors hagerzhang's `phidphi` usage).
function _run_search(phidphi, x0, steps)
    acc = 0.0
    try
        for k in 1:steps
            v, g = phidphi(x0 .+ Float64(k))
            acc += v + g
        end
    catch e
        rethrow(e)
    end
    return acc
end

# `optimize`'s objective parameter is ALSO named `f` (collides with the caller's
# global `f` and with the factory parameter `f`).
function optimize(f, x0, steps)
    d = Obj(f)
    G = zeros(Float64, length(x0))
    phidphi = function (xls)
        fv = value(d, xls)
        grad!(d, G, xls)
        return fv, G[1]
    end
    return _run_search(phidphi, x0, steps), d.calls
end

end # module CaptureProbe

@testset "closure factory captured `f` vs parameter/global `f` (Issue #8080)" begin
    # GLOBAL variable literally named `f`.
    f = x -> sum(xi -> xi^2, x)
    acc, ncalls = CaptureProbe.optimize(f, [1.0, 2.0], 3)
    # value(d, x) = sum(x.^2); the captured `f` must resolve to it (not the wrong
    # frame's `f`), giving a fixed analytic constant for x0=[1,2], steps k=1..3.
    @test acc == 121.0
    @test ncalls == 3

    # A differently-named global must give the identical result.
    myf = x -> sum(xi -> xi^2, x)
    acc2, ncalls2 = CaptureProbe.optimize(myf, [1.0, 2.0], 3)
    @test acc2 == acc
    @test ncalls2 == ncalls
end

true
