# Issue #4890: vector literals (`:([1, 2, 3])`) and parametric type
# expressions (`:(Tuple{Int, Int})`, `:(Vector{Int})`) inside a quoted
# expression were rejected during lowering with
# `UnsupportedExpression("quote for vector_expression not yet supported")`
# and the parametric-type sibling. Surfaced as a follow-up to #4872
# (PR #4888) — once the operator-quote slice was fixed, the same
# `cst_to_expr_constructor` catch-all fall-through tripped on the next
# two unhandled `NodeKind` variants in the original reproducer.
#
# Fix: add top-level arms in
# `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs` for:
# - `NodeKind::VectorExpression` → `Expr(:vect, elem₁, elem₂, ...)`
# - `NodeKind::ParametrizedTypeExpression` → `Expr(:curly, base, p₁, ...)`
# Both mirror the existing `NodeKind::TupleExpression` shape; the head
# Symbol matches upstream Julia's lowering convention (`base/expr.jl`).

using Test

@testset "quoted vector literal lowers to Expr(:vect, ...) (Issue #4890)" begin
    ex = :([1, 2, 3])
    @test ex isa Expr
    @test ex.head === :vect
    @test ex.args == [1, 2, 3]

    # Empty vector literal
    e2 = :([])
    @test e2 isa Expr
    @test e2.head === :vect
    @test isempty(e2.args)

    # Single element
    e3 = :([42])
    @test e3.head === :vect
    @test e3.args == [42]
end

@testset "quoted parametric type lowers to Expr(:curly, ...) (Issue #4890)" begin
    ex = :(Tuple{Int, Int})
    @test ex isa Expr
    @test ex.head === :curly
    @test ex.args[1] === :Tuple
    @test ex.args[2] === :Int
    @test ex.args[3] === :Int

    e2 = :(Vector{Int})
    @test e2.head === :curly
    @test e2.args == [:Vector, :Int]

    e3 = :(Dict{String, Int})
    @test e3.head === :curly
    @test e3.args == [:Dict, :String, :Int]
end

@testset "quoted parametric type can appear inside a call (Issue #4890)" begin
    # The original #4872 reproducer combines quoted operator (PR #4888)
    # with quoted parametric type (this PR).
    ex = :(Base.infer_exception_type(%, Tuple{Int64, Int64}))
    @test ex isa Expr
    @test ex.head === :call
    # First arg is the callee, then operator (%), then the parametric type.
    @test length(ex.args) == 3
    # Third arg is the quoted curly form.
    @test ex.args[3].head === :curly
    @test ex.args[3].args[1] === :Tuple
end

@testset "TupleExpression quote path stays intact (regression guard)" begin
    # The fix mirrors the existing TupleExpression arm; pin its
    # behavior so the two new arms don't shadow or regress it.
    # `:((1, 2, 3))` quotes to `Expr(:tuple, 1, 2, 3)` (upstream's
    # canonical shape), not to a literal Tuple.
    ex = :((1, 2, 3))
    @test ex isa Expr
    @test ex.head === :tuple
    @test ex.args == [1, 2, 3]
end

true
