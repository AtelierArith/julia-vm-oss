using Test

# Issue #5494: in space-separated macro arguments, a space before `(` separates
# arguments instead of fusing into a call. So `@m Ident (expr)` is two arguments
# (`Ident` and `(expr)`), matching upstream Julia, NOT one call `Ident(expr)`.
#
# The canonical failure was `@test_throws TypeError (1 + 1)::Float64`, which was
# (mis)parsed as the single argument `(TypeError(1 + 1))::Float64`. The macro
# then saw only one argument and failed. With the fix it parses as the expected
# two arguments (`TypeError` and `(1 + 1)::Float64`) and the typed assert below
# correctly throws a TypeError.

@testset "macros_macro_arg_space_paren_5494 typed throws" begin
    # `(1 + 1)::Float64` is `2::Float64`, which throws a TypeError because the
    # Int value 2 is not a Float64. `@test_throws` must receive TypeError as its
    # first argument and the typed expression as its second.
    @test_throws TypeError (1 + 1)::Float64

    # A plain parenthesized expression as the second argument, with a space
    # before `(`, must still be a separate argument (not a call `TypeError(...)`).
    @test_throws TypeError (10 + 5)::Float64
end

@testset "macros_macro_arg_space_paren_5494 no-space call still fuses" begin
    # Without a space, `error("boom")` is a single call argument and is invoked,
    # so `@test_throws` sees the thrown ErrorException. This pins that the fix
    # does NOT break adjacent macro-argument calls.
    @test_throws ErrorException error("boom")
end

true
