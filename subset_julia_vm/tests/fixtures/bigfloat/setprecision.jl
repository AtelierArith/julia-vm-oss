# Test setprecision() for BigFloat (Issue #345)

# Save initial precision
initial_prec = precision(BigFloat)
result1 = initial_prec == 256

# Test setprecision with type
setprecision(BigFloat, 128)
result2 = precision(BigFloat) == 128

# Test that new BigFloats use the new precision
x = BigFloat(3.14159)
result3 = precision(x) == 128

# Test setprecision convenience form
setprecision(512)
result4 = precision(BigFloat) == 512

# Restore initial precision
setprecision(BigFloat, initial_prec)
result5 = precision(BigFloat) == initial_prec

result1 && result2 && result3 && result4 && result5
