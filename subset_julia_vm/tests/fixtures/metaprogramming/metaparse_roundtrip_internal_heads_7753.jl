using Test

# Issue #7753: `Meta.parse(src)` / `string(::Expr)` round-trip used to emit
# parser-internal (non-upstream) AST heads for three source forms:
#   * `var"@q"` round-tripped as `Expr(:prefixedstringliteral, var, "@q")`
#     instead of the upstream `Symbol("@q")` (printed back as `var"@q"`).
#   * keyword args round-tripped as `Expr(:keywordargument, a, 2)` instead of
#     `Expr(:kw, :a, 2)` (printed as `a = 2`).
#   * `:a` in pair position dropped its quote so `Dict(:a => 1)` printed as
#     `Dict(a => 1)` instead of `Dict(:a => 1)` (the `:a` must stay a QuoteNode).
#
# Expected strings below were verified against upstream `julia` 1.12.

@testset "Meta.parse roundtrip: upstream-shaped heads (Issue #7753)" begin
    # The three sources from the issue MWE.
    @test string(Meta.parse("Dict(:a => 1)")) == "Dict(:a => 1)"
    @test string(Meta.parse("(var\"@q\", var\"@qq\", postwalk)")) ==
          "(var\"@q\", var\"@qq\", postwalk)"
    @test string(Meta.parse("kw7720(a=2, b=3)")) == "kw7720(a = 2, b = 3)"
end

@testset "Meta.parse: var\"...\" parses to a Symbol (Issue #7753)" begin
    # var"name" is the non-standard identifier syntax: it is a Symbol, not an
    # Expr / string literal.
    @test Meta.parse("var\"@q\"") == Symbol("@q")
    @test Meta.parse("var\"foo bar\"") == Symbol("foo bar")
    @test typeof(Meta.parse("var\"@q\"")) == Symbol
end

@testset "Meta.parse: keyword arg head is :kw (Issue #7753)" begin
    e = Meta.parse("f(a=2)")
    @test Meta.isexpr(e, :call)
    kw = e.args[2]
    @test Meta.isexpr(kw, :kw)
    @test kw.args[1] == :a
    @test kw.args[2] == 2
end

@testset "Meta.parse: pair keeps its quoted symbol (Issue #7753)" begin
    # :x parses to a QuoteNode (matches upstream `Meta.parse(":x")`).
    @test typeof(Meta.parse(":x")) == QuoteNode
    @test Meta.parse(":x") == QuoteNode(:x)
    # The quoted symbol survives in pair position so display keeps the `:`.
    @test occursin(":a", string(Meta.parse("Dict(:a => 1)")))
end

true
