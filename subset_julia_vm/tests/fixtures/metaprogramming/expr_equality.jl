# Test Expr isequal comparison (Issue 287)
# Expr objects should be compared structurally with isequal

using Test

@testset "Expr isequal comparison for structural equality (Issue #287)" begin

    # Basic Expr equality with isequal
    ex1 = :(1 + 2)
    ex2 = :(1 + 2)
    result1 = isequal(ex1, ex2)
    if !result1
        error("identical Expr should be isequal")
    end

    # Different Expr should not be equal
    ex3 = :(1 + 3)
    result3 = !isequal(ex1, ex3)
    if !result3
        error("different Expr should not be isequal")
    end

    # Symbol comparison (symbols are interned, so === works)
    sym1 = :foo
    sym2 = :foo
    result4 = sym1 === sym2
    if !result4
        error("identical symbols should be === equal")
    end

    # Same variable is === equal to itself
    result5 = ex1 === ex1
    if !result5
        error("same Expr variable should be === equal to itself")
    end

    # More complex expressions with isequal
    ex4 = :(f(x, y))
    ex5 = :(f(x, y))
    ex6 = :(g(x, y))
    result6 = isequal(ex4, ex5)
    if !result6
        error("identical call Expr should be isequal")
    end

    result7 = !isequal(ex4, ex6)
    if !result7
        error("different head call Expr should not be isequal")
    end

    @test (true)
end

true  # Test passed
