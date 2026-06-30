# Nested function call inside a type-parameter brace `T{typeof(f(x))}(...)`
# (Issue #7240)
#
# A parametric constructor call whose type argument computes a type from a
# NESTED function call — e.g. `Foo{typeof(float(x))}(float(x))` — failed to
# compile with `Undefined variable: float(x)`. The runtime type-argument string
# (`typeof(float(x))`) was lowered by a hand-rolled comma-splitter that captured
# the inner call `float(x)` as a single identifier name instead of a real call.
#
# This verifies the type argument is evaluated as a true expression so the
# nested call runs, including a deeper nest and a two-type-parameter case, while
# keeping the already-working `typeof(var)` / plain-variable forms as
# regressions.

using Test

# The exact MWE from the issue: nested `float(x)` inside `typeof(...)`.
struct Foo7240{T<:Real}
    x::T
end
Foo7240(x::Real) = Foo7240{typeof(float(x))}(float(x))

# Deeper nesting: `typeof(g(h(x)))`.
g7240(z) = z + 1.0
h7240(z) = z * 2
struct Box7240{T}
    v::T
end
mkbox7240(x) = Box7240{typeof(g7240(h7240(x)))}(g7240(h7240(x)))

# Two type parameters, each computed from a nested call.
struct Pair7240{A,B}
    a::A
    b::B
end
mkpair7240(x, y) = Pair7240{typeof(float(x)),typeof(float(y))}(float(x), float(y))

# Regression: `typeof(var)` with no nested call (already worked).
struct Reg7240{T<:Real}
    x::T
end
Reg7240(x::Real) = Reg7240{typeof(x)}(x)

@testset "Type-parameter brace nested call (Issue #7240)" begin
    f = Foo7240(3)
    @test f.x == 3.0
    @test typeof(f.x) == Float64
    @test typeof(f) == Foo7240{Float64}

    b = mkbox7240(2) # h7240 -> 4, g7240 -> 5.0
    @test b.v == 5.0
    @test typeof(b.v) == Float64
    @test typeof(b) == Box7240{Float64}

    p = mkpair7240(1, 2)
    @test p.a == 1.0
    @test p.b == 2.0
    @test typeof(p) == Pair7240{Float64,Float64}

    # Regression: plain variable type argument stays an Int constructor.
    r = Reg7240(7)
    @test r.x == 7
    @test typeof(r) == Reg7240{Int64}
end

true
