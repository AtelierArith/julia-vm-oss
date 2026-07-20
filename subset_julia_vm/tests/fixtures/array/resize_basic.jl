# Test resize! - resize array to new length (Issue #2860)
# Based on Julia's base/array.jl

# Shrink: resize to smaller length
a1 = [1, 2, 3, 4, 5]
resize!(a1, 3)
r1 = (length(a1) == 3 && a1[1] == 1 && a1[2] == 2 && a1[3] == 3)

# Grow: resize to larger length (new elements are zero-initialized)
a2 = [1, 2, 3]
resize!(a2, 5)
r2 = (length(a2) == 5 && a2[1] == 1 && a2[2] == 2 && a2[3] == 3)

# Resize to same length (no-op)
a3 = [1, 2, 3]
resize!(a3, 3)
r3 = (length(a3) == 3 && a3[1] == 1 && a3[3] == 3)

# Resize to zero
a4 = [1, 2, 3]
resize!(a4, 0)
r4 = (length(a4) == 0)

r1 && r2 && r3 && r4
