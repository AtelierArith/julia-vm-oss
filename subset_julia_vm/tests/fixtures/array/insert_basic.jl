# Test insert! - insert element at index (Issue #2860)
# Based on Julia's base/array.jl:1644

# Insert in the middle: [1,2,3,4,5] -> insert 10 at index 3 -> [1,2,10,3,4,5]
a1 = [1, 2, 3, 4, 5]
insert!(a1, 3, 10)
r1 = (length(a1) == 6 && a1[1] == 1 && a1[2] == 2 && a1[3] == 10 && a1[4] == 3 && a1[6] == 5)

# Insert at beginning
a2 = [2, 3, 4]
insert!(a2, 1, 1)
r2 = (length(a2) == 4 && a2[1] == 1 && a2[2] == 2)

# Insert at end (index == length + 1)
a3 = [1, 2, 3]
insert!(a3, 4, 99)
r3 = (length(a3) == 4 && a3[4] == 99 && a3[3] == 3)

r1 && r2 && r3
