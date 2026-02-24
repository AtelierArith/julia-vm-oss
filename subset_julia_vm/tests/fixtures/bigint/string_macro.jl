# Test big string macro for BigInt (Issue #556)
# big"..." creates a BigInt literal

result = 0.0

# Test 1: Basic big string macro
x = big"123"
if x == big(123)
    result = result + 1.0
end

# Test 2: Large number that doesn't fit in Int64
y = big"9223372036854775808"  # 2^63, just beyond Int64 max
if y > big(9223372036854775807)  # Int64 max
    result = result + 1.0
end

# Test 3: Negative number
z = big"-12345"
if z == big(-12345)
    result = result + 1.0
end

# Test 4: Zero
zero_big = big"0"
if iszero(zero_big)
    result = result + 1.0
end

# Test 5: One
one_big = big"1"
if isone(one_big)
    result = result + 1.0
end

# Test 6: Arithmetic with big string macro result
a = big"100"
b = big"200"
if a + b == big(300)
    result = result + 1.0
end

# Test 7: Very large number
huge = big"123456789012345678901234567890"
if huge > big(0)
    result = result + 1.0
end

# Test 8: BigInt type check
val = big"42"
if typeof(val) == BigInt
    result = result + 1.0
end

result  # Should be 8.0
