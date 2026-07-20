# Test splice! - remove (and optionally replace) element at index (Issue #2860)
# Based on Julia's base/array.jl

# splice!(a, i) - remove element at index i, return it
a1 = [1, 2, 3, 4, 5]
v1 = splice!(a1, 3)
r1 = (v1 == 3 && length(a1) == 4 && a1[1] == 1 && a1[3] == 4 && a1[4] == 5)

# splice!(a, i, val) - replace element at index i with val, return old value
a2 = [1, 2, 3, 4, 5]
v2 = splice!(a2, 3, 99)
r2 = (v2 == 3 && length(a2) == 5 && a2[3] == 99 && a2[2] == 2 && a2[4] == 4)

# splice! at first index
a3 = [10, 20, 30]
v3 = splice!(a3, 1)
r3 = (v3 == 10 && length(a3) == 2 && a3[1] == 20)

r1 && r2 && r3
