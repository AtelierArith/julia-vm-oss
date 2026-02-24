# Test simple identity macro - returns its argument unchanged
macro identity(x)
    x
end

# The macro should expand to just the argument
result = @identity 42
Float64(result)
