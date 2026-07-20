# Test Pure Julia @show macro

# @show returns the value
x = 42
result = @show x  # prints "x = 42", returns 42
@assert result == 42

# @show with expressions
y = @show 1 + 2  # prints "1 + 2 = 3", returns 3
@assert y == 3

# @show with variables
a = 10
b = 20
sum_result = @show a + b  # prints "a + b = 30", returns 30
@assert sum_result == 30

# Final result
100
