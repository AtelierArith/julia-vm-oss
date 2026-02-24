# Test isless function for Missing type

# Test isless(::Missing, ::Missing) = false (missing is not less than itself)
result1 = isless(missing, missing)

# Test isless(::Missing, ::Any) = false (missing is not less than anything)
result2 = isless(missing, 1)

# Test isless(::Any, ::Missing) = true (everything is less than missing)
result3 = isless(1, missing)

# Verify all results
result1 == false && result2 == false && result3 == true
