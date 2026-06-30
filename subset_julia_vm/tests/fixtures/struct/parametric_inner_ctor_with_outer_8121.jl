# Issue #8121: a parametric struct with an explicit inner constructor gets NO
# synthesized default field constructor in upstream Julia, so a bare `Foo(args)`
# or braces `Foo{T}(args)` call whose arity matches the field count MUST invoke
# the user inner/outer constructor — NOT raw default field construction.
#
# Regression: when the precompiled Base cache was in use, defining a user OUTER
# constructor `Foo(...)` of the same arity made the working method table
# non-empty, which mis-classified the user struct as a cached Base struct and
# SKIPPED registering its inner constructors. Both the bare `Foo(1.0, 2.0)` and
# the braces `Foo{Float64}(1.0, 2.0)` then fell back to default field
# construction (raw store) instead of running the inner ctor body.

# Case 1: inner scales the first field; outer forwards to the braces form.
struct Foo{T}
    a::T
    b::T
    Foo{T}(a, b) where {T} = new{T}(a * 10, b)
end
Foo(a::Number, b::Number) = Foo{Float64}(a, b)

f_bare = Foo(1.0, 2.0)             # outer -> Foo{Float64} -> inner (a*10)
f_braces = Foo{Float64}(3.0, 4.0)  # inner directly
case1 = f_bare.a == 10.0 && f_bare.b == 2.0 && f_braces.a == 30.0 && f_braces.b == 4.0

# Case 2: Rotations-style multi-field normalizing inner constructor.
# AngleAxis{T}(theta, x, y, z) normalizes the (x, y, z) axis to unit length;
# proves the normalizing inner body ran rather than storing the raw axis.
struct AngleAxis{T}
    theta::T
    axis_x::T
    axis_y::T
    axis_z::T
    function AngleAxis{T}(theta, x, y, z) where {T}
        n = sqrt(x * x + y * y + z * z)
        new{T}(theta, x / n, y / n, z / n)
    end
end
AngleAxis(theta::Number, x::Number, y::Number, z::Number) =
    AngleAxis{Float64}(theta, x, y, z)

aa = AngleAxis(0.5, 3.0, 0.0, 4.0)            # axis normalized: (0.6, 0.0, 0.8)
aa2 = AngleAxis{Float64}(1.0, 0.0, 6.0, 8.0)  # braces inner directly: (0.0, 0.6, 0.8)
case2 =
    aa.theta == 0.5 &&
    aa.axis_x == 0.6 && aa.axis_y == 0.0 && aa.axis_z == 0.8 &&
    aa2.axis_x == 0.0 && aa2.axis_y == 0.6 && aa2.axis_z == 0.8

# Case 3: regression guard — Base parametric structs with inner constructors
# (Complex, Rational) must keep working (their cached inner ctors not disturbed).
case3 = Complex{Float64}(2, 3) == 2.0 + 3.0im && (3 // 4) == 3 // 4

ok = case1 && case2 && case3
println(ok)
ok
