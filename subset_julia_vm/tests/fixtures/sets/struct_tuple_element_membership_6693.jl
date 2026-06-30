# Issue #6693 (Set half): Set elements that are tuples — including tuples that
# contain immutable structs like `OneTo`, and bare immutable structs — must hash
# and compare by structural value, so construction, membership (`in`), dedup,
# delete!, iteration, and set algebra all work. Previously `Set([(1,2)])` raised
# "Invalid dictionary key" (DictKey had no composite variant) and struct-bearing
# elements compared by heap index. Composite keys are now `DictKey::Composite`
# (heap struct refs resolved before keying); `in` over a Set routes through the
# shared value-equality helper.
#
# NOTE: the parametric element type display is intentionally not asserted here —
# `typeof(Set([(1,2)]))` renders as `Set{Tuple}` rather than
# `Set{Tuple{Int64,Int64}}` (a known limitation; the full element type is not
# recovered by the composite key).

checks = Bool[]

# tuple elements: construction, membership, dedup
s = Set([(1, 2), (3, 4)])
push!(checks, (1, 2) in s)
push!(checks, (3, 4) in s)
push!(checks, !((5, 6) in s))
push!(checks, length(s) == 2)
push!(s, (1, 2))            # duplicate — no growth
push!(checks, length(s) == 2)
push!(s, (7, 8))
push!(checks, length(s) == 3)
delete!(s, (1, 2))
push!(checks, !((1, 2) in s))
push!(checks, length(s) == 2)

# tuple containing an immutable struct, separately constructed
s2 = Set([(Base.OneTo(3),)])
push!(checks, (Base.OneTo(3),) in s2)
push!(checks, !((Base.OneTo(4),) in s2))

# bare immutable struct elements
s3 = Set([Base.OneTo(2), Base.OneTo(3)])
push!(checks, Base.OneTo(2) in s3)
push!(checks, !(Base.OneTo(5) in s3))
push!(checks, length(s3) == 2)
push!(s3, Base.OneTo(2))    # duplicate immutable struct — no growth
push!(checks, length(s3) == 2)

# iteration over a tuple-keyed Set
total = 0
for (a, b) in Set([(1, 2), (3, 4)])
    global total += a + b
end
push!(checks, total == 10)

# set algebra (pure-Julia) over composite elements
a = Set([(1, 2), (3, 4)])
b = Set([(3, 4), (5, 6)])
push!(checks, length(intersect(a, b)) == 1)
push!(checks, length(union(a, b)) == 3)
push!(checks, length(setdiff(a, b)) == 1)

# struct set algebra
sa = Set([Base.OneTo(2), Base.OneTo(3)])
sb = Set([Base.OneTo(3), Base.OneTo(4)])
push!(checks, length(intersect(sa, sb)) == 1)

all(checks)
