# Issue #4920: refines the meta-quote lowering from PR #4914 (Issue
# #4911) to match upstream Julia's `QuoteNode` vs `Expr(:quote, ...)`
# discrimination:
#
# - Inner is an atom (Symbol / operator / numeric literal / string
#   / char / bool) → `QuoteNode(atom)` (unchanged from #4911).
# - Inner is a complex Expr (Call, BinaryExpression, etc.) →
#   `Expr(:quote, complex_expr)` (the refinement this PR adds).
#
# Pre-#4920 the with-children form always emitted `QuoteNode(...)`,
# even for complex inner expressions where upstream returns an
# `Expr` with `head === :quote`. Macros that pattern-match on
# `Expr(:quote, ...)` will now see the upstream shape.

using Test

@testset "atom meta-quote still produces QuoteNode (Issue #4920)" begin
    # `:(:foo)` and `:(:%)` are leaf-form meta-quotes — already
    # produce QuoteNode from PR #4914 (Issue #4911). Pinned here as
    # regression guards so the refinement doesn't break them.
    @test :(:foo) isa QuoteNode
    @test :(:%) isa QuoteNode
end

@testset "complex Expr meta-quote produces Expr(:quote, ...) (Issue #4920)" begin
    ex = :(:(x + y))
    @test ex isa Expr
    @test ex.head === :quote
    @test length(ex.args) == 1
    @test ex.args[1] isa Expr   # the inner :(x+y) is itself an Expr
end

@testset "meta-quote of a call (Issue #4920)" begin
    ex = :(:(f(a, b)))
    @test ex isa Expr
    @test ex.head === :quote
    inner = ex.args[1]
    @test inner isa Expr
    @test inner.head === :call
end

@testset "meta-quote of a tuple (Issue #4920)" begin
    ex = :(:((a, b, c)))
    @test ex isa Expr
    @test ex.head === :quote
    @test ex.args[1] isa Expr   # tuple is its own Expr
    @test ex.args[1].head === :tuple
end

true
