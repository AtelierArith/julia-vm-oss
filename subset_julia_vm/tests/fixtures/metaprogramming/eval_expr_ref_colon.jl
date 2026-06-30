using Test

# A colon index inside a quoted `:ref` must round-trip to the `:` slice marker,
# not an undefined Symbol `:` (Issue #7312). Upstream Julia binds the global `:`
# to `Colon()`, so a quoted `a[:, j]` slices instead of throwing
# `getindex(::Matrix, ::Symbol, ::Int)`. Non-colon indices in quoted `:ref`
# were fixed in #7275; this is the colon gap. Note: eval runs in module (Main)
# scope, so the container Symbol must resolve to a top-level global, not a
# @testset/let local. (The standalone `Colon` type/constructor is a separate
# unsupported feature and is intentionally not exercised here.)
a = [10, 20, 30]
m = [1 2; 3 4]

@testset "colon index in quoted :ref (Issue #7312)" begin
    # bare colon (vector and matrix flatten)
    @test eval(:(a[:])) == [10, 20, 30]
    @test eval(:(m[:])) == [1, 3, 2, 4]
    # column slices
    @test eval(:(m[:, 1])) == [1, 3]
    @test eval(:(m[:, 2])) == [2, 4]
    # row slices
    @test eval(:(m[1, :])) == [1, 2]
    @test eval(:(m[2, :])) == [3, 4]
    # explicit Colon symbol in Expr(:ref, ...)
    @test eval(Expr(:ref, :m, Symbol(":"), 1)) == [1, 3]
    @test eval(Expr(:ref, :m, 1, Symbol(":"))) == [1, 2]
    # regression: direct (non-quote) colon indexing still works
    @test m[:, 1] == [1, 3]
    @test m[1, :] == [1, 2]
end

# The nextest harness only checks the FINAL value == expected(true), and a
# failing bare @test does not abort, so the final expression must be a boolean
# conjunction of the checks (Issue #5932 / #7312).
eval(:(a[:])) == [10, 20, 30] &&
    eval(:(m[:])) == [1, 3, 2, 4] &&
    eval(:(m[:, 1])) == [1, 3] &&
    eval(:(m[:, 2])) == [2, 4] &&
    eval(:(m[1, :])) == [1, 2] &&
    eval(:(m[2, :])) == [3, 4] &&
    eval(Expr(:ref, :m, Symbol(":"), 1)) == [1, 3] &&
    eval(Expr(:ref, :m, 1, Symbol(":"))) == [1, 2] &&
    m[:, 1] == [1, 3] &&
    m[1, :] == [1, 2]
