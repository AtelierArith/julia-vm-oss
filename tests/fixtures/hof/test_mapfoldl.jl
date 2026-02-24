# Test mapfoldl function (map then left-fold)
# mapfoldl(f, op, arr) applies f to each element, then left-folds with op

# Basic mapfoldl: sum of squares
result1 = mapfoldl(x -> x^2, (a, b) -> a + b, [1, 2, 3, 4])
println(result1)  # 1 + 4 + 9 + 16 = 30

# Single element
result2 = mapfoldl(x -> x * 3, (a, b) -> a + b, [5])
println(result2)  # 15

# Left-fold order check: ((1^2 - 2^2) - 3^2) = (1 - 4) - 9 = -12
result3 = mapfoldl(x -> x^2, (a, b) -> a - b, [1, 2, 3])
println(result3)  # -12

# mapfoldl with abs map
result4 = mapfoldl(x -> abs(x), (a, b) -> a * b, [-2, 3, -4])
println(result4)  # 24
