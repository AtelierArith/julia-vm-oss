# Tuple broadcast tests: f.(tuple) -> tuple
# This tests the ability to apply a function to each element of a tuple

# Test 1: map over tuple with identity
t1 = (1, 2, 3)
result1 = map(identity, t1)
@assert result1 == (1, 2, 3)
println("Test 1 passed: map(identity, tuple)")

# Test 2: map with abs over tuple
t2 = (-1, -2, -3)
result2 = map(abs, t2)
@assert result2 == (1, 2, 3)
println("Test 2 passed: map(abs, tuple)")

# Test 3: empty tuple
t3 = ()
result3 = map(identity, t3)
@assert result3 == ()
println("Test 3 passed: empty tuple")

# Test 4: single element tuple
t4 = (42,)
result4 = map(identity, t4)
@assert result4 == (42,)
println("Test 4 passed: single element tuple")

# Test 5: tuple with mixed types (symbols)
t5 = (:a, :b, :c)
result5 = map(identity, t5)
@assert result5 == (:a, :b, :c)
println("Test 5 passed: symbol tuple")

println("All tuple broadcast tests passed!")
