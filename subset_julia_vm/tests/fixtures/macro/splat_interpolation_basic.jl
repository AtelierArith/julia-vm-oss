# Test splat interpolation in quote: $(args...)
# This expands a tuple into individual elements within a quoted expression

# Test: Macro that creates a tuple from varargs using splat interpolation
macro make_tuple(args...)
    quote
        ($(args...),)
    end
end

# Create a tuple (1, 2, 3) from varargs
t = @make_tuple 1 2 3

# Verify the tuple has 3 elements
result = length(t) == 3 && t[1] == 1 && t[2] == 2 && t[3] == 3
Float64(result ? 42 : 0)
