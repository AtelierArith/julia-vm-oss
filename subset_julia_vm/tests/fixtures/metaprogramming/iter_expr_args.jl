# Test iteration over ex.args (Issue 286)
# for arg in ex.args should work without assigning to variable first

using Test

function test_iter_expr_args()
    # Test basic iteration over Expr.args
    ex1 = Expr(:call, :+, 1, 2, 3)
    sum1 = 0
    count1 = 0
    for arg in ex1.args
        if isa(arg, Int64)
            sum1 = sum1 + arg
        end
        count1 = count1 + 1
    end
    if sum1 != 6
        error("Expected sum1=6, got $sum1")
    end
    if count1 != 4  # :+ plus 3 integers
        error("Expected count1=4, got $count1")
    end

    # Test with block expression
    ex2 = Expr(:block, :(x = 1), :(y = 2))
    count2 = 0
    for arg in ex2.args
        count2 = count2 + 1
    end
    if count2 != 2
        error("Expected count2=2, got $count2")
    end

    # Test empty args
    ex3 = Expr(:block)
    count3 = 0
    for arg in ex3.args
        count3 = count3 + 1
    end
    if count3 != 0
        error("Expected count3=0, got $count3")
    end

    true
end

@testset "Iteration over ex.args works directly (Issue #286)" begin


    @test (test_iter_expr_args())
end

true  # Test passed
