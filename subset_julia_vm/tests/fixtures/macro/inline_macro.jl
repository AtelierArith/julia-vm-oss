# Test a user-defined @inline-like macro
# This demonstrates that macros can wrap expressions

macro inline(ex)
    esc(ex)
end

# Use the macro
result = @inline 40 + 2

# Return the result
Float64(result)
