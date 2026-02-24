# Test foldl function (left-associative fold)
# foldl(op, arr) folds from left: op(op(op(a, b), c), d)

# Basic foldl with subtraction: ((1 - 2) - 3) = -4
result1 = foldl((a, b) -> a - b, [1, 2, 3])
println(result1)  # -4

# foldl with addition: ((1 + 2) + 3) = 6
result2 = foldl((a, b) -> a + b, [1, 2, 3])
println(result2)  # 6

# foldl with multiplication: ((2 * 3) * 4) = 24
result3 = foldl((a, b) -> a * b, [2, 3, 4])
println(result3)  # 24

# foldl with single element
result4 = foldl((a, b) -> a + b, [42])
println(result4)  # 42

# foldl with floating point division: ((8 / 4) / 2) = 1.0
result5 = foldl((a, b) -> a / b, [8.0, 4.0, 2.0])
println(result5)  # 1.0
