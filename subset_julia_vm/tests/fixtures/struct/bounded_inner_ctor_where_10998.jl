using Test

# Issue #10998: an explicit parametric inner constructor with a BOUNDED `where`
# binder must lower the binder as `T` with upper bound `Number` (not as the
# literal name "T<:Number"), and the declared bound must be ENFORCED — including
# when the type argument is only known at runtime (`Foo{typeof(x)}(x)`), which
# previously bypassed the inner constructor entirely through the raw dynamic
# allocator.

struct BoundedInner10998{T}
    x::T
    BoundedInner10998{T}(x) where {T<:Number} = new{T}(x * 2)
end

# The binder binds: the explicit self `BoundedInner10998{Int}` matches.
@test BoundedInner10998{Int}(1).x == 2
@test BoundedInner10998{Float64}(1.5).x == 3.0
@test typeof(BoundedInner10998{Int}(1)) === BoundedInner10998{Int}

# The declared bound rejects a violating instantiation, like upstream.
@test_throws MethodError BoundedInner10998{String}("a")

# The inner constructor also runs (and its bound is enforced) when the type
# argument is a runtime value: `Foo{typeof(x)}(x)`.
make10998(v) = BoundedInner10998{typeof(v)}(v)
@test make10998(3).x == 6
@test typeof(make10998(3)) === BoundedInner10998{Int}
@test make10998(2.5).x == 5.0
@test_throws MethodError make10998("a")

# A `where` binder that is unbounded keeps accepting every type argument.
struct Unbounded10998{T}
    x::T
    Unbounded10998{T}(x) where {T} = new{T}(x)
end
mkunbounded10998(v) = Unbounded10998{typeof(v)}(v)
@test mkunbounded10998("a").x == "a"
@test typeof(mkunbounded10998("a")) === Unbounded10998{String}

# Unbraced bounded binders and multi-parameter bounded binders lower the same way.
struct Pair10998{T,S}
    a::T
    b::S
    Pair10998{T,S}(a, b) where {T<:Number,S<:AbstractString} = new{T,S}(a, b)
end
p10998 = Pair10998{Int,String}(1, "x")
@test p10998.a == 1
@test p10998.b == "x"
@test_throws MethodError Pair10998{String,String}("a", "x")

struct Unbraced10998{T}
    x::T
    Unbraced10998{T}(x) where T<:Real = new{T}(x)
end
@test Unbraced10998{Int}(7).x == 7
@test_throws MethodError Unbraced10998{Complex{Int}}(1 + 2im)

true
