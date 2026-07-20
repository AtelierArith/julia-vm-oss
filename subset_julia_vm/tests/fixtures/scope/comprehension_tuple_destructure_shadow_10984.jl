using Test

# Issue #10984: a tuple-destructuring comprehension `[expr for (a, b) in
# iter]` binds `a`/`b` as fresh per-comprehension locals; each must SHADOW a
# same-named live outer local for the comprehension's lifetime, not
# overwrite/leak into it. Same class as #10903 (for-loop induction var leak)
# but through `compile_tuple_destructuring_comprehension`, a separate codegen
# path from the single-variable comprehension fixed alongside #10903.
# Verified against `julia --startup-file=no` (1.12.6): prints
# `([1], "outer")`.
function h()
    a = "outer"
    r = [a for (a, b) in [(1, 2)]]
    return (r, a)
end

result = h()
@test result[1] == [1]
@test result[2] == "outer"

true
