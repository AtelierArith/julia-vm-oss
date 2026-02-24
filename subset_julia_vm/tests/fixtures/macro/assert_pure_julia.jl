# Test Pure Julia @assert macro

# Basic assertions
@assert true
@assert 1 == 1
@assert 1 + 1 == 2
@assert 3 > 2
@assert 2 < 3
@assert 2 >= 2
@assert 2 <= 2

# Boolean operations
@assert true && true
@assert true || false
@assert !false

# Arithmetic conditions
x = 10
@assert x == 10
@assert x > 5
@assert x < 20

# Function return value
f() = true
@assert f()

# Final result
42
