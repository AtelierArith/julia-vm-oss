# Test user-defined function broadcast on tuples
# This is the key use case: esc.(p) in macros

# Test 1: identity.(tuple)
t1 = (1, 2, 3)
result1 = identity.(t1)
@assert result1 == (1, 2, 3)
println("Test 1 passed: identity.(tuple)")

# Test 2: Custom function
double(x) = x * 2
t2 = (1, 2, 3)
result2 = double.(t2)
@assert result2 == (2, 4, 6)
println("Test 2 passed: double.(tuple)")

println("All user function tuple broadcast tests passed!")
