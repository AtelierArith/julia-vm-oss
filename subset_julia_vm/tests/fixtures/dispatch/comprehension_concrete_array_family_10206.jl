# Issue #10206 / #10267: `ValueType::ArrayOf(ArrayElementType::Any, Some(n))`
# is produced by two structurally different sources that share the exact same
# shape:
#   1. A comprehension whose body type could not be resolved statically
#      (Issue #6817) — the RANK is known (number of iterator clauses) but the
#      ELEMENT is a placeholder, not proof the runtime value is `Vector{Any}`
#      (it could turn out to be `Vector{Int64}` etc).
#   2. A value that really IS `Vector{Any}`/`Matrix{Any}` at both compile time
#      and runtime (upstream `Vector{Any}` is a concrete, dispatchable type).
#
# The `infer_julia_type` bridge's `elem_unknown` heuristic (`julia_elem ==
# JuliaType::Any`) could not tell these apart, so it always reported the bare
# `Vector`/`Matrix` alias for both — correct for case 1 (defer to runtime
# dispatch on the concrete value) but this exposed a SEPARATE, narrower bug for
# a multi-iterator (rank >= 2) comprehension: the dispatch resolver's
# rank-unknown-argument fallback only recognized ABSTRACT array-family
# candidates (`::AbstractVector`/`::AbstractMatrix`), not CONCRETE
# `::Array{Any,N}`-typed ones, so a bare `Struct("Matrix")` argument matching
# NO method statically raised a spurious COMPILE-TIME `MethodError` instead of
# deferring to runtime dispatch (which resolves correctly, since the runtime
# value genuinely is `Matrix{Any}`). Issue #10315 later aligned the rank-1
# collector with the same rank-known/element-unresolved representation, so its
# runtime `Vector{Any}` value now selects this fixture's concrete method through
# runtime dispatch rather than through an accidental static collapse.
#
# Fixed by broadening the dispatch-deferral fallback's array-family probe from
# abstract-only to abstract-or-concrete (`core_is_array_family_type`,
# Issue #10206), independent of the `known_any_rank_array_locals` Known/Unknown
# provenance marker (Issue #10267) that fixes the companion case where the
# array-family argument really IS known-`Any` (see
# `dispatch_known_any_field_array_family_10206.jl`).
#
# All expectations verified against upstream Julia 1.12.

using Test

rank_dispatch_any(x::Array{Any,1}) = 1
rank_dispatch_any(x::Array{Any,2}) = 2

f2(i, j) = i == j ? i : string(i, j)
f1(i) = i == 1 ? i : string(i)

@testset "comprehension arg vs concrete Array{Any,N} candidates (Issue #10206)" begin
    # --- rank-2 multi-iterator comprehension, assigned to a variable ---
    c = [f2(i, j) for i in 1:2, j in 1:2]
    @test typeof(c) == Matrix{Any}
    @test rank_dispatch_any(c) == 2

    # --- rank-2 multi-iterator comprehension, passed inline (no intermediate
    # variable) — the direct `Expr::MultiComprehension` inference arm and the
    # `ValueType::ArrayOf` variable bridge must agree ---
    @test rank_dispatch_any([f2(i, j) for i in 1:2, j in 1:2]) == 2

    # --- rank-1 single-iterator comprehension: must keep working (this is the
    # #6817-adjacent path that happened to already work; guard against a
    # regression from unifying the rank-2 fix) ---
    v = [f1(i) for i in 1:2]
    @test typeof(v) == Vector{Any}
    @test rank_dispatch_any(v) == 1
    @test rank_dispatch_any([f1(i) for i in 1:2]) == 1

    # --- 3-clause comprehension against a bare ::Array catch-all: rank-unknown
    # deferral must generalize beyond rank 1/2 ---
    rank_dispatch_any_bare(x::Array) = "bare"
    rank_dispatch_any_bare(x::Array{Any,3}) = "any3"
    f3(i, j, k) = i == j == k ? i : string(i, j, k)
    t = [f3(i, j, k) for i in 1:2, j in 1:2, k in 1:2]
    @test typeof(t) == Array{Any,3}
    @test rank_dispatch_any_bare(t) == "any3"
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
true
