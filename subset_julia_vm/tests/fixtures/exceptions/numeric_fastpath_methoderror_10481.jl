using Test

# Issue #10481: a builtin numeric fast path (SqrtF64 / FloorF64 / CeilF64 /
# CallBuiltin(Round|Trunc|Sqrt) / the runtime intrinsic fallback used by HOFs)
# whose operand fails the numeric check used to raise the internal conversion
# TypeError ("expected numeric value, got ..."). Upstream Julia has no method
# for these calls, so the same failure is a dispatch miss: a catchable
# MethodError. #10406 already made the failure catchable; this fixture pins
# the exception TYPE to MethodError like upstream (Refs #10461, #10405).

@testset "numeric fast-path failures raise MethodError like upstream (Issue #10481)" begin
    # Direct calls: each compiles to a distinct fast path (SqrtF64, FloorF64,
    # CeilF64, CallBuiltin(Round)).
    for f in (sqrt, floor, ceil, round)
        e = try
            f("a")
            nothing
        catch err
            err
        end
        @test typeof(e) == MethodError
    end

    # Dynamic in-function calls (CallBuiltin(Sqrt) / CallDynamicOrBuiltin).
    g(x) = sqrt(x)
    h(x) = floor(x)
    e1 = try
        g("a")
        nothing
    catch err
        err
    end
    @test typeof(e1) == MethodError
    e2 = try
        h("a")
        nothing
    catch err
        err
    end
    @test typeof(e2) == MethodError

    # HOF per-element failures (the runtime callable fallback).
    e3 = try
        map(sqrt, ["a"])
        nothing
    catch err
        err
    end
    @test typeof(e3) == MethodError
    e4 = try
        sum(sqrt, ["a"])
        nothing
    catch err
        err
    end
    @test typeof(e4) == MethodError

    # Broadcast routes through the same per-element callable seam.
    e5 = try
        sqrt.(["a"])
        nothing
    catch err
        err
    end
    @test typeof(e5) == MethodError

    # The remap must not swallow the numeric behavior of the same fast paths:
    # DomainError for negative real sqrt, and plain results elsewhere.
    e6 = try
        sqrt(-1.0)
        nothing
    catch err
        err
    end
    @test typeof(e6) == DomainError
    @test sqrt(4.0) == 2.0
    @test floor(3.7) == 3.0
    @test ceil(3.2) == 4.0
    @test round(2.5) == 2.0
    @test trunc(-3.7) == -3.0
    @test map(sqrt, [1.0, 4.0, 9.0]) == [1.0, 2.0, 3.0]
end
println("done")
true
