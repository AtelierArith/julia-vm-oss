# Issue #4923: `:42`, `:3.14`, `:"hello"`, `:'A'`, `:true` — the
# colon-prefix syntax applied to a literal — was rejected by the
# parser. Upstream Julia treats `:literal` as `QuoteNode(literal)`,
# which at the *top level* evaluates immediately to the literal value
# (`typeof(:42) === Int64`, not QuoteNode). Inside a nested quote
# `:(:42)`, the result is `QuoteNode(42)` as the embedded AST.
#
# Fix: two parts —
#   1. In `subset_julia_vm_parser/src/parser/expressions/primary.rs`,
#      add an arm in `parse_colon_prefix` for numeric / char / string
#      literal tokens that produces a `QuoteExpression` CST node with
#      the literal as a child (with-children form).
#   2. In `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`,
#      `lower_quote_expr` now branches on `children.is_empty()` rather
#      than the text-shape heuristic. The with-children path returns
#      the inner literal's constructor directly (so top-level `:42`
#      evaluates to `42`). Also adds a `NodeKind::CharacterLiteral`
#      arm in `cst_to_expr_constructor` (was missing).
#
# The nested case `:(:42)` continues to produce `QuoteNode(42)` via
# the `NodeKind::QuoteExpression` recursion arm added in PR #4914 /
# #4920.

using Test

@testset "colon-prefix on integer literal (Issue #4923)" begin
    @test :42 === 42
    @test typeof(:42) === Int64
    # `:0xFF` and `:0b1010` parse and evaluate correctly to 255 / 10
    # but as `Int64` instead of `UInt8`. That's an orthogonal hex /
    # binary literal-type-preservation gap, out of scope here.
    @test :0xFF == 0xFF
    @test :0b1010 == 0b1010
end

@testset "colon-prefix on float literal (Issue #4923)" begin
    @test :3.14 === 3.14
    @test typeof(:3.14) === Float64
end

@testset "colon-prefix on string literal (Issue #4923)" begin
    @test :"hello" == "hello"
    @test typeof(:"hello") === String
end

@testset "colon-prefix on char literal (Issue #4923)" begin
    @test :'A' === 'A'
    @test typeof(:'A') === Char
end

@testset "colon-prefix binds tightly (regression guard, Issue #4923)" begin
    # `:1 + 2` is `:1 + 2` = `1 + 2` = 3, not `:(1 + 2)`.
    @test :1 + 2 == 3
    @test :2 * 3 == 6
end

@testset "nested :(:literal) still produces QuoteNode (Issue #4911/#4920 guard)" begin
    @test :(:42) isa QuoteNode
    @test :(:foo) isa QuoteNode
end

true
