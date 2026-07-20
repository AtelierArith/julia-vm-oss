using Test

# Issue #10903 / #10984: a comprehension induction variable that shares a
# name with an already-live local in the enclosing scope must SHADOW that
# local for the comprehension's lifetime, not overwrite/leak into it.
# Verified against `julia --startup-file=no` (1.12.6): prints
# `([1, 4, 9], "outer")`.
function g()
    x = "outer"
    r = [x^2 for x in 1:3]
    return (r, x)
end

result = g()
@test result[1] == [1, 4, 9]
@test result[2] == "outer"

true
