# Issue #10206 / #10267: the companion case to
# `comprehension_concrete_array_family_10206.jl`. `Expr.args` really is
# `Vector{Any}` at both compile time and runtime (upstream `Vector{Any}` is a
# concrete, dispatchable type) — it is NOT a comprehension-style "rank known,
# element unresolved" placeholder. A `Vector{Any}`-typed method parameter must
# still statically bind to it.
#
# Before this fix, assigning `e.args` to a variable stored
# `ValueType::ArrayOf(ArrayElementType::Any, Some(1))` — the exact same shape a
# comprehension produces — and the `infer_julia_type` `Expr::Var` bridge's
# `elem_unknown` heuristic (`julia_elem == JuliaType::Any`) could not tell the
# two apart, so it reported the bare `Vector` alias instead of the concrete
# `Vector{Any}`, and a `g(x::Array{Any,1})`-typed method raised a spurious
# `MethodError: no method matching g(::Vector)` even though `typeof(a) ==
# Vector{Any}` matched upstream exactly.
#
# Fixed by recording explicit provenance at the assignment site
# (`known_any_rank_array_locals`, Issue #10267): only an `expr.args`-shaped
# field access marks the variable as genuinely-`Any`, so the bridge reports the
# concrete `VectorOf(Any)` for it while every other `ArrayOf(Any, Some(n))`
# producer (comprehensions) keeps the conservative bare-alias / runtime-defer
# behavior by default.
#
# All expectations verified against upstream Julia 1.12.

using Test

g(x::Array{Any,1}) = "vec_any"
h(x::Array{Any,1}) = "h_vec_any"
h(x::Array) = "h_bare"

@testset "expr.args is genuinely Vector{Any}, statically binds (Issue #10206)" begin
    e = :(f(1, 2, 3))
    a = e.args
    @test typeof(a) == Vector{Any}
    @test g(a) == "vec_any"
    # A more-specific `Array{Any,1}` candidate must win over the bare `Array`
    # catch-all — proves the static bind is the concrete type, not just a
    # runtime-deferred loose match.
    @test h(a) == "h_vec_any"

    # Inline (no intermediate variable) must agree with the variable-bound form.
    @test g((:(f(1, 2, 3))).args) == "vec_any"

    # Non-regression: the pre-existing empty-array-literal case (`x = []`,
    # ArrayOf(Any, None) — a DIFFERENT shape, rank already erased) must keep
    # working exactly as before this change.
    z = []
    @test typeof(z) == Vector{Any}
    @test g(z) == "vec_any"
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
true
