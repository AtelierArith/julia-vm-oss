# Test that Base @inline macro is available without defining it locally
# This verifies the Base macro pre-registration works

# Use @inline from base/macros.jl
result = @inline 40 + 2

# Return the result
Float64(result)
