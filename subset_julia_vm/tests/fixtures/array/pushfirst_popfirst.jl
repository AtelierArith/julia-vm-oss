# Test pushfirst! and popfirst! (Issue #2860)
# Based on Julia's base/array.jl:1595, 1617

# pushfirst!: add element to front
a1 = [2, 3, 4]
pushfirst!(a1, 1)
r1 = (length(a1) == 4 && a1[1] == 1 && a1[2] == 2 && a1[4] == 4)

# Multiple pushfirst! calls
a2 = Int64[]
pushfirst!(a2, 3)
pushfirst!(a2, 2)
pushfirst!(a2, 1)
r2 = (length(a2) == 3 && a2[1] == 1 && a2[2] == 2 && a2[3] == 3)

# popfirst!: remove and return first element
a3 = [1, 2, 3, 4]
v3 = popfirst!(a3)
r3 = (v3 == 1 && length(a3) == 3 && a3[1] == 2 && a3[3] == 4)

r1 && r2 && r3
