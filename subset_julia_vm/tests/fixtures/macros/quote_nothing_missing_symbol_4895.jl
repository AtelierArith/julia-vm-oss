# Issue #4895: `:(nothing)` and `:(missing)` quote to the Symbols
# `:nothing` / `:missing` (not the literal `nothing` / `missing`
# values), matching upstream Julia.
#
# `nothing` / `missing` are ordinary identifiers in the Julia AST, so
# quoting them yields a Symbol. They only become the actual values when
# the quoted Expr is later evaluated. By contrast `true` / `false` are
# literal Bool AST nodes, so `:(true)` quotes back to the `true` value.
#
# The regression that historically gated this fix: the Pure-Julia
# `@test` macro ends its `quote ... end` block with a bare `nothing`.
# When the quoted block is converted back into executable code during
# macro expansion, the trailing `:nothing` Symbol must resolve to the
# `nothing` value rather than raising `UndefVarError`. The
# value-keyword handling in `quote/code_generation.rs` covers that, so
# the `@test`-based assertions below double as the regression guard.

using Test

@testset "nothing / missing quote to Symbols (Issue #4895)" begin
    # The quoted forms are the Symbols, not the values.
    @test :(nothing) === :nothing
    @test :(missing) === :missing
    @test :(nothing) isa Symbol
    @test :(missing) isa Symbol

    # They are NOT the literal values.
    @test :(nothing) !== nothing
    @test :(missing) !== missing
end

@testset "true / false remain literal Bool nodes (Issue #4895)" begin
    # Contrast: true / false ARE literal Bool AST nodes.
    @test :(true) === true
    @test :(false) === false
    @test :(true) isa Bool
end

@testset "nothing / missing inside quoted Expr args (Issue #4895)" begin
    # Inside a larger quoted Expr they appear as Symbols in args.
    ex = :(f(nothing))
    @test ex.args[2] === :nothing

    ex2 = :(g(missing))
    @test ex2.args[2] === :missing
end

# The fact that every `@test` above ran (and the @testset summaries
# printed) exercises the `@test` macro's trailing-`nothing` block,
# which is the macro-expansion-scope regression guard for #4895.

true
