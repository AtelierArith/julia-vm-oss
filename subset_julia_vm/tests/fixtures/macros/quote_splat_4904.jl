# Issue #4904: `:(f(args...))` previously fell through the
# quote-lowering catch-all with
# `UnsupportedExpression("quote for splat_expression not yet supported")`.
# Upstream Julia lowers `x...` to `Expr(:..., x)` — head is the
# three-dot Symbol literally named "...".
#
# Companion to #4899 (PR #4903, field access) — both surfaced from
# the audit fixture for #4893 (PR #4898) and live in the same file.
#
# Fix: in `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a `NodeKind::SplatExpression` arm that emits
# `Expr(:..., inner)` where `inner` is the recursively-quoted child.

using Test

# Symbol for the `...` head — `:...` cannot currently be parsed by
# sjulia as a Symbol literal (`ParseFailed("unexpected token 'end of
# input'")`), so we construct it explicitly.
const SPLAT_HEAD = Symbol("...")

@testset "quoted splat lowers to Expr(:..., x) (Issue #4904)" begin
    ex = :(f(args...))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :f
    @test ex.args[2] isa Expr
    @test ex.args[2].head === SPLAT_HEAD
    @test ex.args[2].args[1] === :args
end

@testset "quoted splat preserves position (Issue #4904)" begin
    # `g(a, xs..., b)` — splat in the middle of the arg list.
    ex = :(g(a, xs..., b))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :g
    @test ex.args[2] === :a
    @test ex.args[3] isa Expr
    @test ex.args[3].head === SPLAT_HEAD
    @test ex.args[3].args[1] === :xs
    @test ex.args[4] === :b
end

@testset "leading splat (Issue #4904)" begin
    # `h(xs...)` — splat is the only arg.
    ex = :(h(xs...))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :h
    @test ex.args[2].head === SPLAT_HEAD
    @test ex.args[2].args[1] === :xs
end

@testset "splat of an arbitrary expression (Issue #4904)" begin
    # The inner expression is recursively quoted, so any Expr works.
    ex = :(f((a + b)...))
    @test ex.args[2] isa Expr
    @test ex.args[2].head === SPLAT_HEAD
    @test ex.args[2].args[1] isa Expr   # recursively quoted (a + b)
end

true
