# Test @macroexpand macro
# Shows the expansion of a macro call

macro double(x)
    quote
        2 * $x
    end
end

# @macroexpand should expand and evaluate the macro
# The macro @double(5) expands to `2 * 5`, which evaluates to 10
result = @macroexpand @double 5

Float64(result)
