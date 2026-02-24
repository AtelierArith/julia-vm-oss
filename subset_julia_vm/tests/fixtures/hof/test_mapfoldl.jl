# Test mapfoldl function (map then left-fold)
# mapfoldl(f, op, arr) applies f to each element, then left-folds with op

using Test

@testset "mapfoldl - map then left-fold (Issue #351)" begin

    # Basic mapfoldl: sum of squares
    result1 = mapfoldl(x -> x^2, (a, b) -> a + b, [1, 2, 3, 4])
    # 1 + 4 + 9 + 16 = 30

    # mapfoldl with init
    result2 = mapfoldl(x -> x * 2, (a, b) -> a + b, [1, 2, 3], 10)
    # 10 + 2 + 4 + 6 = 22

    # Single element
    result3 = mapfoldl(x -> x * 3, (a, b) -> a + b, [5])
    # 15

    # Left-fold order check: ((1^2 - 2^2) - 3^2) = (1 - 4) - 9 = -12
    result4 = mapfoldl(x -> x^2, (a, b) -> a - b, [1, 2, 3])
    # -12

    @test (result1 == 30 && result2 == 22 && result3 == 15 && result4 == -12)
end

true  # Test passed
