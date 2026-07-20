using Test

# Issue #10984: a tuple-destructuring `for (a, b) in iterable` loop's
# induction variables must shadow same-named outer locals just like a
# single-variable `for`/`foreach` loop. Verified against
# `julia --startup-file=no` (1.12.6): prints `("outerA", "outerB")`.
function tuple_foreach_shadow()
    a = "outerA"
    b = "outerB"
    for (a, b) in [(1, 2), (3, 4)]
    end
    return (a, b)
end

result = tuple_foreach_shadow()
@test result == ("outerA", "outerB")

true
