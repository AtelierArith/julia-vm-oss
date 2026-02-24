# Test isequal and isless functions for Missing type

# Test isequal(::Missing, ::Missing) = true
result1 = isequal(missing, missing)

# Test isequal(::Missing, ::Any) = false
result2 = isequal(missing, 1)

# Test isequal(::Any, ::Missing) = false
result3 = isequal(1, missing)

# Verify isequal results
result1 == true && result2 == false && result3 == false
