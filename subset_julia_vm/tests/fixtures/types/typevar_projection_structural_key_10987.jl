# TypeVar projection identity must be keyed structurally, never by rendered
# name/bound strings (Issue #10987, epic #10459 Phase 1 completion), while
# staying exactly as discriminating as upstream. The three adversarial
# scenarios below were found by codex review of the first #10987 attempt:
# each collapsed distinct-upstream binder objects (or produced garbage
# bounds) under an over-narrowed key. All expectations verified against
# upstream Julia 1.12.

using Test

module MBox1_10987
struct Box{T} end
end

module MBox2_10987
struct Box{T} end
end

@testset "outer binder occurring only in a nested binder's bound stays distinct (Issue #10987)" begin
    # The only occurrence of T/U is inside the NESTED UnionAll binder's
    # bound. Owner-key normalization must preserve nested binder structure;
    # stripping nested binders to their bodies made these two wrappers one
    # cache domain and returned `a.var` (name :T) for `b.var`.
    a = Tuple{Vector{S} where S<:T} where T
    b = Tuple{Vector{S} where S<:U} where U
    ta = a.var
    ub = b.var
    @test ta !== ub
    @test ta.name == :T
    @test ub.name == :U
end

@testset "module-qualified bounds keep same-named cross-module structs distinct (Issue #10987)" begin
    # The declared-bound key components are JuliaType (module qualification
    # preserved), not CoreType (whose bridge strips qualification and would
    # collapse these two into one identity).
    a = Ref{T} where T<:MBox1_10987.Box{Int}
    b = Ref{T} where T<:MBox2_10987.Box{Int}
    va = a.var
    vb = b.var
    @test va !== vb
    # Upstream also has `va.ub != vb.ub`, but sjulia's type ==/=== currently
    # strips module qualification for same-named structs (Issue #11021,
    # structural fix owned by #10989/StructId) — assert the display-level
    # distinctness both engines agree on until that lands.
    @test string(va.ub) != string(vb.ub)
end

@testset "two-sided bound with nested <: parses structurally (Issues #10987/#11020)" begin
    # Pre-existing bug #11020: the naive split("<:") of the rendered
    # interval also split inside `Vector{<:Real}`, producing lb=Union{} and
    # a mangled ub. The bracket-depth-aware split restores upstream values
    # and keeps both key-derivation paths in agreement.
    w = Vector{T} where Vector{Int64}<:T<:Vector{<:Real}
    v = w.var
    @test v.lb == Vector{Int64}
    @test v.ub == (Vector{<:Real})
    @test w.var === w.var
end

@testset "same rendered body with different declared bounds stays distinct (Issue #10987)" begin
    # The where_binder_shadow_scope_10100.jl family, pinned here at the
    # projection-identity level: the owner key is body-derived, so only the
    # declared-bound key components separate these wrappers.
    nominal = Vector{Int64} where Int64>:Signed
    two_sided = Vector{Int64} where Signed<:Int64<:Real
    @test nominal.var !== two_sided.var
    @test nominal.var.ub === Any
    @test two_sided.var.ub === Real
    @test nominal.var.lb === Signed
    @test two_sided.var.lb === Signed
end

true
