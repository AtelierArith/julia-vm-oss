# Prevention test: Short-form function definitions with varargs (Issue #1721)
# Verifies that short-form varargs functions `f(args...) = expr` work identically
# to full-form `function f(args...) ... end`

using Test

# Short-form varargs: basic sum
sum_short(args...) = sum(args)

# Short-form varargs: count
count_short(args...) = length(args)

# Short-form varargs: pass-through to another function
function add2(a, b)
    a + b
end
apply_short(f, args...) = f(args...)

# Short-form varargs: no arguments (empty tuple)
empty_short(args...) = length(args)

# Short-form varargs: with non-varargs params before
first_plus_rest(x, rest...) = x + sum(rest)

# Pre-compute results outside @testset
r_sum = sum_short(1, 2, 3)
r_count = count_short(10, 20, 30, 40)
r_apply = apply_short(add2, 3, 4)
r_empty = empty_short()
r_first_plus = first_plus_rest(10, 1, 2, 3)

@testset "Short-form function varargs (Issue #1721)" begin
    # Basic sum via short-form varargs
    @test r_sum == 6

    # Count arguments
    @test r_count == 4

    # HOF pattern with short-form varargs
    @test r_apply == 7

    # Zero arguments becomes empty tuple
    @test r_empty == 0

    # Mixed params: regular + varargs in short form
    @test r_first_plus == 16
end

true
