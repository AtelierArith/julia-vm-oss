# where-binder shadowing a builtin/user type name (Issue #10100, tech-debt
# epic #10049): a `where`-clause TypeVar binder whose spelling collides with
# an existing type name must shadow it lexically for the extent of the
# `where` expression, matching upstream's per-`where` scoping. Previously:
#   - a self-referential bound (`where Int64<:Int64`) resolved the bound
#     expression WITH the new binder already visible, so the bound became
#     the TypeVar itself -> self-referential upper_bound -> uncatchable
#     stack overflow, even just constructing the value (no `<:`/`==`).
#   - a non-self-referential collision (`where Int64<:Real`) silently
#     dropped the `where`, returning the bare non-parametric type.
# All expectations below were verified against upstream Julia 1.12.

using Test

struct WrapA10100{T} end

@testset "self-referential where-binder shadowing a builtin name (Issue #10100)" begin
    # Previously: uncatchable process abort (Rust-level stack overflow while
    # lowering, before any VM execution) -- even just constructing the value.
    x = Vector{Int64} where Int64<:Int64
    @test typeof(x) == UnionAll
    @test string(x) == "Vector{Int64} where Int64<:Int64"

    # Same self-referential shape against a user-defined parametric struct.
    y = WrapA10100{Int64} where Int64<:Int64
    @test typeof(y) == UnionAll
    @test string(y) == "WrapA10100{Int64} where Int64<:Int64"
end

@testset "non-self-referential where-binder shadowing a builtin name (Issue #10100)" begin
    # Previously: silently dropped to the bare `Vector{Int64}` (a DataType),
    # discarding the `where` clause entirely instead of raising an error or
    # keeping the (degenerate but legal) shadowed UnionAll.
    z = Vector{Int64} where Int64<:Real
    @test typeof(z) == UnionAll
    @test string(z) == "Vector{Int64} where Int64<:Real"

    # `z` is structurally correct, not merely cosmetic display: it denotes
    # the FAMILY `Vector{X} for X<:Real`, not the single concrete type
    # `Vector{Int64}` -- so `==`/`isa` must see through the shadowed name to
    # the real bound, not stop at the raw (unrebound) concrete leaf.
    @test z != Vector{Int64}
    @test Int64[1, 2, 3] isa z
    @test Float64[1.0, 2.0] isa z
    @test !(["a"] isa z)

    # Same structural check for the user-struct variant.
    a = WrapA10100{Int64} where Int64<:Real
    @test a != WrapA10100{Int64}
end

@testset "regression guards: ordinary where-clauses are unaffected (Issue #10100)" begin
    # Non-colliding binder name: unaffected by the bound-resolution-order fix.
    w = Vector{T} where T<:Int64
    @test typeof(w) == UnionAll
    @test string(w) == "Vector{T} where T<:Int64"

    # Colliding binder name that is genuinely UNUSED in the body still
    # collapses to the bare body, matching upstream `jl_type_unionall`
    # (this must keep working -- it is not the bug).
    unused = Vector{Float64} where Int64<:Real
    @test typeof(unused) == DataType
    @test unused == Vector{Float64}

    # Chained where + collision in the OUTER clause: the outer binder still
    # correctly resolves its bound against the true enclosing (global) scope,
    # and the (unrelated, still-used) inner binder is preserved alongside it.
    chained = Tuple{Int64,S} where S<:Int64 where Int64<:Real
    @test typeof(chained) == UnionAll
    @test string(chained) == "Tuple{Int64, S} where {Int64<:Real, S<:Int64}"

    # A nested binder's bounds are evaluated in the enclosing scope. The
    # inner `S` therefore keeps an exact reference to the outer, shadowing
    # `Int64` TypeVar rather than the global primitive type.
    nested_bound = Tuple{Vector{S} where S<:Int64} where Int64<:Real
    @test nested_bound.body.parameters[1].var.ub === nested_bound.var
    @test Tuple{Vector{Float64}} <: nested_bound
    @test !(Tuple{Vector{String}} <: nested_bound)

    # Bare and module-qualified spellings can coexist. Only the bare leaf is
    # captured by the source binder; `Core.Builtin` remains the concrete,
    # qualified abstract type.
    mixed_qualified = Tuple{Builtin, Core.Builtin} where Builtin<:Function
    @test Tuple{typeof(sin), Core.Builtin} <: mixed_qualified
    @test !(Tuple{typeof(sin), Function} <: mixed_qualified)
    @test mixed_qualified != (Tuple{T,T} where T<:Function)
end

@testset "where-binder shadowing the top type Any (Issue #10100 codex review)" begin
    # `CoreType::Any` is a distinct top-level variant (not `Primitive`/
    # `Abstract`/`Named`/`Struct`), so it needs its own `rebind_where_binders`
    # arm alongside the others -- without it, a binder spelled `Any` stayed
    # structurally the concrete top type, inverting `isa` for every member.
    r = Vector{Any} where Any<:Real
    @test typeof(r) == UnionAll
    @test string(r) == "Vector{Any} where Any<:Real"
    @test Float64[1.0] isa r
    @test !(Any[1, 2] isa r)
end

@testset "where-binder shadowing the builtin Module type (Issue #10299)" begin
    # `CoreType::Module(String)` is the variant a bare `Module` identifier
    # converts to -- a distinct leaf like `Any`, needing its own
    # `rebind_where_binders` arm. Without it the kept UnionAll's body held
    # the raw unrebound Module leaf: `<:` still passed via the
    # mutual-subtype fallback, but structural `==` against the canonical
    # `Vector{T} where T<:Real` returned false.
    r = Vector{Module} where Module<:Real
    s = Vector{T} where T<:Real
    @test typeof(r) == UnionAll
    @test string(r) == "Vector{Module} where Module<:Real"
    @test r <: s
    @test s <: r
    @test r == s
    @test Float64[1.0] isa r
    @test !(["a"] isa r)
end

@testset "unbounded nominal-named binder is a generic alias (Issue #10613)" begin
    module_vector = Vector{Module} where Module
    int_vector = Vector{Int64} where Int64

    for alias in (module_vector, int_vector)
        @test alias == Vector
        @test alias <: Vector
        @test Vector <: alias
        @test typejoin(alias, Vector) === Vector
        @test alias !== Vector
    end
end

@testset "lower-bounded source binders preserve strict identity (Issue #10613)" begin
    nominal = Vector{Int64} where Int64>:Signed
    alpha = Vector{T} where T>:Signed

    @test nominal.var.lb === Signed
    @test nominal.var.ub === Any
    @test alpha.var.lb === Signed
    @test alpha.var.ub === Any
    @test nominal == alpha
    @test nominal !== alpha
    @test nominal <: alpha
    @test alpha <: nominal

    same_name_a = Vector{T} where T>:Signed
    same_name_b = Vector{T} where T>:Signed
    @test same_name_a === same_name_b

    two_sided = Vector{Int64} where Signed<:Int64<:Real
    @test two_sided.var.lb === Signed
    @test two_sided.var.ub === Real
end

true
