# Test deleteat! - delete element at index (Issue #2860)
# Based on Julia's base/array.jl:1880

# Delete middle element: [1,2,3,4,5] -> delete at 3 -> [1,2,4,5]
a1 = [1, 2, 3, 4, 5]
deleteat!(a1, 3)
r1 = (length(a1) == 4 && a1[1] == 1 && a1[2] == 2 && a1[3] == 4 && a1[4] == 5)

# Delete first element
a2 = [10, 20, 30]
deleteat!(a2, 1)
r2 = (length(a2) == 2 && a2[1] == 20 && a2[2] == 30)

# Delete last element
a3 = [1, 2, 3]
deleteat!(a3, 3)
r3 = (length(a3) == 2 && a3[1] == 1 && a3[2] == 2)

r1 && r2 && r3
