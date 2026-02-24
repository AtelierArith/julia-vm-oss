# Test keyword symbols: :if, :for, :quote, etc.
# These are symbols created from Julia keywords

using Test

@testset "Keyword symbols: :if, :for, :quote, :end as Symbol values" begin

    # Basic keyword symbols
    s_if = :if
    s_for = :for
    s_while = :while
    s_end = :end
    s_quote = :quote
    s_begin = :begin
    s_function = :function
    s_return = :return
    s_true = :true
    s_false = :false

    # Check they are Symbols
    is_symbol_if = typeof(s_if) == Symbol
    is_symbol_for = typeof(s_for) == Symbol
    is_symbol_quote = typeof(s_quote) == Symbol

    # Compare with Symbol() constructor (workaround)
    eq_if = s_if == Symbol("if")
    eq_for = s_for == Symbol("for")
    eq_quote = s_quote == Symbol("quote")
    eq_end = s_end == Symbol("end")

    # Use in Expr comparison (Meta.isexpr pattern)
    ex = :(x + 1)
    head_is_call = ex.head == :call

    # All tests pass
    @test (is_symbol_if && is_symbol_for && is_symbol_quote && eq_if && eq_for && eq_quote && eq_end && head_is_call)
end

true  # Test passed
