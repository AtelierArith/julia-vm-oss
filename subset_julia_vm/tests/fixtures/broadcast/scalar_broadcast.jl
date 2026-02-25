# Scalar broadcasting: broadcasting over single numbers should return scalars (Issue #4)

# Sub-problem 1: Float64.(6) should return 6.0 (scalar), not [6.0] (vector)
a = Float64.(6)

# Sub-problem 2: 6 .+ 4 should return 10 (scalar)
b = 6 .+ 4

# Verify results are scalars by using them in arithmetic
# If they were arrays, this arithmetic would fail or produce wrong results
result = a + b  # 6.0 + 10 = 16.0
result