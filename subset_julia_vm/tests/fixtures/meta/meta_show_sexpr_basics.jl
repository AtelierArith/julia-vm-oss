# Test Meta.show_sexpr function
# Meta.show_sexpr displays an expression as a Lisp-style S-expression.
# Output goes to stdout; the test verifies no errors are raised.

using Test

@testset "Meta.show_sexpr" begin
    # Call show_sexpr on various expression types.
    # The function returns nothing and prints to stdout.
    # We verify it doesn't throw an error.

    # Simple function call expression
    ex1 = :(f(x, y))
    r1 = Meta.show_sexpr(ex1)
    println()
    @assert r1 === nothing

    # Arithmetic expression
    ex2 = :(1 + 2)
    r2 = Meta.show_sexpr(ex2)
    println()
    @assert r2 === nothing

    # Symbol (not an Expr - should print as :x)
    r3 = Meta.show_sexpr(:x)
    println()
    @assert r3 === nothing

    # Integer literal
    r4 = Meta.show_sexpr(42)
    println()
    @assert r4 === nothing

    @test true
end

true
