# broadcast!(f, dest, A, B) in-place broadcast tests

# Simple addition function
f(x, y) = x + y

# Test 1: Basic in-place broadcast
A = [1.0, 2.0, 3.0]
B = [10.0, 20.0, 30.0]
dest1 = zeros(3)
broadcast!(f, dest1, A, B)
test1 = dest1[1] == 11.0 && dest1[2] == 22.0 && dest1[3] == 33.0

# Test 2: In-place with scalar
C = [1.0, 2.0, 3.0]
dest2 = zeros(3)
broadcast!(f, dest2, C, 10.0)
test2 = dest2[1] == 11.0 && dest2[2] == 12.0 && dest2[3] == 13.0

# Test 3: Multiplication function
g(x, y) = x * y
D = [2.0, 3.0, 4.0]
E = [5.0, 6.0, 7.0]
dest3 = zeros(3)
broadcast!(g, dest3, D, E)
test3 = dest3[1] == 10.0 && dest3[2] == 18.0 && dest3[3] == 28.0

# Test 4: Verify original arrays unchanged
A_copy = [1.0, 2.0, 3.0]
B_copy = [10.0, 20.0, 30.0]
dest4 = zeros(3)
broadcast!(f, dest4, A_copy, B_copy)
test4 = A_copy[1] == 1.0 && B_copy[1] == 10.0  # Originals unchanged

# Test 5: Return value is the destination array
dest5 = zeros(3)
result5 = broadcast!(f, dest5, [1.0, 2.0, 3.0], [10.0, 20.0, 30.0])
test5 = result5[1] == 11.0 && result5[2] == 22.0 && result5[3] == 33.0

test1 && test2 && test3 && test4 && test5
