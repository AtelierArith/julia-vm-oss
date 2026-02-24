# Test big string literals (Issue #554)
# Tests the big"..." string macro for BigInt and BigFloat

using Test

# Test 1: big"..." with integer content creates BigInt
x = big"1234"
@test typeof(x) == BigInt
@test x == 1234

# Test 2: big"..." with decimal content creates BigFloat
y = big"100.0"
@test typeof(y) == BigFloat

# Test 3: big"..." with scientific notation creates BigFloat
z = big"1e100"
@test typeof(z) == BigFloat

# Test 4: big"..." with large integer
large_int = big"99999999999999999999999999999999"
@test typeof(large_int) == BigInt

# Test 5: big"..." with E (uppercase) creates BigFloat
w = big"1E10"
@test typeof(w) == BigFloat

# Test 6: basic BigInt arithmetic still works
a = big"10"
b = big"20"
@test a + b == 30

# Return true to indicate success
true
