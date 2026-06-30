# =============================================================================
# iterators.jl - Iterator types and utilities
# =============================================================================
# Based on Julia's base/iterators.jl
#
# The iterate protocol:
#   iterate(collection) -> (element, state) | nothing
#   iterate(collection, state) -> (element, state) | nothing
#
# Note: Builtin types (Array, Tuple, Range, String) use VM instructions
# for iteration (IterateFirst/IterateNext). This file only defines iterate
# methods for custom iterator wrapper types.

# =============================================================================
# Enumerate - counter-based iteration wrapper
# =============================================================================
# Based on Julia's base/iterators.jl
#
# enumerate(iter) yields (i, x) where i is a counter starting at 1

struct Enumerate{I}
    itr::I
end

enumerate(iter) = Enumerate(iter)

function iterate(e::Enumerate)
    next = iterate(e.itr)
    if next === nothing
        return nothing
    end
    return ((1, next[1]), (2, next[2]))
end

function iterate(e::Enumerate, state)
    i = state[1]
    inner_state = state[2]
    next = iterate(e.itr, inner_state)
    if next === nothing
        return nothing
    end
    return ((i, next[1]), (i + 1, next[2]))
end

function length(e::Enumerate)
    return length(e.itr)
end

function size(e::Enumerate)
    return size(e.itr)
end

function last(e::Enumerate)
    return (length(e.itr), last(e.itr))
end

function eltype(::Type{Enumerate{I}}) where {I}
    return Tuple{Int64, eltype(I)}
end

function eltype(e::Enumerate)
    return Tuple{Int64, eltype(e.itr)}
end

function IteratorSize(::Type{Enumerate{I}}) where {I}
    return IteratorSize(I)
end

function IteratorSize(e::Enumerate)
    return IteratorSize(e.itr)
end

function IteratorEltype(::Type{Enumerate{I}}) where {I}
    return IteratorEltype(I)
end

function IteratorEltype(e::Enumerate)
    return IteratorEltype(e.itr)
end

function collect(e::Enumerate{I}) where {I}
    return _collect(1:1, e, IteratorEltype(e), IteratorSize(e))
end

# =============================================================================
# Zip - parallel iteration over multiple collections
# =============================================================================
# Based on Julia's base/iterators.jl
#
# zip(a, b) yields (a[i], b[i]) until either is exhausted

struct Zip{I1, I2}
    itr1::I1
    itr2::I2
end

zip(a, b) = Zip(a, b)

function iterate(z::Zip)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    if next1 === nothing || next2 === nothing
        return nothing
    end
    return ((next1[1], next2[1]), (next1[2], next2[2]))
end

function iterate(z::Zip, state)
    state1 = state[1]
    state2 = state[2]
    next1 = iterate(z.itr1, state1)
    next2 = iterate(z.itr2, state2)
    if next1 === nothing || next2 === nothing
        return nothing
    end
    return ((next1[1], next2[1]), (next1[2], next2[2]))
end

function length(z::Zip)
    return _zip_length_result(_zip_min_length(z.itr1, _zip_min_length(z.itr2, nothing)))
end

function _and_iteratorsize(a::IteratorSize, b::IteratorSize)
    if typeof(a) == typeof(b)
        return a
    elseif isa(a, HasLength) && isa(b, HasShape)
        return HasLength()
    elseif isa(a, HasShape) && isa(b, HasLength)
        return HasLength()
    end
    return SizeUnknown()
end

function _and_iteratoreltype(a::IteratorEltype, b::IteratorEltype)
    if typeof(a) == typeof(b)
        return a
    end
    return EltypeUnknown()
end

function zip_iteratorsize(a::IteratorSize, b::IteratorSize)
    if isa(a, IsInfinite) && isa(b, IsInfinite)
        return IsInfinite()
    elseif isa(a, HasLength) && isa(b, IsInfinite)
        return HasLength()
    elseif isa(a, HasShape) && isa(b, IsInfinite)
        return HasLength()
    elseif isa(a, IsInfinite)
        return zip_iteratorsize(b, a)
    end
    return _and_iteratorsize(a, b)
end

function zip_iteratorsize(a::IteratorSize, b::IteratorSize, c::IteratorSize)
    return zip_iteratorsize(a, zip_iteratorsize(b, c))
end

function zip_iteratorsize(a::IteratorSize, b::IteratorSize, c::IteratorSize, d::IteratorSize)
    return zip_iteratorsize(a, zip_iteratorsize(b, c, d))
end

function zip_iteratorsize(a::IteratorSize, b::IteratorSize, c::IteratorSize, d::IteratorSize, e::IteratorSize)
    return zip_iteratorsize(a, zip_iteratorsize(b, c, d, e))
end

function zip_iteratorsize(a::IteratorSize, b::IteratorSize, c::IteratorSize, d::IteratorSize, e::IteratorSize, f::IteratorSize)
    return zip_iteratorsize(a, zip_iteratorsize(b, c, d, e, f))
end

function zip_iteratorsize(a::IteratorSize, b::IteratorSize, c::IteratorSize, d::IteratorSize, e::IteratorSize, f::IteratorSize, g::IteratorSize)
    return zip_iteratorsize(a, zip_iteratorsize(b, c, d, e, f, g))
end

function zip_iteratoreltype(a::IteratorEltype, b::IteratorEltype)
    return _and_iteratoreltype(a, b)
end

function zip_iteratoreltype(a::IteratorEltype, b::IteratorEltype, c::IteratorEltype)
    return zip_iteratoreltype(a, zip_iteratoreltype(b, c))
end

function zip_iteratoreltype(a::IteratorEltype, b::IteratorEltype, c::IteratorEltype, d::IteratorEltype)
    return zip_iteratoreltype(a, zip_iteratoreltype(b, c, d))
end

function zip_iteratoreltype(a::IteratorEltype, b::IteratorEltype, c::IteratorEltype, d::IteratorEltype, e::IteratorEltype)
    return zip_iteratoreltype(a, zip_iteratoreltype(b, c, d, e))
end

function zip_iteratoreltype(a::IteratorEltype, b::IteratorEltype, c::IteratorEltype, d::IteratorEltype, e::IteratorEltype, f::IteratorEltype)
    return zip_iteratoreltype(a, zip_iteratoreltype(b, c, d, e, f))
end

function zip_iteratoreltype(a::IteratorEltype, b::IteratorEltype, c::IteratorEltype, d::IteratorEltype, e::IteratorEltype, f::IteratorEltype, g::IteratorEltype)
    return zip_iteratoreltype(a, zip_iteratoreltype(b, c, d, e, f, g))
end

function _zip_min_length(itr, n)
    if IteratorSize(itr) isa IsInfinite
        return n
    end
    len = length(itr)
    if n === nothing
        return len
    end
    return min(n, len)
end

function _zip_length_result(n)
    if n === nothing
        throw(ArgumentError("iterator is of undefined length"))
    end
    return n
end

function _zip_promote_size(a, b)
    if length(a) == 1 && length(b) == 1
        return (min(a[1], b[1]),)
    elseif length(a) == 2 && length(b) == 2
        return (min(a[1], b[1]), min(a[2], b[2]))
    elseif length(a) == 3 && length(b) == 3
        return (min(a[1], b[1]), min(a[2], b[2]), min(a[3], b[3]))
    elseif length(a) == 4 && length(b) == 4
        return (min(a[1], b[1]), min(a[2], b[2]), min(a[3], b[3]), min(a[4], b[4]))
    end
    return (min(_tuple_product(a), _tuple_product(b)),)
end

function _zip_promote_size(a, b, c)
    return _zip_promote_size(a, _zip_promote_size(b, c))
end

function _zip_promote_size(a, b, c, d)
    return _zip_promote_size(a, _zip_promote_size(b, c, d))
end

function _zip_promote_size(a, b, c, d, e)
    return _zip_promote_size(a, _zip_promote_size(b, c, d, e))
end

function _zip_promote_size(a, b, c, d, e, f)
    return _zip_promote_size(a, _zip_promote_size(b, c, d, e, f))
end

function _zip_promote_size(a, b, c, d, e, f, g)
    return _zip_promote_size(a, _zip_promote_size(b, c, d, e, f, g))
end

function _tuple_product(t)
    result = 1
    for x in t
        result = result * x
    end
    return result
end

function _axes_from_size_tuple(s)
    if length(s) == 1
        return (1:s[1],)
    elseif length(s) == 2
        return (1:s[1], 1:s[2])
    elseif length(s) == 3
        return (1:s[1], 1:s[2], 1:s[3])
    elseif length(s) == 4
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4])
    end
    return (1:_tuple_product(s),)
end

function IteratorSize(z::Zip)
    return zip_iteratorsize(IteratorSize(z.itr1), IteratorSize(z.itr2))
end

function IteratorEltype(z::Zip)
    return zip_iteratoreltype(IteratorEltype(z.itr1), IteratorEltype(z.itr2))
end

function eltype(z::Zip)
    return Tuple{eltype(z.itr1), eltype(z.itr2)}
end

function size(z::Zip)
    return _zip_promote_size(size(z.itr1), size(z.itr2))
end

function axes(z::Zip)
    return _axes_from_size_tuple(size(z))
end

# =============================================================================
# Zip3 - parallel iteration over 3 collections (Issue #1990)
# =============================================================================

struct Zip3{I1, I2, I3}
    itr1::I1
    itr2::I2
    itr3::I3
end

zip(a, b, c) = Zip3(a, b, c)

function iterate(z::Zip3)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    next3 = iterate(z.itr3)
    if next1 === nothing || next2 === nothing || next3 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1]), (next1[2], next2[2], next3[2]))
end

function iterate(z::Zip3, state)
    next1 = iterate(z.itr1, state[1])
    next2 = iterate(z.itr2, state[2])
    next3 = iterate(z.itr3, state[3])
    if next1 === nothing || next2 === nothing || next3 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1]), (next1[2], next2[2], next3[2]))
end

function length(z::Zip3)
    return _zip_length_result(_zip_min_length(z.itr1, _zip_min_length(z.itr2, _zip_min_length(z.itr3, nothing))))
end

function IteratorSize(z::Zip3)
    return zip_iteratorsize(IteratorSize(z.itr1), IteratorSize(z.itr2), IteratorSize(z.itr3))
end

function IteratorEltype(z::Zip3)
    return zip_iteratoreltype(IteratorEltype(z.itr1), IteratorEltype(z.itr2), IteratorEltype(z.itr3))
end

function eltype(z::Zip3)
    return Tuple{eltype(z.itr1), eltype(z.itr2), eltype(z.itr3)}
end

function size(z::Zip3)
    return _zip_promote_size(size(z.itr1), size(z.itr2), size(z.itr3))
end

function axes(z::Zip3)
    return _axes_from_size_tuple(size(z))
end

# =============================================================================
# Zip4 - parallel iteration over 4 collections (Issue #1990)
# =============================================================================

struct Zip4{I1, I2, I3, I4}
    itr1::I1
    itr2::I2
    itr3::I3
    itr4::I4
end

zip(a, b, c, d) = Zip4(a, b, c, d)

function iterate(z::Zip4)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    next3 = iterate(z.itr3)
    next4 = iterate(z.itr4)
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1]), (next1[2], next2[2], next3[2], next4[2]))
end

function iterate(z::Zip4, state)
    next1 = iterate(z.itr1, state[1])
    next2 = iterate(z.itr2, state[2])
    next3 = iterate(z.itr3, state[3])
    next4 = iterate(z.itr4, state[4])
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1]), (next1[2], next2[2], next3[2], next4[2]))
end

function length(z::Zip4)
    return _zip_length_result(_zip_min_length(z.itr1, _zip_min_length(z.itr2, _zip_min_length(z.itr3, _zip_min_length(z.itr4, nothing)))))
end

function IteratorSize(z::Zip4)
    return zip_iteratorsize(IteratorSize(z.itr1), IteratorSize(z.itr2), IteratorSize(z.itr3), IteratorSize(z.itr4))
end

function IteratorEltype(z::Zip4)
    return zip_iteratoreltype(IteratorEltype(z.itr1), IteratorEltype(z.itr2), IteratorEltype(z.itr3), IteratorEltype(z.itr4))
end

function eltype(z::Zip4)
    return Tuple{eltype(z.itr1), eltype(z.itr2), eltype(z.itr3), eltype(z.itr4)}
end

function size(z::Zip4)
    return _zip_promote_size(size(z.itr1), size(z.itr2), size(z.itr3), size(z.itr4))
end

function axes(z::Zip4)
    return _axes_from_size_tuple(size(z))
end

# =============================================================================
# Zip5 - parallel iteration over 5 collections (Issue #4281)
# =============================================================================

struct Zip5{I1, I2, I3, I4, I5}
    itr1::I1
    itr2::I2
    itr3::I3
    itr4::I4
    itr5::I5
end

zip(a, b, c, d, e) = Zip5(a, b, c, d, e)

function iterate(z::Zip5)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    next3 = iterate(z.itr3)
    next4 = iterate(z.itr4)
    next5 = iterate(z.itr5)
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing || next5 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1], next5[1]), (next1[2], next2[2], next3[2], next4[2], next5[2]))
end

function iterate(z::Zip5, state)
    next1 = iterate(z.itr1, state[1])
    next2 = iterate(z.itr2, state[2])
    next3 = iterate(z.itr3, state[3])
    next4 = iterate(z.itr4, state[4])
    next5 = iterate(z.itr5, state[5])
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing || next5 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1], next5[1]), (next1[2], next2[2], next3[2], next4[2], next5[2]))
end

function length(z::Zip5)
    return _zip_length_result(_zip_min_length(z.itr1, _zip_min_length(z.itr2, _zip_min_length(z.itr3, _zip_min_length(z.itr4, _zip_min_length(z.itr5, nothing))))))
end

function IteratorSize(z::Zip5)
    return zip_iteratorsize(IteratorSize(z.itr1), IteratorSize(z.itr2), IteratorSize(z.itr3), IteratorSize(z.itr4), IteratorSize(z.itr5))
end

function IteratorEltype(z::Zip5)
    return zip_iteratoreltype(IteratorEltype(z.itr1), IteratorEltype(z.itr2), IteratorEltype(z.itr3), IteratorEltype(z.itr4), IteratorEltype(z.itr5))
end

function eltype(z::Zip5)
    return Tuple{eltype(z.itr1), eltype(z.itr2), eltype(z.itr3), eltype(z.itr4), eltype(z.itr5)}
end

function size(z::Zip5)
    return _zip_promote_size(size(z.itr1), size(z.itr2), size(z.itr3), size(z.itr4), size(z.itr5))
end

function axes(z::Zip5)
    return _axes_from_size_tuple(size(z))
end

# =============================================================================
# Zip6 - parallel iteration over 6 collections (Issue #4281)
# =============================================================================

struct Zip6{I1, I2, I3, I4, I5, I6}
    itr1::I1
    itr2::I2
    itr3::I3
    itr4::I4
    itr5::I5
    itr6::I6
end

zip(a, b, c, d, e, f) = Zip6(a, b, c, d, e, f)

function iterate(z::Zip6)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    next3 = iterate(z.itr3)
    next4 = iterate(z.itr4)
    next5 = iterate(z.itr5)
    next6 = iterate(z.itr6)
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing || next5 === nothing || next6 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1], next5[1], next6[1]), (next1[2], next2[2], next3[2], next4[2], next5[2], next6[2]))
end

function iterate(z::Zip6, state)
    next1 = iterate(z.itr1, state[1])
    next2 = iterate(z.itr2, state[2])
    next3 = iterate(z.itr3, state[3])
    next4 = iterate(z.itr4, state[4])
    next5 = iterate(z.itr5, state[5])
    next6 = iterate(z.itr6, state[6])
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing || next5 === nothing || next6 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1], next5[1], next6[1]), (next1[2], next2[2], next3[2], next4[2], next5[2], next6[2]))
end

function length(z::Zip6)
    return _zip_length_result(_zip_min_length(z.itr1, _zip_min_length(z.itr2, _zip_min_length(z.itr3, _zip_min_length(z.itr4, _zip_min_length(z.itr5, _zip_min_length(z.itr6, nothing)))))))
end

function IteratorSize(z::Zip6)
    return zip_iteratorsize(IteratorSize(z.itr1), IteratorSize(z.itr2), IteratorSize(z.itr3), IteratorSize(z.itr4), IteratorSize(z.itr5), IteratorSize(z.itr6))
end

function IteratorEltype(z::Zip6)
    return zip_iteratoreltype(IteratorEltype(z.itr1), IteratorEltype(z.itr2), IteratorEltype(z.itr3), IteratorEltype(z.itr4), IteratorEltype(z.itr5), IteratorEltype(z.itr6))
end

function eltype(z::Zip6)
    return Tuple{eltype(z.itr1), eltype(z.itr2), eltype(z.itr3), eltype(z.itr4), eltype(z.itr5), eltype(z.itr6)}
end

function size(z::Zip6)
    return _zip_promote_size(size(z.itr1), size(z.itr2), size(z.itr3), size(z.itr4), size(z.itr5), size(z.itr6))
end

function axes(z::Zip6)
    return _axes_from_size_tuple(size(z))
end

# =============================================================================
# Zip7 - parallel iteration over 7 collections (Issue #4281)
# =============================================================================

struct Zip7{I1, I2, I3, I4, I5, I6, I7}
    itr1::I1
    itr2::I2
    itr3::I3
    itr4::I4
    itr5::I5
    itr6::I6
    itr7::I7
end

zip(a, b, c, d, e, f, g) = Zip7(a, b, c, d, e, f, g)

function iterate(z::Zip7)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    next3 = iterate(z.itr3)
    next4 = iterate(z.itr4)
    next5 = iterate(z.itr5)
    next6 = iterate(z.itr6)
    next7 = iterate(z.itr7)
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing || next5 === nothing || next6 === nothing || next7 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1], next5[1], next6[1], next7[1]), (next1[2], next2[2], next3[2], next4[2], next5[2], next6[2], next7[2]))
end

function iterate(z::Zip7, state)
    next1 = iterate(z.itr1, state[1])
    next2 = iterate(z.itr2, state[2])
    next3 = iterate(z.itr3, state[3])
    next4 = iterate(z.itr4, state[4])
    next5 = iterate(z.itr5, state[5])
    next6 = iterate(z.itr6, state[6])
    next7 = iterate(z.itr7, state[7])
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing || next5 === nothing || next6 === nothing || next7 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1], next5[1], next6[1], next7[1]), (next1[2], next2[2], next3[2], next4[2], next5[2], next6[2], next7[2]))
end

function length(z::Zip7)
    return _zip_length_result(_zip_min_length(z.itr1, _zip_min_length(z.itr2, _zip_min_length(z.itr3, _zip_min_length(z.itr4, _zip_min_length(z.itr5, _zip_min_length(z.itr6, _zip_min_length(z.itr7, nothing))))))))
end

function IteratorSize(z::Zip7)
    return zip_iteratorsize(IteratorSize(z.itr1), IteratorSize(z.itr2), IteratorSize(z.itr3), IteratorSize(z.itr4), IteratorSize(z.itr5), IteratorSize(z.itr6), IteratorSize(z.itr7))
end

function IteratorEltype(z::Zip7)
    return zip_iteratoreltype(IteratorEltype(z.itr1), IteratorEltype(z.itr2), IteratorEltype(z.itr3), IteratorEltype(z.itr4), IteratorEltype(z.itr5), IteratorEltype(z.itr6), IteratorEltype(z.itr7))
end

function eltype(z::Zip7)
    return Tuple{eltype(z.itr1), eltype(z.itr2), eltype(z.itr3), eltype(z.itr4), eltype(z.itr5), eltype(z.itr6), eltype(z.itr7)}
end

function size(z::Zip7)
    return _zip_promote_size(size(z.itr1), size(z.itr2), size(z.itr3), size(z.itr4), size(z.itr5), size(z.itr6), size(z.itr7))
end

function axes(z::Zip7)
    return _axes_from_size_tuple(size(z))
end

# =============================================================================
# Take - iterate first N elements
# =============================================================================
# Based on Julia's base/iterators.jl
#
# take(iter, n) yields at most the first n elements from iter

struct Take{I}
    xs::I
    n::Int64
end

take(xs, n::Integer) = Take(xs, Int64(n))

function _min_length(a, b, asize, bsize)
    return min(length(a), length(b))
end

function _min_length(a, b, asize, bsize::IsInfinite)
    return length(a)
end

function _min_length(a, b, asize::IsInfinite, bsize)
    return length(b)
end

function iterate(it::Take)
    if it.n <= 0
        return nothing
    end
    next = iterate(it.xs)
    if next === nothing
        return nothing
    end
    return (next[1], (it.n - 1, next[2]))
end

function iterate(it::Take, state)
    n = state[1]
    if n <= 0
        return nothing
    end
    inner_state = state[2]
    next = iterate(it.xs, inner_state)
    if next === nothing
        return nothing
    end
    return (next[1], (n - 1, next[2]))
end

function length(it::Take)
    return _min_length(it.xs, 1:it.n, IteratorSize(it.xs), HasLength())
end

function _take_iteratorsize(isz)
    if isa(isz, SizeUnknown)
        return SizeUnknown()
    end
    return HasLength()
end

function IteratorSize(it::Take)
    return _take_iteratorsize(IteratorSize(it.xs))
end

function IteratorEltype(it::Take)
    return IteratorEltype(it.xs)
end

function eltype(it::Take)
    return eltype(it.xs)
end

# =============================================================================
# Drop - skip first N elements
# =============================================================================
# Based on Julia's base/iterators.jl
#
# drop(iter, n) skips the first n elements and yields the rest

struct Drop{I}
    xs::I
    n::Int64
end

drop(xs, n::Integer) = Drop(xs, Int64(n))

function iterate(it::Drop)
    y = iterate(it.xs)
    for i in 1:it.n
        if y === nothing
            return y
        end
        y = iterate(it.xs, y[2])
    end
    return y
end

function iterate(it::Drop, state)
    return iterate(it.xs, state)
end

function length(it::Drop)
    n = length(it.xs) - it.n
    if n < 0
        return 0
    end
    return n
end

function _drop_iteratorsize(isz)
    if isa(isz, SizeUnknown)
        return SizeUnknown()
    elseif isa(isz, IsInfinite)
        return IsInfinite()
    end
    return HasLength()
end

function IteratorSize(it::Drop)
    return _drop_iteratorsize(IteratorSize(it.xs))
end

function IteratorEltype(it::Drop)
    return IteratorEltype(it.xs)
end

function eltype(it::Drop)
    return eltype(it.xs)
end

# =============================================================================
# TakeWhile - yield elements while predicate is true
# =============================================================================
# Based on Julia's base/iterators.jl
#
# takewhile(pred, iter) yields elements from iter as long as pred returns true,
# then stops. Once pred returns false, no more elements are yielded.

struct TakeWhile{I, P}
    pred::P
    xs::I
end

function IteratorSize(it::TakeWhile)
    return SizeUnknown()
end

function IteratorEltype(it::TakeWhile)
    return IteratorEltype(it.xs)
end

function eltype(it::TakeWhile)
    return eltype(it.xs)
end

"""
    takewhile(pred, iter)

An iterator that yields elements from `iter` as long as predicate `pred`
is true, afterwards drops every element.

# Examples
```julia
collect(takewhile(x -> x < 4, [1, 2, 3, 4, 5]))
# => [1, 2, 3]
```
"""
takewhile(pred, xs) = TakeWhile(pred, xs)

function iterate(ibl::TakeWhile)
    next = iterate(ibl.xs)
    if next === nothing
        return nothing
    end
    if !ibl.pred(next[1])
        return nothing
    end
    return next
end

function iterate(ibl::TakeWhile, state)
    next = iterate(ibl.xs, state)
    if next === nothing
        return nothing
    end
    if !ibl.pred(next[1])
        return nothing
    end
    return next
end

# =============================================================================
# DropWhile - skip elements while predicate is true, then yield the rest
# =============================================================================
# Based on Julia's base/iterators.jl
#
# dropwhile(pred, iter) skips elements from iter as long as pred returns true,
# then yields all remaining elements (even if pred becomes true again later).

struct DropWhile{I, P}
    pred::P
    xs::I
end

function IteratorSize(it::DropWhile)
    return SizeUnknown()
end

function IteratorEltype(it::DropWhile)
    return IteratorEltype(it.xs)
end

function eltype(it::DropWhile)
    return eltype(it.xs)
end

"""
    dropwhile(pred, iter)

An iterator that drops elements from `iter` as long as predicate `pred`
is true, afterwards returns every element.

# Examples
```julia
collect(dropwhile(x -> x < 3, [1, 2, 3, 4, 1]))
# => [3, 4, 1]
```
"""
dropwhile(pred, xs) = DropWhile(pred, xs)

function iterate(ibl::DropWhile)
    next = iterate(ibl.xs)
    while next !== nothing
        if !ibl.pred(next[1])
            return next
        end
        next = iterate(ibl.xs, next[2])
    end
    return nothing
end

function iterate(ibl::DropWhile, state)
    return iterate(ibl.xs, state)
end

# =============================================================================
# Collect - materialize iterator to array
# =============================================================================
# Generic collect using the iterator trait protocol.
# Based on Julia's base/array.jl:
#   collect(itr) = _collect(..., itr, IteratorEltype(itr), IteratorSize(itr))
#
# SubsetJuliaVM keeps the default IteratorEltype conservative for now because
# not every built-in iterator exposes eltype through Pure Julia yet. Specific
# methods such as collect(::Tuple) and collect(::Array) preserve element types.

function _collect_to_any(itr)
    result = Vector{Any}()
    next = iterate(itr)
    while next !== nothing
        x, state = next
        push!(result, x)
        next = iterate(itr, state)
    end
    return result
end

function _collect_with_eltype(itr, ::Type{T}) where {T}
    result = Vector{T}(undef, 0)
    next = iterate(itr)
    while next !== nothing
        x, state = next
        push!(result, x)
        next = iterate(itr, state)
    end
    return result
end

function _collect_with_eltype_length(itr, ::Type{T}, len::Integer) where {T}
    return _collect_to!(_array_for_inner(T, HasLength(), len), itr)
end

function _collect_with_eltype_shape(itr, ::Type{T}) where {T}
    return _collect_to!(_array_for_inner_shape(T, _shape_to_dims(axes(itr))), itr)
end

function _collect_to!(result, itr)
    i = Int64(1)
    next = iterate(itr)
    while next !== nothing
        x, state = next
        result[i] = x
        i += Int64(1)
        next = iterate(itr, state)
    end
    return result
end

function _copy_indexed_collect_values_to!(result, values)
    for i in 1:length(values)
        result[i] = values[i]
    end
    return result
end

function _similar_shape(itr, isz)
    if isa(isz, HasLength)
        return length(itr)
    elseif isa(isz, HasShape{1})
        return axes(itr)
    elseif isa(isz, HasShape{2})
        return axes(itr)
    elseif isa(isz, HasShape{3})
        return axes(itr)
    elseif isa(isz, HasShape{4})
        return axes(itr)
    elseif isa(isz, HasShape)
        return axes(itr)
    end
    return nothing
end

function _similar_for(cont, ::Type{T}, itr, isz, shp) where {T}
    return _array_for_inner(T, isz, shp)
end

function _similar_for(cont::Array, ::Type{T}, itr, isz::SizeUnknown, shp) where {T}
    return similar(cont, T, 0)
end

function _similar_for(cont::Array, ::Type{T}, itr, isz::HasLength, len::Integer) where {T}
    return similar(cont, T, len)
end

function _similar_for(cont::Array, ::Type{T}, itr, isz::HasShape, axs) where {T}
    return similar(cont, T, _shape_to_dims(axs))
end

function _similar_for(cont::Memory, ::Type{T}, itr, isz::SizeUnknown, shp) where {T}
    return similar(cont, T, Int64(0))
end

function _similar_for(cont::Memory, ::Type{T}, itr, isz::HasLength, len::Integer) where {T}
    return similar(cont, T, Int64(len))
end

function _similar_for(cont::Memory, ::Type{T}, itr, isz::HasShape, axs) where {T}
    return similar(cont, T, _shape_to_dims(axs))
end

function _shape_to_dims(axs::Tuple)
    dims = ()
    for ax in axs
        dims = tuple(dims..., length(ax))
    end
    return dims
end

function _array_for_inner(::Type{T}, isz, shp) where {T}
    if isa(isz, HasLength)
        return Vector{T}(undef, Int64(shp))
    elseif isa(isz, HasShape{1})
        return _array_for_inner_shape(T, _shape_to_dims(shp))
    elseif isa(isz, HasShape{2})
        return _array_for_inner_shape(T, _shape_to_dims(shp))
    elseif isa(isz, HasShape{3})
        return _array_for_inner_shape(T, _shape_to_dims(shp))
    elseif isa(isz, HasShape{4})
        return _array_for_inner_shape(T, _shape_to_dims(shp))
    elseif isa(isz, HasShape)
        return _array_for_inner_shape(T, _shape_to_dims(shp))
    end
    return Vector{T}(undef, 0)
end

function _array_for_inner(::Type{SubArray{T}}, isz::HasLength, shp) where T
    return _array_undef_from_dims(SubArray{T}, (Int64(shp),))
end

function _check_collect_shape_rank(dims::Tuple)
    n = length(dims)
    if n > 8
        throw(ArgumentError("collect shape rank currently supports up to 8 dimensions"))
    end
    return nothing
end

function _array_for_inner_shape(::Type{T}, dims::Tuple) where {T}
    return _array_undef_from_dims(T, dims)
end

function _array_for_inner_shape(::Type{Int64}, dims::Tuple)
    return _array_undef_from_dims(Int64, dims)
end

function _array_for_inner_shape(::Type{Float64}, dims::Tuple)
    return _array_undef_from_dims(Float64, dims)
end

function _array_for_inner_shape(::Type{Float32}, dims::Tuple)
    return _array_undef_from_dims(Float32, dims)
end

function _array_for_inner_shape(::Type{Bool}, dims::Tuple)
    return _array_undef_from_dims(Bool, dims)
end

function _array_for_inner_shape(::Type{String}, dims::Tuple)
    return _array_undef_from_dims(String, dims)
end

function _array_for_inner_shape(::Type{Char}, dims::Tuple)
    return _array_undef_from_dims(Char, dims)
end

function _array_for_inner_shape(::Type{Complex{Float64}}, dims::Tuple)
    return _array_undef_from_dims(Complex{Float64}, dims)
end

function _array_for_inner_shape(::Type{Any}, dims::Tuple)
    return _array_undef_from_dims(Any, dims)
end

function _array_for_inner_shape(::Type{Real}, dims::Tuple)
    return _array_undef_from_dims(Real, dims)
end

function _copy_collect_to_eltype(src, ::Type{T}) where {T}
    return _copy_collect_to_eltype(src, T, length(src))
end

function _copy_collect_to_eltype(src, ::Type{T}, filled::Int64) where {T}
    result = similar(src, T)
    for i in 1:filled
        result[i] = src[i]
    end
    return result
end

function setindex_widen_up_to(dest, el, i)
    T = typejoin(eltype(dest), typeof(el))
    new = _copy_collect_to_eltype(dest, T, i - Int64(1))
    new[i] = el
    return new
end

function collect_to_with_first!(dest, v1, itr, st)
    T = eltype(dest)
    dest[1] = v1
    i = Int64(2)
    next = iterate(itr, st)
    while next !== nothing
        x, state = next
        if isa(x, T)
            dest[i] = x
        else
            dest = setindex_widen_up_to(dest, x, i)
            T = eltype(dest)
        end
        i += Int64(1)
        next = iterate(itr, state)
    end
    return dest
end

function _empty_widen_container(dest, ::Type{T}) where {T}
    return similar(dest, T, Int64(0))
end

function grow_to!(dest, itr)
    next = iterate(itr)
    if next === nothing
        return dest
    end
    x, state = next
    dest2 = _empty_widen_container(dest, typeof(x))
    push!(dest2, x)
    return grow_to!(dest2, itr, state)
end

function push_widen(dest, el)
    T = typejoin(eltype(dest), typeof(el))
    new = _copy_collect_to_eltype(dest, T)
    push!(new, el)
    return new
end

function grow_to!(dest, itr, state)
    T = eltype(dest)
    next = iterate(itr, state)
    while next !== nothing
        x, state = next
        if isa(x, T)
            push!(dest, x)
        else
            dest = push_widen(dest, x)
            T = eltype(dest)
        end
        next = iterate(itr, state)
    end
    return dest
end

function _collect_unknown_with_shape(itr, isz)
    return _collect_unknown_with_shape(1:1, itr, isz)
end

function _collect_unknown_with_shape(cont, itr, isz)
    shp = _similar_shape(itr, isz)
    next = iterate(itr)
    if next === nothing
        return _similar_for(cont, Union{}, itr, isz, shp)
    end
    x, state = next
    return collect_to_with_first!(_similar_for(cont, typeof(x), itr, isz, shp), x, itr, state)
end

function _collect(::Type{T}, itr, isz::HasLength) where {T}
    return _collect_to!(_array_for_inner(T, isz, _similar_shape(itr, isz)), itr)
end

function _collect(::Type{T}, itr::Array, isz::HasShape) where {T}
    return _collect_to!(_array_for_inner(T, isz, _similar_shape(itr, isz)), itr)
end

function _collect(::Type{T}, itr, isz::HasShape) where {T}
    return _collect_to!(_array_for_inner(T, isz, _similar_shape(itr, isz)), itr)
end

function _collect(::Type{T}, itr, isz::SizeUnknown) where {T}
    result = Vector{T}(undef, 0)
    for x in itr
        push!(result, x)
    end
    return result
end

function collect(::Type{T}, itr::Array) where {T}
    if ndims(itr) > 1
        return _collect_with_eltype_shape(itr, T)
    end
    return _collect_with_eltype_length(itr, T, length(itr))
end

function collect(m::Memory)
    return _collect_with_eltype_length(m, eltype(m), length(m))
end

function _collect_unknown_widen(itr)
    next = iterate(itr)
    if next === nothing
        return Vector{Union{}}(undef, 0)
    end

    x, state = next
    T = typeof(x)
    result = Vector{T}(undef, 0)
    push!(result, x)

    next = iterate(itr, state)
    while next !== nothing
        x, state = next
        if isa(x, T)
            push!(result, x)
        else
            T = typejoin(T, typeof(x))
            result = _copy_collect_to_eltype(result, T)
            push!(result, x)
        end
        next = iterate(itr, state)
    end
    return result
end

function _collect_unknown_sizeunknown(cont, itr)
    return grow_to!(_similar_for(cont, Any, itr, SizeUnknown(), nothing), itr)
end

function _collect_unknown_widen(cont, itr, isz::HasLength)
    shp = _similar_shape(itr, isz)
    next = iterate(itr)
    if next === nothing
        # The iterator yielded nothing, so no runtime element fixes the eltype.
        # `_similar_for(cont, Union{}, ...)` hit an unbound type parameter when the
        # container is a range (e.g. `collect(Iterators.take(generator, 0))`);
        # return the empty `Vector{Union{}}` directly, mirroring the single-argument
        # `_collect_unknown_widen` empty path above (Issue #5752). (Upstream's
        # compile-time element inference would narrow this to e.g. `Int64[]`, which
        # the no-JIT runtime cannot reproduce — the empty value compares equal.)
        return Vector{Union{}}(undef, 0)
    end

    x, state = next
    return collect_to_with_first!(_similar_for(cont, typeof(x), itr, isz, shp), x, itr, state)
end

function _collect(cont, itr, et, isz::HasShape)
    if isa(et, EltypeUnknown)
        return _collect_unknown_with_shape(cont, itr, isz)
    end
    return _collect_to!(_similar_for(cont, eltype(itr), itr, isz, _similar_shape(itr, isz)), itr)
end

function _collect(cont, itr, ::HasEltype, isz::HasShape)
    return _collect_to!(_similar_for(cont, eltype(itr), itr, isz, _similar_shape(itr, isz)), itr)
end

function _collect(cont, itr, ::EltypeUnknown, isz::HasShape)
    return _collect_unknown_with_shape(cont, itr, isz)
end

function _collect_memory_generator_values(cont::Memory, itr::Generator)
    values = collect_similar([0.0], itr)
    T = eltype(values)
    if ndims(values) > 1
        return values
    else
        result = similar(cont, T, length(values))
    end
    return _copy_indexed_collect_values_to!(result, values)
end

function _collect(cont::Memory, itr::Generator, ::EltypeUnknown, isz::HasShape)
    values = collect_similar([0.0], itr)
    T = eltype(values)
    if ndims(values) > 1
        return values
    else
        result = similar(cont, T, length(values))
    end
    return _copy_indexed_collect_values_to!(result, values)
end

function _collect(cont, itr, et, isz::HasLength)
    if isa(et, EltypeUnknown)
        return _collect_unknown_widen(cont, itr, isz)
    end
    if isa(itr, Array) && ndims(itr) > 1
        array_size = IteratorSize(itr)
        return _collect_to!(_similar_for(cont, eltype(itr), itr, array_size, _similar_shape(itr, array_size)), itr)
    end
    return _collect_to!(_similar_for(cont, eltype(itr), itr, isz, _similar_shape(itr, isz)), itr)
end

function _collect(cont, itr, ::HasEltype, isz::HasLength)
    return _collect_to!(_similar_for(cont, eltype(itr), itr, isz, _similar_shape(itr, isz)), itr)
end

function _collect(cont, itr, ::EltypeUnknown, isz::HasLength)
    return _collect_unknown_widen(cont, itr, isz)
end

function _collect(cont::Memory, itr::Generator, ::EltypeUnknown, isz::HasLength)
    values = collect_similar([0.0], itr)
    T = eltype(values)
    if ndims(values) > 1
        return values
    else
        result = similar(cont, T, length(values))
    end
    return _copy_indexed_collect_values_to!(result, values)
end

function _collect(cont, itr, ::HasEltype, isz::SizeUnknown)
    result = _similar_for(cont, eltype(itr), itr, isz, nothing)
    for x in itr
        push!(result, x)
    end
    return result
end

function _collect(cont, itr, ::EltypeUnknown, isz::SizeUnknown)
    return _collect_unknown_sizeunknown(cont, itr)
end

function _collect(cont, itr, et, isz::IteratorSize)
    if isa(et, EltypeUnknown)
        if isa(isz, HasLength)
            return _collect_unknown_widen(cont, itr, isz)
        elseif isa(isz, HasShape)
            return _collect_unknown_with_shape(cont, itr, isz)
        end
        return _collect_to_any(itr)
    end
    # `et` is `HasEltype`. The more specific `_collect(cont, itr, ::HasEltype,
    # isz::HasShape)` should win dispatch, but when this abstract `::IteratorSize`
    # catch-all is selected for a shaped iterator (Issue #5846) we must still
    # preserve the iterator's shape rather than flatten it via `_collect_with_eltype`.
    if isa(isz, HasShape)
        return _collect_to!(_similar_for(cont, eltype(itr), itr, isz, _similar_shape(itr, isz)), itr)
    end
    return _collect_with_eltype(itr, eltype(itr))
end

function _collect(cont, itr::Tuple, ::EltypeUnknown, isz::IteratorSize)
    if isa(isz, HasLength)
        return _collect_unknown_widen(cont, itr, isz)
    elseif isa(isz, HasShape)
        return _collect_unknown_with_shape(cont, itr, isz)
    end
    return _collect_unknown_widen(itr)
end

function _collect(cont, itr::Tuple, ::EltypeUnknown, isz::HasLength)
    return _collect_unknown_widen(cont, itr, isz)
end

function _collect(cont, itr, ::EltypeUnknown, isz::IteratorSize)
    if isa(isz, HasLength)
        return _collect_unknown_widen(cont, itr, isz)
    elseif isa(isz, HasShape)
        return _collect_unknown_with_shape(cont, itr, isz)
    end
    return _collect_to_any(itr)
end

function collect(t::Tuple)
    if length(t) == 0
        return Vector{Union{}}(undef, 0)
    end

    T = typeof(t[1])
    for x in t
        if typeof(x) != T
            T = typejoin(T, typeof(x))
        end
    end
    return _collect_with_eltype(t, T)
end

function collect(s::String)
    return _collect_with_eltype(s, Char)
end

function collect_similar(cont, itr)
    if itr isa Base.Generator
        return collect(itr)
    end
    return _collect(cont, itr, IteratorEltype(itr), IteratorSize(itr))
end

function collect_similar(cont::Memory, itr::Generator)
    values = collect_similar([0.0], itr)
    T = eltype(values)
    if ndims(values) > 1
        return values
    else
        result = similar(cont, T, length(values))
    end
    return _collect_to!(result, values)
end

function collect_similar(cont, itr::Generator)
    return collect(itr)
end

function collect_similar(cont, itr::Tuple)
    return _collect(cont, itr, EltypeUnknown(), IteratorSize(itr))
end

function collect_similar(cont, m::Memory)
    return _collect_with_eltype_length(m, eltype(m), length(m))
end

function _collect_empty_product_iterator()
    result = Array{Tuple{}}(undef, ())
    result[1] = ()
    return result
end

function collect(it::ProductIterator)
    if _product_iterator_arity(it) == 0
        return _collect_empty_product_iterator()
    end
    return _collect(1:1, it, IteratorEltype(it), IteratorSize(it))
end

function collect(it::Take)
    return _collect(1:1, it, IteratorEltype(it), IteratorSize(it))
end

function collect(it::Drop)
    return _collect(1:1, it, IteratorEltype(it), IteratorSize(it))
end

function collect(it::TakeWhile)
    return _collect(1:1, it, IteratorEltype(it), IteratorSize(it))
end

function collect(it::DropWhile)
    return _collect(1:1, it, IteratorEltype(it), IteratorSize(it))
end

function collect(itr)
    return _collect(1:1, itr, IteratorEltype(itr), IteratorSize(itr))
end

# =============================================================================
# CartesianIndex - multi-dimensional index wrapper
# =============================================================================
# Based on Julia's base/multidimensional.jl
#
# CartesianIndex(i, j, k...) creates a multi-dimensional index
# A[I] is equivalent to A[i, j, k...]

struct CartesianIndex
    I
end

# Note: The struct constructor CartesianIndex(tuple) is auto-generated.
# The splat constructor CartesianIndex(i, j, k...) wraps the integer args into a
# tuple (matching upstream `CartesianIndex(index::Integer...) = CartesianIndex(index)`).
# The single-tuple form `CartesianIndex((1, 2))` continues to hit the auto-generated
# single-field constructor.
CartesianIndex(index::Int64...) = CartesianIndex(index)

# Access to index tuple
Tuple(ci::CartesianIndex) = ci.I

# Length (number of dimensions)
length(ci::CartesianIndex) = length(ci.I)

# Indexing into CartesianIndex
getindex(ci::CartesianIndex, i::Int64) = ci.I[i]

# Equality
==(a::CartesianIndex, b::CartesianIndex) = a.I == b.I

# Arithmetic and scalar multiplication (Issue #5136).
# Mirrors upstream base/multidimensional.jl:
#   (+)(index) = index
#   (-)(index) = CartesianIndex(map(-, index.I))
#   (+)(i1, i2) = CartesianIndex(map(+, i1.I, i2.I))
#   (-)(i1, i2) = CartesianIndex(map(-, i1.I, i2.I))
#   (*)(index, a) = CartesianIndex(map(x -> x * a, index.I)); (*)(a, index) = index * a
+(index::CartesianIndex) = index
-(index::CartesianIndex) = CartesianIndex(map(-, index.I))
+(a::CartesianIndex, b::CartesianIndex) = CartesianIndex(map(+, a.I, b.I))
-(a::CartesianIndex, b::CartesianIndex) = CartesianIndex(map(-, a.I, b.I))
*(index::CartesianIndex, a::Int64) = CartesianIndex(map(x -> x * a, index.I))
*(a::Int64, index::CartesianIndex) = index * a

# Show
function show(io::IO, ci::CartesianIndex)
    print(io, "CartesianIndex(")
    for i in 1:length(ci.I)
        if i > 1
            print(io, ", ")
        end
        print(io, ci.I[i])
    end
    print(io, ")")
end

# =============================================================================
# CartesianIndices - iterator over all CartesianIndex in a region
# =============================================================================
# Based on Julia's base/multidimensional.jl
#
# CartesianIndices((m, n)) iterates over all (i, j) where 1 <= i <= m, 1 <= j <= n
# in column-major order (i varies fastest)

struct CartesianIndices
    dims
end

# Constructor from array
CartesianIndices(A::Array) = CartesianIndices(size(A))

# Size and length
size(ci::CartesianIndices) = ci.dims
function length(ci::CartesianIndices)
    n = length(ci.dims)
    if n == 0
        return 1  # Scalar case
    elseif n == 1
        return Int64(ci.dims[1])
    elseif n == 2
        return Int64(ci.dims[1]) * Int64(ci.dims[2])
    elseif n == 3
        return Int64(ci.dims[1]) * Int64(ci.dims[2]) * Int64(ci.dims[3])
    else
        return Int64(prod(ci.dims))
    end
end

# First and last indices
function first(ci::CartesianIndices)
    return CartesianIndex(_ones_tuple(length(ci.dims)))
end

function last(ci::CartesianIndices)
    return CartesianIndex(ci.dims)
end

# Helper: create tuple of ones
function _ones_tuple(n::Int64)
    if n == 0
        return ()
    elseif n == 1
        return (1,)
    elseif n == 2
        return (1, 1)
    elseif n == 3
        return (1, 1, 1)
    elseif n == 4
        return (1, 1, 1, 1)
    elseif n == 5
        return (1, 1, 1, 1, 1)
    elseif n == 6
        return (1, 1, 1, 1, 1, 1)
    elseif n == 7
        return (1, 1, 1, 1, 1, 1, 1)
    elseif n == 8
        return (1, 1, 1, 1, 1, 1, 1, 1)
    else
        error("CartesianIndices supports up to 8 dimensions")
    end
end

# Linear -> Cartesian index conversion (Issue #5136).
# `CartesianIndices((m, n))[k]` returns the k-th CartesianIndex in column-major
# order. Mirrors upstream `getindex(::CartesianIndices, ::Int)`.
function getindex(ci::CartesianIndices, i::Int64)
    dims = ci.dims
    n = length(dims)
    if n == 0
        return CartesianIndex(())
    elseif n == 1
        return CartesianIndex((i,))
    elseif n == 2
        d1 = Int64(dims[1])
        i0 = i - 1
        c1 = (i0 % d1) + 1
        c2 = (i0 ÷ d1) + 1
        return CartesianIndex((c1, c2))
    elseif n == 3
        d1 = Int64(dims[1])
        d2 = Int64(dims[2])
        i0 = i - 1
        c1 = (i0 % d1) + 1
        rest = i0 ÷ d1
        c2 = (rest % d2) + 1
        c3 = (rest ÷ d2) + 1
        return CartesianIndex((c1, c2, c3))
    else
        error("CartesianIndices getindex supports up to 3 dimensions")
    end
end

# Iteration protocol for CartesianIndices
# NOTE: iterate(::CartesianIndices) is handled by VM builtins in type_ops.rs
# for better performance and to avoid method dispatch issues during base loading.
# The VM builtin returns (CartesianIndex(indices), state) tuples.

# Show
function show(io::IO, ci::CartesianIndices)
    print(io, "CartesianIndices(")
    show(io, ci.dims)
    print(io, ")")
end

# =============================================================================
# eachindex - iterate over array indices
# =============================================================================
# For multi-dimensional arrays, returns CartesianIndices
# Note: The basic eachindex(arr) = 1:length(arr) is defined in range.jl for linear indexing.
# This version returns CartesianIndices for multi-dimensional iteration.

# eachindex for CartesianIndices-style iteration over arrays
function eachindex(::IndexCartesian, A::Array)
    return CartesianIndices(size(A))
end

# =============================================================================
# IndexStyle - Array indexing trait types
# =============================================================================
# Based on Julia's base/indices.jl
#
# IndexStyle is an abstract type used to describe the optimal indexing style
# for arrays. IndexLinear and IndexCartesian are its two subtypes.

"""
    IndexStyle

Abstract type for describing the optimal indexing style for arrays.
Subtypes are `IndexLinear` and `IndexCartesian`.
"""
abstract type IndexStyle end

"""
    IndexLinear()

Subtype of `IndexStyle` used to describe arrays which are optimally
indexed by one linear index.

A linear indexing style uses one integer index to describe the position
in the array (even if it's a multidimensional array).
"""
struct IndexLinear <: IndexStyle end

"""
    IndexCartesian()

Subtype of `IndexStyle` used to describe arrays which are optimally
indexed by a Cartesian index. This is the default for new custom
`AbstractArray` subtypes.

A Cartesian indexing style uses multiple integer indices to describe
the position in a multidimensional array, with exactly one index per dimension.
"""
struct IndexCartesian <: IndexStyle end

# =============================================================================
# LinearIndices - linear index iterator
# =============================================================================
# Based on Julia's base/indices.jl
#
# LinearIndices(A) returns 1:length(A) for iteration over linear indices
# Simplified implementation that stores the total length directly

# Stores both the total length (`len`) and the source dimensions (`dims`).
# `dims` is left untyped so that the single-int convenience constructor and the
# auto-generated two-field constructor coexist cleanly under the VM's struct
# dispatch (an abstract `::Tuple` field annotation interferes with constructor
# resolution when outer constructors are present).
struct LinearIndices
    len::Int64
    dims
end

# Single-length convenience constructor (1-D). The 1-tuple `(len,)` is the
# canonical column-major shape, matching `LinearIndices((n,))`.
LinearIndices(len::Int64) = LinearIndices(len, (len,))

# Constructor from tuple (dims) - compute product of dimensions and retain the
# shape so that Cartesian->linear indexing (`li[i, j]`) is supported (Issue #5136).
# Use explicit handling to avoid compile issues with tuple iteration.
function LinearIndices(dims::Tuple)
    n = length(dims)
    if n == 0
        return LinearIndices(1, dims)
    elseif n == 1
        return LinearIndices(Int64(dims[1]), dims)
    elseif n == 2
        return LinearIndices(Int64(dims[1]) * Int64(dims[2]), dims)
    elseif n == 3
        return LinearIndices(Int64(dims[1]) * Int64(dims[2]) * Int64(dims[3]), dims)
    elseif n == 4
        return LinearIndices(Int64(dims[1]) * Int64(dims[2]) * Int64(dims[3]) * Int64(dims[4]), dims)
    else
        # Fallback to prod for higher dimensions
        return LinearIndices(Int64(prod(dims)), dims)
    end
end

# Length
length(li::LinearIndices) = li.len

function eltype(li::LinearIndices)
    return Int64
end

function eltype(::Type{LinearIndices})
    return Int64
end

# Iteration protocol - return linear indices 1:length
function iterate(li::LinearIndices)
    if li.len == 0
        return nothing
    end
    return (1, 2)
end

function iterate(li::LinearIndices, state::Int64)
    if state > li.len
        return nothing
    end
    return (state, state + 1)
end

# First and last indices
function first(li::LinearIndices)
    return 1
end

function last(li::LinearIndices)
    return li.len
end

# getindex for linear indices - just return the index
function getindex(li::LinearIndices, i::Int64)
    if i < 1 || i > li.len
        error("BoundsError")
    end
    return i
end

# Cartesian -> linear index conversion (Issue #5136).
# `LinearIndices((m, n))[i, j]` returns the column-major linear index, i.e.
# `(i - 1) + (j - 1) * m + 1`. Mirrors upstream `getindex(::LinearIndices, ...)`.
function getindex(li::LinearIndices, i::Int64, j::Int64)
    d1 = Int64(li.dims[1])
    return (i - 1) + (j - 1) * d1 + 1
end

function getindex(li::LinearIndices, i::Int64, j::Int64, k::Int64)
    d1 = Int64(li.dims[1])
    d2 = Int64(li.dims[2])
    return (i - 1) + (j - 1) * d1 + (k - 1) * d1 * d2 + 1
end

# Indexing a LinearIndices with a CartesianIndex (Issue #5136).
function getindex(li::LinearIndices, ci::CartesianIndex)
    idx = ci.I
    n = length(idx)
    if n == 1
        return Int64(idx[1])
    elseif n == 2
        return getindex(li, Int64(idx[1]), Int64(idx[2]))
    elseif n == 3
        return getindex(li, Int64(idx[1]), Int64(idx[2]), Int64(idx[3]))
    else
        # Column-major fold for higher dimensions.
        lin = Int64(idx[n]) - 1
        d = n - 1
        while d >= 1
            lin = lin * Int64(li.dims[d]) + (Int64(idx[d]) - 1)
            d = d - 1
        end
        return lin + 1
    end
end

# Show
function show(io::IO, li::LinearIndices)
    print(io, "LinearIndices((1:", li.len, ",))")
end

# axes for LinearIndices mirrors the one-dimensional key space used by
# upstream Base.Pairs over vectors and Memory.
function axes(li::LinearIndices)
    return (1:li.len,)
end

# =============================================================================
# Pairs - index/value dictionary view for indexable collections
# =============================================================================
# Based on Julia's base/essentials.jl and base/iterators.jl:
#   Pairs(data, itr)
#   pairs(::IndexLinear, A::AbstractArray) = Pairs(A, LinearIndices(A))
#   iterate(p::Pairs) yields Pair(index, data[index])

struct Pairs{K,V,I,A} <: AbstractDict{K,V}
    data::A
    itr::I
end

function Pairs(data::A, itr::I) where {A,I}
    return Pairs{eltype(itr), eltype(data), I, A}(data, itr)
end

function pairs(::IndexLinear, A::Array)
    return Pairs(A, keys(A))
end

function pairs(A::Array)
    return pairs(IndexLinear(), A)
end

function pairs(tuple::Tuple)
    return Pairs(tuple, keys(tuple))
end

function pairs(m::Memory)
    itr = keys(m)
    return Pairs{Int64,eltype(m),typeof(itr),typeof(m)}(m, itr)
end

function keys(p::Pairs)
    return p.itr
end

function values(p::Pairs)
    return p.data
end

function length(p::Pairs)
    return length(keys(p))
end

function axes(p::Pairs)
    return axes(keys(p))
end

function keytype(p::Pairs{K,V,I,A}) where {K,V,I,A}
    return K
end

function valtype(p::Pairs{K,V,I,A}) where {K,V,I,A}
    return V
end

function eltype(::Type{Pairs{K,V,I,A}}) where {K,V,I,A}
    return Pair{K,V}
end

function eltype(::Type{<:Pairs{K,V,I,A}}) where {K,V,I,A}
    return Pair{K,V}
end

function eltype(p::Pairs)
    return Pair{keytype(p), valtype(p)}
end

function eltype(p::Pairs{K,V,I,A}) where {K,V,I,A}
    return Pair{K,V}
end

function _pairs_collect_result(::Type{Int64}, ::Type{Int8}, n::Int64)
    mem = Memory{Pair{Int64,Int8}}(n)
    return wrap(Array, mem, (n,))
end

function _pairs_collect_result(::Type{K}, ::Type{V}, n::Int64) where {K,V}
    return _array_undef_from_dims(Pair{K,V}, (n,))
end

function _pairs_collect(::Type{Int64}, ::Type{Int8}, p)
    result = _pairs_collect_result(Int64, Int8, length(p))
    return _collect_to!(result, p)
end

function _pairs_collect(::Type{K}, ::Type{V}, p) where {K,V}
    result = _pairs_collect_result(K, V, length(p))
    return _collect_to!(result, p)
end

function _pairs_collect_dynamic(p)
    result = _array_undef_from_dims(eltype(p), (length(p),))
    return _collect_to!(result, p)
end

function collect(p::Pairs)
    return _pairs_collect_dynamic(p)
end

function IteratorEltype(::Type{<:Pairs})
    return HasEltype()
end

function IteratorSize(::Type{<:Pairs{K,V,I,A}}) where {K,V,I,A}
    return IteratorSize(I)
end

function _pairs_data_value(data, idx)
    if idx isa Symbol
        return getfield(data, idx)
    end
    return data[idx]
end

function getindex(p::Pairs, idx)
    return _pairs_data_value(p.data, idx)
end

function _pairs_elt(p::Pairs, idx)
    return Pair(idx, _pairs_data_value(p.data, idx))
end

function first(p::Pairs)
    x = iterate(p)
    if x === nothing
        throw(ArgumentError("collection must be non-empty"))
    end
    return x[1]
end

function iterate(p::Pairs)
    x = iterate(keys(p))
    if x === nothing
        return nothing
    end
    idx = x[1]
    next = x[2]
    return (_pairs_elt(p, idx), next)
end

function iterate(p::Pairs, state)
    x = iterate(keys(p), state)
    if x === nothing
        return nothing
    end
    idx = x[1]
    next = x[2]
    return (_pairs_elt(p, idx), next)
end

# =============================================================================
# only - return single element from collection
# =============================================================================
# Based on Julia's base/iterators.jl
#
# only(x) returns the one and only element of collection x, and throws
# an ArgumentError if the collection has zero or more than one element.

function only(x)
    n = length(x)
    if n == 0
        error("ArgumentError: Collection is empty, must contain exactly one element")
    elseif n > 1
        error("ArgumentError: Collection has multiple elements, must contain exactly one element")
    end
    return x[1]
end

# =============================================================================
# EachCol - iterate over columns of a matrix
# =============================================================================
# Based on Julia's base/iterators.jl
#
# eachcol(A) yields each column of matrix A as a 1D array

struct EachCol
    mat
end

eachcol(A::Array) = EachCol(A)

function iterate(ec::EachCol)
    s = size(ec.mat)
    if length(s) < 2
        # 1D array: treat as single column
        return (ec.mat, 2)
    end
    ncols = s[2]
    if ncols == 0
        return nothing
    end
    # Return first column
    col = ec.mat[:, 1]
    return (col, 2)
end

function iterate(ec::EachCol, state::Int64)
    s = size(ec.mat)
    if length(s) < 2
        # 1D array: only one "column"
        return nothing
    end
    ncols = s[2]
    if state > ncols
        return nothing
    end
    col = ec.mat[:, state]
    return (col, state + 1)
end

function length(ec::EachCol)
    s = size(ec.mat)
    if length(s) < 2
        return 1  # 1D array has 1 "column"
    end
    return s[2]
end

# =============================================================================
# EachRow - iterate over rows of a matrix
# =============================================================================
# Based on Julia's base/iterators.jl
#
# eachrow(A) yields each row of matrix A as a 1D array

struct EachRow
    mat
end

eachrow(A::Array) = EachRow(A)

function iterate(er::EachRow)
    s = size(er.mat)
    if length(s) < 2
        # 1D array: each element is a "row"
        return iterate(er.mat)
    end
    nrows = s[1]
    if nrows == 0
        return nothing
    end
    # Return first row
    row = er.mat[1, :]
    return (row, 2)
end

function iterate(er::EachRow, state::Int64)
    s = size(er.mat)
    if length(s) < 2
        # 1D array: delegate to array iteration
        return iterate(er.mat, state)
    end
    nrows = s[1]
    if state > nrows
        return nothing
    end
    row = er.mat[state, :]
    return (row, state + 1)
end

function length(er::EachRow)
    s = size(er.mat)
    if length(s) < 2
        return length(er.mat)  # 1D array: each element is a "row"
    end
    return s[1]
end

# =============================================================================
# EachSlice - iterate over slices of an array along a specified dimension
# =============================================================================
# Based on Julia's Base.eachslice
#
# eachslice(A; dims) generalizes eachrow (dims=1) and eachcol (dims=2)

struct EachSlice
    mat
    dim::Int64
end

eachslice(A; dims) = EachSlice(A, dims)

function iterate(es::EachSlice)
    s = size(es.mat)
    if length(s) < 2
        if es.dim == 1
            return iterate(es.mat)
        else
            return (es.mat, 2)
        end
    end
    n = s[es.dim]
    if n == 0
        return nothing
    end
    if es.dim == 1
        slice = es.mat[1, :]
    else
        slice = es.mat[:, 1]
    end
    return (slice, 2)
end

function iterate(es::EachSlice, state::Int64)
    s = size(es.mat)
    if length(s) < 2
        if es.dim == 1
            return iterate(es.mat, state)
        else
            return nothing
        end
    end
    n = s[es.dim]
    if state > n
        return nothing
    end
    if es.dim == 1
        slice = es.mat[state, :]
    else
        slice = es.mat[:, state]
    end
    return (slice, state + 1)
end

function length(es::EachSlice)
    s = size(es.mat)
    if length(s) < 2
        if es.dim == 1
            return length(es.mat)
        else
            return 1
        end
    end
    return s[es.dim]
end

# =============================================================================
# SkipMissing - skip missing values in iteration
# =============================================================================
# Based on Julia's base/missing.jl
#
# skipmissing(itr) wraps an iterator to skip all missing values

struct SkipMissing
    x
end

skipmissing(itr) = SkipMissing(itr)

function IteratorSize(::Type{SkipMissing})
    return SizeUnknown()
end

function IteratorSize(itr::SkipMissing)
    return SizeUnknown()
end

function IteratorEltype(::Type{SkipMissing})
    return EltypeUnknown()
end

function IteratorEltype(itr::SkipMissing)
    return IteratorEltype(itr.x)
end

function eltype(itr::SkipMissing)
    return eltype(itr.x)
end

function collect(itr::SkipMissing)
    return _collect_with_eltype(itr, eltype(itr))
end

function iterate(itr::SkipMissing)
    next = iterate(itr.x)
    if next === nothing
        return nothing
    end
    val = next[1]
    state = next[2]
    if ismissing(val)
        return iterate(itr, state)
    end
    return (val, state)
end

function iterate(itr::SkipMissing, state)
    next = iterate(itr.x, state)
    if next === nothing
        return nothing
    end
    val = next[1]
    newstate = next[2]
    if ismissing(val)
        return iterate(itr, newstate)
    end
    return (val, newstate)
end

# Length is unknown without iteration
# (would need to count non-missing elements)

# =============================================================================
# Flatten - flatten nested iterables
# =============================================================================
# Based on Julia's Iterators.flatten
#
# flatten(iter) iterates over all elements of each element in iter
# Example: flatten([[1,2], [3,4]]) yields 1, 2, 3, 4

struct Flatten
    it
end

flatten(itr) = Flatten(itr)

function IteratorSize(f::Flatten)
    return SizeUnknown()
end

function IteratorEltype(f::Flatten)
    return HasEltype()
end

function eltype(f::Flatten)
    return _flatten_runtime_eltype(f)
end

function _flatten_inner_eltype(inner)
    T = typeof(inner)
    if T <: Number
        return T
    end
    return eltype(inner)
end

function _flatten_runtime_eltype(f::Flatten)
    outer_next = iterate(f.it)
    if outer_next === nothing
        return Union{}
    end

    inner = outer_next[1]
    T = _flatten_inner_eltype(inner)
    outer_state = outer_next[2]
    outer_next = iterate(f.it, outer_state)
    while outer_next !== nothing
        inner = outer_next[1]
        T = typejoin(T, _flatten_inner_eltype(inner))
        outer_state = outer_next[2]
        outer_next = iterate(f.it, outer_state)
    end
    return T
end

function collect(f::Flatten)
    return _collect(1:1, f, IteratorEltype(f), IteratorSize(f))
end

function iterate(f::Flatten)
    outer_next = iterate(f.it)
    if outer_next === nothing
        return nothing
    end
    inner = outer_next[1]
    outer_state = outer_next[2]
    inner_next = iterate(inner)
    while inner_next === nothing
        outer_next = iterate(f.it, outer_state)
        if outer_next === nothing
            return nothing
        end
        inner = outer_next[1]
        outer_state = outer_next[2]
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state))
end

function iterate(f::Flatten, state)
    inner = state[1]
    inner_state = state[2]
    outer_state = state[3]
    inner_next = iterate(inner, inner_state)
    while inner_next === nothing
        outer_next = iterate(f.it, outer_state)
        if outer_next === nothing
            return nothing
        end
        inner = outer_next[1]
        outer_state = outer_next[2]
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state))
end

# =============================================================================
# flatmap - map then flatten
# =============================================================================
# Based on Julia's Iterators.flatmap (julia/base/iterators.jl:1371)
#
# flatmap(f, itr) applies f to each element then flattens the results
# In official Julia: flatmap(f, c...) = flatten(map(f, c...))
# Workaround: use FlatMap struct due to map transposition bug (Issue #2119)
# Issue #2115

struct FlatMap
    f
    itr
end

flatmap(f, itr) = FlatMap(f, itr)

function IteratorSize(fm::FlatMap)
    return SizeUnknown()
end

function IteratorEltype(fm::FlatMap)
    return EltypeUnknown()
end

function collect(fm::FlatMap)
    return _collect(1:1, fm, IteratorEltype(fm), IteratorSize(fm))
end

function iterate(fm::FlatMap)
    # Get first outer element
    outer_next = iterate(fm.itr)
    if outer_next === nothing
        return nothing
    end
    outer_val = outer_next[1]
    outer_state = outer_next[2]
    # Apply f to get inner iterable
    inner = fm.f(outer_val)
    inner_next = iterate(inner)
    # Skip empty inner iterables
    while inner_next === nothing
        outer_next = iterate(fm.itr, outer_state)
        if outer_next === nothing
            return nothing
        end
        outer_val = outer_next[1]
        outer_state = outer_next[2]
        inner = fm.f(outer_val)
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state, fm))
end

function iterate(fm::FlatMap, state)
    inner = state[1]
    inner_state = state[2]
    outer_state = state[3]
    inner_next = iterate(inner, inner_state)
    # If inner exhausted, advance outer
    while inner_next === nothing
        outer_next = iterate(fm.itr, outer_state)
        if outer_next === nothing
            return nothing
        end
        outer_val = outer_next[1]
        outer_state = outer_next[2]
        inner = fm.f(outer_val)
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state, fm))
end

# =============================================================================
# Rest - return iterator from an iteration state
# =============================================================================
# Based on Julia's Iterators.rest
#
# rest(iter) returns iter itself
# rest(iter, state) returns an iterator starting from the given state

struct Rest{I,S}
    itr::I
    st::S
end

# INTENTIONAL_NOOP (Issue #4703): upstream `rest(itr) = itr`
# (julia/base/iterators.jl:655) returns the iterator unchanged when no
# state is supplied; the typed `rest(itr, state)` below builds the real
# Rest iterator. A `return itr` body is correct, not a stub.
function rest(itr)
    return itr
end

function rest(itr, state)
    return Rest(itr, state)
end

function rest(itr::Rest, state)
    return Rest(itr.itr, state)
end

function iterate(r::Rest)
    return iterate(r.itr, r.st)
end

function iterate(r::Rest, state)
    return iterate(r.itr, state)
end

function isdone(r::Rest, state)
    return isdone(r.itr, state)
end

function eltype(::Type{Rest{I,S}}) where {I,S}
    return eltype(I)
end

function eltype(r::Rest)
    return eltype(r.itr)
end

function IteratorEltype(::Type{Rest{I,S}}) where {I,S}
    return IteratorEltype(I)
end

function IteratorEltype(r::Rest)
    return IteratorEltype(r.itr)
end

function _rest_iteratorsize(isz)
    return SizeUnknown()
end

function _rest_iteratorsize(isz::IsInfinite)
    return IsInfinite()
end

function IteratorSize(::Type{Rest{I,S}}) where {I,S}
    return _rest_iteratorsize(IteratorSize(I))
end

function IteratorSize(r::Rest)
    return _rest_iteratorsize(IteratorSize(r.itr))
end

function collect(r::Rest{I,S}) where {I,S}
    return _collect(1:1, r, IteratorEltype(r), IteratorSize(r))
end

# =============================================================================
# Cycle - infinite cyclic iterator
# =============================================================================
# Based on Julia's Iterators.cycle
#
# cycle(iter) repeats iter forever
# Warning: Creates infinite iterator! Use with take() or break

struct Cycle
    xs
end

cycle(itr) = Cycle(itr)

function eltype(c::Cycle)
    return eltype(c.xs)
end

function IteratorSize(c::Cycle)
    return IsInfinite()
end

function IteratorEltype(c::Cycle)
    return IteratorEltype(c.xs)
end

function iterate(c::Cycle)
    next = iterate(c.xs)
    if next === nothing
        return nothing  # Empty collection
    end
    return (next[1], next[2])
end

function iterate(c::Cycle, state)
    next = iterate(c.xs, state)
    if next === nothing
        # Restart from beginning
        next = iterate(c.xs)
        if next === nothing
            return nothing
        end
    end
    return (next[1], next[2])
end

# =============================================================================
# Repeated - repeat a value
# =============================================================================
# Based on Julia's Iterators.repeated
#
# repeated(x) repeats x forever
# repeated(x, n) repeats x exactly n times

struct Repeated
    x
    n::Int64  # -1 means infinite
end

repeated(x) = Repeated(x, -1)
repeated(x, n::Integer) = Repeated(x, Int64(n))

function eltype(r::Repeated)
    return typeof(r.x)
end

function IteratorSize(r::Repeated)
    if r.n < 0
        return IsInfinite()
    end
    return HasLength()
end

function IteratorEltype(r::Repeated)
    return HasEltype()
end

function iterate(r::Repeated)
    if r.n == 0
        return nothing
    end
    if r.n < 0
        return (r.x, -1)  # Infinite: state doesn't matter
    end
    return (r.x, r.n - 1)
end

function iterate(r::Repeated, remaining::Int64)
    if remaining == 0
        return nothing
    end
    if remaining < 0
        return (r.x, -1)  # Infinite
    end
    return (r.x, remaining - 1)
end

function length(r::Repeated)
    if r.n < 0
        error("Infinite iterator has no length")
    end
    return r.n
end

# =============================================================================
# Partition - group elements into chunks
# =============================================================================
# Based on Julia's Iterators.partition
#
# partition(iter, n) yields tuples/arrays of n consecutive elements
# Example: partition([1,2,3,4,5], 2) yields [1,2], [3,4], [5]

struct Partition
    xs
    n::Int64
end

function partition(itr, n::Integer)
    n < 1 && throw(ArgumentError(string("cannot create partitions of length ", n)))
    return Partition(itr, Int64(n))
end

function _partition_iteratorsize(isz::HasShape)
    return HasLength()
end

# INTENTIONAL_NOOP (Issue #4703): upstream
# `partition_iteratorsize(isz) = isz` (julia/base/iterators.jl:1410) is
# the generic fallback that passes the IteratorSize trait through
# unchanged; the typed `_partition_iteratorsize(::HasShape)` above
# handles the only special case. A `return isz` body is correct.
function _partition_iteratorsize(isz)
    return isz
end

function IteratorSize(p::Partition)
    return _partition_iteratorsize(IteratorSize(p.xs))
end

function IteratorEltype(p::Partition)
    return IteratorEltype(p.xs)
end

function eltype(p::Partition)
    if isa(p.xs, Vector)
        return SubArray{eltype(p.xs)}
    end
    return Vector{eltype(p.xs)}
end

function length(p::Partition)
    return cld(length(p.xs), p.n)
end

function iterate(p::Partition)
    if isa(p.xs, Vector)
        if length(p.xs) == 0
            return nothing
        end
        r = min(p.n, length(p.xs))
        return (view(p.xs, 1:r), r + 1)
    end

    chunk = []
    next = iterate(p.xs)
    if next === nothing
        return nothing
    end
    for i in 1:p.n
        if next === nothing
            break
        end
        push!(chunk, next[1])
        state = next[2]
        next = iterate(p.xs, state)
    end
    if length(chunk) == 0
        return nothing
    end
    if next === nothing
        return (chunk, nothing)
    end
    return (chunk, next)
end

function iterate(p::Partition, state)
    if isa(p.xs, Vector)
        if state > length(p.xs)
            return nothing
        end
        r = min(state + p.n - 1, length(p.xs))
        return (view(p.xs, state:r), r + 1)
    end

    if state === nothing
        return nothing
    end
    chunk = []
    next = state
    for i in 1:p.n
        if next === nothing
            break
        end
        push!(chunk, next[1])
        inner_state = next[2]
        next = iterate(p.xs, inner_state)
    end
    if length(chunk) == 0
        return nothing
    end
    if next === nothing
        return (chunk, nothing)
    end
    return (chunk, next)
end

# =============================================================================
# Product - Cartesian product of iterables
# =============================================================================
# Based on Julia's Iterators.product
#
# product(a, b) yields all (x, y) where x in a, y in b
# The first iterator changes fastest, matching upstream Julia.

struct Product
    a
    b
end

product(a, b) = Product(a, b)

struct ProductIterator
    iterators
end

product(iters...) = ProductIterator(iters)

function iterate(p::Product)
    a_next = iterate(p.a)
    if a_next === nothing
        return nothing
    end
    b_next = iterate(p.b)
    if b_next === nothing
        return nothing
    end
    return ((a_next[1], b_next[1]), (a_next[2], b_next[1], b_next[2]))
end

function iterate(p::Product, state)
    a_state = state[1]
    b_val = state[2]
    b_state = state[3]

    a_next = iterate(p.a, a_state)
    if a_next !== nothing
        return ((a_next[1], b_val), (a_next[2], b_val, b_state))
    end

    b_next = iterate(p.b, b_state)
    if b_next === nothing
        return nothing
    end

    a_restart = iterate(p.a)
    if a_restart === nothing
        return nothing
    end

    return ((a_restart[1], b_next[1]), (a_restart[2], b_next[1], b_next[2]))
end

function _prod_iteratorsize(a::IsInfinite, b)
    return IsInfinite()
end

function _prod_iteratorsize(a, b::IsInfinite)
    return IsInfinite()
end

function _prod_iteratorsize(a::SizeUnknown, b)
    return SizeUnknown()
end

function _prod_iteratorsize(a, b::SizeUnknown)
    return SizeUnknown()
end

function _prod_iteratorsize(a, b)
    return HasShape{2}()
end

function IteratorSize(p::Product)
    return _prod_iteratorsize(IteratorSize(p.a), IteratorSize(p.b))
end

function IteratorEltype(p::Product)
    if IteratorEltype(p.a) isa EltypeUnknown || IteratorEltype(p.b) isa EltypeUnknown
        return EltypeUnknown()
    end
    return HasEltype()
end

function eltype(p::Product)
    return Tuple{eltype(p.a), eltype(p.b)}
end

function size(p::Product)
    return (length(p.a), length(p.b))
end

function axes(p::Product)
    return (1:length(p.a), 1:length(p.b))
end

function ndims(p::Product)
    return length(axes(p))
end

function length(p::Product)
    return length(p.a) * length(p.b)
end

function _product_iterator_arity(p::ProductIterator)
    return length(p.iterators)
end

function _product_iterator_tuple(values)
    return tuple(values...)
end

function _product_iterator_tuple_setindex(t, index, value)
    result = ()
    i = 1
    while i <= length(t)
        if i == index
            result = tuple(result..., value)
        else
            result = tuple(result..., t[i])
        end
        i += 1
    end
    return result
end

function iterate(p::ProductIterator)
    n = _product_iterator_arity(p)
    if n == 0
        return ((), true)
    end

    values = ()
    states = ()
    i = 1
    while i <= n
        next = iterate(p.iterators[i])
        if next === nothing
            return nothing
        end
        values = tuple(values..., next[1])
        states = tuple(states..., next[2])
        i += 1
    end
    return (_product_iterator_tuple(values), (values, states))
end

function iterate(p::ProductIterator, state)
    n = _product_iterator_arity(p)
    if n == 0
        return nothing
    end

    values = state[1]
    states = state[2]
    i = 1
    while i <= n
        next = iterate(p.iterators[i], states[i])
        if next !== nothing
            values = _product_iterator_tuple_setindex(values, i, next[1])
            states = _product_iterator_tuple_setindex(states, i, next[2])
            return (_product_iterator_tuple(values), (values, states))
        end

        restart = iterate(p.iterators[i])
        if restart === nothing
            return nothing
        end
        values = _product_iterator_tuple_setindex(values, i, restart[1])
        states = _product_iterator_tuple_setindex(states, i, restart[2])
        i += 1
    end

    return nothing
end

function _product_iterator_hasshape(rank)
    if rank == 0
        return HasShape{0}()
    elseif rank == 1
        return HasShape{1}()
    elseif rank == 2
        return HasShape{2}()
    elseif rank == 3
        return HasShape{3}()
    elseif rank == 4
        return HasShape{4}()
    elseif rank == 5
        return HasShape{5}()
    elseif rank == 6
        return HasShape{6}()
    elseif rank == 7
        return HasShape{7}()
    elseif rank == 8
        return HasShape{8}()
    elseif rank == 9
        return HasShape{9}()
    elseif rank == 10
        return HasShape{10}()
    elseif rank == 11
        return HasShape{11}()
    elseif rank == 12
        return HasShape{12}()
    elseif rank == 13
        return HasShape{13}()
    elseif rank == 14
        return HasShape{14}()
    elseif rank == 15
        return HasShape{15}()
    elseif rank == 16
        return HasShape{16}()
    end
    return SizeUnknown()
end

function IteratorSize(p::ProductIterator)
    n = _product_iterator_arity(p)
    rank = 0
    i = 1
    while i <= n
        sz = IteratorSize(p.iterators[i])
        if sz isa IsInfinite
            return IsInfinite()
        elseif sz isa SizeUnknown
            return SizeUnknown()
        elseif sz isa HasShape
            rank += length(size(p.iterators[i]))
        elseif sz isa HasLength
            rank += 1
        else
            return SizeUnknown()
        end
        i += 1
    end
    return _product_iterator_hasshape(rank)
end

function IteratorEltype(p::ProductIterator)
    n = _product_iterator_arity(p)
    if n == 0
        return HasEltype()
    end
    result = HasEltype()
    i = 1
    while i <= n
        result = IteratorEltype(p.iterators[i])
        if result isa EltypeUnknown
            return EltypeUnknown()
        end
        i += 1
    end
    return result
end

function eltype(p::ProductIterator)
    n = _product_iterator_arity(p)
    if n == 0
        return Tuple{}
    elseif n == 1
        return Tuple{eltype(p.iterators[1])}
    elseif n == 2
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2])}
    elseif n == 3
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3])}
    elseif n == 4
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4])}
    elseif n == 5
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5])}
    elseif n == 6
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6])}
    elseif n == 7
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7])}
    elseif n == 8
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8])}
    elseif n == 9
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9])}
    elseif n == 10
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9]), eltype(p.iterators[10])}
    elseif n == 11
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9]), eltype(p.iterators[10]), eltype(p.iterators[11])}
    elseif n == 12
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9]), eltype(p.iterators[10]), eltype(p.iterators[11]), eltype(p.iterators[12])}
    elseif n == 13
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9]), eltype(p.iterators[10]), eltype(p.iterators[11]), eltype(p.iterators[12]), eltype(p.iterators[13])}
    elseif n == 14
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9]), eltype(p.iterators[10]), eltype(p.iterators[11]), eltype(p.iterators[12]), eltype(p.iterators[13]), eltype(p.iterators[14])}
    elseif n == 15
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9]), eltype(p.iterators[10]), eltype(p.iterators[11]), eltype(p.iterators[12]), eltype(p.iterators[13]), eltype(p.iterators[14]), eltype(p.iterators[15])}
    elseif n == 16
        return Tuple{eltype(p.iterators[1]), eltype(p.iterators[2]), eltype(p.iterators[3]), eltype(p.iterators[4]), eltype(p.iterators[5]), eltype(p.iterators[6]), eltype(p.iterators[7]), eltype(p.iterators[8]), eltype(p.iterators[9]), eltype(p.iterators[10]), eltype(p.iterators[11]), eltype(p.iterators[12]), eltype(p.iterators[13]), eltype(p.iterators[14]), eltype(p.iterators[15]), eltype(p.iterators[16])}
    end
    return Tuple
end

function size(p::ProductIterator)
    n = _product_iterator_arity(p)
    dims = ()
    i = 1
    while i <= n
        sz = IteratorSize(p.iterators[i])
        if sz isa HasShape
            inner = size(p.iterators[i])
            for d in inner
                dims = tuple(dims..., d)
            end
        elseif sz isa HasLength
            dims = tuple(dims..., length(p.iterators[i]))
        else
            throw(ArgumentError("iterator size is unknown"))
        end
        i += 1
    end
    return tuple(dims...)
end

function axes(p::ProductIterator)
    n = _product_iterator_arity(p)
    inds = ()
    i = 1
    while i <= n
        sz = IteratorSize(p.iterators[i])
        if sz isa HasShape
            inner = axes(p.iterators[i])
            for ax in inner
                inds = tuple(inds..., ax)
            end
        elseif sz isa HasLength
            inds = tuple(inds..., 1:length(p.iterators[i]))
        else
            throw(ArgumentError("iterator size is unknown"))
        end
        i += 1
    end
    return tuple(inds...)
end

function ndims(p::ProductIterator)
    return length(axes(p))
end

function length(p::ProductIterator)
    result = 1
    for dim in size(p)
        result = result * dim
    end
    return result
end

# =============================================================================
# EachSplit - string split iterator
# =============================================================================
# Based on Julia's base/strings/util.jl
#
# eachsplit(str, delim) yields substrings split by delimiter
# Simplified version without limit/keepempty options

struct EachSplit
    str::String
    delim::String
end

function IteratorSize(es::EachSplit)
    return SizeUnknown()
end

function IteratorEltype(es::EachSplit)
    return HasEltype()
end

function eltype(es::EachSplit)
    return String
end

function collect(es::EachSplit)
    return _collect_with_eltype(es, String)
end

eachsplit(str::String, delim::String) = EachSplit(str, delim)
eachsplit(str::String, delim::Char) = EachSplit(str, string(delim))

# Default: split on whitespace
eachsplit(str::String) = EachSplit(str, " ")

function iterate(es::EachSplit)
    n = length(es.str)
    if n == 0
        return nothing
    end
    # Find first delimiter
    idx = findfirst(es.delim, es.str)
    if idx === nothing
        # No delimiter, return whole string
        return (es.str, n + 1)
    end
    # Return substring before delimiter
    i = first(idx)
    if i == 1
        # Empty first part, skip to next
        start = length(es.delim) + 1
        if start > n
            return nothing
        end
        rest = es.str[start:n]
        return iterate(EachSplit(rest, es.delim))
    end
    substr = es.str[1:i-1]
    return (substr, i + length(es.delim))
end

function iterate(es::EachSplit, state::Int64)
    n = length(es.str)
    if state > n
        return nothing
    end
    rest = es.str[state:n]
    if length(rest) == 0
        return nothing
    end
    # Find next delimiter
    idx = findfirst(es.delim, rest)
    if idx === nothing
        # No more delimiters, return rest
        return (rest, n + 1)
    end
    i = first(idx)
    if i == 1
        # Empty part, skip
        new_start = state + length(es.delim)
        if new_start > n
            return nothing
        end
        return iterate(es, new_start)
    end
    substr = rest[1:i-1]
    return (substr, state + i - 1 + length(es.delim))
end

# =============================================================================
# EachRSplit - reverse string split iterator (Issue #1994)
# =============================================================================
# Based on Julia's base/strings/util.jl (lines 806-898)
#
# eachrsplit(str, delim) yields substrings split by delimiter,
# iterating from right to left.
# Unlike eachsplit which yields left-to-right, eachrsplit yields
# the rightmost substring first.

struct EachRSplit
    str::String
    delim::String
end

function IteratorSize(ers::EachRSplit)
    return SizeUnknown()
end

function IteratorEltype(ers::EachRSplit)
    return HasEltype()
end

function eltype(ers::EachRSplit)
    return String
end

function collect(ers::EachRSplit)
    return _collect_with_eltype(ers, String)
end

eachrsplit(str::String, delim::String) = EachRSplit(str, delim)
eachrsplit(str::String, delim::Char) = EachRSplit(str, string(delim))

# Default: split on whitespace
eachrsplit(str::String) = EachRSplit(str, " ")

function iterate(ers::EachRSplit)
    n = length(ers.str)
    if n == 0
        return nothing
    end
    # Find last delimiter
    idx = findlast(ers.delim, ers.str)
    if idx === nothing
        # No delimiter, return whole string and signal done
        return (ers.str, 0)
    end
    i = first(idx)
    dlen = length(ers.delim)
    # Return substring after the last delimiter
    start = i + dlen
    if start > n
        # Empty trailing part, skip to searching in remaining string
        return iterate(ers, i - 1)
    end
    substr = ers.str[start:n]
    return (substr, i - 1)
end

function iterate(ers::EachRSplit, state::Int64)
    if state <= 0
        if state == 0
            return nothing
        end
        return nothing
    end
    # Search within str[1:state]
    part = ers.str[1:state]
    idx = findlast(ers.delim, part)
    if idx === nothing
        # No more delimiters, return remaining string
        return (part, 0)
    end
    i = first(idx)
    dlen = length(ers.delim)
    start = i + dlen
    if start > state
        # Empty part between delimiters, skip
        return iterate(ers, i - 1)
    end
    substr = ers.str[start:state]
    return (substr, i - 1)
end

# =============================================================================
# Count - infinite counting iterator
# =============================================================================
# Based on Julia's base/iterators.jl
#
# countfrom(start, step) yields start, start+step, start+2*step, ...
# Warning: Creates infinite iterator! Use with take() or break

struct Count{T}
    start::T
    step::T
end

countfrom(start::Int64, step::Int64) = Count{Int64}(start, step)
countfrom(start::Float64, step::Float64) = Count{Float64}(start, step)
countfrom(start::Int64, step::Float64) = Count{Float64}(Float64(start), step)
countfrom(start::Float64, step::Int64) = Count{Float64}(start, Float64(step))
countfrom(start::Int64) = Count{Int64}(start, Int64(1))
countfrom(start::Float64) = Count{Float64}(start, 1.0)
countfrom() = Count{Int64}(Int64(1), Int64(1))

function eltype(::Type{Count{T}}) where {T}
    return T
end

function eltype(c::Count{T}) where {T}
    return T
end

function IteratorSize(::Type{Count{T}}) where {T}
    return IsInfinite()
end

function IteratorSize(c::Count)
    return IsInfinite()
end

function IteratorEltype(c::Count)
    return HasEltype()
end

function iterate(c::Count{Int64})
    return (c.start, c.start + c.step)
end

function iterate(c::Count{Int64}, state::Int64)
    return (state, state + c.step)
end

function iterate(c::Count{Float64})
    return (c.start, c.start + c.step)
end

function iterate(c::Count{Float64}, state::Float64)
    return (state, state + c.step)
end

# =============================================================================
# Peel - split iterator into first element and rest
# =============================================================================
# Based on Julia's base/iterators.jl
#
# peel(iter) returns (first_element, rest_iterator) or nothing if empty
# This is useful for extracting the first element while keeping the rest
# as an iterator.
#
# Examples:
#   peel([1, 2, 3]) => (1, Rest([1,2,3], state))
#   peel([]) => nothing

# NOTE: Due to Issue #777 (Union{Nothing, Tuple} return type bug), this function
# does NOT work correctly for empty iterators. When called with an empty iterator,
# the VM has type inference issues.
# Workaround: Check if the iterator is empty before calling peel.
# Example: if iterate(itr) !== nothing; result = peel(itr); end
function peel(itr)
    y = iterate(itr)
    result = nothing
    if y !== nothing
        val = y[1]
        s = y[2]
        result = (val, rest(itr, s))
    end
    return result
end

# =============================================================================
# Nth - get the nth element of an iterator
# =============================================================================
# Based on Julia's base/iterators.jl
#
# nth(itr, n) returns the nth element of the iterator, or throws BoundsError
# if the iterator has fewer than n elements.
#
# This is a simplified implementation that works with any iterable.
# Unlike Julia's full implementation, we don't use IteratorSize traits.
#
# Examples:
#   nth([1, 2, 3], 2) => 2
#   nth(1:10, 5) => 5
#   nth(enumerate([10, 20, 30]), 2) => (2, 20)

"""
    nth(itr, n::Integer)

Get the `n`th element of an iterable collection.
Throws a `BoundsError` if the iterator doesn't have `n` elements.

# Examples
```julia
julia> nth(2:2:10, 4)
8

julia> nth([10, 20, 30], 2)
20

julia> nth(enumerate([5, 6, 7]), 2)
(2, 6)
```

See also: [`first`](@ref), [`last`](@ref)
"""
function nth(itr, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    y = iterate(itr)
    i = 1
    while i < n
        if y === nothing
            error("BoundsError: iterator exhausted before reaching index $n")
        end
        y = iterate(itr, y[2])
        i = i + 1
    end
    if y === nothing
        error("BoundsError: iterator exhausted before reaching index $n")
    end
    return y[1]
end

# Optimized version for arrays using direct indexing
function nth(arr::Array, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    n <= length(arr) || error("BoundsError: index $n out of bounds for array of length $(length(arr))")
    return arr[n]
end

# Optimized version for ranges using direct indexing
function nth(r::UnitRange, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    n <= length(r) || error("BoundsError: index $n out of bounds for range of length $(length(r))")
    return first(r) + n - 1
end

function nth(r::StepRange, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    n <= length(r) || error("BoundsError: index $n out of bounds for range of length $(length(r))")
    return first(r) + (n - 1) * step(r)
end

# =============================================================================
# Higher-Order Functions - Pure Julia implementations
# =============================================================================
# Based on Julia's base/abstractarray.jl
#
# These implementations use Generator and collect to transform collections
# using user-defined functions.
#
# Note: This requires the field function call feature (Issue #1357) to work.

# =============================================================================
# map - apply function to each element
# =============================================================================
# Based on Julia's base/abstractarray.jl:3420
#
# map(f, A) returns a new collection with f applied to each element of A
#
# Examples:
#   map(x -> x * 2, [1, 2, 3]) => [2, 4, 6]
#   map(abs, [-1, 2, -3]) => [1, 2, 3]

"""
    map(f, A)

Apply function `f` to each element of collection `A`, returning a new collection
with the results.

# Examples
```julia
julia> map(x -> x * 2, [1, 2, 3])
[2, 4, 6]

julia> map(abs, [-1, 2, -3])
[1, 2, 3]
```
"""
map(f, A::Memory) = _collect_memory_generator_values(A, Generator(f, A))
map(f, A) = collect(Generator(f, A))

function _map_unary_into!(result, f, A)
    n = length(result)
    for i in 1:n
        result[i] = f(A[i])
    end
    return result
end

map(::typeof(identity), A::Vector{Int64}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{Int8}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{Int16}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{Int32}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{UInt8}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{UInt16}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{UInt32}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{UInt64}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(identity), A::Vector{Float64}) = _map_unary_into!(_array_undef_from_dims(Float64, (length(A),)), identity, A)
map(::typeof(identity), A::Vector{Float32}) = _map_unary_into!(_array_undef_from_dims(Float32, (length(A),)), identity, A)
map(::typeof(identity), A::Vector{Bool}) = _map_unary_into!(similar(A, length(A)), identity, A)
map(::typeof(iszero), A::Vector{Int64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{Int8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{Int16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{Int32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{UInt8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{UInt16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{UInt32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{UInt64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{Float64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{Float32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(iszero), A::Vector{Bool}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iszero, A)
map(::typeof(isone), A::Vector{Int64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{Int8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{Int16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{Int32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{UInt8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{UInt16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{UInt32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{UInt64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{Float64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{Float32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(isone), A::Vector{Bool}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isone, A)
map(::typeof(signbit), A::Vector{Int64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{Int8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{Int16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{Int32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{UInt8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{UInt16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{UInt32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{UInt64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{Float64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{Float32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(signbit), A::Vector{Bool}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), signbit, A)
map(::typeof(iseven), A::Vector{Int64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(iseven), A::Vector{Int8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(iseven), A::Vector{Int16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(iseven), A::Vector{Int32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(iseven), A::Vector{UInt8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(iseven), A::Vector{UInt16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(iseven), A::Vector{UInt32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(iseven), A::Vector{UInt64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), iseven, A)
map(::typeof(isodd), A::Vector{Int64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(isodd), A::Vector{Int8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(isodd), A::Vector{Int16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(isodd), A::Vector{Int32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(isodd), A::Vector{UInt8}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(isodd), A::Vector{UInt16}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(isodd), A::Vector{UInt32}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(isodd), A::Vector{UInt64}) = _map_unary_into!(_array_undef_from_dims(Bool, (length(A),)), isodd, A)
map(::typeof(abs), A::Vector{Int64}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{Int8}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{Int16}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{Int32}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{UInt8}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{UInt16}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{UInt32}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{UInt64}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs), A::Vector{Float64}) = _map_unary_into!(_array_undef_from_dims(Float64, (length(A),)), abs, A)
map(::typeof(abs), A::Vector{Float32}) = _map_unary_into!(_array_undef_from_dims(Float32, (length(A),)), abs, A)
map(::typeof(abs), A::Vector{Bool}) = _map_unary_into!(similar(A, length(A)), abs, A)
map(::typeof(abs2), A::Vector{Int64}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{Int8}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{Int16}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{Int32}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{UInt8}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{UInt16}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{UInt32}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{UInt64}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(abs2), A::Vector{Float64}) = _map_unary_into!(_array_undef_from_dims(Float64, (length(A),)), abs2, A)
map(::typeof(abs2), A::Vector{Float32}) = _map_unary_into!(_array_undef_from_dims(Float32, (length(A),)), abs2, A)
map(::typeof(abs2), A::Vector{Bool}) = _map_unary_into!(similar(A, length(A)), abs2, A)
map(::typeof(-), A::Vector{Int64}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{Int8}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{Int16}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{Int32}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{UInt8}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{UInt16}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{UInt32}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{UInt64}) = _map_unary_into!(similar(A, length(A)), -, A)
map(::typeof(-), A::Vector{Float64}) = _map_unary_into!(_array_undef_from_dims(Float64, (length(A),)), -, A)
map(::typeof(-), A::Vector{Float32}) = _map_unary_into!(_array_undef_from_dims(Float32, (length(A),)), -, A)

# map(f, A, B) - apply binary function to corresponding elements of two collections
# Based on Julia's base/abstractarray.jl
function _binary_map_length(A, B)
    nA = length(A)
    nB = length(B)
    if nA < nB
        return nA
    end
    return nB
end

function _map_binary_into!(result, f, A, B)
    n = length(result)
    for i in 1:n
        result[i] = f(A[i], B[i])
    end
    return result
end

function _map_array_vararg_length(A, B, C, As)
    n = length(A)
    n = min(n, length(B))
    n = min(n, length(C))
    for j in 1:length(As)
        n = min(n, length(As[j]))
    end
    return n
end

function _map_array_vararg_value(f, i, A, B, C, As)
    nargs = length(As)
    if nargs == 0
        return f(A[i], B[i], C[i])
    elseif nargs == 1
        return f(A[i], B[i], C[i], As[1][i])
    elseif nargs == 2
        return f(A[i], B[i], C[i], As[1][i], As[2][i])
    elseif nargs == 3
        return f(A[i], B[i], C[i], As[1][i], As[2][i], As[3][i])
    elseif nargs == 4
        return f(A[i], B[i], C[i], As[1][i], As[2][i], As[3][i], As[4][i])
    elseif nargs == 5
        return f(A[i], B[i], C[i], As[1][i], As[2][i], As[3][i], As[4][i], As[5][i])
    end

    values = Any[]
    push!(values, A[i])
    push!(values, B[i])
    push!(values, C[i])
    for j in 1:nargs
        push!(values, As[j][i])
    end
    return f(values...)
end

function _map_array_vararg_plus_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = A[i] + B[i]
        value = value + C[i]
        for j in 1:length(As)
            value = value + As[j][i]
        end
        result[i] = value
    end
    return result
end

function _map_array_vararg_plus_similar(A, B, C, As)
    n = _map_array_vararg_length(A, B, C, As)
    return _map_array_vararg_plus_into!(similar(A, n), A, B, C, As)
end

function _map_array_vararg_mul_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = A[i] * B[i]
        value = value * C[i]
        for j in 1:length(As)
            value = value * As[j][i]
        end
        result[i] = value
    end
    return result
end

function _map_array_vararg_mul_similar(A, B, C, As)
    n = _map_array_vararg_length(A, B, C, As)
    return _map_array_vararg_mul_into!(similar(A, n), A, B, C, As)
end

function _map_array_vararg_min_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = min(A[i], B[i])
        value = min(value, C[i])
        for j in 1:length(As)
            value = min(value, As[j][i])
        end
        result[i] = value
    end
    return result
end

function _map_array_vararg_min_similar(A, B, C, As)
    n = _map_array_vararg_length(A, B, C, As)
    return _map_array_vararg_min_into!(similar(A, n), A, B, C, As)
end

function _map_array_vararg_max_into!(result, A, B, C, As)
    n = length(result)
    for i in 1:n
        value = max(A[i], B[i])
        value = max(value, C[i])
        for j in 1:length(As)
            value = max(value, As[j][i])
        end
        result[i] = value
    end
    return result
end

function _map_array_vararg_max_similar(A, B, C, As)
    n = _map_array_vararg_length(A, B, C, As)
    return _map_array_vararg_max_into!(similar(A, n), A, B, C, As)
end

function map(f::Function, A, B)
    result = []
    iter = iterate(zip(A, B))
    while iter !== nothing
        (pair, state) = iter
        push!(result, f(pair[1], pair[2]))
        iter = iterate(zip(A, B), state)
    end
    return result
end

function map(f::Function, A::Array, B::Array)
    n = _binary_map_length(A, B)
    if n == 0
        return similar(A, 0)
    end
    first_value = f(A[1], B[1])
    result = _array_undef_from_dims(typeof(first_value), (n,))
    result[1] = first_value
    for i in 2:n
        result[i] = f(A[i], B[i])
    end
    return result
end

function map(f::Function, A::Array, B::Array, C::Array, As::Array...)
    n = _map_array_vararg_length(A, B, C, As)
    if n == 0
        return similar(A, 0)
    end
    first_value = _map_array_vararg_value(f, 1, A, B, C, As)
    result = _array_undef_from_dims(typeof(first_value), (n,))
    result[1] = first_value
    for i in 2:n
        result[i] = _map_array_vararg_value(f, i, A, B, C, As)
    end
    return result
end

map(::typeof(+), A::Vector{Int64}, B::Vector{Int64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{Int8}, B::Vector{Int8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{Int16}, B::Vector{Int16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{Int32}, B::Vector{Int32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{UInt8}, B::Vector{UInt8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{UInt16}, B::Vector{UInt16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{UInt32}, B::Vector{UInt32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{UInt64}, B::Vector{UInt64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{Float32}, B::Vector{Float32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{Float64}, B::Vector{Float64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), +, A, B)
map(::typeof(+), A::Vector{Bool}, B::Vector{Bool}) = _map_binary_into!(_array_undef_from_dims(Int64, (_binary_map_length(A, B),)), +, A, B)

map(::typeof(+), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _map_array_vararg_plus_similar(A, B, C, As)
map(::typeof(+), A::Vector{Bool}, B::Vector{Bool}, C::Vector{Bool}, As::Vector{Bool}...) = _map_array_vararg_plus_into!(_array_undef_from_dims(Int64, (_map_array_vararg_length(A, B, C, As),)), A, B, C, As)

map(::typeof(*), A::Vector{Int64}, B::Vector{Int64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{Int8}, B::Vector{Int8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{Int16}, B::Vector{Int16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{Int32}, B::Vector{Int32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{UInt8}, B::Vector{UInt8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{UInt16}, B::Vector{UInt16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{UInt32}, B::Vector{UInt32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{UInt64}, B::Vector{UInt64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{Float32}, B::Vector{Float32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{Float64}, B::Vector{Float64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)
map(::typeof(*), A::Vector{Bool}, B::Vector{Bool}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), *, A, B)

map(::typeof(min), A::Vector{Int64}, B::Vector{Int64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{Int8}, B::Vector{Int8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{Int16}, B::Vector{Int16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{Int32}, B::Vector{Int32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{UInt8}, B::Vector{UInt8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{UInt16}, B::Vector{UInt16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{UInt32}, B::Vector{UInt32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{UInt64}, B::Vector{UInt64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{Float32}, B::Vector{Float32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{Float64}, B::Vector{Float64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)
map(::typeof(min), A::Vector{Bool}, B::Vector{Bool}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), min, A, B)

map(::typeof(max), A::Vector{Int64}, B::Vector{Int64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{Int8}, B::Vector{Int8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{Int16}, B::Vector{Int16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{Int32}, B::Vector{Int32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{UInt8}, B::Vector{UInt8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{UInt16}, B::Vector{UInt16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{UInt32}, B::Vector{UInt32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{UInt64}, B::Vector{UInt64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{Float32}, B::Vector{Float32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{Float64}, B::Vector{Float64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)
map(::typeof(max), A::Vector{Bool}, B::Vector{Bool}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), max, A, B)

map(::typeof(*), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _map_array_vararg_mul_similar(A, B, C, As)
map(::typeof(*), A::Vector{Bool}, B::Vector{Bool}, C::Vector{Bool}, As::Vector{Bool}...) = _map_array_vararg_mul_similar(A, B, C, As)

map(::typeof(min), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _map_array_vararg_min_similar(A, B, C, As)
map(::typeof(min), A::Vector{Bool}, B::Vector{Bool}, C::Vector{Bool}, As::Vector{Bool}...) = _map_array_vararg_min_similar(A, B, C, As)

map(::typeof(max), A::Vector{Int64}, B::Vector{Int64}, C::Vector{Int64}, As::Vector{Int64}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{Int8}, B::Vector{Int8}, C::Vector{Int8}, As::Vector{Int8}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{Int16}, B::Vector{Int16}, C::Vector{Int16}, As::Vector{Int16}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{Int32}, B::Vector{Int32}, C::Vector{Int32}, As::Vector{Int32}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{UInt8}, B::Vector{UInt8}, C::Vector{UInt8}, As::Vector{UInt8}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{UInt16}, B::Vector{UInt16}, C::Vector{UInt16}, As::Vector{UInt16}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{UInt32}, B::Vector{UInt32}, C::Vector{UInt32}, As::Vector{UInt32}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{UInt64}, B::Vector{UInt64}, C::Vector{UInt64}, As::Vector{UInt64}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{Float32}, B::Vector{Float32}, C::Vector{Float32}, As::Vector{Float32}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{Float64}, B::Vector{Float64}, C::Vector{Float64}, As::Vector{Float64}...) = _map_array_vararg_max_similar(A, B, C, As)
map(::typeof(max), A::Vector{Bool}, B::Vector{Bool}, C::Vector{Bool}, As::Vector{Bool}...) = _map_array_vararg_max_similar(A, B, C, As)

map(::typeof(-), A::Vector{Int64}, B::Vector{Int64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{Int8}, B::Vector{Int8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{Int16}, B::Vector{Int16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{Int32}, B::Vector{Int32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{UInt8}, B::Vector{UInt8}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{UInt16}, B::Vector{UInt16}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{UInt32}, B::Vector{UInt32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{UInt64}, B::Vector{UInt64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{Float32}, B::Vector{Float32}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)
map(::typeof(-), A::Vector{Float64}, B::Vector{Float64}) = _map_binary_into!(similar(A, _binary_map_length(A, B)), -, A, B)

map(::typeof(/), A::Vector{Int64}, B::Vector{Int64}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{Int8}, B::Vector{Int8}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{Int16}, B::Vector{Int16}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{Int32}, B::Vector{Int32}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{UInt8}, B::Vector{UInt8}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{UInt16}, B::Vector{UInt16}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{UInt32}, B::Vector{UInt32}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{UInt64}, B::Vector{UInt64}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{Float32}, B::Vector{Float32}) = _map_binary_into!(_array_undef_from_dims(Float32, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{Float64}, B::Vector{Float64}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)
map(::typeof(/), A::Vector{Bool}, B::Vector{Bool}) = _map_binary_into!(_array_undef_from_dims(Float64, (_binary_map_length(A, B),)), /, A, B)

# =============================================================================
# Filter - iterator wrapper for filtering
# =============================================================================
# Based on Julia's base/iterators.jl
#
# Filter wraps an iterator and yields only elements for which the predicate
# function returns true.

struct Filter
    flt::Function
    itr
end

function IteratorSize(f::Filter)
    return SizeUnknown()
end

function IteratorEltype(f::Filter)
    return IteratorEltype(f.itr)
end

function eltype(f::Filter)
    return eltype(f.itr)
end

function collect(f::Filter)
    return _collect(1:1, f, IteratorEltype(f), IteratorSize(f))
end

function iterate(f::Filter)
    y = iterate(f.itr)
    while y !== nothing
        if f.flt(y[1])
            return (y[1], y[2])
        end
        y = iterate(f.itr, y[2])
    end
    return nothing
end

function iterate(f::Filter, state)
    y = iterate(f.itr, state)
    while y !== nothing
        if f.flt(y[1])
            return (y[1], y[2])
        end
        y = iterate(f.itr, y[2])
    end
    return nothing
end

# =============================================================================
# filter - select elements satisfying predicate
# =============================================================================
# Based on Julia's base/array.jl
#
# filter(f, A) returns a new collection containing only elements x for which f(x) is true
#
# Examples:
#   filter(iseven, [1, 2, 3, 4, 5]) => [2, 4]
#   filter(x -> x > 0, [-1, 2, -3, 4]) => [2, 4]

"""
    filter(f, A)

Return a new collection containing only elements of `A` for which `f` returns `true`.

# Examples
```julia
julia> filter(iseven, [1, 2, 3, 4, 5])
[2, 4]

julia> filter(x -> x > 0, [-1, 2, -3, 4])
[2, 4]
```
"""
filter(f::Function, A) = collect(Filter(f, A))

function _filter_unary_into!(result, f, A)
    n = length(A)
    for i in 1:n
        value = A[i]
        if f(value)
            push!(result, value)
        end
    end
    return result
end

filter(::typeof(iseven), A::Vector{Int64}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(iseven), A::Vector{Int8}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(iseven), A::Vector{Int16}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(iseven), A::Vector{Int32}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(iseven), A::Vector{UInt8}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(iseven), A::Vector{UInt16}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(iseven), A::Vector{UInt32}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(iseven), A::Vector{UInt64}) = _filter_unary_into!(similar(A, 0), iseven, A)
filter(::typeof(isodd), A::Vector{Int64}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(isodd), A::Vector{Int8}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(isodd), A::Vector{Int16}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(isodd), A::Vector{Int32}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(isodd), A::Vector{UInt8}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(isodd), A::Vector{UInt16}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(isodd), A::Vector{UInt32}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(isodd), A::Vector{UInt64}) = _filter_unary_into!(similar(A, 0), isodd, A)
filter(::typeof(iszero), A::Vector{Int64}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{Int8}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{Int16}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{Int32}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{UInt8}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{UInt16}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{UInt32}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{UInt64}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{Float64}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{Float32}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(iszero), A::Vector{Bool}) = _filter_unary_into!(similar(A, 0), iszero, A)
filter(::typeof(isone), A::Vector{Int64}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{Int8}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{Int16}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{Int32}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{UInt8}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{UInt16}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{UInt32}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{UInt64}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{Float64}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{Float32}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(isone), A::Vector{Bool}) = _filter_unary_into!(similar(A, 0), isone, A)
filter(::typeof(signbit), A::Vector{Int64}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{Int8}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{Int16}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{Int32}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{UInt8}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{UInt16}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{UInt32}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{UInt64}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{Float64}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{Float32}) = _filter_unary_into!(similar(A, 0), signbit, A)
filter(::typeof(signbit), A::Vector{Bool}) = _filter_unary_into!(similar(A, 0), signbit, A)

# =============================================================================
# reduce/foldl - reduce collection to single value
# =============================================================================
# Based on Julia's base/reduce.jl
#
# reduce(op, itr) combines elements using the binary operator op
# foldl(op, itr) is left-fold (same as reduce)
#
# Examples:
#   reduce(+, [1, 2, 3, 4]) => 10
#   reduce(*, [1, 2, 3, 4]) => 24

"""
    reduce(op, itr)
    reduce(op, itr, init)

Reduce `itr` using the binary operator `op`. The optional `init` argument
provides the initial value.

# Examples
```julia
julia> reduce(+, [1, 2, 3, 4])
10

julia> reduce(*, [1, 2, 3, 4])
24
```
"""
function reduce(op::Function, itr)
    y = iterate(itr)
    if y === nothing
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = y[1]
    y = iterate(itr, y[2])
    while y !== nothing
        acc = op(acc, y[1])
        y = iterate(itr, y[2])
    end
    return acc
end

function reduce(op::Function, itr, init)
    acc = init
    y = iterate(itr)
    while y !== nothing
        acc = op(acc, y[1])
        y = iterate(itr, y[2])
    end
    return acc
end

reduce(::typeof(+), A::Vector{Bool}) = _mapfoldl_identity_plus_bool(A)
reduce(::typeof(+), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_plus_bool(A, init)
reduce(::typeof(+), A::Vector{Int8}) = _mapfoldl_identity_plus_int8(A)
reduce(::typeof(+), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_plus_int8(A, init)
reduce(::typeof(+), A::Vector{Int16}) = _mapfoldl_identity_plus_int16(A)
reduce(::typeof(+), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_plus_int16(A, init)
reduce(::typeof(+), A::Vector{Int32}) = _mapfoldl_identity_plus_int32(A)
reduce(::typeof(+), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_plus_int32(A, init)
reduce(::typeof(+), A::Vector{Int64}) = _mapfoldl_identity_plus_int64(A)
reduce(::typeof(+), A::Vector{UInt8}) = _mapfoldl_identity_plus_uint8(A)
reduce(::typeof(+), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_plus_uint8(A, init)
reduce(::typeof(+), A::Vector{UInt16}) = _mapfoldl_identity_plus_uint16(A)
reduce(::typeof(+), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_plus_uint16(A, init)
reduce(::typeof(+), A::Vector{UInt32}) = _mapfoldl_identity_plus_uint32(A)
reduce(::typeof(+), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_plus_uint32(A, init)
reduce(::typeof(+), A::Vector{UInt64}) = _mapfoldl_identity_plus_uint64(A)
reduce(::typeof(+), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_plus_uint64(A, init)
reduce(::typeof(+), A::Vector{Float32}) = _mapfoldl_identity_plus_float32(A)
reduce(::typeof(+), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_plus_float32(A, init)
reduce(::typeof(+), A::Vector{Float64}) = _mapfoldl_identity_plus_float64(A)
reduce(::typeof(*), A::Vector{Bool}) = _mapfoldl_identity_mul_bool(A)
reduce(::typeof(*), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_mul_bool(A, init)
reduce(::typeof(*), A::Vector{Int8}) = _mapfoldl_identity_mul_int8(A)
reduce(::typeof(*), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_mul_int8(A, init)
reduce(::typeof(*), A::Vector{Int16}) = _mapfoldl_identity_mul_int16(A)
reduce(::typeof(*), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_mul_int16(A, init)
reduce(::typeof(*), A::Vector{Int32}) = _mapfoldl_identity_mul_int32(A)
reduce(::typeof(*), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_mul_int32(A, init)
reduce(::typeof(*), A::Vector{Int64}) = _mapfoldl_identity_mul_int64(A)
reduce(::typeof(*), A::Vector{UInt8}) = _mapfoldl_identity_mul_uint8(A)
reduce(::typeof(*), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_mul_uint8(A, init)
reduce(::typeof(*), A::Vector{UInt16}) = _mapfoldl_identity_mul_uint16(A)
reduce(::typeof(*), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_mul_uint16(A, init)
reduce(::typeof(*), A::Vector{UInt32}) = _mapfoldl_identity_mul_uint32(A)
reduce(::typeof(*), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_mul_uint32(A, init)
reduce(::typeof(*), A::Vector{UInt64}) = _mapfoldl_identity_mul_uint64(A)
reduce(::typeof(*), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_mul_uint64(A, init)
reduce(::typeof(*), A::Vector{Float32}) = _mapfoldl_identity_mul_float32(A)
reduce(::typeof(*), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_mul_float32(A, init)
reduce(::typeof(*), A::Vector{Float64}) = _mapfoldl_identity_mul_float64(A)
reduce(::typeof(-), A::Vector{Int8}) = _foldl_identity_minus_int8(A)
reduce(::typeof(-), A::Vector{Int8}, init::Int8) = _foldl_identity_minus_int8(A, init)
reduce(::typeof(-), A::Vector{Int16}) = _foldl_identity_minus_int16(A)
reduce(::typeof(-), A::Vector{Int16}, init::Int16) = _foldl_identity_minus_int16(A, init)
reduce(::typeof(-), A::Vector{Int32}) = _foldl_identity_minus_int32(A)
reduce(::typeof(-), A::Vector{Int32}, init::Int32) = _foldl_identity_minus_int32(A, init)
reduce(::typeof(-), A::Vector{UInt8}) = _foldl_identity_minus_uint8(A)
reduce(::typeof(-), A::Vector{UInt8}, init::UInt8) = _foldl_identity_minus_uint8(A, init)
reduce(::typeof(-), A::Vector{UInt16}) = _foldl_identity_minus_uint16(A)
reduce(::typeof(-), A::Vector{UInt16}, init::UInt16) = _foldl_identity_minus_uint16(A, init)
reduce(::typeof(-), A::Vector{UInt32}) = _foldl_identity_minus_uint32(A)
reduce(::typeof(-), A::Vector{UInt32}, init::UInt32) = _foldl_identity_minus_uint32(A, init)
reduce(::typeof(-), A::Vector{UInt64}) = _foldl_identity_minus_uint64(A)
reduce(::typeof(-), A::Vector{UInt64}, init::UInt64) = _foldl_identity_minus_uint64(A, init)
reduce(::typeof(-), A::Vector{Float32}) = _foldl_identity_minus_float32(A)
reduce(::typeof(-), A::Vector{Float32}, init::Float32) = _foldl_identity_minus_float32(A, init)
reduce(::typeof(min), A::Vector{Int8}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{Int16}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{Int32}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{Int64}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{UInt8}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{UInt16}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{UInt32}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{UInt64}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{Float32}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(min), A::Vector{Float64}) = _reduce_binary_nonempty(min, A)
reduce(::typeof(min), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(min, A, init)
reduce(::typeof(max), A::Vector{Int8}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{Int16}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{Int32}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{Int64}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{UInt8}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{UInt16}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{UInt32}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{UInt64}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{Float32}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(max, A, init)
reduce(::typeof(max), A::Vector{Float64}) = _reduce_binary_nonempty(max, A)
reduce(::typeof(max), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(max, A, init)

# Keyword argument form: reduce(op, itr; init=val) is handled at compiler level
# in call.rs by converting to reduce(op, itr, val) (Issue #2077, #2084)

# foldl is an alias for reduce (left-fold)
foldl(op::Function, itr) = reduce(op, itr)
foldl(op::Function, itr, init) = reduce(op, itr, init)
foldl(::typeof(min), A::Vector{Int8}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{Int16}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{Int32}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{Int64}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{UInt8}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{UInt16}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{UInt32}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{UInt64}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{Float32}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(min), A::Vector{Float64}) = _reduce_binary_nonempty(min, A)
foldl(::typeof(min), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(min, A, init)
foldl(::typeof(max), A::Vector{Int8}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{Int16}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{Int32}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{Int64}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{UInt8}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{UInt16}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{UInt32}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{UInt64}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{Float32}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(max, A, init)
foldl(::typeof(max), A::Vector{Float64}) = _reduce_binary_nonempty(max, A)
foldl(::typeof(max), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(max, A, init)
# Keyword argument form: foldl(op, itr; init=val) is handled at compiler level
# in call.rs by converting to foldl(op, itr, val) (Issue #2077, #2084)

function _reduce_binary_nonempty(op, A)
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = A[1]
    for i in 2:n
        acc = op(acc, A[i])
    end
    return acc
end

function _reduce_binary_with_init(op, A, init)
    acc = init
    for i in 1:length(A)
        acc = op(acc, A[i])
    end
    return acc
end

function _mapfoldl_identity_plus_bool(A::Vector{Bool})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return acc
end

function _mapfoldl_identity_plus_bool(A::Vector{Bool}, init::Bool)
    n = length(A)
    if n == 0
        return init
    end
    acc = init + A[1]
    for i in 2:n
        acc = acc + A[i]
    end
    return acc
end

function _mapfoldl_identity_plus_int8(A::Vector{Int8})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return Int8(acc)
end

function _mapfoldl_identity_plus_int8(A::Vector{Int8}, init::Int8)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return Int8(acc)
end

function _mapfoldl_identity_plus_int16(A::Vector{Int16})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return Int16(acc)
end

function _mapfoldl_identity_plus_int16(A::Vector{Int16}, init::Int16)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return Int16(acc)
end

function _mapfoldl_identity_plus_int32(A::Vector{Int32})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return Int32(acc)
end

function _mapfoldl_identity_plus_int32(A::Vector{Int32}, init::Int32)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return Int32(acc)
end

function _mapfoldl_identity_plus_int64(A::Vector{Int64})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return acc
end

function _mapfoldl_identity_plus_uint8(A::Vector{UInt8})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt8(acc)
end

function _mapfoldl_identity_plus_uint8(A::Vector{UInt8}, init::UInt8)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt8(acc)
end

function _mapfoldl_identity_plus_uint16(A::Vector{UInt16})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt16(acc)
end

function _mapfoldl_identity_plus_uint16(A::Vector{UInt16}, init::UInt16)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt16(acc)
end

function _mapfoldl_identity_plus_uint32(A::Vector{UInt32})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt32(acc)
end

function _mapfoldl_identity_plus_uint32(A::Vector{UInt32}, init::UInt32)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt32(acc)
end

function _mapfoldl_identity_plus_uint64(A::Vector{UInt64})
    acc = 0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt64(acc)
end

function _mapfoldl_identity_plus_uint64(A::Vector{UInt64}, init::UInt64)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return UInt64(acc)
end

function _mapfoldl_identity_plus_float32(A::Vector{Float32})
    acc = Float32(0)
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return acc
end

function _mapfoldl_identity_plus_float32(A::Vector{Float32}, init::Float32)
    acc = init
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return acc
end

function _mapfoldl_identity_plus_float64(A::Vector{Float64})
    acc = 0.0
    for i in 1:length(A)
        acc = acc + A[i]
    end
    return acc
end

function _mapfoldl_identity_mul_bool(A::Vector{Bool})
    for i in 1:length(A)
        if !A[i]
            return false
        end
    end
    return true
end

function _mapfoldl_identity_mul_bool(A::Vector{Bool}, init::Bool)
    if !init
        return false
    end
    for i in 1:length(A)
        if !A[i]
            return false
        end
    end
    return true
end

function _mapfoldl_identity_mul_int8(A::Vector{Int8})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return Int8(acc)
end

function _mapfoldl_identity_mul_int8(A::Vector{Int8}, init::Int8)
    acc = 1 * init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return Int8(acc)
end

function _mapfoldl_identity_mul_int16(A::Vector{Int16})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return Int16(acc)
end

function _mapfoldl_identity_mul_int16(A::Vector{Int16}, init::Int16)
    acc = 1 * init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return Int16(acc)
end

function _mapfoldl_identity_mul_int32(A::Vector{Int32})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return Int32(acc)
end

function _mapfoldl_identity_mul_int32(A::Vector{Int32}, init::Int32)
    acc = 1 * init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return Int32(acc)
end

function _mapfoldl_identity_mul_int64(A::Vector{Int64})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return acc
end

function _mapfoldl_identity_mul_uint8(A::Vector{UInt8})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt8(acc)
end

function _mapfoldl_identity_mul_uint8(A::Vector{UInt8}, init::UInt8)
    acc = 1 * init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt8(acc)
end

function _mapfoldl_identity_mul_uint16(A::Vector{UInt16})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt16(acc)
end

function _mapfoldl_identity_mul_uint16(A::Vector{UInt16}, init::UInt16)
    acc = 1 * init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt16(acc)
end

function _mapfoldl_identity_mul_uint32(A::Vector{UInt32})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt32(acc)
end

function _mapfoldl_identity_mul_uint32(A::Vector{UInt32}, init::UInt32)
    acc = 1 * init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt32(acc)
end

function _mapfoldl_identity_mul_uint64(A::Vector{UInt64})
    acc = 1
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt64(acc)
end

function _mapfoldl_identity_mul_uint64(A::Vector{UInt64}, init::UInt64)
    acc = 1 * init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return UInt64(acc)
end

function _mapfoldl_identity_mul_float32(A::Vector{Float32})
    acc = Float32(1)
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return acc
end

function _mapfoldl_identity_mul_float32(A::Vector{Float32}, init::Float32)
    acc = init
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return acc
end

function _mapfoldl_identity_mul_float64(A::Vector{Float64})
    acc = 1.0
    for i in 1:length(A)
        acc = acc * A[i]
    end
    return acc
end

_wrap_uint8_minus_result(acc) = UInt8(mod(acc, 256))
_wrap_uint16_minus_result(acc) = UInt16(mod(acc, 65536))
_wrap_uint32_minus_result(acc) = UInt32(mod(acc, 4294967296))
_wrap_uint64_minus_result(acc) = unsigned(Int64(acc))

function _foldl_identity_minus_int8(A::Vector{Int8})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return Int8(acc)
end

function _foldl_identity_minus_int8(A::Vector{Int8}, init::Int8)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return Int8(acc)
end

function _foldl_identity_minus_int16(A::Vector{Int16})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return Int16(acc)
end

function _foldl_identity_minus_int16(A::Vector{Int16}, init::Int16)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return Int16(acc)
end

function _foldl_identity_minus_int32(A::Vector{Int32})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return Int32(acc)
end

function _foldl_identity_minus_int32(A::Vector{Int32}, init::Int32)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return Int32(acc)
end

function _foldl_identity_minus_uint8(A::Vector{UInt8})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint8_minus_result(acc)
end

function _foldl_identity_minus_uint8(A::Vector{UInt8}, init::UInt8)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint8_minus_result(acc)
end

function _foldl_identity_minus_uint16(A::Vector{UInt16})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint16_minus_result(acc)
end

function _foldl_identity_minus_uint16(A::Vector{UInt16}, init::UInt16)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint16_minus_result(acc)
end

function _foldl_identity_minus_uint32(A::Vector{UInt32})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint32_minus_result(acc)
end

function _foldl_identity_minus_uint32(A::Vector{UInt32}, init::UInt32)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint32_minus_result(acc)
end

function _foldl_identity_minus_uint64(A::Vector{UInt64})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint64_minus_result(acc)
end

function _foldl_identity_minus_uint64(A::Vector{UInt64}, init::UInt64)
    acc = 0 + init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return _wrap_uint64_minus_result(acc)
end

function _foldl_identity_minus_float32(A::Vector{Float32})
    if length(A) == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = A[1]
    for i in 2:length(A)
        acc = acc - A[i]
    end
    return acc
end

function _foldl_identity_minus_float32(A::Vector{Float32}, init::Float32)
    acc = init
    for i in 1:length(A)
        acc = acc - A[i]
    end
    return acc
end

function _foldr_identity_minus_int8(A::Vector{Int8})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return Int8(acc)
end

function _foldr_identity_minus_int8(A::Vector{Int8}, init::Int8)
    acc = 0 + init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return Int8(acc)
end

function _foldr_identity_minus_int16(A::Vector{Int16})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return Int16(acc)
end

function _foldr_identity_minus_int16(A::Vector{Int16}, init::Int16)
    acc = 0 + init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return Int16(acc)
end

function _foldr_identity_minus_int32(A::Vector{Int32})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return Int32(acc)
end

function _foldr_identity_minus_int32(A::Vector{Int32}, init::Int32)
    acc = 0 + init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return Int32(acc)
end

function _foldr_identity_minus_uint8(A::Vector{UInt8})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint8_minus_result(acc)
end

function _foldr_identity_minus_uint8(A::Vector{UInt8}, init::UInt8)
    acc = 0 + init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint8_minus_result(acc)
end

function _foldr_identity_minus_uint16(A::Vector{UInt16})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint16_minus_result(acc)
end

function _foldr_identity_minus_uint16(A::Vector{UInt16}, init::UInt16)
    acc = 0 + init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint16_minus_result(acc)
end

function _foldr_identity_minus_uint32(A::Vector{UInt32})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint32_minus_result(acc)
end

function _foldr_identity_minus_uint32(A::Vector{UInt32}, init::UInt32)
    acc = 0 + init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint32_minus_result(acc)
end

function _foldr_identity_minus_uint64(A::Vector{UInt64})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = 0 + A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint64_minus_result(acc)
end

function _foldr_identity_minus_uint64(A::Vector{UInt64}, init::UInt64)
    acc = 0 + init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return _wrap_uint64_minus_result(acc)
end

function _foldr_identity_minus_float32(A::Vector{Float32})
    n = length(A)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = A[n]
    i = n - 1
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return acc
end

function _foldr_identity_minus_float32(A::Vector{Float32}, init::Float32)
    acc = init
    i = length(A)
    while i >= 1
        acc = A[i] - acc
        i = i - 1
    end
    return acc
end

# =============================================================================
# foldr - right fold
# =============================================================================
# Based on Julia's base/reduce.jl
#
# foldr(op, itr) combines elements from right to left
#
# Examples:
#   foldr(-, [1, 2, 3]) => 1 - (2 - 3) = 2

"""
    foldr(op, itr)
    foldr(op, itr, init)

Right-fold `itr` using the binary operator `op`.

# Examples
```julia
julia> foldr(-, [1, 2, 3])
2  # = 1 - (2 - 3) = 1 - (-1) = 2
```
"""
function foldr(op::Function, itr)
    # Collect to array first, then fold from right
    arr = collect(itr)
    n = length(arr)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = arr[n]
    i = n - 1
    while i >= 1
        acc = op(arr[i], acc)
        i = i - 1
    end
    return acc
end

function foldr(op::Function, itr, init)
    arr = collect(itr)
    n = length(arr)
    acc = init
    i = n
    while i >= 1
        acc = op(arr[i], acc)
        i = i - 1
    end
    return acc
end

foldr(::typeof(+), A::Vector{Bool}) = _mapfoldl_identity_plus_bool(A)
foldr(::typeof(+), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_plus_bool(A, init)
foldr(::typeof(+), A::Vector{Int8}) = _mapfoldl_identity_plus_int8(A)
foldr(::typeof(+), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_plus_int8(A, init)
foldr(::typeof(+), A::Vector{Int16}) = _mapfoldl_identity_plus_int16(A)
foldr(::typeof(+), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_plus_int16(A, init)
foldr(::typeof(+), A::Vector{Int32}) = _mapfoldl_identity_plus_int32(A)
foldr(::typeof(+), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_plus_int32(A, init)
foldr(::typeof(+), A::Vector{Int64}) = _mapfoldl_identity_plus_int64(A)
foldr(::typeof(+), A::Vector{UInt8}) = _mapfoldl_identity_plus_uint8(A)
foldr(::typeof(+), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_plus_uint8(A, init)
foldr(::typeof(+), A::Vector{UInt16}) = _mapfoldl_identity_plus_uint16(A)
foldr(::typeof(+), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_plus_uint16(A, init)
foldr(::typeof(+), A::Vector{UInt32}) = _mapfoldl_identity_plus_uint32(A)
foldr(::typeof(+), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_plus_uint32(A, init)
foldr(::typeof(+), A::Vector{UInt64}) = _mapfoldl_identity_plus_uint64(A)
foldr(::typeof(+), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_plus_uint64(A, init)
foldr(::typeof(+), A::Vector{Float32}) = _mapfoldl_identity_plus_float32(A)
foldr(::typeof(+), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_plus_float32(A, init)
foldr(::typeof(+), A::Vector{Float64}) = _mapfoldl_identity_plus_float64(A)
foldr(::typeof(*), A::Vector{Bool}) = _mapfoldl_identity_mul_bool(A)
foldr(::typeof(*), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_mul_bool(A, init)
foldr(::typeof(*), A::Vector{Int8}) = _mapfoldl_identity_mul_int8(A)
foldr(::typeof(*), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_mul_int8(A, init)
foldr(::typeof(*), A::Vector{Int16}) = _mapfoldl_identity_mul_int16(A)
foldr(::typeof(*), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_mul_int16(A, init)
foldr(::typeof(*), A::Vector{Int32}) = _mapfoldl_identity_mul_int32(A)
foldr(::typeof(*), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_mul_int32(A, init)
foldr(::typeof(*), A::Vector{Int64}) = _mapfoldl_identity_mul_int64(A)
foldr(::typeof(*), A::Vector{UInt8}) = _mapfoldl_identity_mul_uint8(A)
foldr(::typeof(*), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_mul_uint8(A, init)
foldr(::typeof(*), A::Vector{UInt16}) = _mapfoldl_identity_mul_uint16(A)
foldr(::typeof(*), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_mul_uint16(A, init)
foldr(::typeof(*), A::Vector{UInt32}) = _mapfoldl_identity_mul_uint32(A)
foldr(::typeof(*), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_mul_uint32(A, init)
foldr(::typeof(*), A::Vector{UInt64}) = _mapfoldl_identity_mul_uint64(A)
foldr(::typeof(*), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_mul_uint64(A, init)
foldr(::typeof(*), A::Vector{Float32}) = _mapfoldl_identity_mul_float32(A)
foldr(::typeof(*), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_mul_float32(A, init)
foldr(::typeof(*), A::Vector{Float64}) = _mapfoldl_identity_mul_float64(A)
foldr(::typeof(-), A::Vector{Int8}) = _foldr_identity_minus_int8(A)
foldr(::typeof(-), A::Vector{Int8}, init::Int8) = _foldr_identity_minus_int8(A, init)
foldr(::typeof(-), A::Vector{Int16}) = _foldr_identity_minus_int16(A)
foldr(::typeof(-), A::Vector{Int16}, init::Int16) = _foldr_identity_minus_int16(A, init)
foldr(::typeof(-), A::Vector{Int32}) = _foldr_identity_minus_int32(A)
foldr(::typeof(-), A::Vector{Int32}, init::Int32) = _foldr_identity_minus_int32(A, init)
foldr(::typeof(-), A::Vector{UInt8}) = _foldr_identity_minus_uint8(A)
foldr(::typeof(-), A::Vector{UInt8}, init::UInt8) = _foldr_identity_minus_uint8(A, init)
foldr(::typeof(-), A::Vector{UInt16}) = _foldr_identity_minus_uint16(A)
foldr(::typeof(-), A::Vector{UInt16}, init::UInt16) = _foldr_identity_minus_uint16(A, init)
foldr(::typeof(-), A::Vector{UInt32}) = _foldr_identity_minus_uint32(A)
foldr(::typeof(-), A::Vector{UInt32}, init::UInt32) = _foldr_identity_minus_uint32(A, init)
foldr(::typeof(-), A::Vector{UInt64}) = _foldr_identity_minus_uint64(A)
foldr(::typeof(-), A::Vector{UInt64}, init::UInt64) = _foldr_identity_minus_uint64(A, init)
foldr(::typeof(-), A::Vector{Float32}) = _foldr_identity_minus_float32(A)
foldr(::typeof(-), A::Vector{Float32}, init::Float32) = _foldr_identity_minus_float32(A, init)
foldr(::typeof(min), A::Vector{Int8}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{Int16}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{Int32}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{Int64}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{UInt8}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{UInt16}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{UInt32}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{UInt64}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{Float32}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(min), A::Vector{Float64}) = _reduce_binary_nonempty(min, A)
foldr(::typeof(min), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(min, A, init)
foldr(::typeof(max), A::Vector{Int8}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{Int16}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{Int32}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{Int64}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{UInt8}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{UInt16}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{UInt32}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{UInt64}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{Float32}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(max, A, init)
foldr(::typeof(max), A::Vector{Float64}) = _reduce_binary_nonempty(max, A)
foldr(::typeof(max), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(max, A, init)

# Keyword argument form: foldr(op, itr; init=val) is handled at compiler level
# in call.rs by converting to foldr(op, itr, val) (Issue #2077, #2084)

# =============================================================================
# mapfoldl - left fold with transformation
# =============================================================================
# Based on Julia's base/reduce.jl
#
# mapfoldl(f, op, itr) applies f to each element, then left-folds with op
# mapfoldl(f, op, itr, init) starts accumulation from init
#
# Examples:
#   mapfoldl(x -> x^2, +, [1, 2, 3]) => 1 + 4 + 9 = 14
#   mapfoldl(x -> x^2, -, [1, 2, 3]) => (1 - 4) - 9 = -12

function mapfoldl(f::Function, op::Function, itr)
    y = iterate(itr)
    if y === nothing
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = f(y[1])
    y = iterate(itr, y[2])
    while y !== nothing
        acc = op(acc, f(y[1]))
        y = iterate(itr, y[2])
    end
    return acc
end

function mapfoldl(f::Function, op::Function, itr, init)
    acc = init
    y = iterate(itr)
    while y !== nothing
        acc = op(acc, f(y[1]))
        y = iterate(itr, y[2])
    end
    return acc
end

mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Bool}) = _mapfoldl_identity_plus_bool(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_plus_bool(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Int8}) = _mapfoldl_identity_plus_int8(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_plus_int8(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Int16}) = _mapfoldl_identity_plus_int16(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_plus_int16(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Int32}) = _mapfoldl_identity_plus_int32(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_plus_int32(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Int64}) = _mapfoldl_identity_plus_int64(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt8}) = _mapfoldl_identity_plus_uint8(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_plus_uint8(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt16}) = _mapfoldl_identity_plus_uint16(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_plus_uint16(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt32}) = _mapfoldl_identity_plus_uint32(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_plus_uint32(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt64}) = _mapfoldl_identity_plus_uint64(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_plus_uint64(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Float32}) = _mapfoldl_identity_plus_float32(A)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_plus_float32(A, init)
mapfoldl(::typeof(identity), ::typeof(+), A::Vector{Float64}) = _mapfoldl_identity_plus_float64(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Bool}) = _mapfoldl_identity_mul_bool(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_mul_bool(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Int8}) = _mapfoldl_identity_mul_int8(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_mul_int8(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Int16}) = _mapfoldl_identity_mul_int16(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_mul_int16(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Int32}) = _mapfoldl_identity_mul_int32(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_mul_int32(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Int64}) = _mapfoldl_identity_mul_int64(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt8}) = _mapfoldl_identity_mul_uint8(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_mul_uint8(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt16}) = _mapfoldl_identity_mul_uint16(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_mul_uint16(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt32}) = _mapfoldl_identity_mul_uint32(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_mul_uint32(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt64}) = _mapfoldl_identity_mul_uint64(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_mul_uint64(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Float32}) = _mapfoldl_identity_mul_float32(A)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_mul_float32(A, init)
mapfoldl(::typeof(identity), ::typeof(*), A::Vector{Float64}) = _mapfoldl_identity_mul_float64(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Int8}) = _foldl_identity_minus_int8(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Int8}, init::Int8) = _foldl_identity_minus_int8(A, init)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Int16}) = _foldl_identity_minus_int16(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Int16}, init::Int16) = _foldl_identity_minus_int16(A, init)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Int32}) = _foldl_identity_minus_int32(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Int32}, init::Int32) = _foldl_identity_minus_int32(A, init)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt8}) = _foldl_identity_minus_uint8(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt8}, init::UInt8) = _foldl_identity_minus_uint8(A, init)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt16}) = _foldl_identity_minus_uint16(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt16}, init::UInt16) = _foldl_identity_minus_uint16(A, init)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt32}) = _foldl_identity_minus_uint32(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt32}, init::UInt32) = _foldl_identity_minus_uint32(A, init)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt64}) = _foldl_identity_minus_uint64(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{UInt64}, init::UInt64) = _foldl_identity_minus_uint64(A, init)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Float32}) = _foldl_identity_minus_float32(A)
mapfoldl(::typeof(identity), ::typeof(-), A::Vector{Float32}, init::Float32) = _foldl_identity_minus_float32(A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int8}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int16}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int32}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int64}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt8}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt16}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt32}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt64}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Float32}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Float64}) = _reduce_binary_nonempty(min, A)
mapfoldl(::typeof(identity), ::typeof(min), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(min, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int8}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int16}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int32}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int64}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt8}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt16}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt32}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt64}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Float32}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(max, A, init)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Float64}) = _reduce_binary_nonempty(max, A)
mapfoldl(::typeof(identity), ::typeof(max), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(max, A, init)

# Keyword argument form: mapfoldl(f, op, itr; init=val) is handled at compiler level
# in call.rs by converting to mapfoldl(f, op, itr, val) (Issue #2077)

# =============================================================================
# mapfoldr - right fold with transformation
# =============================================================================
# Based on Julia's base/reduce.jl
#
# mapfoldr(f, op, itr) applies f to each element, then right-folds with op
# mapfoldr(f, op, itr, init) starts accumulation from init
#
# Examples:
#   mapfoldr(x -> x^2, -, [1, 2, 3]) => 1 - (4 - 9) = 6
#   mapfoldr(x -> x + 1, +, [1, 2, 3]) => 2 + (3 + 4) = 9

function mapfoldr(f::Function, op::Function, itr)
    arr = collect(itr)
    n = length(arr)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = f(arr[n])
    i = n - 1
    while i >= 1
        acc = op(f(arr[i]), acc)
        i = i - 1
    end
    return acc
end

function mapfoldr(f::Function, op::Function, itr, init)
    arr = collect(itr)
    n = length(arr)
    acc = init
    i = n
    while i >= 1
        acc = op(f(arr[i]), acc)
        i = i - 1
    end
    return acc
end

mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Bool}) = _mapfoldl_identity_plus_bool(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_plus_bool(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Int8}) = _mapfoldl_identity_plus_int8(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_plus_int8(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Int16}) = _mapfoldl_identity_plus_int16(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_plus_int16(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Int32}) = _mapfoldl_identity_plus_int32(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_plus_int32(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Int64}) = _mapfoldl_identity_plus_int64(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt8}) = _mapfoldl_identity_plus_uint8(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_plus_uint8(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt16}) = _mapfoldl_identity_plus_uint16(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_plus_uint16(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt32}) = _mapfoldl_identity_plus_uint32(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_plus_uint32(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt64}) = _mapfoldl_identity_plus_uint64(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_plus_uint64(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Float32}) = _mapfoldl_identity_plus_float32(A)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_plus_float32(A, init)
mapfoldr(::typeof(identity), ::typeof(+), A::Vector{Float64}) = _mapfoldl_identity_plus_float64(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Bool}) = _mapfoldl_identity_mul_bool(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Bool}, init::Bool) = _mapfoldl_identity_mul_bool(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Int8}) = _mapfoldl_identity_mul_int8(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Int8}, init::Int8) = _mapfoldl_identity_mul_int8(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Int16}) = _mapfoldl_identity_mul_int16(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Int16}, init::Int16) = _mapfoldl_identity_mul_int16(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Int32}) = _mapfoldl_identity_mul_int32(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Int32}, init::Int32) = _mapfoldl_identity_mul_int32(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Int64}) = _mapfoldl_identity_mul_int64(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt8}) = _mapfoldl_identity_mul_uint8(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt8}, init::UInt8) = _mapfoldl_identity_mul_uint8(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt16}) = _mapfoldl_identity_mul_uint16(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt16}, init::UInt16) = _mapfoldl_identity_mul_uint16(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt32}) = _mapfoldl_identity_mul_uint32(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt32}, init::UInt32) = _mapfoldl_identity_mul_uint32(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt64}) = _mapfoldl_identity_mul_uint64(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{UInt64}, init::UInt64) = _mapfoldl_identity_mul_uint64(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Float32}) = _mapfoldl_identity_mul_float32(A)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Float32}, init::Float32) = _mapfoldl_identity_mul_float32(A, init)
mapfoldr(::typeof(identity), ::typeof(*), A::Vector{Float64}) = _mapfoldl_identity_mul_float64(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Int8}) = _foldr_identity_minus_int8(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Int8}, init::Int8) = _foldr_identity_minus_int8(A, init)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Int16}) = _foldr_identity_minus_int16(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Int16}, init::Int16) = _foldr_identity_minus_int16(A, init)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Int32}) = _foldr_identity_minus_int32(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Int32}, init::Int32) = _foldr_identity_minus_int32(A, init)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt8}) = _foldr_identity_minus_uint8(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt8}, init::UInt8) = _foldr_identity_minus_uint8(A, init)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt16}) = _foldr_identity_minus_uint16(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt16}, init::UInt16) = _foldr_identity_minus_uint16(A, init)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt32}) = _foldr_identity_minus_uint32(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt32}, init::UInt32) = _foldr_identity_minus_uint32(A, init)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt64}) = _foldr_identity_minus_uint64(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{UInt64}, init::UInt64) = _foldr_identity_minus_uint64(A, init)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Float32}) = _foldr_identity_minus_float32(A)
mapfoldr(::typeof(identity), ::typeof(-), A::Vector{Float32}, init::Float32) = _foldr_identity_minus_float32(A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int8}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int16}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int32}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int64}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt8}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt16}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt32}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt64}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Float32}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Float64}) = _reduce_binary_nonempty(min, A)
mapfoldr(::typeof(identity), ::typeof(min), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(min, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int8}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int16}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int32}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int64}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt8}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt16}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt32}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt64}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Float32}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(max, A, init)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Float64}) = _reduce_binary_nonempty(max, A)
mapfoldr(::typeof(identity), ::typeof(max), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(max, A, init)

# Keyword argument form: mapfoldr(f, op, itr; init=val) is handled at compiler level
# in call.rs by converting to mapfoldr(f, op, itr, val) (Issue #2077)

# =============================================================================
# mapreduce - map and reduce (alias for mapfoldl)
# =============================================================================
# Based on Julia's base/reduce.jl:305
# mapreduce(f, op, itr) is an alias for mapfoldl(f, op, itr)

mapreduce(f::Function, op::Function, itr) = mapfoldl(f, op, itr)
mapreduce(f::Function, op::Function, itr, init) = mapfoldl(f, op, itr, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int8}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int16}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int32}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int64}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt8}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt16}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt32}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt64}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Float32}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Float64}) = _reduce_binary_nonempty(min, A)
mapreduce(::typeof(identity), ::typeof(min), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(min, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int8}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int8}, init::Int8) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int16}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int16}, init::Int16) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int32}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int32}, init::Int32) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int64}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Int64}, init::Int64) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt8}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt8}, init::UInt8) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt16}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt16}, init::UInt16) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt32}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt32}, init::UInt32) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt64}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{UInt64}, init::UInt64) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Float32}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Float32}, init::Float32) = _reduce_binary_with_init(max, A, init)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Float64}) = _reduce_binary_nonempty(max, A)
mapreduce(::typeof(identity), ::typeof(max), A::Vector{Float64}, init::Float64) = _reduce_binary_with_init(max, A, init)

# Keyword argument form: mapreduce(f, op, itr; init=val) is handled at compiler level
# in call.rs by converting to mapreduce(f, op, itr, val) (Issue #2077)

# =============================================================================
# Iterators module namespace
# =============================================================================
# Upstream Julia defines these helpers in Base.Iterators and exports the module
# itself from Base. SubsetJuliaVM keeps the implementations above at Base scope
# for compatibility with existing Base-level call paths, then exposes the
# upstream namespace as thin aliases/wrappers.

module Iterators

export enumerate, zip, rest, countfrom, take, drop, takewhile, dropwhile,
       cycle, repeated, product, flatten, flatmap, partition, nth

public accumulate, filter, map, peel, reverse

const Enumerate = Base.Enumerate
const Zip = Base.Zip
const Zip3 = Base.Zip3
const Zip4 = Base.Zip4
const Zip5 = Base.Zip5
const Zip6 = Base.Zip6
const Zip7 = Base.Zip7
const Take = Base.Take
const Drop = Base.Drop
const TakeWhile = Base.TakeWhile
const DropWhile = Base.DropWhile
const Flatten = Base.Flatten
const FlatMap = Base.FlatMap
const Rest = Base.Rest
const Cycle = Base.Cycle
const Repeated = Base.Repeated
const Partition = Base.Partition
const Product = Base.Product
const ProductIterator = Base.ProductIterator
const Count = Base.Count
const Filter = Base.Filter

enumerate(iter) = Base.enumerate(iter)
zip(args...) = Base.zip(args...)
rest(args...) = Base.rest(args...)
countfrom(args...) = Base.countfrom(args...)
take(iter, n) = Base.take(iter, n)
drop(iter, n) = Base.drop(iter, n)
takewhile(f, iter) = Base.takewhile(f, iter)
dropwhile(f, iter) = Base.dropwhile(f, iter)
cycle(iter) = Base.cycle(iter)
repeated(args...) = Base.repeated(args...)
product(args...) = Base.ProductIterator(args)
flatten(iter) = Base.flatten(iter)
flatmap(f, iter) = Base.flatmap(f, iter)
partition(iter, n) = Base.partition(iter, n)
nth(iter, n) = Base.nth(iter, n)
peel(iter) = Base.peel(iter)

filter(flt, itr) = Base.Filter(flt, itr)
map(f, arg, args...) = Base.Generator(f, arg, args...)
accumulate(args...) = Base.accumulate(args...)
reverse(iter) = Base.reverse(iter)

end # module Iterators
