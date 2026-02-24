# Test @inbounds and @boundscheck macros
# These are no-op macros for compatibility with Julia code
# In SubsetJuliaVM, bounds checking is always performed for safety

# Test @inbounds - simple expression (skips bounds checking in Julia)
arr = [10, 20, 30, 40, 50]
println(@inbounds arr[3])        # 30
println(@inbounds arr[1])        # 10

# Test @boundscheck - marks code as bounds checking (skipped in @inbounds)
println(@boundscheck 1 + 2)      # 3

# Test nested @inbounds
println(@inbounds @inbounds arr[4])  # 40

# All macros work as pass-through (no-op for safety)
true
