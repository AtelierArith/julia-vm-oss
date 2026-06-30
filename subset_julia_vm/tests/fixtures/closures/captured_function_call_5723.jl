using Test

# Issue #5723: a closure could not CALL a variable captured from an enclosing
# scope that holds a function value — `makeapply(f) = x -> f(x)` errored with
# "Unknown function: f". Three gaps: (1) the free-variable analysis ignored the
# call-target name, so `f` was never captured; (2) the call compiler did not
# treat a captured name in function position as a callable; (3) the variable
# call loaded the name with `LoadAny` instead of `LoadCaptured`, so even once
# captured it was "not defined" at runtime. Capturing non-function values
# already worked and must keep working.

@testset "closure calls a captured function variable (Issue #5723)" begin
    # The motivating case
    makeapply(f) = x -> f(x)
    g = makeapply(iseven)
    @test g(4) == true
    @test g(3) == false

    # Capture a builtin and call it
    function mk(f)
        return x -> f(x)
    end
    @test mk(abs)(-3) == 3

    # Named nested function capturing a function
    function mk_named(f)
        h(x) = f(x)
        return h
    end
    @test mk_named(abs)(-7) == 7

    # Capture and immediately call (zero-arg closure)
    function applyto(f, val)
        return () -> f(val)
    end
    @test applyto(sqrt, 16.0)() == 4.0

    # Compose: capture two function variables
    compose(f, g) = x -> f(g(x))
    @test compose(x -> x * 2, x -> x + 1)(3) == 8
    @test compose(abs, y -> y - 10)(3) == 7

    # Captured function used inside a higher-order call
    mapper(f) = arr -> map(f, arr)
    @test mapper(x -> x * 2)([1, 2, 3]) == [2, 4, 6]

    # Capturing a non-function value still works (regression guard)
    adder(n) = x -> x + n
    @test adder(5)(10) == 15
end

true
