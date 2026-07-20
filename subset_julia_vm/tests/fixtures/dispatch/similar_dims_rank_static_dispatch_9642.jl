using Test

# Regression test for Issue #9642:
# `similar(a, dims...)` compile-time return type used to reuse the *source*
# array `a`'s rank (`self.infer_expr_type(&args[0])`) instead of the number
# of explicit dims, matching upstream's `to_shape(dims)`
# (`julia/base/abstractarray.jl`), whose rank is the dims count and NOT the
# source's rank. A call `rank_dispatch(similar(a, dims...))` could then
# statically bind to the wrong rank-specialized method.

rank_dispatch(x::Array{Int64, 1}) = 1
rank_dispatch(x::Array{Int64, 2}) = 2
rank_dispatch(x::Array{Int64, 3}) = 3

# Original MWE: 1-D source, 3 dims -> rank 3 (was wrongly statically bound
# to the rank-1 method before the fix).
a3 = similar([1, 2, 3], 1, 3, 1)
@test rank_dispatch(a3) == 3
@test size(a3) == (1, 3, 1)

# 1-D source, 2 dims -> rank 2.
a2 = similar([1, 2, 3], 2, 3)
@test rank_dispatch(a2) == 2
@test size(a2) == (2, 3)

# 1-D source, 1 dim -> rank 1 (same rank as source here, but driven by the
# dims count, not a rank passthrough).
a1 = similar([1, 2, 3], 5)
@test rank_dispatch(a1) == 1
@test size(a1) == (5,)

# 2-D source, 1 dim -> rank 1 Vector even though the source is a Matrix.
m2 = [1 2; 3 4]
b1 = similar(m2, 5)
@test rank_dispatch(b1) == 1
@test size(b1) == (5,)

# 2-D source, 2 dims -> rank 2, independent of the source's own shape.
b2 = similar(m2, 3, 2)
@test rank_dispatch(b2) == 2
@test size(b2) == (3, 2)

# A single tuple dims arg with a statically known arity resolves the rank
# from the tuple's arity, not the source's rank.
b3 = similar([1, 2, 3], (2, 3))
@test rank_dispatch(b3) == 2

# No dims: same shape/rank as the source (unaffected control case) — must
# stay statically bound and correct for a 1-D source. (`similar(a)` on a
# 2-D matrix-literal source has a separate, pre-existing static-dispatch
# rank bug — Issue #10076 — untouched by this fix and not exercised here.)
c1 = similar([1, 2, 3])
@test rank_dispatch(c1) == 1

# Statically-known argument type (not routed through `similar` at all) must
# remain correctly and precisely statically bound — a control against
# over-widening the fix.
d1 = [10, 20, 30]
@test rank_dispatch(d1) == 1
d2 = [1 2 3; 4 5 6]
@test rank_dispatch(d2) == 2

# Element type must still round-trip through the fixed rank-tracking path.
e3 = similar([1.0, 2.0, 3.0], 2, 2)
@test eltype(e3) == Float64
@test size(e3) == (2, 2)

true
