# Issue #4915 (partial): `===` on two `QuoteNode` values previously
# fell through the `Egal` builtin's main `match (&left, &right)` block
# to `_ => false`, so `QuoteNode(:x) === QuoteNode(:x)` returned
# false even when both operands wrapped the same Symbol. After
# adding a `(Value::QuoteNode(a), Value::QuoteNode(b))` arm that
# compares the wrapped inner values structurally (mirrors the
# `Expr` arm right above it), `===` now reports equality correctly.
#
# Scope: `===` only. The companion `==` operator still errors with
# `Compilation error: "Cannot convert QuoteNode to I64"` because the
# compile-time `==` dispatch path doesn't have a `QuoteNode`-aware
# arm and falls into a generic numeric coercion that tries to
# convert both operands to `I64`. Tracked as the remaining piece of
# #4915.

using Test

@testset "QuoteNode === QuoteNode is reflexive (Issue #4915)" begin
    q = QuoteNode(:foo)
    @test q === q
end

@testset "QuoteNode === detects value equality (Issue #4915)" begin
    @test QuoteNode(:foo) === QuoteNode(:foo)
    @test QuoteNode(:bar) === QuoteNode(:bar)
    @test QuoteNode(42) === QuoteNode(42)
end

@testset "QuoteNode === detects inequality (Issue #4915)" begin
    @test !(QuoteNode(:foo) === QuoteNode(:bar))
    @test !(QuoteNode(:foo) === QuoteNode(42))
end

@testset "QuoteNode === interoperates with quote lowering (Issue #4915)" begin
    # Pre-#4911 (and pre-this-PR) this would have errored.
    # Post-#4911 the meta-quote produces a QuoteNode; post-this-PR
    # `===` correctly compares it against the user-constructed
    # equivalent.
    @test :(:foo) === QuoteNode(:foo)
end

true
