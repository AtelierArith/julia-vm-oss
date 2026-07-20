# Test findnext and findprev with predicate function (Issue #2860, #2109)
# Based on Julia's base/array.jl

a = [1, 2, 3, 2, 1]

# findnext: find first index >= start where predicate is true
is2 = x -> x == 2
r1 = (findnext(is2, a, 1) == 2)
r2 = (findnext(is2, a, 3) == 4)
r3 = (findnext(is2, a, 5) === nothing)

# findnext for element not in array
is9 = x -> x == 9
r4 = (findnext(is9, a, 1) === nothing)

# findprev: find last index <= start where predicate is true
r5 = (findprev(is2, a, 5) == 4)
r6 = (findprev(is2, a, 3) == 2)
r7 = (findprev(is2, a, 1) === nothing)

r1 && r2 && r3 && r4 && r5 && r6 && r7
