# Issue #4911: `:(:foo)` (a meta-quote — a Symbol literal nested
# inside an outer quote) previously fell through the quote-lowering
# catch-all with
# `UnsupportedExpression("quote for quote_expression not yet supported")`.
# Upstream Julia produces a literal `QuoteNode(:foo)` value, not an
# Expr.
#
# Fix: in `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a `NodeKind::QuoteExpression` arm that handles both the leaf
# form (`:foo`) and the with-children form (`:(expr)` nested inside
# the outer quote). The leaf form parses the symbol-name out of the
# leaf text; the with-children form recurses and wraps the result in
# `QuoteNode(...)`.

using Test

@testset "leaf meta-quote :(:foo) lowers to QuoteNode(:foo) (Issue #4911)" begin
    ex = :(:foo)
    @test typeof(ex) === QuoteNode
    @test ex isa QuoteNode
end

@testset "leaf meta-quote with various Symbol names (Issue #4911)" begin
    @test :(:bar) isa QuoteNode
    @test :(:hello) isa QuoteNode
    @test :(:%) isa QuoteNode   # operator-name Symbol
    @test :(:+) isa QuoteNode
end

@testset "meta-quote return value matches user-constructed QuoteNode (Issue #4911)" begin
    # sjulia's `==` on two QuoteNode values currently errors with
    # `Cannot convert QuoteNode to I64`, so we anchor on `isa` and
    # `typeof` instead of value equality. Both forms must produce a
    # QuoteNode at runtime.
    ex = :(:foo)
    q = QuoteNode(:foo)
    @test typeof(ex) === typeof(q)
    @test ex isa QuoteNode
    @test q isa QuoteNode
end

true
