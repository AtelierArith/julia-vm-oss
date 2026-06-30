# Test deleteat!(a, inds) with a Vector/Range of indices (Issue #5738)
# Based on Julia's base/array.jl deleteat!(a, inds::AbstractVector)

# Delete by a vector of indices: [1,2,3,4] -> delete 1 and 3 -> [2,4]
a1 = [1, 2, 3, 4]
deleteat!(a1, [1, 3])
r1 = (a1 == [2, 4])

# Delete by a range of indices: [1,2,3,4,5] -> delete 2:3 -> [1,4,5]
a2 = [1, 2, 3, 4, 5]
deleteat!(a2, 2:3)
r2 = (a2 == [1, 4, 5])

# Non-contiguous vector indices on a longer array
a3 = [10, 20, 30, 40, 50]
deleteat!(a3, [2, 4])
r3 = (a3 == [10, 30, 50])

# Empty index vector leaves the array unchanged
a4 = [1, 2, 3]
deleteat!(a4, Int[])
r4 = (a4 == [1, 2, 3])

# Return value is the mutated array itself
a5 = [1, 2, 3, 4, 5, 6]
ret = deleteat!(a5, [2, 3, 5])
r5 = (ret == [1, 4, 6] && a5 == [1, 4, 6])

# Scalar index form still works (regression guard)
a6 = [1, 2, 3, 4]
deleteat!(a6, 2)
r6 = (a6 == [1, 3, 4])

# Deleting all elements via a full range yields an empty array
a7 = [1, 2, 3]
deleteat!(a7, 1:3)
r7 = (length(a7) == 0)

r1 && r2 && r3 && r4 && r5 && r6 && r7
