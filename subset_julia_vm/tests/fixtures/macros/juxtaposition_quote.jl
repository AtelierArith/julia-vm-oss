# Test quoting juxtaposition expressions (2x => 2 * x)
# Verifies that juxtaposition in quotes doesn't cause lowering errors

# Basic juxtaposition in quote - should compile without error
ex = :(2x)

# Juxtaposition with addition - should also compile
ex2 = :(2x + 1)

# Juxtaposition with more complex expressions
ex3 = :(3x + 2y)

# Test eval of quoted juxtaposition expressions
# This requires variable lookup during eval
x = 3
result1 = eval(:(2x + 1))  # Should be 7 (2*3 + 1)

y = 4
result2 = eval(:(3x + 2y))  # Should be 17 (3*3 + 2*4)

# Verify results
result1 == 7 && result2 == 17
