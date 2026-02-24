# Test precision() for BigFloat type and values (Issue #345)

# Test precision of the type (default precision)
default_prec = precision(BigFloat)
result1 = default_prec == 256

# Test precision of a BigFloat value
x = BigFloat(1.5)
prec_x = precision(x)
result2 = prec_x == 256

result1 && result2
