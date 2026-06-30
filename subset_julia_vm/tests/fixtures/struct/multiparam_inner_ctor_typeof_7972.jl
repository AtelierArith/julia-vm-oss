# Issue #7972: a multi-parameter parametric struct built through its inner
# constructor `new{A,B,...}(...)` must report ALL type parameters in `typeof`,
# not just the first. sjulia previously produced `P3{Int64}` instead of
# `P3{Int64, Float64}` (the field values were stored correctly; only the
# instance type was wrong).
#
# Root cause: the inner-constructor frame carried no type bindings, so the
# parametric-struct name fell back to inferring a single type parameter from the
# first field value. The fix recovers every parameter that appears as a bare
# field type from its matching field's value.
using Test

struct P3{A,B}
    a::A
    b::B
    P3(a::A, b::B) where {A,B} = new{A,B}(a, b)
end

struct T3{A,B,C}
    a::A
    b::B
    c::C
    T3(a::A, b::B, c::C) where {A,B,C} = new{A,B,C}(a, b, c)
end

# Single type parameter (regression: must keep working).
struct S1{T}
    x::T
    S1(x::T) where T = new{T}(x)
end

@testset "Issue #7972: multi-param inner-ctor typeof reports all parameters" begin
    p = P3(1, 2.5)
    @test typeof(p) == P3{Int64,Float64}
    @test (p.a, p.b) == (1, 2.5)

    p2 = P3(2.0f0, 3)
    @test typeof(p2) == P3{Float32,Int64}

    t = T3(1, 2.5, true)
    @test typeof(t) == T3{Int64,Float64,Bool}
    @test (t.a, t.b, t.c) == (1, 2.5, true)

    # Single-parameter inner constructors are unaffected.
    @test typeof(S1(7)) == S1{Int64}
    @test typeof(S1(1.5)) == S1{Float64}

    # Built-in parametric structs (single param) still resolve correctly.
    @test typeof(1 // 2) == Rational{Int64}
    @test typeof(Complex(1, 2)) == Complex{Int64}
end

true
