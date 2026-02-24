# Test eval() function for evaluating Expr at runtime

# Basic arithmetic
@assert eval(:(1 + 1)) == 2
@assert eval(:(2 * 3)) == 6
@assert eval(:(10 - 4)) == 6
@assert eval(:(8 / 2)) == 4.0

# More complex expressions
@assert eval(:(2 + 3 * 4)) == 14
@assert eval(:(2 ^ 10)) == 1024

# Math functions
@assert eval(:(sqrt(16))) == 4.0
@assert eval(:(abs(42))) == 42

# Comparisons - can be used as assertions directly
@assert eval(:(1 < 2))
@assert eval(:(2 == 2))
@assert eval(:(1 != 2))

# Return success value
42.0
