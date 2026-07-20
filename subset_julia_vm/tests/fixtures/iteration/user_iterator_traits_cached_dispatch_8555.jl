# User IteratorSize/IteratorEltype/eltype extensions must dispatch correctly
# when Base is served from the precompiled cache (Issue #8555 retires the
# #4088 disable-the-whole-Base-cache fallback for iterator trait hooks;
# cached CallTypedDispatch candidate lists are refreshed post-merge).
# Verified against upstream julia 1.12.

import Base: iterate, eltype, size, IteratorSize, IteratorEltype

struct Countdown8555
    start::Int
end

iterate(c::Countdown8555) = c.start <= 0 ? nothing : (c.start, c.start - 1)
iterate(c::Countdown8555, state::Int) = state <= 0 ? nothing : (state, state - 1)
IteratorSize(::Type{Countdown8555}) = Base.SizeUnknown()
IteratorEltype(::Type{Countdown8555}) = Base.HasEltype()
eltype(::Type{Countdown8555}) = Int

struct Grid8555
end

iterate(g::Grid8555) = (10, 1)
iterate(g::Grid8555, s::Int) = s >= 4 ? nothing : (10 + s, s + 1)
IteratorSize(::Type{Grid8555}) = Base.HasShape{2}()
IteratorEltype(::Type{Grid8555}) = Base.HasEltype()
eltype(::Type{Grid8555}) = Int
size(::Grid8555) = (2, 2)

ok = true

# Trait queries dispatch to the user methods (type-object dispatch through
# cached Base bytecode call sites).
ok = ok && Base.IteratorSize(Countdown8555) === Base.SizeUnknown()
ok = ok && Base.IteratorSize(Countdown8555(2)) === Base.SizeUnknown()
ok = ok && Base.IteratorEltype(Countdown8555) === Base.HasEltype()
ok = ok && Base.IteratorSize(Grid8555) === Base.HasShape{2}()

# collect drives _collect through the user traits: SizeUnknown + HasEltype
# must produce a typed Vector via the user eltype method.
v = collect(Countdown8555(3))
ok = ok && v == [3, 2, 1]
ok = ok && typeof(v) === Vector{Int64}

# HasShape{2} collects to a Matrix with the user size.
m = collect(Grid8555())
ok = ok && m == [10 12; 11 13]
ok = ok && typeof(m) === Matrix{Int64}
ok = ok && size(m) == (2, 2)

# Base iterators keep their own traits.
ok = ok && Base.IteratorSize(1:5) === Base.HasShape{1}()
ok = ok && typeof(Base.IteratorEltype(1:5)) === typeof(Base.HasEltype())

ok
