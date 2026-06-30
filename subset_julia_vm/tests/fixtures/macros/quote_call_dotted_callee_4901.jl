# Issue #4901: `:(M.f(x))` — quoted call with dotted callee.
#
# Surfaced from the fixture for Issue #4899 (PR #4903, field-access
# quote). Bare `:(a.b)` worked after that PR, but when the dotted
# name is the callee of a call, a different lowering path
# (the `NodeKind::CallExpression` arm in
# `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`)
# flattens the callee to a single `Symbol("Base.foo")` instead of
# emitting the proper `Expr(:., :Base, QuoteNode(:foo))` Expr tree.
#
# Upstream Julia produces:
#   julia> ex = :(Base.foo(x))
#   julia> typeof(ex.args[1])
#   Expr
#   julia> ex.args[1]
#   :(Base.foo)        # an Expr(:., :Base, QuoteNode(:foo))
#
# sjulia previously produced `:Base.foo` — a flat Symbol — making
# reflection code that builds call ASTs from a module-qualified
# function reference work around it manually.
#
# Fix: in the `NodeKind::CallExpression` arm, recurse into the
# callee child via `cst_to_expr_constructor` rather than extracting
# its raw text and wrapping in `Symbol(text)`. The recursion routes
# `FieldExpression` through its existing arm (added in #4899's PR),
# which already produces the canonical `Expr(:., obj, QuoteNode(:f))`
# shape — and falls back to the identifier/operator arms (which
# emit Symbol(text)) for the plain-callee cases that already worked.

using Test

@testset "quoted call with dotted callee (Issue #4901)" begin
    ex = :(Base.foo(x))
    @test ex isa Expr
    @test ex.head === :call

    callee = ex.args[1]
    @test callee isa Expr
    @test callee.head === Symbol(".")
    @test callee.args[1] === :Base
    @test callee.args[2] == QuoteNode(:foo)

    @test ex.args[2] === :x
end

@testset "quoted call with plain identifier callee still works (Issue #4901)" begin
    ex = :(foo(x))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :foo
    @test ex.args[2] === :x
end

@testset "quoted call with deeper dotted callee Foo.Bar.baz(x) (Issue #4901)" begin
    ex = :(Foo.Bar.baz(x))
    @test ex isa Expr
    @test ex.head === :call

    callee = ex.args[1]
    @test callee isa Expr
    @test callee.head === Symbol(".")
    @test callee.args[2] == QuoteNode(:baz)

    inner = callee.args[1]
    @test inner isa Expr
    @test inner.head === Symbol(".")
    @test inner.args[1] === :Foo
    @test inner.args[2] == QuoteNode(:Bar)
end

@testset "quoted call with dotted callee and multiple args (Issue #4901)" begin
    ex = :(Base.foo(x, y))
    @test ex.head === :call
    callee = ex.args[1]
    @test callee isa Expr
    @test callee.head === Symbol(".")
    @test callee.args[1] === :Base
    @test callee.args[2] == QuoteNode(:foo)
    @test ex.args[2] === :x
    @test ex.args[3] === :y
end

true
