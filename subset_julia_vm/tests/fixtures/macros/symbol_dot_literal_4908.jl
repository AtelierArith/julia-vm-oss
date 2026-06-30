# Issue #4908: sjulia's parser rejected `:.` and `:...` Symbol
# literals — the Symbols whose names are the dot and ellipsis
# operators. These are the canonical Symbol forms of the field-access
# (`:.`) and splat (`:...`) Expr heads in upstream Julia.
#
# Surfaced while writing fixtures for PR #4903 (field access,
# Issue #4899) and PR #4906 (splat, Issue #4904) — both fixtures
# had to use `Symbol(".")` / `Symbol("...")` as workarounds because
# the colon-literal sugar failed to parse.
#
# Fix: in `subset_julia_vm_parser/src/parser/expressions/primary.rs`,
# add an explicit arm to `parse_colon_prefix` for `Token::Dot` and
# `Token::Ellipsis` so they produce `QuoteExpression` leaves like
# every other operator-name Symbol literal. These tokens are
# deliberately not in `Token::is_operator()` (they have grammatical
# meaning as field-access / splat markers), so the existing
# operator-arm doesn't pick them up.

using Test

@testset "`:.` is the Symbol \".\" (Issue #4908)" begin
    @test typeof(:.) === Symbol
    @test string(:.) == "."
    @test :. === Symbol(".")
end

@testset "`:...` is the Symbol \"...\" (Issue #4908)" begin
    @test typeof(:...) === Symbol
    @test string(:...) == "..."
    @test :... === Symbol("...")
end

@testset "`:.` / `:...` interoperate with quoted Expr heads (Issue #4908)" begin
    # The reason these Symbol literals matter: they're the heads of
    # field-access and splat Exprs produced by quote lowering (PRs
    # #4903 / #4906). Now that the colon-literal sugar works, the
    # head can be asserted without the `Symbol(name)` workaround.
    field_ex = :(a.b)
    @test field_ex.head === :.

    splat_ex = :(f(args...))
    @test splat_ex.args[2].head === :...
end

@testset "other operator-name Symbol literals stay intact (regression)" begin
    # The existing `:operator` arm (operators in
    # `Token::is_operator()`) must continue to work — the new
    # `Token::Dot | Token::Ellipsis` arm is additive, not a
    # replacement.
    @test :+ === Symbol("+")
    @test :- === Symbol("-")
    @test :* === Symbol("*")
    @test :/ === Symbol("/")
    @test :(==) === Symbol("==")
    @test :% === Symbol("%")
end

true
