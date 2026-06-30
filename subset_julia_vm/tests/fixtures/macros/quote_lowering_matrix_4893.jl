# Issue #4893: regression-guard matrix for the quote-lowering pass in
# `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`.
#
# Background: `cst_to_expr_constructor` is a `match node.kind() { ... }`
# block. Unhandled `NodeKind` variants fall to a catch-all that raises
# `UnsupportedExpression("quote for X not yet supported")`. Coverage
# was historically discovered one variant at a time as user reports
# rolled in (#4872 operator, #4890 vector + parametric type). This
# fixture pins every variant that *currently* lowers so we can detect
# regressions immediately, and documents the variants that still fall
# to the catch-all so the next contributor knows what's left.
#
# Each entry below was probed via:
#   `./target/release/sjulia -e 'ex = :(<form>); println(typeof(ex))'`
# Cases that printed `OK` are asserted here; cases that returned a
# Lowering error are listed in the "Known gaps" comment block — those
# need their own follow-up issues / PRs.

using Test

# ---------------------------------------------------------------------------
# Currently supported (regression guards)
# ---------------------------------------------------------------------------

@testset "literal forms inside :(...) (Issue #4893)" begin
    @test :(42) === 42                  # Int literal
    @test :(3.14) === 3.14              # Float literal
    @test :("hi") == "hi"               # String literal
    @test :(true) === true              # Bool literal (special-cased)
    @test :(false) === false
    # Issue #4895: `nothing` / `missing` are ordinary identifiers in
    # upstream Julia, so `:(nothing)` is the Symbol `:nothing` and
    # `:(missing)` the Symbol `:missing` (only `true` / `false` are
    # literal Bool AST nodes that quote back to the value).
    @test :(nothing) === :nothing
    @test :(missing) === :missing
end

@testset "identifier and operator forms (Issue #4893; covers #4872)" begin
    @test :(foo) === :foo
    @test :(some_name) === :some_name
    @test :(%) === :%
    @test :(+) === :+
    @test :(*) === :*
end

# Helper to keep `@test` at top level inside @testset so that `using
# Test` resolves from this module rather than from inside a `let`
# (sjulia parser limitation: @test inside `let ... end` reports
# "@test macro requires `using Test`").
@testset "tuple aggregate (Issue #4893)" begin
    ex = :((1, 2, 3))
    @test ex isa Expr
    @test ex.head === :tuple
    @test ex.args == [1, 2, 3]
end

@testset "vector aggregate (Issue #4893; covers #4890)" begin
    ex = :([1, 2, 3])
    @test ex isa Expr
    @test ex.head === :vect
    @test ex.args == [1, 2, 3]
end

@testset "matrix aggregate (Issue #4893; covers #7763)" begin
    row_ex = :([1 2 3])
    @test row_ex isa Expr
    @test row_ex.head === :hcat
    @test row_ex.args == [1, 2, 3]

    col_ex = :([1; 2; 3])
    @test col_ex isa Expr
    @test col_ex.head === :vcat
    @test col_ex.args == [1, 2, 3]

    mat_ex = :([1 2; 3 4])
    @test mat_ex isa Expr
    @test mat_ex.head === :vcat
    @test mat_ex.args[1].head === :row
    @test mat_ex.args[1].args == [1, 2]
    @test mat_ex.args[2].head === :row
    @test mat_ex.args[2].args == [3, 4]
end

@testset "parametric type aggregate (Issue #4893; covers #4890)" begin
    ex = :(Tuple{Int, Int})
    @test ex isa Expr
    @test ex.head === :curly
    @test ex.args[1] === :Tuple
end

@testset "call expression (Issue #4893)" begin
    ex = :(f(x))
    @test ex isa Expr
    @test ex.head === :call
    @test ex.args[1] === :f
end

@testset "binary op (Issue #4893)" begin
    ex = :(a + b)
    @test ex isa Expr   # head shape may differ; just confirm lowered
end

@testset "control-flow forms (Issue #4893)" begin
    @test :(if x; y else z end) isa Expr
    @test :(for i in xs; f(i) end) isa Expr
    @test :(while p; f() end) isa Expr
    @test :(try f() catch e; rethrow() end) isa Expr
end

@testset "assignment, ternary, range (Issue #4893)" begin
    @test :(a = b) isa Expr
    @test :(x ? y : z) isa Expr
    @test :(a:b) isa Expr
end

# ---------------------------------------------------------------------------
# Known gaps (NOT asserted — tracked separately)
# ---------------------------------------------------------------------------
#
# As of this fixture's creation, the following quoteable forms still
# fall to the catch-all `quote for {} not yet supported` and need
# their own arms in `cst_to_constructor.rs`. Each is a separate
# follow-up issue (see #4893 for the meta-tracking issue):
#
#   (`:(a.b)` field access — fixed in #4899 PR; pinned in
#   `quote_field_access_4899.jl`)
#   :(let x=1; x end)                — NodeKind::LetExpression
#                                      lowers to Expr(:let, Expr(:(=), ...), body)
#   (`:(:foo)` meta-quote — fixed in #4911 PR; pinned in
#   `quote_metaquote_4911.jl`)
#   :([f(x) for x in xs])            — NodeKind::ComprehensionExpression
#                                      lowers to Expr(:comprehension, generator)
#   (`:(f(args...))` splat — fixed in #4904 PR; pinned in
#   `quote_splat_4904.jl`)
#
# When a new arm is added in `cst_to_constructor.rs`, move the
# corresponding line from this block into a new @testset above.

true
