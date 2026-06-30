# Inline parametric constructor field access with an argument-promoting outer
# constructor (Issue #7284)
#
# A user-defined OUTER constructor that promotes its arguments through a LOCAL
# variable (`v = float(x); Foo{typeof(v)}(v)`) was mis-inferred at an INLINE
# `Foo(3).x` field access. The call-site bound the struct's type parameter from
# the raw `Int64` argument (`Foo{Int64}`), typed the field load as `Int64`, and
# the runtime `Float64` field then failed the slot check
# ("Type error: expected I64, got Float64"). The runtime type (`Foo{Float64}`)
# was already correct; only the compile-time call-site return-type inference,
# which ignored the `float()`/`promote()` in the constructor body, was wrong.
#
# The fix re-infers the return type through the user constructor's body when one
# exists, instead of the naive default-inner-constructor type-arg model. This is
# the sibling of #7240 (which fixed the INLINE-brace lowering); #7284 is the
# call-site INFERENCE path reached by the local-variable form.

using Test

# Single-field form: `Foo7284(x::Real)` overrides the auto-generated default
# constructor, so it is the dispatched method and its `float` promotion wins.
struct Foo7284{T<:Real}
    x::T
end
function Foo7284(x::Real)
    v = float(x)
    Foo7284{typeof(v)}(v)
end

# Two-argument promote form (mirrors Distributions' `Normal(μ, σ)`). MIXED
# argument types are used so the user constructor (not the same-type default
# `N(a::T, b::T)`) is the dispatched method, matching upstream Julia.
struct N7284{T<:Real}
    a::T
    b::T
end
function N7284(a::Real, b::Real)
    m, s = promote(float(a), float(b))
    return N7284{typeof(m)}(m, s)
end

# Regression: a parametric struct constructed only through its DEFAULT
# constructor must keep its precise integer field type at an inline access — the
# fix must NOT widen this path.
struct Pt7284{T}
    x::T
    y::T
end

@testset "Inline parametric constructor field access (Issue #7284)" begin
    # Inline construct + field access (the previously failing form).
    @test Foo7284(3).x == 3.0
    @test typeof(Foo7284(3).x) == Float64
    @test typeof(Foo7284(3)) == Foo7284{Float64}

    # Two-argument promote, mixed args -> user constructor -> promoted.
    @test N7284(2, 3.0).a == 2.0
    @test N7284(2, 3.0).b == 3.0
    @test typeof(N7284(2, 3.0)) == N7284{Float64}

    # Bound-variable form also works.
    f = Foo7284(3)
    @test f.x == 3.0
    @test typeof(f) == Foo7284{Float64}

    # Regression: default-constructor parametric struct keeps Int field type at
    # an inline access.
    @test Pt7284(3, 4).x == 3
    @test typeof(Pt7284(3, 4)) == Pt7284{Int64}
    @test typeof(Pt7284(1.5, 2.5)) == Pt7284{Float64}
end

true
