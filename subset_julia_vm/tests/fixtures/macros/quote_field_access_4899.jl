# Issue #4899: `:(a.b)` (field access inside a quote) raised
# `UnsupportedExpression("quote for field_expression not yet supported")`
# at lowering. Upstream Julia lowers `a.b` to
# `Expr(:., :a, QuoteNode(:b))` — note the second arg is a
# `QuoteNode` wrapping the field-name Symbol, not a bare Symbol.
#
# Surfaced from the audit fixture for Issue #4893 (PR #4898), which
# listed this as one of six remaining `NodeKind` variants still
# falling to the quote-lowering catch-all.
#
# Fix: in `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a `NodeKind::FieldExpression` arm that emits
# `Expr(:., quoted_object, QuoteNode(:field_name))`. Field-expression
# CST has two named children — the object (recursively quoted) and
# the field-name identifier.

using Test

@testset "quoted field access lowers to Expr(:., obj, QuoteNode(:field)) (Issue #4899)" begin
    ex = :(a.b)
    @test ex isa Expr
    @test ex.head === Symbol(".")
    @test ex.args[1] === :a
    @test ex.args[2] isa QuoteNode
    @test ex.args[2] == QuoteNode(:b)
end

@testset "quoted field access supports any object expression (Issue #4899)" begin
    # Module-qualified access (the common reflection-builder case).
    ex = :(Base.foo)
    @test ex isa Expr
    @test ex.head === Symbol(".")
    @test ex.args[1] === :Base
    # Use `==` to compare QuoteNode values directly: sjulia's
    # quote-lowering-produced QuoteNode doesn't expose its inner
    # `value` via `.value` field access the way a user-constructed
    # QuoteNode does, but `==` still recognises them as equal.
    @test ex.args[2] == QuoteNode(:foo)
end

@testset "nested field access x.y.z (Issue #4899)" begin
    # `:(x.y.z)` is `:.((x.y), :z)`, i.e. the outer head is `.` and
    # the first arg is itself a nested field-access Expr.
    ex = :(x.y.z)
    @test ex isa Expr
    @test ex.head === Symbol(".")
    @test ex.args[2] == QuoteNode(:z)

    inner = ex.args[1]
    @test inner isa Expr
    @test inner.head === Symbol(".")
    @test inner.args[1] === :x
    @test inner.args[2] == QuoteNode(:y)
end

# Note: `:(Base.foo(x))` (a call with a dotted callee) is NOT
# asserted here. sjulia's call-lowering path currently collapses
# the dotted callee into a single `:Base.foo` Symbol instead of an
# `Expr(:., :Base, QuoteNode(:foo))`. That's a separate code path
# from the `NodeKind::FieldExpression` arm this PR fixes — it's the
# `CallExpression`-with-dotted-callee arm in the same
# `cst_to_constructor.rs` file. Tracked separately.

true
