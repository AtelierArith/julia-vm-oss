# Issue #7803: `==` on tuples whose elements are mutable Vectors must compare the
# vectors structurally (element by value), not by heap identity. Two separately
# constructed but equal `Vector`s compare equal (`[a] == [a]` is `true`), so a
# tuple holding them must too. Previously sjulia's native tuple-`==` fold compared
# Vector elements by heap index, so `([0.2, 0.5, 0.8],) == ([0.2, 0.5, 0.8],)`
# returned `false` even though the inner vectors are `==`. This regressed package
# APIs such as `params(PoissonBinomial(...))`, where upstream returns a tuple
# containing the probability vector. Resolved by the tuple-`==` value-fold work;
# all assertions match upstream Julia 1.12.

checks = Bool[]

# --- single Vector element (the exact MWE) -------------------------------
push!(checks, ([0.2, 0.5, 0.8],) == ([0.2, 0.5, 0.8],))
push!(checks, !(([0.2, 0.5, 0.8],) == ([0.2, 0.5, 0.9],)))

# --- multiple Vector elements --------------------------------------------
push!(checks, ([1, 2], [3, 4]) == ([1, 2], [3, 4]))
push!(checks, !(([1, 2], [3, 4]) == ([1, 2], [3, 5])))

# --- Vector elements with different concrete element types (Issue #10631) -
push!(checks, (Any[1, 2], Any[3, 4]) == ([1, 2], [3, 4]))
push!(checks, !((Any[1, 2], Any[3, 4]) == ([1, 2], [3, 5])))
push!(checks, ismissing((Any[missing],) == ([missing],)))

# --- mixed Vector + primitive elements -----------------------------------
push!(checks, ([1.0, 2.0], 3) == ([1.0, 2.0], 3))
push!(checks, !(([1.0, 2.0], 3) == ([1.0, 2.0], 4)))

# --- nested tuple holding a Vector ---------------------------------------
push!(checks, (([1, 2],),) == (([1, 2],),))

# --- `!=` mirrors `==` ----------------------------------------------------
push!(checks, !(([0.2, 0.5, 0.8],) != ([0.2, 0.5, 0.8],)))
push!(checks, ([0.2, 0.5, 0.8],) != ([0.2, 0.5, 0.9],))

all(checks)
