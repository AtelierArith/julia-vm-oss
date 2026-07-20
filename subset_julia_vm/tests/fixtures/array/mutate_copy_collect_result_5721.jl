# Issue #5721: insert!/deleteat!/pushfirst! must work on copy()/collect() results
# (Memory-backed `Array` wrappers / StructRef), matching push! which already did.
# Previously these raised "expected Array, got StructRef".

# Direct (non-variable) forms on collect/copy results.
r1 = (insert!(collect(1:3), 1, 0) == [0, 1, 2, 3])
r2 = (insert!(copy([1, 2, 3]), 2, 99) == [1, 99, 2, 3])
r3 = (deleteat!(collect(1:4), 2) == [1, 3, 4])
r4 = (pushfirst!(copy([1, 2, 3]), 0) == [0, 1, 2, 3])
r5 = (push!(collect(1:3), 9) == [1, 2, 3, 9])

# In-place via a variable bound to a collect/copy result.
x = collect(1:3)
insert!(x, 1, 0)
r6 = (x == [0, 1, 2, 3])

y = copy([1, 2, 3])
deleteat!(y, 2)
r7 = (y == [1, 3])

z = collect(1:3)
pushfirst!(z, 0)
r8 = (z == [0, 1, 2, 3])

# Native array literals still work (regression guard).
a = [1, 2, 3]
insert!(a, 2, 99)
r9 = (a == [1, 99, 2, 3])

b = [1, 2, 3]
deleteat!(b, 1)
r10 = (b == [2, 3])

# Return value is the mutated array itself.
c = collect(1:3)
ret = insert!(c, 2, 7)
r11 = (ret == [1, 7, 2, 3] && c == [1, 7, 2, 3])

r1 && r2 && r3 && r4 && r5 && r6 && r7 && r8 && r9 && r10 && r11
