# Test append! and prepend! (Issue #2860)
# Based on Julia's base/array.jl:1408, 1428

# append! basic: add elements to end
a1 = [1, 2, 3]
append!(a1, [4, 5, 6])
r1 = (length(a1) == 6 && a1[1] == 1 && a1[4] == 4 && a1[6] == 6)

# append! empty collection (no-op)
a2 = [1, 2, 3]
append!(a2, Int64[])
r2 = (length(a2) == 3 && a2[3] == 3)

# prepend! basic: add elements to beginning
a3 = [4, 5, 6]
prepend!(a3, [1, 2, 3])
r3 = (length(a3) == 6 && a3[1] == 1 && a3[4] == 4 && a3[6] == 6)

r1 && r2 && r3
