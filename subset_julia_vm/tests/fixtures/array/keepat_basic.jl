# Test keepat! - keep elements at specified indices (Issue #2860)
# Based on Julia's base/array.jl:3078

# keepat! with integer indices: keep elements at positions [1, 3, 5]
a1 = [1, 2, 3, 4, 5]
keepat!(a1, [1, 3, 5])
r1 = (length(a1) == 3 && a1[1] == 1 && a1[2] == 3 && a1[3] == 5)

# keepat! with boolean mask: keep where mask is true
a2 = [10, 20, 30, 40, 50]
keepat!(a2, [true, false, true, false, true])
r2 = (length(a2) == 3 && a2[1] == 10 && a2[2] == 30 && a2[3] == 50)

# keepat! all indices (no-op)
a3 = [1, 2, 3]
keepat!(a3, [1, 2, 3])
r3 = (length(a3) == 3 && a3[1] == 1 && a3[2] == 2 && a3[3] == 3)

r1 && r2 && r3
