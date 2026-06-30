# Array-backed binary heap helpers, adapted from
# extern/DataStructures.jl/src/heaps/arrays_as_heaps.jl.

heapleft(i::Integer) = 2i
heapright(i::Integer) = 2i + 1
heapparent(i::Integer) = div(i, 2)

function percolate_down!(
    xs::AbstractArray,
    i::Integer,
    x,
    o::Ordering=Forward,
    len::Integer=length(xs),
)
    checkbounds(xs, i)
    checkbounds(xs, len)

    while (l = heapleft(i)) <= len
        r = heapright(i)
        j = r > len || lt(o, xs[l], xs[r]) ? l : r
        lt(o, xs[j], x) || break
        xs[i] = xs[j]
        i = j
    end
    xs[i] = x
    return xs
end

percolate_down!(
    xs::AbstractArray,
    i::Integer,
    o::Ordering=Forward,
    len::Integer=length(xs),
) = percolate_down!(xs, i, xs[i], o, len)

function percolate_up!(xs::AbstractArray, i::Integer, x, o::Ordering=Forward)
    checkbounds(xs, i)

    while (j = heapparent(i)) >= 1
        lt(o, x, xs[j]) || break
        xs[i] = xs[j]
        i = j
    end
    xs[i] = x
    return xs
end

percolate_up!(xs::AbstractArray, i::Integer, o::Ordering=Forward) =
    percolate_up!(xs, i, xs[i], o)

function heappop!(xs::AbstractArray, o::Ordering=Forward)
    x = xs[1]
    y = pop!(xs)
    if !isempty(xs)
        percolate_down!(xs, 1, y, o)
    end
    return x
end

function heappush!(xs::AbstractArray, x, o::Ordering=Forward)
    push!(xs, x)
    percolate_up!(xs, length(xs), o)
    return xs
end

function heapify!(xs::AbstractArray, o::Ordering=Forward)
    for i in heapparent(length(xs)):-1:1
        percolate_down!(xs, i, o)
    end
    return xs
end

heapify(xs::AbstractArray, o::Ordering=Forward) = heapify!(copy(xs), o)

function isheap(xs::AbstractArray, o::Ordering=Forward)
    for i in 1:div(length(xs), 2)
        if lt(o, xs[heapleft(i)], xs[i]) ||
           (heapright(i) <= length(xs) && lt(o, xs[heapright(i)], xs[i]))
            return false
        end
    end
    return true
end
