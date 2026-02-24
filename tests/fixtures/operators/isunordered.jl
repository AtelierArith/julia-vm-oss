# Test isunordered function
# isunordered tests if a value is unordered (NaN for floats, always true for Missing)
# Used internally by isgreater for total ordering

result = 0

# Test 1: Regular integers are ordered
if !isunordered(1)
    result = result + 1
end

# Test 2: Regular floats are ordered
if !isunordered(1.0)
    result = result + 1
end

# Test 3: Zero is ordered
if !isunordered(0)
    result = result + 1
end

# Test 4: Negative numbers are ordered
if !isunordered(-5)
    result = result + 1
end

# Test 5: NaN is unordered
if isunordered(NaN)
    result = result + 1
end

# Test 6: Float64 NaN is unordered
x = 0.0 / 0.0
if isunordered(x)
    result = result + 1
end

# Test 7: Inf is ordered (not NaN)
if !isunordered(Inf)
    result = result + 1
end

# Test 8: -Inf is ordered
if !isunordered(-Inf)
    result = result + 1
end

# Test 9: Missing is unordered
if isunordered(missing)
    result = result + 1
end

result  # Expected: 9
