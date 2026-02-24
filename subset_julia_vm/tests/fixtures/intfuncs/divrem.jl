# Test divrem function (Issue #480)

using Test

# Test divrem basic cases
@test divrem(7, 3) == (2, 1)
@test divrem(3, 7) == (0, 3)
@test divrem(10, 2) == (5, 0)
@test divrem(10, 3) == (3, 1)

# Test divrem with larger numbers
@test divrem(100, 7) == (14, 2)
@test divrem(1000000, 7) == (142857, 1)

# Return true to indicate success
true
