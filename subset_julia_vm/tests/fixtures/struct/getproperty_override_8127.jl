# Issue #8127: a custom `Base.getproperty` override must intercept property
# access `x.f`. In Julia `x.f` always lowers to `getproperty(x, :f)`, whose
# default falls back to `getfield`; user overloads expose computed properties.

struct Foo
    a::Float64
    b::Float64
end
function Base.getproperty(x::Foo, f::Symbol)
    if f === :sum
        getfield(x, :a) + getfield(x, :b)
    elseif f === :diff
        getfield(x, :a) - getfield(x, :b)
    else
        getfield(x, f)
    end
end

foo = Foo(3.0, 4.0)

# Computed (non-declared) properties resolve through the override.
check_sum = foo.sum == 7.0
check_diff = foo.diff == -1.0
# Declared fields still work (the override falls back to getfield).
check_a = foo.a == 3.0
check_b = foo.b == 4.0

# A wrapper that stores one field and exposes computed components,
# mirroring the Rotations.QuatRotation pattern from the issue.
struct Wrap
    data::Vector{Float64}
end
function Base.getproperty(w::Wrap, f::Symbol)
    if f === :x
        getfield(w, :data)[1]
    elseif f === :y
        getfield(w, :data)[2]
    else
        getfield(w, f)
    end
end

w = Wrap([10.0, 20.0])
check_wx = w.x == 10.0
check_wy = w.y == 20.0
check_wdata = w.data == [10.0, 20.0]

# Access inside a function (exercises typed parameter dispatch).
total(x::Foo) = x.sum
check_fn = total(foo) == 7.0

# A parametric (specialization-eligible) function that reads a *declared* field
# whose override transforms it. The function specializer must NOT emit a direct
# `GetField` for `p.v` (that would bypass the override and yield the raw value);
# the hot loop must keep going through `getproperty` dispatch (Issue #8127).
struct Scaled
    v::Float64
end
Base.getproperty(s::Scaled, f::Symbol) = f === :v ? 2.0 * getfield(s, :v) : getfield(s, f)
accum(x::T, s::Scaled) where {T} = x + s.v
sc = Scaled(10.0)
acc = 0.0
for _ in 1:50000
    global acc += accum(1.0, sc)   # 1.0 + 20.0 = 21.0 each iteration
end
check_spec = acc == 50000 * 21.0

check_sum && check_diff && check_a && check_b &&
    check_wx && check_wy && check_wdata && check_fn && check_spec
