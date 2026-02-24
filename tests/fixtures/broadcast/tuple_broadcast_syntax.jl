# Test f.(tuple) broadcast syntax
# This should be equivalent to map(f, tuple)

# Test 1: abs.(tuple) - returns Float64
t1 = (-1, -2, -3)
result1 = abs.(t1)
# abs returns Float64, so compare with floats
@assert result1 == (1.0, 2.0, 3.0)
println("Test 1 passed: abs.(tuple)")

# Test 2: sqrt.(tuple)
t2 = (1.0, 4.0, 9.0)
result2 = sqrt.(t2)
@assert result2 == (1.0, 2.0, 3.0)
println("Test 2 passed: sqrt.(tuple)")

# Test 3: sin.(tuple)
t3 = (0.0,)
result3 = sin.(t3)
@assert result3 == (0.0,)
println("Test 3 passed: sin.(tuple)")

println("All f.(tuple) syntax tests passed!")
