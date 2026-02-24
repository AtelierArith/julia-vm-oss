# Test mapfoldr function (map then right-fold)
# mapfoldr(f, op, arr) applies f to each element, then right-folds with op

using Test

@testset "mapfoldr - map then right-fold (Issue #351)" begin

    # Basic mapfoldr: sum of squares (same as mapfoldl for +)
    result1 = mapfoldr(x -> x^2, (a, b) -> a + b, [1, 2, 3, 4])
    # 1 + 4 + 9 + 16 = 30

    # mapfoldr with init
    result2 = mapfoldr(x -> x * 2, (a, b) -> a + b, [1, 2, 3], 10)
    # 2 + (4 + (6 + 10)) = 22

    # Single element
    result3 = mapfoldr(x -> x * 3, (a, b) -> a + b, [5])
    # 15

    # Right-fold order check: (1^2 - (2^2 - 3^2)) = 1 - (4 - 9) = 1 - (-5) = 6
    result4 = mapfoldr(x -> x^2, (a, b) -> a - b, [1, 2, 3])
    # 6

    @test (result1 == 30 && result2 == 22 && result3 == 15 && result4 == 6)
end

true  # Test passed
