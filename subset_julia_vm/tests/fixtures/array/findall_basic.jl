# Test findall function for finding indices where values are true
# findall(A) - returns indices where A[i] is true

# =============================================================================
# findall(A::Array{Bool}) - single argument for boolean arrays
# =============================================================================

# Test basic boolean array
b1 = [true, false, true, false, true]
result1 = findall(b1)
check1 = length(result1) == 3 && result1[1] == 1 && result1[2] == 3 && result1[3] == 5

# Test all false
b2 = [false, false, false]
result2 = findall(b2)
check2 = length(result2) == 0

# Test all true
b3 = [true, true, true]
result3 = findall(b3)
check3 = length(result3) == 3 && result3[1] == 1 && result3[2] == 2 && result3[3] == 3

# Test single element
b4 = [true]
result4 = findall(b4)
check4 = length(result4) == 1 && result4[1] == 1

b5 = [false]
result5 = findall(b5)
check5 = length(result5) == 0

# Test alternating pattern
b6 = [false, true, false, true, false, true]
result6 = findall(b6)
check6 = length(result6) == 3 && result6[1] == 2 && result6[2] == 4 && result6[3] == 6

# Test first and last only
b7 = [true, false, false, false, true]
result7 = findall(b7)
check7 = length(result7) == 2 && result7[1] == 1 && result7[2] == 5

# =============================================================================
# Final check
# =============================================================================

check1 && check2 && check3 && check4 && check5 && check6 && check7
