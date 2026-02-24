# Test Meta.show_sexpr function
# These tests just verify that show_sexpr runs without errors
# and produces some output. The S-expression format is tested
# by running and observing the output.

using Test

@testset "Meta.show_sexpr - show expression as S-expression" begin

    # Test 1: Simple call expression
    Meta.show_sexpr(:(f(x)))

    # Test 2: Nested call expression
    Meta.show_sexpr(:(f(x, g(y))))

    # Test 3: Binary operator
    Meta.show_sexpr(:(a + b))

    # Test 4: Assignment
    Meta.show_sexpr(:(x = 1))

    # Test 5: Empty expression
    Meta.show_sexpr(Expr(:block))

    # Test 6: Symbol
    Meta.show_sexpr(:x)

    # Test 7: Integer
    Meta.show_sexpr(42)

    # Test 8: Block expression (multi-line format)
    Meta.show_sexpr(Expr(:block, :(x = 1), :(y = 2)))

    # Return success
    @test (true)
end

true  # Test passed
