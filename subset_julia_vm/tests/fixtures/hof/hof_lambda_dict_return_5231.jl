# HOF lambda returning a non-array value (Issue #5231)
#
# `map(i -> Dict(i => i*i), [1,2,3])` (and other HOFs whose closure returns a
# Dict / Set / Tuple / NamedTuple) used to raise
#   Type error: ReturnArray: expected Array or TypedArray, got "Dict"
# The closure runs through the broadcast-HOF driver, but its typed return
# instruction (`ReturnDict` / `ReturnTuple` / `ReturnNamedTuple`, and the
# `ReturnArray` hint when the inferred element type was an array) did not route
# the value back through the HOF/generator continuation. The value therefore
# leaked past the driver and `map`'s own `ReturnArray` rejected it. The typed
# return handlers now funnel through the same continuation machinery as
# `ReturnAny`, so non-scalar closure results are collected correctly.
#
# Verified against upstream Julia 1.12.

using Test

@testset "HOF lambda returning Dict/Set/containers (Issue #5231)" begin
    # map lambda returning a Dict — the core repro
    @test map(i -> Dict(i => i * i), [1, 2, 3]) == [Dict(1 => 1), Dict(2 => 4), Dict(3 => 9)]

    # map lambda returning a Dict over a range
    @test map(x -> Dict(x => x + 1), 1:2) == [Dict(1 => 2), Dict(2 => 3)]

    # map lambda returning a Tuple
    @test map(x -> (x, x * x), [1, 2, 3]) == [(1, 1), (2, 4), (3, 9)]

    # map lambda returning a NamedTuple
    @test map(x -> (a = x, b = x * x), [1, 2, 3]) ==
          [(a = 1, b = 1), (a = 2, b = 4), (a = 3, b = 9)]

    # map lambda returning a Set (Set `==` is order-independent)
    @test map(x -> Set([x, x + 1]), [1, 2]) == [Set([1, 2]), Set([2, 3])]

    # Set element counts are order-independent
    @test map(x -> length(Set([x, x, x + 1])), [1, 2, 3]) == [2, 2, 2]

    # filter feeding a map that returns Dicts
    @test map(x -> Dict(x => x), filter(x -> x > 1, [1, 2, 3])) == [Dict(2 => 2), Dict(3 => 3)]

    # nested container: a Dict whose value is itself a Tuple
    @test map(x -> Dict(x => (x, x)), [1, 2]) == [Dict(1 => (1, 1)), Dict(2 => (2, 2))]

    # single-element map returning a Dict (collect-with-first path)
    @test map(x -> Dict(x => x * x), [5]) == [Dict(5 => 25)]

    # regression guards: scalar and string closure returns still work
    @test map(x -> x * x, [1, 2, 3]) == [1, 4, 9]
    @test map(x -> string(x), [1, 2, 3]) == ["1", "2", "3"]
end

true
