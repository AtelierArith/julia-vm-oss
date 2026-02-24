# Test type-based macro dispatch
# Multiple macro definitions with the same name and arity but different parameter types

# Version for Symbol (identifier) argument - returns 1
macro describe(x::Symbol)
    :(1)
end

# Version for Expr (expression) argument - returns 2
macro describe(x::Expr)
    :(2)
end

# Test with a Symbol (identifier) - should return 1
result1 = @describe(foo)

# Test with an Expr (expression) - should return 2
result2 = @describe(1 + 2)

# Return sum: if both work correctly, 1 + 2 = 3
# If both dispatch to Symbol version: 1 + 1 = 2
# If both dispatch to Expr version: 2 + 2 = 4
result1 + result2
