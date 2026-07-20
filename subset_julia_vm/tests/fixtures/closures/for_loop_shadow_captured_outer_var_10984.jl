using Test

# Issue #10984: a closure created BEFORE a `for` loop that shadows the same
# name must keep seeing the outer (pre-loop) value both during and after the
# loop — the loop's induction variable is a completely separate binding that
# does not alias the closure's captured variable. Verified against
# `julia --startup-file=no` (1.12.6): prints `("outer", "outer")`.
function closure_shadow()
    i = "outer"
    g = () -> i
    for i in 1:3
    end
    return (g(), i)
end

result = closure_shadow()
@test result == ("outer", "outer")

true
