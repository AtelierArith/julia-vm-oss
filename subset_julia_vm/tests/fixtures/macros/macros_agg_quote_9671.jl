# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: macros/globalref_any_field_7743.jl =====

@testset "Any-typed GlobalRef exposes mod and name fields (Issue #7743)" begin
    g = GlobalRef(Core, Symbol("@doc"))
    x = Any[g][1]
    @test x == g
    @test x.mod == Core
    @test x.name == Symbol("@doc")
end

# ===== source: macros/quote_adjoint_expr_7554.jl =====

@testset "quote construction supports adjoint expressions (Issue #7554)" begin
    ex = :(x')

    @test ex isa Expr
    @test ex.head == Symbol("'")
    @test length(ex.args) == 1
    @test ex.args[1] == :x
end

# ===== source: macros/quote_assign_dotted_lhs_4993.jl =====
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
# `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`,
# recurse into the LHS via `cst_to_expr_constructor`. A
# `FieldExpression` LHS then routes through the arm added in
# PR #4903 (for #4899) and produces the canonical
# `Expr(:., obj, QuoteNode(:f))` shape. Identifier LHS continues
# to emit `Symbol(text)` via its own arm — `:(x = 1)` unchanged.


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

# ===== source: macros/quote_assign_typed_lhs_7622.jl =====

@testset "quoted assignment with typed LHS (Issue #7622)" begin
    ex = :(x::Int = nothing)
    @test ex isa Expr
    @test ex.head === :(=)

    lhs = ex.args[1]
    @test lhs isa Expr
    @test lhs.head === :(::)
    @test lhs.args[1] === :x
    @test lhs.args[2] === :Int
    @test ex.args[2] === :nothing
end

@testset "plain quoted assignment LHS remains a Symbol (Issue #7622)" begin
    ex = :(x = nothing)
    @test ex isa Expr
    @test ex.head === :(=)
    @test ex.args[1] === :x
    @test ex.args[2] === :nothing
end

# ===== source: macros/quote_call_dotted_callee_4901.jl =====
# Issue #4901: `:(M.f(x))` — quoted call with dotted callee.
#
# Surfaced from the fixture for Issue #4899 (PR #4903, field-access
# quote). Bare `:(a.b)` worked after that PR, but when the dotted
# name is the callee of a call, a different lowering path
# (the `NodeKind::CallExpression` arm in
# `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`)
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

# ===== source: macros/quote_field_access_4899.jl =====
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
# Fix: in `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a `NodeKind::FieldExpression` arm that emits
# `Expr(:., quoted_object, QuoteNode(:field_name))`. Field-expression
# CST has two named children — the object (recursively quoted) and
# the field-name identifier.


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

# ===== source: macros/quote_interp_type_annotation_9176.jl =====
# Issue #9176: two regressions found while making `using MacroTools` load.
#
# (1) Quote lowering — `$` binds tighter than `::` in Julia, so `:($a::T)` is
#     `:(($a)::T)`: interpolate the name, quote the annotation. sjulia previously
#     lowered the whole `a::T` as plain code (a real `typeassert`), so `:($a::T)`
#     threw at runtime and `:($a::$b)` failed to lower ("unsupported operator: $").
#
# (2) Parser — a `do ... end` block body is a fresh statement block; its newlines
#     separate statements even though the do-clause is parsed while the enclosing
#     call's paren depth is still elevated. A statement followed by a line
#     starting with `:` used to merge into a range (`(b = :Int):(…)`).
#
# MacroTools' `combinestructdef` (`map(...) do field … :($fieldname::$typ) end`)
# depends on both, so `using MacroTools` failed to load.
#
# Note: assertions use `.head`/`.args` rather than `expr1 == expr2`, because
# scalar `Expr == Expr` is a separate, pre-existing sjulia gap (Issue #9183).

@testset "interpolated `::` annotation in a quote (Issue #9176)" begin
    a = :x
    b = :Int

    both = :($a::$b)               # both operands interpolated
    @test both.head == :(::)
    @test both.args == [:x, :Int]

    name_only = :($a::Int)         # only the name interpolated
    @test name_only.head == :(::)
    @test name_only.args == [:x, :Int]

    type_only = :(y::$b)           # only the annotation interpolated
    @test type_only.head == :(::)
    @test type_only.args == [:y, :Int]
end

@testset "do-block body newlines stay significant (Issue #9176)" begin
    # A statement ending in an expression, then a line starting with `:(...)` —
    # the MacroTools `combinestructdef` shape.
    r = map([(:fld, :Int)]) do field
        fieldname, typ = field
        :($fieldname::$typ)
    end
    @test length(r) == 1
    @test r[1].head == :(::)
    @test r[1].args == [:fld, :Int]

    # A local (non-destructured) variable interpolated in a do-block quote.
    s = map([1]) do _
        b = :Int
        :($b)
    end
    @test s == [:Int]
end

# ===== source: macros/quote_lowering_matrix_4893.jl =====
# Issue #4893: regression-guard matrix for the quote-lowering pass in
# `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`.
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

# ===== source: macros/quote_metaquote_4911.jl =====
# Issue #4911: `:(:foo)` (a meta-quote — a Symbol literal nested
# inside an outer quote) previously fell through the quote-lowering
# catch-all with
# `UnsupportedExpression("quote for quote_expression not yet supported")`.
# Upstream Julia produces a literal `QuoteNode(:foo)` value, not an
# Expr.
#
# Fix: in `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a `NodeKind::QuoteExpression` arm that handles both the leaf
# form (`:foo`) and the with-children form (`:(expr)` nested inside
# the outer quote). The leaf form parses the symbol-name out of the
# leaf text; the with-children form recurses and wraps the result in
# `QuoteNode(...)`.


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

# ===== source: macros/quote_metaquote_expr_4920.jl =====
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

# ===== source: macros/quote_nothing_missing_symbol_4895.jl =====
# Issue #4895: `:(nothing)` and `:(missing)` quote to the Symbols
# `:nothing` / `:missing` (not the literal `nothing` / `missing`
# values), matching upstream Julia.
#
# `nothing` / `missing` are ordinary identifiers in the Julia AST, so
# quoting them yields a Symbol. They only become the actual values when
# the quoted Expr is later evaluated. By contrast `true` / `false` are
# literal Bool AST nodes, so `:(true)` quotes back to the `true` value.
#
# The regression that historically gated this fix: the Pure-Julia
# `@test` macro ends its `quote ... end` block with a bare `nothing`.
# When the quoted block is converted back into executable code during
# macro expansion, the trailing `:nothing` Symbol must resolve to the
# `nothing` value rather than raising `UndefVarError`. The
# value-keyword handling in `quote/code_generation.rs` covers that, so
# the `@test`-based assertions below double as the regression guard.


@testset "nothing / missing quote to Symbols (Issue #4895)" begin
    # The quoted forms are the Symbols, not the values.
    @test :(nothing) === :nothing
    @test :(missing) === :missing
    @test :(nothing) isa Symbol
    @test :(missing) isa Symbol

    # They are NOT the literal values.
    @test :(nothing) !== nothing
    @test :(missing) !== missing
end

@testset "true / false remain literal Bool nodes (Issue #4895)" begin
    # Contrast: true / false ARE literal Bool AST nodes.
    @test :(true) === true
    @test :(false) === false
    @test :(true) isa Bool
end

@testset "nothing / missing inside quoted Expr args (Issue #4895)" begin
    # Inside a larger quoted Expr they appear as Symbols in args.
    ex = :(f(nothing))
    @test ex.args[2] === :nothing

    ex2 = :(g(missing))
    @test ex2.args[2] === :missing
end

# The fact that every `@test` above ran (and the @testset summaries
# printed) exercises the `@test` macro's trailing-`nothing` block,
# which is the macro-expansion-scope regression guard for #4895.

# ===== source: macros/quote_vector_parametric_4890.jl =====
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
# `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs` for:
# - `NodeKind::VectorExpression` → `Expr(:vect, elem₁, elem₂, ...)`
# - `NodeKind::ParametrizedTypeExpression` → `Expr(:curly, base, p₁, ...)`
# Both mirror the existing `NodeKind::TupleExpression` shape; the head
# Symbol matches upstream Julia's lowering convention (`base/expr.jl`).


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

# ===== source: macros/quote_where_expr_7553.jl =====

@testset "quote construction supports where expressions (Issues #7553, #7714)" begin
    ex = :(x where {T})

    @test ex isa Expr
    @test ex.head == :where
    @test length(ex.args) == 2
    @test ex.args[1] == :x
    @test ex.args[2] == :T

    bare = :(f(a::T) where T)
    @test bare isa Expr
    @test bare.head == :where
    @test length(bare.args) == 2
    @test bare.args[1] == :(f(a::T))
    @test bare.args[2] == :T

    braced = :(f(a::T) where {T})
    @test braced isa Expr
    @test braced.head == :where
    @test length(braced.args) == 2
    @test braced.args[1] == :(f(a::T))
    @test braced.args[2] == :T
end

# ===== source: macros/quoted_docstring_core_doc_7712.jl =====

@testset "quoted docstrings lower to Core.@doc macrocall (Issue #7712)" begin
    ex = quote
        "doc"
        f(x) = x
    end
    @test length(ex.args) == 2
    doccall = ex.args[2]
    @test doccall isa Expr
    @test doccall.head == :macrocall
    @test doccall.args[1] == GlobalRef(Core, Symbol("@doc"))
    @test doccall.args[3] == "doc"
    @test doccall.args[4].head == :(=)

    semicolon = quote; "doc"; g(x) = x; end
    no_lines = Any[]
    for arg in semicolon.args
        if !(arg isa LineNumberNode)
            push!(no_lines, arg)
        end
    end
    @test no_lines[1] == "doc"
    @test no_lines[2].head == :(=)
end

# ===== source: macros/quoted_function_name_interpolation_7520.jl =====

@testset "quoted function name interpolation (Issue #7520)" begin
    fname = :f
    ex = :(function $fname(x)
        x
    end)
    sig = ex.args[1]

    @test ex isa Expr
    @test ex.head == :function
    @test sig isa Expr
    @test sig.head == :call
    @test sig.args[1] == :f
    @test sig.args[2] == :x

    args = [:x]
    kwargs = [:(y=2)]
    combined = :(function $fname($(args...); $(kwargs...))
        x + y
    end)
    combined_sig = combined.args[1]
    params = combined_sig.args[2]
    kw = params.args[1]

    @test combined isa Expr
    @test combined.head == :function
    @test combined_sig.head == :call
    @test combined_sig.args[1] == :f
    @test params.head == :parameters
    @test kw.head == :(=)
    @test kw.args[1] == :y
    @test combined_sig.args[3] == :x
end

# ===== source: macros/quoted_function_pair_7517.jl =====

@testset "quoted function definition pair (Issue #7517)" begin
    ex = :(begin
        function f_(args__)
            body_
        end => rhs
    end)
    pair = ex.args[2]
    lhs = pair.args[2]
    sig = lhs.args[1]

    @test ex isa Expr
    @test ex.head == :block
    @test pair isa Expr
    @test pair.head == :call
    @test pair.args[1] == Symbol("=>")
    @test lhs isa Expr
    @test lhs.head == :function
    @test sig.head == :call
    @test sig.args[1] == :f_
    @test sig.args[2] == :args__
    @test pair.args[3] == :rhs
end

# ===== source: macros/quoted_function_param_splat_7522.jl =====

@testset "quoted function parameter splat interpolation (Issue #7522)" begin
    args = [:x]
    ex = :(function f($(args...))
        x
    end)
    sig = ex.args[1]

    @test ex isa Expr
    @test ex.head == :function
    @test sig isa Expr
    @test sig.head == :call
    @test sig.args[1] == :f
    @test sig.args[2] == :x

    kwargs = [:(y=2)]
    kw_ex = :(function g($(args...); $(kwargs...))
        x + y
    end)
    kw_sig = kw_ex.args[1]
    params = kw_sig.args[2]
    kw = params.args[1]

    @test kw_ex isa Expr
    @test kw_ex.head == :function
    @test kw_sig.head == :call
    @test kw_sig.args[1] == :g
    @test params.head == :parameters
    @test kw.head == :(=)
    @test kw.args[1] == :y
    @test kw.args[2] == 2
    @test kw_sig.args[3] == :x
end

# ===== source: macros/quoted_interpolated_field_7523.jl =====

@testset "quoted interpolated field expressions (Issue #7523)" begin
    x = :obj
    f = :field
    v = :val

    plus_eq = :($x.$f += $v)
    assign = :($x.$f = $v)
    plus_target = plus_eq.args[1]
    assign_target = assign.args[1]

    @test plus_eq isa Expr
    @test plus_eq.head == :(+=)
    @test plus_target isa Expr
    @test plus_target.head == Symbol(".")
    @test plus_target.args[1] == :obj
    @test plus_target.args[2] == QuoteNode(:field)
    @test plus_eq.args[2] == :val

    @test assign isa Expr
    @test assign.head == :(=)
    @test assign_target isa Expr
    @test assign_target.head == Symbol(".")
    @test assign_target.args[1] == :obj
    @test assign_target.args[2] == QuoteNode(:field)
    @test assign.args[2] == :val

    f_string = "field"
    field_access = :($x.$f_string)
    @test field_access.args[2] == QuoteNode("field")
end

# ===== source: macros/quoted_let_expression_7512.jl =====

@testset "quoted let expression (Issue #7512)" begin
    ex = :(let x = 1
        x
    end)
    binding = ex.args[1]
    body = ex.args[2]

    @test ex isa Expr
    @test ex.head == :let
    @test binding isa Expr
    @test binding.head == :(=)
    @test binding.args[1] == :x
    @test binding.args[2] == 1
    @test body isa Expr
    @test body.head == :block
end

# ===== source: macros/quoted_operator_4872.jl =====
# Issue #4872: a bare operator inside a quoted expression (`:(%)`,
# `:(+)`, `:(*)`, etc., or an operator used as a value like
# `:(foo(%, x))`) was rejected during lowering with
# `UnsupportedExpression("quote for operator not yet supported")`.
# Upstream Julia treats a quoted operator as a `Symbol`, identical to
# a quoted identifier.
#
# Fix: in `subset_julia_vm_lowering/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a top-level `NodeKind::Operator` arm that mirrors the existing
# `NodeKind::Identifier` arm — wrap the operator's text in
# `BuiltinOp::SymbolNew`, producing `Symbol(text)`.


@testset "bare quoted operators become Symbols (Issue #4872)" begin
    # Each `:(<op>)` should equal the directly-written `:<op>` Symbol.
    @test :(%) == :%
    @test :(+) == :+
    @test :(*) == :*
    @test :(/) == :/
    @test :(-) == :-
    @test :(==) == :(==)
    @test :(<) == :<
    @test :(>) == :>
    @test :(<=) == :<=
    @test :(>=) == :>=
end

@testset "quoted operators have Symbol type (Issue #4872)" begin
    @test :(%) isa Symbol
    @test :(+) isa Symbol
    @test :(*) isa Symbol
    @test :(==) isa Symbol
end

@testset "quoted operator equals the upstream shorthand (Issue #4872)" begin
    # `:%`, `:+`, etc. are the canonical Symbol forms; `:(%)` should
    # quote to the same value.
    @test :(%) === :%
    @test :(+) === :+
    @test :(*) === :*
    @test :(/) === :/
end

@testset "quoted-identifier path stays intact (regression guard)" begin
    # The fix mirrors the Identifier arm; pin the original behavior so
    # the new Operator arm doesn't shadow or regress it.
    @test :(foo) == :foo
    @test :(bar) === :bar
    @test :(some_name) === :some_name
end

# ===== source: macros/quoted_operator_function_head_7519.jl =====

@testset "quoted operator function heads (Issue #7519)" begin
    ex = :(function (fcall_ | fcall_)
        body_
    end)
    sig = ex.args[1]

    @test ex isa Expr
    @test ex.head == :function
    @test sig isa Expr
    @test sig.head == :call
    @test sig.args[1] == :|
    @test sig.args[2] == :fcall_
    @test sig.args[3] == :fcall_
end

# ===== source: macros/quoted_semicolon_block_7511.jl =====

@testset "quoted semicolon block interpolation (Issue #7511)" begin
    line = nothing
    yes = :(1)
    ex = :($line;$yes)

    @test ex isa Expr
    @test ex.head == :block
    @test nothing in ex.args
    @test 1 in ex.args
end

# ===== source: macros/quoted_single_tuple_interpolation_7514.jl =====

@testset "quoted single tuple interpolation (Issue #7514)" begin
    arg = :x
    ex = :($arg,)

    @test ex isa Expr
    @test ex.head == :tuple
    @test length(ex.args) == 1
    @test ex.args[1] == :x
end

# ===== source: macros/quotenode_egal_4915.jl =====
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

# ===== source: macros/show_macro_form_4865.jl =====
# Issue #4865: `@show` emitted values in print-form instead of
# show-form, so `@show "positive"` produced bare
# `x = positive` instead of upstream Julia's `x = "positive"`, and
# `@show :foo` dropped the leading colon. Floats and ints already
# matched because their `print` and `show` forms agree.
#
# Fix: the `_do_show` helper in `base/macros.jl` now uses
# `println(expr_str, " = ", repr(value))`, mirroring upstream
# Julia's `@show` lowering in `julia/base/show.jl:1283-1291`:
#
#     macro show(exs...)
#         blk = Expr(:block)
#         for ex in exs
#             push!(blk.args, :(println($(sprint(show_unquoted,ex)*" = "),
#                                       repr(begin local value = $(esc(ex)) end))))
#         end
#         ...
#     end
#
# `repr(value)` produces the show-form String once, which `println`
# then emits verbatim (println uses print-form for String args, so
# the embedded quotes / colon survive unchanged).
#
# These tests verify the underlying `repr` of each `@show`-returned
# value still matches what upstream Julia would render — the
# user-visible stdout shape is the same. Capturing stdout directly
# from a `@show` invocation is not portable across sjulia and julia
# (no `redirect_stdout` parity), so we anchor on (a) the value being
# returned unchanged and (b) `repr(x)` agreeing with the upstream
# show-form text — which is exactly the part the bug fix shifts.


@testset "@show returns its value unchanged (regression guard)" begin
    @test (@show "positive") == "positive"
    @test (@show :foo) == :foo
    @test (@show 42) == 42
    @test (@show 3.14) == 3.14
    @test (@show 'A') == 'A'
    @test (@show (1, "two", :three)) == (1, "two", :three)
    @test (@show [1, 2, 3]) == [1, 2, 3]
    @test (@show nothing) === nothing
    @test (@show missing) === missing
end

@testset "show-form text for the @show-emitted value (Issue #4865)" begin
    # The fix routes the value through `repr(value)` (upstream's
    # canonical `@show` lowering). `repr(x)` exercises the exact same
    # show-form output — anchoring on it pins what the user now sees
    # in `@show`'s stdout line after `... = `.
    @test repr("positive") == "\"positive\""
    @test repr(:foo) == ":foo"
    @test repr('A') == "'A'"
    @test repr(42) == "42"
    @test repr(3.14) == "3.14"
    @test repr((1, "two", :three)) == "(1, \"two\", :three)"
    @test repr([1, 2, 3]) == "[1, 2, 3]"
    @test repr(nothing) == "nothing"
    @test repr(missing) == "missing"
end

@testset "@show on nested expressions still returns the result" begin
    f_4865(x) = x + 1
    @test (@show f_4865(5)) == 6
    # Wrap arithmetic in a local to keep the `@show` stdout line
    # ("EXPR = VALUE") free of multiple bare integer tokens; the
    # fixture-parity helper's awk fallback otherwise misparses
    # `1 + 2 = 3` as a testset summary row.
    sum_4865 = 1 + 2
    @test (@show sum_4865) == 3
    @test (@show length("abc")) == 3
end

# ===== source: macros/show_multiarg_4868.jl =====
# Issue #4868: `@show x y z` (multiple arguments) failed at lowering
# with "Base macro @show not found (with 3 args)" because the
# `macro show(ex)` in `base/macros.jl` was single-arg only.
#
# Fix: change to `macro show(exs...)`, looping over the arguments and
# emitting one `_do_show(expr_str, value)` per arg (mirroring upstream
# `julia/base/show.jl`), returning the value of the last argument.
#
# Like the single-arg regression fixture (#4865), capturing the
# multi-line stdout is not portable across sjulia/julia, so we anchor
# on the documented contract: `@show a b c` returns the value of the
# last argument, and each argument's value is returned unchanged when
# shown alone.


@testset "@show with multiple arguments returns last value (Issue #4868)" begin
    x_4868 = 1
    y_4868 = 2
    z_4868 = 3
    @test (@show x_4868 y_4868 z_4868) == 3
end

@testset "@show single-arg regression still works (Issue #4868)" begin
    # Bind to a local first so the `@show` stdout line is `name = 42`
    # rather than a bare `42 = 42`, which the fixture-parity helper's
    # awk fallback would otherwise misread as a testset summary row
    # (same guard as the #4865 fixture).
    forty_two_4868 = 42
    @test (@show forty_two_4868) == 42
    @test (@show "hi") == "hi"
end

@testset "@show two args returns last (Issue #4868)" begin
    a_4868 = 10
    b_4868 = 20
    @test (@show a_4868 b_4868) == 20
end

# ===== source: macros/symbol_dot_literal_4908.jl =====
# Issue #4908: sjulia's parser rejected `:.` and `:...` Symbol
# literals — the Symbols whose names are the dot and ellipsis
# operators. These are the canonical Symbol forms of the field-access
# (`:.`) and splat (`:...`) Expr heads in upstream Julia.
#
# Surfaced while writing fixtures for PR #4903 (field access,
# Issue #4899) and PR #4906 (splat, Issue #4904) — both fixtures
# had to use `Symbol(".")` / `Symbol("...")` as workarounds because
# the colon-literal sugar failed to parse.
#
# Fix: in `subset_julia_vm_parser/src/parser/expressions/primary.rs`,
# add an explicit arm to `parse_colon_prefix` for `Token::Dot` and
# `Token::Ellipsis` so they produce `QuoteExpression` leaves like
# every other operator-name Symbol literal. These tokens are
# deliberately not in `Token::is_operator()` (they have grammatical
# meaning as field-access / splat markers), so the existing
# operator-arm doesn't pick them up.


@testset "`:.` is the Symbol \".\" (Issue #4908)" begin
    @test typeof(:.) === Symbol
    @test string(:.) == "."
    @test :. === Symbol(".")
end

@testset "`:...` is the Symbol \"...\" (Issue #4908)" begin
    @test typeof(:...) === Symbol
    @test string(:...) == "..."
    @test :... === Symbol("...")
end

@testset "`:.` / `:...` interoperate with quoted Expr heads (Issue #4908)" begin
    # The reason these Symbol literals matter: they're the heads of
    # field-access and splat Exprs produced by quote lowering (PRs
    # #4903 / #4906). Now that the colon-literal sugar works, the
    # head can be asserted without the `Symbol(name)` workaround.
    field_ex = :(a.b)
    @test field_ex.head === :.

    splat_ex = :(f(args...))
    @test splat_ex.args[2].head === :...
end

@testset "other operator-name Symbol literals stay intact (regression)" begin
    # The existing `:operator` arm (operators in
    # `Token::is_operator()`) must continue to work — the new
    # `Token::Dot | Token::Ellipsis` arm is additive, not a
    # replacement.
    @test :+ === Symbol("+")
    @test :- === Symbol("-")
    @test :* === Symbol("*")
    @test :/ === Symbol("/")
    @test :(==) === Symbol("==")
    @test :% === Symbol("%")
end

true
