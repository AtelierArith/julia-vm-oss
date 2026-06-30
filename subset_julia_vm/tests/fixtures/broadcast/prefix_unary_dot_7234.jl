# Prefix dotted unary operators `.-v`, `.+v`, `.~v` broadcast the corresponding
# unary operator elementwise, matching upstream Julia where `.-v` lowers to
# `broadcast(-, v)` (Issue #7234).

using Test

@testset "prefix broadcast unary operators (Issue #7234)" begin
    v = [1.0, 2.0, 3.0]
    @test .-v == [-1.0, -2.0, -3.0]
    @test .+v == [1.0, 2.0, 3.0]
    @test .-v == broadcast(-, v)
    @test .+v == broadcast(+, v)

    # Integer vector
    w = [1, 2, 3]
    @test .-w == [-1, -2, -3]

    # Precedence: `.^` binds tighter than prefix `.-`, so `.-x .^ 2` is `.-(x .^ 2)`.
    # (Compared against a literal carrying `-0.0` in the middle slot, matching
    # the elementwise result; `.-x .^ 2 == .-(x .^ 2)` cross-checks the grouping.)
    x = -1.0:0.5:1.0
    @test .-x .^ 2 == [-1.0, -0.25, -0.0, -0.25, -1.0]
    @test .-x .^ 2 == .-(x .^ 2)

    # Prefix broadcast bitwise-not on an integer vector (`~` is defined there).
    # `.~` is parsed standalone here; inside a `@test` macro argument the
    # two-token `.~` form is not yet recognized, so this uses a temporary.
    n = [1, 2, 3]
    notn = .~n
    @test notn == [-2, -3, -4]
end

true
