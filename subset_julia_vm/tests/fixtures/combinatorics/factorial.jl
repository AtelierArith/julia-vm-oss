# Test factorial function
# Issue #507: factorial for BigInt

using Test

# Test factorial for small integers (all fit in i64)
@test factorial(0) == 1
@test factorial(1) == 1
@test factorial(2) == 2
@test factorial(3) == 6
@test factorial(4) == 24
@test factorial(5) == 120
@test factorial(10) == 3628800
@test factorial(20) == 2432902008176640000

# Test factorial for BigInt input
@test factorial(big(0)) == big(1)
@test factorial(big(5)) == big(120)
@test factorial(big(10)) == big(3628800)
@test factorial(big(20)) == big(2432902008176640000)

# Test larger BigInt factorial (doesn't overflow)
# factorial(21) on Int64 would overflow, but factorial(big(21)) works
result_big_21 = factorial(big(21))
println(result_big_21)  # Should print: 51090942171709440000

result_big_25 = factorial(big(25))
println(result_big_25)  # Should print: 15511210043330985984000000

# Return true to indicate success
true
