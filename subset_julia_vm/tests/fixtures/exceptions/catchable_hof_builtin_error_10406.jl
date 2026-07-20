using Test

# Issue #10406: a builtin / higher-order-function per-element failure used to
# abort the whole program with an uncatchable "Runtime error: ..." instead of a
# catchable Julia exception that a surrounding try/catch can observe. The cause
# was the VM run loop's terminal error arm returning the error straight out
# instead of routing it through the exception-handler machinery: instruction
# handlers that propagate a catchable error with a bare `?` (e.g. the numeric
# fast paths' `value_to_f64` type check, surfaced directly by `sqrt("a")` and
# per-element by HOFs like `map`/`sum`) bypassed the enclosing handler.
#
# Upstream Julia raises a catchable MethodError here; sjulia's numeric fast
# paths raise a catchable TypeError. The exact exception TYPE (TypeError vs
# MethodError) is a separately tracked parity refinement (Refs #10405, #10461)
# that needs the numeric fast path routed through the dispatch resolver — NOT
# fixed here. This fixture asserts only OBSERVABILITY (the catch runs and
# execution continues), which matches upstream and is parity-clean: both
# interpreters catch an `isa Exception` value and run to completion.

@testset "builtin/HOF per-element failure is catchable, not a hard abort (Issue #10406)" begin
    # Direct builtin numeric type failure (the underlying seam).
    e1 = try
        sqrt("a")
        nothing
    catch e
        e
    end
    @test e1 isa Exception

    # HOF `map`: the callee fails on an element it cannot handle.
    e2 = try
        map(sqrt, ["a", "b"])
        nothing
    catch e
        e
    end
    @test e2 isa Exception

    # HOF `sum(f, itr)`: a different reduction path over the same seam.
    e3 = try
        sum(sqrt, ["a", "b"])
        nothing
    catch e
        e
    end
    @test e3 isa Exception

    # Execution continues normally after the caught failures: a well-typed
    # `map` still evaluates. (If the failures above were uncatchable, the
    # program would have aborted before reaching here.)
    @test map(sqrt, [1.0, 4.0, 9.0]) == [1.0, 2.0, 3.0]
end
println("after")
true
