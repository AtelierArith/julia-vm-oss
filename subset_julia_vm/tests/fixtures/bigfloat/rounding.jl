# Test rounding() and setrounding() for BigFloat (Issue #345)

# Test default rounding mode (RoundNearest)
mode = rounding(BigFloat)
result1 = mode.mode == :Nearest

# Test setrounding with different modes
setrounding(BigFloat, RoundToZero)
result2 = rounding(BigFloat).mode == :ToZero

setrounding(BigFloat, RoundUp)
result3 = rounding(BigFloat).mode == :Up

setrounding(BigFloat, RoundDown)
result4 = rounding(BigFloat).mode == :Down

setrounding(BigFloat, RoundFromZero)
result5 = rounding(BigFloat).mode == :FromZero

# Reset to default mode
setrounding(BigFloat, RoundNearest)
result6 = rounding(BigFloat).mode == :Nearest

result1 && result2 && result3 && result4 && result5 && result6
