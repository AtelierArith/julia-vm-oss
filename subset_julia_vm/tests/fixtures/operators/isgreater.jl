# Test isgreater function
# isgreater(x, y) tests if x is greater than y in a descending total order

using Test
using Base: isgreater

@testset "isgreater function - descending total order comparison" begin

    # Note: In Julia, isgreater is Base.isgreater (not exported)

    # Basic comparisons
    check1 = isgreater(3, 2) == true
    check2 = isgreater(2, 3) == false
    check3 = isgreater(2, 2) == false

    # Float comparisons
    check4 = isgreater(3.5, 2.5) == true
    check5 = isgreater(2.5, 3.5) == false

    # NaN handling - NaN is smallest in descending order
    # isgreater puts NaN at the end (as smallest)
    check6 = isgreater(1.0, NaN) == true   # 1.0 is greater than NaN
    check7 = isgreater(NaN, 1.0) == false  # NaN is not greater than 1.0
    check8 = isgreater(NaN, NaN) == false  # NaN is not greater than NaN

    # Negative numbers
    check9 = isgreater(-1, -2) == true   # -1 > -2
    check10 = isgreater(-2, -1) == false

    # Mixed integer/float
    check11 = isgreater(3, 2.5) == true
    check12 = isgreater(2.5, 3) == false

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8 && check9 && check10 && check11 && check12)
end

true  # Test passed
