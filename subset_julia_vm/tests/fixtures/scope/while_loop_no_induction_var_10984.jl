using Test

# Issue #10984 regression guard: `while` introduces no induction variable of
# its own, so a pre-existing local read/written inside the loop body must
# NOT be shadowed/restored — it is an ordinary reassignment, and the
# post-loop value must be the loop's final value. Verified against
# `julia --startup-file=no` (1.12.6): prints `("outer", 3)`.
function while_no_shadow()
    i = "outer"
    j = 0
    while j < 3
        j += 1
    end
    return (i, j)
end

result = while_no_shadow()
@test result == ("outer", 3)

true
