# Issue #9185: `@req` is defined in AbstractAlgebra's `Assertions.jl` via
# `@doc raw"…" macro req(cond, msg) … end` and exported in `exports.jl`. Such
# `@doc`-documented macros previously registered only in the lowering context
# (via `add_macro`) and never landed in `module.macros`, so the bundled-package
# macro registry — which reads `module.macros` — did not expose them. As a
# result `@req` was `unknown macro @req` at top level after `using
# AbstractAlgebra`, even though plain `macro`s (e.g. `@attributes`) were exposed.
#
# This is the regression guard for the bundled-package boundary (an inline
# `module … end` already worked via the shared lowering context).
using AbstractAlgebra
using Test

@testset "@doc-defined `@req` is exported via `using AbstractAlgebra` (Issue #9185)" begin
    # A true assertion is a no-op.
    @req true "never thrown"

    # A false assertion throws an ArgumentError carrying the message.
    @test_throws ArgumentError (@req false "boom")

    # Escaped operands resolve in the caller scope.
    x = 5
    @req x > 0 "x must be positive"
    @test_throws ArgumentError (@req x < 0 "x must be negative")
end

true
