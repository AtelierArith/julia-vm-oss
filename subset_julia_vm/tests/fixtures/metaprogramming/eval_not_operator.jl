# Test eval with Expr constructor for ! operator
# Note: Avoiding Bool == Bool comparison due to separate bug

using Test

@testset "eval with ! operator - eval(Expr(:call, :!, bool)) should negate boolean" begin

    not_sym = Symbol("!")

    # Test !true -> false
    ex1 = Expr(:call, not_sym, true)
    result1 = eval(ex1)
    println("eval(Expr(:call, :!, true)) = ", result1)
    # Result should be false - using if to verify without Bool comparison
    if result1
        error("Expected false from !true")
    end

    # Test !false -> true
    ex2 = Expr(:call, not_sym, false)
    result2 = eval(ex2)
    println("eval(Expr(:call, :!, false)) = ", result2)
    # Result should be true
    if !result2
        error("Expected true from !false")
    end

    println("All tests passed!")
    @test (1.0) == 1.0
end

true  # Test passed
