# broadcast(f, A, B) with different shapes

f(x, y) = x + y
g(x, y) = x * y

# Test 1: Broadcast with scalar and array
A = [1.0, 2.0, 3.0]
result1 = broadcast(f, A, 10.0)
test1 = result1[1] == 11.0 && result1[2] == 12.0 && result1[3] == 13.0

# Test 2: Broadcast array and scalar (reversed order)
result2 = broadcast(f, 100.0, A)
test2 = result2[1] == 101.0 && result2[2] == 102.0 && result2[3] == 103.0

# Test 3: Two scalars — broadcast(f, a::Number, b::Number) returns scalar
result3 = broadcast(f, 5.0, 3.0)
test3 = result3 == 8.0

# Test 4: Same-shape arrays
B = [10.0, 20.0, 30.0]
result4 = broadcast(g, A, B)
test4 = result4[1] == 10.0 && result4[2] == 40.0 && result4[3] == 90.0

# Test 5: Integer scalar with array
C = [2.0, 4.0, 6.0]
result5 = broadcast(g, C, 2)
test5 = result5[1] == 4.0 && result5[2] == 8.0 && result5[3] == 12.0

test1 && test2 && test3 && test4 && test5
