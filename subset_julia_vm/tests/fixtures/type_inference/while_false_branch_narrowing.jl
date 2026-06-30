# Test: post-`while` env should reflect the false-branch narrowing of the
# loop condition (Issue #3517). After `while x !== nothing` exits normally,
# `x` must be narrowed to `Nothing`.

using Test

# `x` enters as Union{Int,Nothing} (untyped param). Loop exits only when
# `x === nothing`, so the post-loop `x` must be the nothing path.
function f(x)
    while x !== nothing
        x = nothing
    end
    x
end

# Already-nothing path — loop body never runs, but the post-loop `x` is
# still `nothing`.
function g()
    x = nothing
    y = 0
    while x !== nothing
        x = nothing
        y = 1
    end
    # After loop, x is narrowed to Nothing. Returning x demonstrates the
    # narrowed type is consistent and y is unmodified.
    (x, y)
end

@testset "While-loop false branch narrowing (Issue #3517)" begin
    @test f(1) === nothing
    @test f(nothing) === nothing
    @test g() === (nothing, 0)
end

true  # Test passed
