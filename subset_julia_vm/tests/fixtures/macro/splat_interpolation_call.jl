# Test splat interpolation in function call within quote: $(args...)
# This expands varargs into function arguments

# A function that sums its arguments
mysum(a, b, c) = a + b + c

# Simpler macro that calls mysum directly with splatted arguments
macro call_mysum(args...)
    quote
        mysum($(args...))
    end
end

# Call mysum(10, 20, 12) = 42
result = @call_mysum 10 20 12
Float64(result)
