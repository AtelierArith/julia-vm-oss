# Test arity-based macro dispatch
# Multiple macro definitions with the same name but different arities

# Single argument version - returns the value
macro m(x)
    :($x)
end

# Two argument version - returns the sum
macro m(x, y)
    :($x + $y)
end

# Three argument version - returns the product
macro m(x, y, z)
    :($x * $y * $z)
end

# Test single argument
result1 = @m(42)
@assert result1 == 42 "Single argument should return 42"

# Test two arguments
result2 = @m(10, 20)
@assert result2 == 30 "Two arguments should return 30"

# Test three arguments
result3 = @m(2, 3, 4)
@assert result3 == 24 "Three arguments should return 24"

# Test with expressions
a = 5
b = 7
result4 = @m(a)
@assert result4 == 5 "Variable argument should work"

result5 = @m(a, b)
@assert result5 == 12 "Two variable arguments should return 12"

result5
