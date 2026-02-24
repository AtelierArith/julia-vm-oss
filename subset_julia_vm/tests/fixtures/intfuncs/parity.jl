# Test iseven and isodd functions (Issue #480)

using Test

# Test isodd with positive numbers
@test isodd(1) == true
@test isodd(3) == true
@test isodd(9) == true
@test isodd(0) == false
@test isodd(2) == false
@test isodd(10) == false

# Test iseven with positive numbers
@test iseven(0) == true
@test iseven(2) == true
@test iseven(10) == true
@test iseven(1) == false
@test iseven(3) == false
@test iseven(9) == false

# Return true to indicate success
true
