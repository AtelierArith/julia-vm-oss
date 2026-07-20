using Test

# Issue #10984: nested `for` loops that both introduce a fresh binding for
# the SAME name must each shadow their own enclosing scope independently —
# the outer local is restored after the inner loop, then again after the
# outer loop. Verified against `julia --startup-file=no` (1.12.6): prints
# `outer`.
function nested_same_name()
    i = "outer"
    for i in 1:2
        for i in 10:11
        end
    end
    return i
end

@test nested_same_name() == "outer"

true
