using Test

# Issue #10903 / #10984: a `for`-loop induction variable that shares a name
# with an already-live local in the enclosing scope must SHADOW that local
# for the loop's lifetime, not overwrite/leak into it. Verified against
# `julia --startup-file=no` (1.12.6): prints `outer`.
function f()
    i = "outer"
    for i in 1:3
    end
    return i
end

@test f() == "outer"

true
