# broadcast(f, A, B) basic tests

# Simple addition function
f(x, y) = x + y

# Test 1: broadcast over two arrays of same shape
A = [1.0, 2.0, 3.0]
B = [10.0, 20.0, 30.0]
result1 = broadcast(f, A, B)
test1 = result1[1] == 11.0 && result1[2] == 22.0 && result1[3] == 33.0

# Test 2: broadcast with scalar
C = [1.0, 2.0, 3.0]
result2 = broadcast(f, C, 10.0)
test2 = result2[1] == 11.0 && result2[2] == 12.0 && result2[3] == 13.0

# Test 3: broadcast scalar with array
result3 = broadcast(f, 100.0, B)
test3 = result3[1] == 110.0 && result3[2] == 120.0 && result3[3] == 130.0

# Test 4: Multiplication function
g(x, y) = x * y
D = [2.0, 3.0, 4.0]
E = [5.0, 6.0, 7.0]
result4 = broadcast(g, D, E)
test4 = result4[1] == 10.0 && result4[2] == 18.0 && result4[3] == 28.0

# Test 5: Custom function with more complex logic
h(x, y) = x * x + y
F = [1.0, 2.0, 3.0]
G = [10.0, 20.0, 30.0]
result5 = broadcast(h, F, G)
test5 = result5[1] == 11.0 && result5[2] == 24.0 && result5[3] == 39.0

test1 && test2 && test3 && test4 && test5
