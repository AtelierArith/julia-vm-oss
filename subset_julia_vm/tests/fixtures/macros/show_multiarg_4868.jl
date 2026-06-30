# Issue #4868: `@show x y z` (multiple arguments) failed at lowering
# with "Base macro @show not found (with 3 args)" because the
# `macro show(ex)` in `base/macros.jl` was single-arg only.
#
# Fix: change to `macro show(exs...)`, looping over the arguments and
# emitting one `_do_show(expr_str, value)` per arg (mirroring upstream
# `julia/base/show.jl`), returning the value of the last argument.
#
# Like the single-arg regression fixture (#4865), capturing the
# multi-line stdout is not portable across sjulia/julia, so we anchor
# on the documented contract: `@show a b c` returns the value of the
# last argument, and each argument's value is returned unchanged when
# shown alone.

using Test

@testset "@show with multiple arguments returns last value (Issue #4868)" begin
    x_4868 = 1
    y_4868 = 2
    z_4868 = 3
    @test (@show x_4868 y_4868 z_4868) == 3
end

@testset "@show single-arg regression still works (Issue #4868)" begin
    # Bind to a local first so the `@show` stdout line is `name = 42`
    # rather than a bare `42 = 42`, which the fixture-parity helper's
    # awk fallback would otherwise misread as a testset summary row
    # (same guard as the #4865 fixture).
    forty_two_4868 = 42
    @test (@show forty_two_4868) == 42
    @test (@show "hi") == "hi"
end

@testset "@show two args returns last (Issue #4868)" begin
    a_4868 = 10
    b_4868 = 20
    @test (@show a_4868 b_4868) == 20
end

true
