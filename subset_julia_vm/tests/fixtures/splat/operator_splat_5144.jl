# Issue #5144: operator-function calls with splat / multi-arg parentheses.
# `+(xs...)`, `*(xs...)`, partial splat `+(0, xs...)`, and literal multi-arg
# `+(1, 2, 3)` must dispatch to the operator as a function, matching Julia.

using Test

@testset "operator function splat (Issue #5144)" begin
    xs = [1, 2, 3]

    # Bare operator-function calls with array splat
    @test +(xs...) == 6
    @test *(xs...) == 6

    # Partial splat: literal head + splatted tail
    @test +(0, xs...) == 6
    @test *(1, xs...) == 6

    # Tuple splat
    t = (4, 5, 6)
    @test +(t...) == 15
    @test *(t...) == 120

    # Literal multi-arg operator calls (no splat)
    @test +(1, 2, 3) == 6
    @test *(2, 3, 4) == 24
    @test -(10, 3) == 7

    # Type preservation for float splat
    fs = [1.0, 2.0, 3.0]
    @test +(fs...) === 6.0
    @test typeof(*(fs...)) === Float64

    # Single-element splat
    one = [42]
    @test +(one...) == 42
end

true
