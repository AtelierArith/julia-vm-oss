using Test

# Regression test for Issue #10190:
# A 3-D (or higher) array literal using Julia's `;;`/`;;;`/... dimension-
# separator syntax was mis-shaped by sjulia: the parser collapsed every
# semicolon run into a single "next row" boundary regardless of how many
# semicolons it contained, so `[1 2; 3 4;;; 5 6; 7 8]` (a genuine
# `Array{Int64,3}` of size `(2, 2, 2)` in upstream Julia) lowered to a
# `Matrix{Int64}` of size `(4, 2)` instead — the 3rd dimension was silently
# collapsed into the 2nd. The fix preserves each separator run's semicolon
# count (its dimension level: `;` = dim 1, `;;` = dim 2, `;;;` = dim 3, ...)
# through parsing and folds the rows into their true N-dimensional
# column-major shape during lowering, generalizing to arbitrary N per repo
# Design Principle #10 rather than special-casing the 2x2x2 MWE.
#
# All expected values below were verified against `julia --startup-file=no`
# first (both `size`/`typeof` and the full column-major element order via
# linear indexing).

# ── Original MWE: 3-D literal via `;;;` ─────────────────────────────────────

a3 = [1 2; 3 4;;; 5 6; 7 8]
@test typeof(a3) == Array{Int64,3}
@test size(a3) == (2, 2, 2)
@test [a3[i] for i in 1:8] == [1, 3, 2, 4, 5, 7, 6, 8]
@test a3[1, 1, 1] == 1 && a3[2, 1, 1] == 3 && a3[1, 2, 1] == 2 && a3[2, 2, 1] == 4
@test a3[1, 1, 2] == 5 && a3[2, 1, 2] == 7 && a3[1, 2, 2] == 6 && a3[2, 2, 2] == 8

# ── 3 blocks in the 3rd dimension (not just the MWE's 2) ───────────────────

a3b = [1 2; 3 4;;; 5 6; 7 8;;; 9 10; 11 12]
@test typeof(a3b) == Array{Int64,3}
@test size(a3b) == (2, 2, 3)
@test [a3b[i] for i in 1:12] == [1, 3, 2, 4, 5, 7, 6, 8, 9, 11, 10, 12]

# ── 4-D literal via `;;;;` ───────────────────────────────────────────────────

a4 = [1 2; 3 4;;; 5 6; 7 8;;;; 9 10; 11 12;;; 13 14; 15 16]
@test typeof(a4) == Array{Int64,4}
@test size(a4) == (2, 2, 2, 2)
@test [a4[i] for i in 1:16] == [1, 3, 2, 4, 5, 7, 6, 8, 9, 11, 10, 12, 13, 15, 14, 16]

# ── Explicit dim-2 separator `;;` without space, skipping dim 2 for `;;;` ──

c = [1; 2;; 3; 4]
@test typeof(c) == Matrix{Int64}
@test size(c) == (2, 2)
@test [c[i] for i in 1:4] == [1, 2, 3, 4]

# A literal that skips level 2 entirely (only `;` and `;;;` present) still
# produces the correct rank-3 shape, padding the unused dim-2 to size 1.
d = [1; 2;;; 3; 4]
@test typeof(d) == Array{Int64,3}
@test size(d) == (2, 1, 2)
@test [d[i] for i in 1:4] == [1, 2, 3, 4]

# ── Typed array literal `T[...]` with `;;;` (shares the same parser path) ──

e = Int64[1 2; 3 4;;; 5 6; 7 8]
@test typeof(e) == Array{Int64,3}
@test size(e) == (2, 2, 2)
@test [e[i] for i in 1:8] == [1, 3, 2, 4, 5, 7, 6, 8]

ef = Float64[1 2; 3 4;;; 5 6; 7 8]
@test typeof(ef) == Array{Float64,3}
@test size(ef) == (2, 2, 2)

# ── Existing 2-D / 1-D literals must stay unaffected (no regression) ───────

m2 = [1 2; 3 4]
@test typeof(m2) == Matrix{Int64}
@test size(m2) == (2, 2)
@test [m2[i] for i in 1:4] == [1, 3, 2, 4]

row_vec = [1 2 3]
@test typeof(row_vec) == Matrix{Int64}
@test size(row_vec) == (1, 3)

v1 = [1, 2, 3]
@test typeof(v1) == Vector{Int64}
@test size(v1) == (3,)

# A single-`;` vertical scalar chain is `vcat`-like and collapses to rank 1,
# not an N×1 matrix (Issue #10380). Typed literals share the same shape rule.
vsemi = [1; 2; 3]
@test typeof(vsemi) == Vector{Int64}
@test size(vsemi) == (3,)
@test vsemi == [1, 2, 3]

tvsemi = Int64[1; 2; 3]
@test typeof(tvsemi) == Vector{Int64}
@test size(tvsemi) == (3,)
@test tvsemi == [1, 2, 3]

# Higher-level separators still create higher-rank arrays; only the single-`;`
# all-one-element-row case collapses to a Vector.
wide_semis = [1;; 2;; 3]
@test typeof(wide_semis) == Matrix{Int64}
@test size(wide_semis) == (1, 3)

# Trailing higher-dimension separators before `]` are still tolerated
# (Issue #8759) and pad the shape with a trailing size-1 dimension rather
# than being dropped, for both the untyped and typed literal paths
# (Issue #10378, fixed alongside #10190).
@test size([1 2;;;]) == (1, 2, 1)
@test typeof([1 2;;;]) == Array{Int64,3}
@test size(Int64[1 2;;;]) == (1, 2, 1)
@test size([1 2; 3 4;;;]) == (2, 2, 1)

# Degenerate all-semicolon literals contain no elements but their semicolon
# count determines the rank (Issue #10379).
empty1 = [;]
@test typeof(empty1) == Vector{Any}
@test size(empty1) == (0,)

empty2 = [;;]
@test typeof(empty2) == Matrix{Any}
@test size(empty2) == (0, 0)

empty3 = [;;;]
@test typeof(empty3) == Array{Any,3}
@test size(empty3) == (0, 0, 0)

true
