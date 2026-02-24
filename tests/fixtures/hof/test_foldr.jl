# Test foldr function (right-associative fold)
# foldr(op, arr) folds from right: op(a, op(b, op(c, d)))

# Basic foldr with subtraction: (1 - (2 - 3)) = 1 - (-1) = 2
result1 = foldr((a, b) -> a - b, [1, 2, 3])
println(result1)  # 2

# foldr with addition: (1 + (2 + 3)) = 6 (same as foldl for +)
result2 = foldr((a, b) -> a + b, [1, 2, 3])
println(result2)  # 6

# foldr with exponentiation: (2 ^ (3 ^ 2)) = 2 ^ 9 = 512
result3 = foldr((a, b) -> a ^ b, [2, 3, 2])
println(result3)  # 512

# foldr with single element
result4 = foldr((a, b) -> a + b, [42])
println(result4)  # 42

# foldr with division: (8 / (4 / 2)) = 8 / 2 = 4
result5 = foldr((a, b) -> a / b, [8.0, 4.0, 2.0])
println(result5)  # 4.0
