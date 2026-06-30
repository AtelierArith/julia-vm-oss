using Test

# Issue #5978: the runtime `eval` mini-interpreter (`eval_expr_value`) must handle
# a `quote` Expr head. A *nested* quote literal `:(...)` inside an eval'd
# expression arrives as `Expr(:quote, inner)`; `eval(Expr(:quote, e))` returns the
# inner expression `e` UNEVALUATED (one level of quoting removed), matching
# upstream Julia. Previously this raised
# `Feature not implemented: eval: unsupported Expr head 'quote'`.
#
# NOTE: an *error* raised through a doubly-nested eval (e.g.
# `eval(:(eval(:(boom()))))` under a `try`) is a separate, deeper routing gap
# (Raised double-handling) tracked in #5979 — out of scope here, which only pins
# the `quote` Expr-head support and the no-error evaluation path.

h() = 42

@testset "eval of a nested quote literal returns the inner expr unevaluated (Issue #5978)" begin
    # `:(:(h()))` is `Expr(:quote, Expr(:call, :h))`; eval removes one quote level
    # and yields the unevaluated `:(h())` (an `Expr` with head `:call`), NOT 42.
    e = eval(:(:(h())))
    @test e isa Expr
    @test e.head == :call

    e2 = eval(:(:(1 + 2)))
    @test e2 isa Expr
    @test e2.head == :call
end

@testset "doubly-nested eval evaluates through the inner quote (Issue #5978)" begin
    # The outer eval unwraps the quote to `:(h())`, the inner eval then runs it.
    @test eval(:(eval(:(h())))) == 42
    @test eval(:(eval(:(1 + 2)))) == 3
    # Triple nesting peels two quote levels then evaluates.
    @test eval(:(eval(:(eval(:(h())))))) == 42
end

@testset "a quote-literal argument bound to a variable still works (no regression)" begin
    ex = :(1 + 2)
    @test eval(:(eval(ex))) == 3
end

true
