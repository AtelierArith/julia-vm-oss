# Test mapfoldr function (map then right-fold)
# mapfoldr(f, op, arr) applies f to each element, then right-folds with op

# Basic mapfoldr: sum of squares (same as mapfoldl for +)
result1 = mapfoldr(x -> x^2, (a, b) -> a + b, [1, 2, 3, 4])
println(result1)  # 1 + 4 + 9 + 16 = 30

# Single element
result2 = mapfoldr(x -> x * 3, (a, b) -> a + b, [5])
println(result2)  # 15

# Right-fold order check: (1^2 - (2^2 - 3^2)) = 1 - (4 - 9) = 1 - (-5) = 6
result3 = mapfoldr(x -> x^2, (a, b) -> a - b, [1, 2, 3])
println(result3)  # 6

# mapfoldr with abs map
result4 = mapfoldr(x -> abs(x), (a, b) -> a * b, [-2, 3, -4])
println(result4)  # 24
