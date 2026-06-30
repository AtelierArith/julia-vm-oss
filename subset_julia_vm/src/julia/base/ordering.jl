# Minimal ordering helpers, based on julia/base/ordering.jl.

module Order

export Ordering,
    Forward,
    Reverse,
    By,
    Lt,
    Perm,
    ReverseOrdering,
    ForwardOrdering,
    DirectOrdering,
    lt,
    ord,
    ordtype

abstract type Ordering end

struct ForwardOrdering <: Ordering end

struct ReverseOrdering{Fwd<:Ordering} <: Ordering
    fwd::Fwd
end

ReverseOrdering(rev::ReverseOrdering) = rev.fwd
ReverseOrdering(fwd::Fwd) where {Fwd} = ReverseOrdering{Fwd}(fwd)
ReverseOrdering() = ReverseOrdering(ForwardOrdering())

reverse(o::Ordering) = ReverseOrdering(o)

const DirectOrdering = Union{ForwardOrdering, ReverseOrdering{ForwardOrdering}}
const Forward = ForwardOrdering()
const Reverse = ReverseOrdering()

struct By{T,O} <: Ordering
    by::T
    order::O
end
By(by, order) = By{typeof(by),typeof(order)}(by, order)
By(by) = By(by, Forward)

struct Lt{T} <: Ordering
    lt::T
end
Lt(ltfn) = Lt{typeof(ltfn)}(ltfn)

struct Perm{O<:Ordering,V<:AbstractVector} <: Ordering
    order::O
    data::V
end
Perm(order::O, data::V) where {O<:Ordering,V<:AbstractVector} = Perm{O,V}(order, data)

ReverseOrdering(by::By) = By(by.by, ReverseOrdering(by.order))
ReverseOrdering(perm::Perm) = Perm(ReverseOrdering(perm.order), perm.data)

lt(o::ForwardOrdering, a, b) = isless(a, b)
lt(o::ReverseOrdering, a, b) = lt(o.fwd, b, a)
lt(o::By, a, b) = lt(o.order, o.by(a), o.by(b))
lt(o::Lt, a, b) = o.lt(a, b)

function lt(p::Perm, a::Integer, b::Integer)
    da = p.data[a]
    db = p.data[b]
    return (lt(p.order, da, db)::Bool) | (!(lt(p.order, db, da)::Bool) & (a < b))
end

_ord(ltfn::typeof(isless), by, order::Ordering) = _by(by, order)
_ord(ltfn::typeof(isless), by, order::ForwardOrdering) = _by(by, order)
_ord(ltfn::typeof(isless), by, order::ReverseOrdering{ForwardOrdering}) = _by(by, order)
_ord(ltfn, by, order::ForwardOrdering) = _by(by, Lt(ltfn))
_ord(ltfn, by, order::ReverseOrdering{ForwardOrdering}) = reverse(_by(by, Lt(ltfn)))
_ord(ltfn, by, order::Ordering) = error("Passing both lt= and order= arguments is ambiguous; please pass order=Forward or order=Reverse (or leave default)")

_by(by, order::Ordering) = By(by, order)
_by(::typeof(identity), order::Ordering) = order

ord(ltfn, by, rev::Nothing, order::Ordering=Forward) = _ord(ltfn, by, order)

function ord(ltfn, by, rev::Bool, order::Ordering=Forward)
    o = _ord(ltfn, by, order)
    return rev ? ReverseOrdering(o) : o
end

ordtype(o::ReverseOrdering, vs::AbstractArray) = ordtype(o.fwd, vs)
ordtype(o::Perm, vs::AbstractArray) = ordtype(o.order, o.data)
ordtype(o::By, vs::AbstractArray) = typeof(o.by(vs[1]))
ordtype(o::Ordering, vs::AbstractArray) = eltype(vs)

end
