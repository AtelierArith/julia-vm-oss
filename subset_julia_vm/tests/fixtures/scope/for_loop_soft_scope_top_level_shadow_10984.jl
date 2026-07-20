using Test

# Issue #10984: the shadow/restore fix must also hold at top level (Julia's
# "soft scope"), not just inside a function body (hard scope) — a `for`
# loop's induction variable shadows a pre-existing same-named top-level
# global too. Verified against `julia --startup-file=no` (1.12.6): prints
# `outer_top`.
i_top = "outer_top"
for i_top in 1:3
end

@test i_top == "outer_top"

true
