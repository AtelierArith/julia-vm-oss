# Test: function should preserve input type through arithmetic
# f(x) = 2x + 1 should return:
# - Float64 when x is Float64
# - Int64 when x is Int64

f(x) = 2x + 1

# Check that f(1) returns Int64, not Float64
typeof(f(1)) === Int64
