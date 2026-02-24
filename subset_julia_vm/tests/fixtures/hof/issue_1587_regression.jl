# Regression test for Issue #1587: Arrow functions in stdlib loading
#
# The exact reported failure was:
#   map(_ -> Complex(0.0, 0.0), 1:m)
# which raised: UnsupportedFeature { kind: UnsupportedExpression("arrow_function_expression") }
#
# Arrow functions are now fully supported in the lowering phase.

# Exact scenario from Issue #1587: underscore parameter with constant Complex result
m = 3
y = map(_ -> Complex(0.0, 0.0), 1:m)
@assert length(y) == 3
@assert y[1] == Complex(0.0, 0.0)
@assert y[2] == Complex(0.0, 0.0)
@assert y[3] == Complex(0.0, 0.0)
@assert real(y[1]) == 0.0
@assert imag(y[1]) == 0.0

# Verify with different range lengths
y2 = map(_ -> Complex(1.0, -1.0), 1:5)
@assert length(y2) == 5
@assert y2[1] == Complex(1.0, -1.0)
@assert y2[5] == Complex(1.0, -1.0)

# Verify the underscore arrow function works with array input too
y3 = map(_ -> Complex(0.0, 0.0), [10, 20, 30])
@assert length(y3) == 3
@assert y3[1] == Complex(0.0, 0.0)

true
