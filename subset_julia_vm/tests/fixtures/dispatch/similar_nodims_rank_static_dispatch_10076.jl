using Test

# Regression test for Issue #10076:
# `similar(a)` (no explicit dims) computes its compile-time return type as
# `self.infer_expr_type(&args[0])` — the source array `a`'s own inferred
# `ValueType`. For a 2-D (or higher) matrix-literal source, the array-literal
# ValueType inference used to always erase the rank to `None` (both in
# `infer_expr_type`'s `Expr::ArrayLiteral` arm and in the parallel
# `compile_expr` bytecode-emission arm that actually populates a variable's
# stored local type), unlike the separate `infer_julia_type`, whose
# `Expr::ArrayLiteral` arm correctly computes the rank from `shape.len()`.
# A rank-dispatched call on `similar(a)`'s result then statically bound to
# the `Array{T,1}` method instead of `Array{T,2}`, even though the runtime
# value is a correctly-shaped `Matrix`. Unaffected by the #9642 fix, which
# only touched the explicit-dims `similar(a, dims...)` code path.

rank_dispatch(x::Array{Int64, 1}) = 1
rank_dispatch(x::Array{Int64, 2}) = 2
rank_dispatch3(x::Array{Int64, 3}) = 3

# NOTE: a genuine 3-D array-literal source (e.g. `[1 2; 3 4;;; 5 6; 7 8]`) is
# intentionally NOT exercised here — it hits a separate, pre-existing bug
# (Issue #10182: the `;;;`-separated 3-D array literal itself mis-shapes to a
# `Matrix`/`(4, 2)` instead of `Array{T,3}`/`(2, 2, 2)`, independent of
# `similar`). The explicit-dims `similar(a, dims..., N-D)` case below already
# covers rank 3 via #9642's fix.

# Original MWE: 2-D matrix-literal source, no dims -> rank 2 (was wrongly
# statically bound to the rank-1 method before the fix).
m2 = [1 2; 3 4]
c2 = similar(m2)
@test rank_dispatch(c2) == 2
@test size(c2) == (2, 2)
@test typeof(c2) == Matrix{Int64}

# 1-D vector-literal source, no dims -> rank 1 (control case; must stay
# correct).
v1 = [1, 2, 3]
c1 = similar(v1)
@test rank_dispatch(c1) == 1
@test size(c1) == (3,)
@test typeof(c1) == Vector{Int64}

# Direct dispatch on the matrix/vector literals themselves (no `similar`
# involved) was never affected — kept here as a control against
# over-widening the fix.
@test rank_dispatch(m2) == 2
@test rank_dispatch(v1) == 1

# `similar` applied directly to a fresh literal (no intermediate variable)
# must also resolve the correct rank.
@test rank_dispatch(similar([1 2; 3 4])) == 2
@test rank_dispatch(similar([1, 2, 3])) == 1

# Explicit-dims form (Issue #9642) must remain correct alongside this fix.
c2b = similar(m2, 1, 3, 1)
@test rank_dispatch3(c2b) == 3

true
