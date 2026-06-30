# Issue #4993: `:(x.y = z)` — quoted assignment with dotted LHS.
#
# Surfaced from the postmortem of PR #4992 (Issue #4901). PR #4992
# fixed the `NodeKind::CallExpression` arm by recursing into the
# callee via `cst_to_expr_constructor`. The structurally identical
# bug remained in the `NodeKind::Assignment` arm: the LHS text was
# extracted via `walker.text(target)` and wrapped in
# `Symbol(target_name)`, collapsing a dotted LHS to a single flat
# Symbol.
#
# Upstream Julia:
#   julia> ex = :(x.y = z);
#   julia> typeof(ex.args[1])
#   Expr               # Expr(:., :x, QuoteNode(:y))
#
# sjulia previously returned `Symbol("x.y")`.
#
# Fix: in the `NodeKind::Assignment` arm of
# `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`,
# recurse into the LHS via `cst_to_expr_constructor`. A
# `FieldExpression` LHS then routes through the arm added in
# PR #4903 (for #4899) and produces the canonical
# `Expr(:., obj, QuoteNode(:f))` shape. Identifier LHS continues
# to emit `Symbol(text)` via its own arm — `:(x = 1)` unchanged.

using Test

@testset "quoted assignment with dotted LHS (Issue #4993)" begin
    ex = :(x.y = z)
    @test ex isa Expr
    @test ex.head === :(=)

    lhs = ex.args[1]
    @test lhs isa Expr
    @test lhs.head === Symbol(".")
    @test lhs.args[1] === :x
    @test lhs.args[2] == QuoteNode(:y)

    @test ex.args[2] === :z
end

@testset "quoted assignment with module-qualified LHS (Issue #4993)" begin
    ex = :(Mod.field = 42)
    @test ex isa Expr
    @test ex.head === :(=)

    lhs = ex.args[1]
    @test lhs isa Expr
    @test lhs.head === Symbol(".")
    @test lhs.args[1] === :Mod
    @test lhs.args[2] == QuoteNode(:field)

    @test ex.args[2] == 42
end

@testset "quoted assignment with plain identifier LHS still works (Issue #4993)" begin
    # Regression guard — :(x = 1) must keep producing Symbol LHS.
    ex = :(x = 1)
    @test ex isa Expr
    @test ex.head === :(=)
    @test ex.args[1] === :x
    @test ex.args[2] == 1
end

true
