# Test tuple broadcast with extra arguments (Issue #417)
# broadcast((x, y) -> x + y, (1, 2), 10) should return (11, 12)

result = 0.0

# Test 1: broadcast with tuple and scalar extra arg
t1 = broadcast((x, y) -> x + y, (1, 2, 3), 10)
if t1[1] == 11 && t1[2] == 12 && t1[3] == 13
    result = result + 1.0
end

# Test 2: broadcast with tuple and scalar using multiplication
t2 = broadcast((x, y) -> x * y, (2, 3, 4), 10)
if t2[1] == 20 && t2[2] == 30 && t2[3] == 40
    result = result + 1.0
end

# Test 3: broadcast with tuple and multiple extra args
t3 = broadcast((x, a, b) -> x + a + b, (1, 2), 10, 100)
if t3[1] == 111 && t3[2] == 112
    result = result + 1.0
end

# Test 4: broadcast with tuple using subtraction
t4 = broadcast((x, y) -> x - y, (10, 20, 30), 5)
if t4[1] == 5 && t4[2] == 15 && t4[3] == 25
    result = result + 1.0
end

# Test 5: broadcast with Float64 extra arg
t5 = broadcast((x, y) -> x + y, (1.0, 2.0), 0.5)
if t5[1] == 1.5 && t5[2] == 2.5
    result = result + 1.0
end

result  # Should be 5.0
