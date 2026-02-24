# Test typemax and typemin functions (Issue #480)

using Test

# Test typemax
@test typemax(Int64) == 9223372036854775807

# Test typemin (use computation to avoid literal parsing issues)
@test typemin(Int64) == 0 - 9223372036854775807 - 1

# Return true to indicate success
true
