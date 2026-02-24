# Test macroexpand() and macroexpand!() functions (Issue #300, #296)
# These functions take a module and an expression, and return the expanded form.

using Test

# =============================================================================
# macroexpand(m, x) - Return expanded form of macro call
# =============================================================================

@testset "macroexpand function" begin
    # Test with a simple quoted expression (no macro call)
    # When there's no macro to expand, the expression is returned as-is
    ex = :(1 + 2)
    result = macroexpand(Main, ex)
    @test result.head == :call
    @test result.args[1] == :+
    @test result.args[2] == 1
    @test result.args[3] == 2

    # Test with a literal (non-Expr)
    # Literals pass through unchanged
    lit_result = macroexpand(Main, 42)
    @test lit_result == 42

    # Test with a symbol
    sym_result = macroexpand(Main, :x)
    @test sym_result == :x
end

# =============================================================================
# macroexpand!(m, x) - Destructively expand macro call
# =============================================================================

@testset "macroexpand! function" begin
    # In SubsetJuliaVM, macroexpand! has the same behavior as macroexpand
    # (no mutation distinction at the VM level)

    # Test with a simple expression
    ex = :(a * b)
    result = macroexpand!(Main, ex)
    @test result.head == :call
    @test result.args[1] == :*
    @test result.args[2] == :a
    @test result.args[3] == :b

    # Test with a literal
    lit_result = macroexpand!(Main, 100)
    @test lit_result == 100

    # Test with a symbol
    sym_result = macroexpand!(Main, :foo)
    @test sym_result == :foo
end

# =============================================================================
# API compatibility tests
# =============================================================================

@testset "macroexpand API compatibility" begin
    # Both functions should accept Module as first argument
    # In SubsetJuliaVM, Main is the only supported module

    # Test that both functions exist and are callable
    ex = :(x + y)
    r1 = macroexpand(Main, ex)
    r2 = macroexpand!(Main, ex)

    # Both should return equivalent results for non-macro expressions
    @test r1.head == r2.head
    @test length(r1.args) == length(r2.args)
end

println("All macroexpand function tests passed!")

true
