# =============================================================================
# Tuple - Tuple utilities
# =============================================================================
# Based on Julia's base/tuple.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
#
# Removed functions (not in Julia Base with these names):
#   - ntuple_indices, ntuple_fill (use ntuple with lambda)
#   - tuple_reverse (use reverse)
#   - tuple_map_square, tuple_map_double (use map)
#   - tuple_sum, tuple_prod (use sum, prod)
#   - tuple_min, tuple_max (use minimum, maximum)
#   - tuple_contains (use in)
#   - tuple_index (use findfirst)
#   - tuple_count (use count)
#
# Note: Julia's tuple operations use standard functions like sum, prod, etc.
# that work on any iterable. No special tuple-specific functions are needed.

# =============================================================================
# copy(t::Tuple) - copy of a Tuple (identity, since Tuples are immutable)
# Reference: julia/base/tuple.jl
# =============================================================================

tuple(args...) = args

# =============================================================================
# ntuple(f, n::Integer) - construct a tuple of length n from f(i)
# Reference: julia/base/ntuple.jl
# =============================================================================
# Issue #4973: `ntuple` previously existed only as a Rust builtin HOF
# (`BuiltinId::Ntuple`), so it was not a first-class function value
# (`f = ntuple` / `Base.ntuple` raised UndefVarError). Providing a pure-Julia
# method here gives `ntuple` a method-table entry so it dispatches as an
# ordinary function when referenced or passed as a value, while the compiler
# still intercepts the *direct* `ntuple(f, n)` / `ntuple(f, Val(N))` call shapes
# in `compile/expr/builtin_hof.rs` for the constant-propagation fast path.
#
# This body must NOT recurse into `ntuple` (the compiler would intercept the
# self call); it builds the result with a comprehension + splat instead, which
# matches upstream `_ntuple`.
function ntuple(f, n::Integer)
    n >= 0 || throw(ArgumentError("tuple length should be ≥ 0, got $(n)"))
    result = ()
    for i in 1:n
        result = tuple(result..., f(i))
    end
    return result
end

# Tuples are immutable in Julia, so copy simply returns the tuple itself.
# This matches Julia's behavior where copy on immutable types is identity.
copy(t::Tuple) = t

# =============================================================================
# map for Tuple
# =============================================================================
# Based on Julia's base/tuple.jl:353-357.  Tuple mapping preserves tuple shape
# and per-slot result types for small arities instead of falling through to the
# generic iterator `map(f, A)` path, which materializes `Vector{Any}`.

function map(f, t::Tuple)
    n = length(t)
    if n == 0
        return ()
    elseif n == 1
        return (f(t[1]),)
    elseif n == 2
        return (f(t[1]), f(t[2]))
    elseif n == 3
        return (f(t[1]), f(t[2]), f(t[3]))
    elseif n == 4
        return (f(t[1]), f(t[2]), f(t[3]), f(t[4]))
    elseif n == 5
        return (f(t[1]), f(t[2]), f(t[3]), f(t[4]), f(t[5]))
    elseif n == 6
        return (f(t[1]), f(t[2]), f(t[3]), f(t[4]), f(t[5]), f(t[6]))
    elseif n == 7
        return (f(t[1]), f(t[2]), f(t[3]), f(t[4]), f(t[5]), f(t[6]), f(t[7]))
    elseif n == 8
        return (f(t[1]), f(t[2]), f(t[3]), f(t[4]), f(t[5]), f(t[6]), f(t[7]), f(t[8]))
    else
        result = ()
        for i in 1:n
            result = tuple(result..., f(t[i]))
        end
        return result
    end
end

function map(f, t::Tuple, s::Tuple)
    n = min(length(t), length(s))
    if n == 0
        return ()
    elseif n == 1
        return (f(t[1], s[1]),)
    elseif n == 2
        return (f(t[1], s[1]), f(t[2], s[2]))
    elseif n == 3
        return (f(t[1], s[1]), f(t[2], s[2]), f(t[3], s[3]))
    elseif n == 4
        return (f(t[1], s[1]), f(t[2], s[2]), f(t[3], s[3]), f(t[4], s[4]))
    elseif n == 5
        return (f(t[1], s[1]), f(t[2], s[2]), f(t[3], s[3]), f(t[4], s[4]), f(t[5], s[5]))
    elseif n == 6
        return (f(t[1], s[1]), f(t[2], s[2]), f(t[3], s[3]), f(t[4], s[4]), f(t[5], s[5]), f(t[6], s[6]))
    elseif n == 7
        return (f(t[1], s[1]), f(t[2], s[2]), f(t[3], s[3]), f(t[4], s[4]), f(t[5], s[5]), f(t[6], s[6]), f(t[7], s[7]))
    elseif n == 8
        return (f(t[1], s[1]), f(t[2], s[2]), f(t[3], s[3]), f(t[4], s[4]), f(t[5], s[5]), f(t[6], s[6]), f(t[7], s[7]), f(t[8], s[8]))
    else
        result = ()
        for i in 1:n
            result = tuple(result..., f(t[i], s[i]))
        end
        return result
    end
end

function map(f, t::Tuple, s::Tuple, u::Tuple)
    n = min(length(t), min(length(s), length(u)))
    if n == 0
        return ()
    elseif n == 1
        return (f(t[1], s[1], u[1]),)
    elseif n == 2
        return (f(t[1], s[1], u[1]), f(t[2], s[2], u[2]))
    elseif n == 3
        return (f(t[1], s[1], u[1]), f(t[2], s[2], u[2]), f(t[3], s[3], u[3]))
    elseif n == 4
        return (f(t[1], s[1], u[1]), f(t[2], s[2], u[2]), f(t[3], s[3], u[3]), f(t[4], s[4], u[4]))
    elseif n == 5
        return (f(t[1], s[1], u[1]), f(t[2], s[2], u[2]), f(t[3], s[3], u[3]), f(t[4], s[4], u[4]), f(t[5], s[5], u[5]))
    elseif n == 6
        return (f(t[1], s[1], u[1]), f(t[2], s[2], u[2]), f(t[3], s[3], u[3]), f(t[4], s[4], u[4]), f(t[5], s[5], u[5]), f(t[6], s[6], u[6]))
    elseif n == 7
        return (f(t[1], s[1], u[1]), f(t[2], s[2], u[2]), f(t[3], s[3], u[3]), f(t[4], s[4], u[4]), f(t[5], s[5], u[5]), f(t[6], s[6], u[6]), f(t[7], s[7], u[7]))
    elseif n == 8
        return (f(t[1], s[1], u[1]), f(t[2], s[2], u[2]), f(t[3], s[3], u[3]), f(t[4], s[4], u[4]), f(t[5], s[5], u[5]), f(t[6], s[6], u[6]), f(t[7], s[7], u[7]), f(t[8], s[8], u[8]))
    else
        result = ()
        for i in 1:n
            result = tuple(result..., f(t[i], s[i], u[i]))
        end
        return result
    end
end

function map(f, t::Tuple, s::Tuple, u::Tuple, v::Tuple)
    n = min(min(length(t), length(s)), min(length(u), length(v)))
    if n == 0
        return ()
    elseif n == 1
        return (f(t[1], s[1], u[1], v[1]),)
    elseif n == 2
        return (f(t[1], s[1], u[1], v[1]), f(t[2], s[2], u[2], v[2]))
    elseif n == 3
        return (f(t[1], s[1], u[1], v[1]), f(t[2], s[2], u[2], v[2]), f(t[3], s[3], u[3], v[3]))
    elseif n == 4
        return (f(t[1], s[1], u[1], v[1]), f(t[2], s[2], u[2], v[2]), f(t[3], s[3], u[3], v[3]), f(t[4], s[4], u[4], v[4]))
    elseif n == 5
        return (f(t[1], s[1], u[1], v[1]), f(t[2], s[2], u[2], v[2]), f(t[3], s[3], u[3], v[3]), f(t[4], s[4], u[4], v[4]), f(t[5], s[5], u[5], v[5]))
    elseif n == 6
        return (f(t[1], s[1], u[1], v[1]), f(t[2], s[2], u[2], v[2]), f(t[3], s[3], u[3], v[3]), f(t[4], s[4], u[4], v[4]), f(t[5], s[5], u[5], v[5]), f(t[6], s[6], u[6], v[6]))
    elseif n == 7
        return (f(t[1], s[1], u[1], v[1]), f(t[2], s[2], u[2], v[2]), f(t[3], s[3], u[3], v[3]), f(t[4], s[4], u[4], v[4]), f(t[5], s[5], u[5], v[5]), f(t[6], s[6], u[6], v[6]), f(t[7], s[7], u[7], v[7]))
    elseif n == 8
        return (f(t[1], s[1], u[1], v[1]), f(t[2], s[2], u[2], v[2]), f(t[3], s[3], u[3], v[3]), f(t[4], s[4], u[4], v[4]), f(t[5], s[5], u[5], v[5]), f(t[6], s[6], u[6], v[6]), f(t[7], s[7], u[7], v[7]), f(t[8], s[8], u[8], v[8]))
    else
        result = ()
        for i in 1:n
            result = tuple(result..., f(t[i], s[i], u[i], v[i]))
        end
        return result
    end
end

# =============================================================================
# == for Tuple
# =============================================================================
# Based on Julia's base/tuple.jl comparison definitions.

function ==(t1::Tuple, t2::Tuple)
    if length(t1) != length(t2)
        return false
    end
    for i in 1:length(t1)
        if (t1[i] == t2[i]) == false
            return false
        end
    end
    return true
end

# =============================================================================
# findfirst / findlast / findall over a Tuple (Issue #5681)
# =============================================================================
# Match upstream: findfirst/findlast return the Int index of the first/last
# element satisfying the predicate, or `nothing`; findall returns a Vector{Int}
# of all matching indices. (Indices only — no Tuple is rebuilt, so these avoid
# the dynamic tuple-construction limitations of the subset.)

function findfirst(f::Function, t::Tuple)
    n = length(t)
    i = 1
    while i <= n
        if f(t[i])
            return i
        end
        i += 1
    end
    return nothing
end

function findlast(f::Function, t::Tuple)
    i = length(t)
    while i >= 1
        if f(t[i])
            return i
        end
        i -= 1
    end
    return nothing
end

function findall(f::Function, t::Tuple)
    result = Int[]
    n = length(t)
    i = 1
    while i <= n
        if f(t[i])
            push!(result, i)
        end
        i += 1
    end
    return result
end

# =============================================================================
# filter / cumsum / cumprod over a Tuple (Issue #5681)
# =============================================================================
# These return a length-varying / same-length Tuple. The subset has no tuple
# splat `(t..., x)` and no `Tuple(::Vector)` constructor, so the result is built
# from a Vector by explicit output-arity dispatch (the same no-splat idiom
# `map(::Tuple)` uses). `Any[]` preserves element types after Issue #5717.

function _tuple_from_vector(v)
    n = length(v)
    if n == 0
        return ()
    elseif n == 1
        return (v[1],)
    elseif n == 2
        return (v[1], v[2])
    elseif n == 3
        return (v[1], v[2], v[3])
    elseif n == 4
        return (v[1], v[2], v[3], v[4])
    elseif n == 5
        return (v[1], v[2], v[3], v[4], v[5])
    elseif n == 6
        return (v[1], v[2], v[3], v[4], v[5], v[6])
    elseif n == 7
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7])
    elseif n == 8
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8])
    elseif n == 9
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9])
    elseif n == 10
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10])
    elseif n == 11
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11])
    elseif n == 12
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12])
    elseif n == 13
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13])
    elseif n == 14
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13], v[14])
    elseif n == 15
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13], v[14], v[15])
    elseif n == 16
        return (v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13], v[14], v[15], v[16])
    else
        error("tuple result longer than 16 elements is not supported")
    end
end

# Tuple(itr) — construct a Tuple from an array / abstract array (Issue #8132).
# Upstream Base provides `(::Type{T})(itr) where {T<:Tuple}`; the subset has no
# generic `Tuple` constructor over arbitrary collections, so `Tuple([1.0, 2.0])`
# (and `Tuple(dg)` where a package override returned an `SVector` whose runtime
# type differs from the inferred generic `Vector{T}`) previously failed to
# compile with "Unknown function: Tuple". `AbstractArray` covers both `Vector`
# and StaticArrays' `SVector`/`SMatrix`, reusing the no-splat `_tuple_from_vector`
# arity dispatch (which reads through `getindex`, robust to the flat StaticArray
# representation).
Tuple(a::AbstractArray) = _tuple_from_vector(a)

# Tuple(g::Generator) — materialize a lazy generator, then build the tuple
# (Issue #9103). Upstream reaches this through the generic
# `(::Type{T})(itr) where {T<:Tuple}` constructor; the subset's `Tuple` only
# covered `AbstractArray`, so `Tuple(x^2 for x in 1:3)` had no method once
# generator expressions became lazy.
Tuple(g::Generator) = _tuple_from_vector(collect(g))

function filter(f::Function, t::Tuple)
    kept = Any[]
    for x in t
        if f(x)
            push!(kept, x)
        end
    end
    return _tuple_from_vector(kept)
end

function cumsum(t::Tuple)
    n = length(t)
    if n == 0
        return ()
    end
    acc = Any[]
    s = t[1]
    push!(acc, s)
    for i in 2:n
        s = s + t[i]
        push!(acc, s)
    end
    return _tuple_from_vector(acc)
end

function cumprod(t::Tuple)
    n = length(t)
    if n == 0
        return ()
    end
    acc = Any[]
    p = t[1]
    push!(acc, p)
    for i in 2:n
        p = p * t[i]
        push!(acc, p)
    end
    return _tuple_from_vector(acc)
end

# =============================================================================
# eltype for Tuple
# =============================================================================
# Based on Julia's base/tuple.jl:275-309.  Julia computes tuple eltype as the
# promote/typejoin of element types, with the empty tuple returning Union{}.

function eltype(t::Tuple)
    n = length(t)
    if n == 0
        return Union{}
    end

    T = typeof(t[1])
    i = 2
    while i <= n
        T = typejoin(T, typeof(t[i]))
        T === Any && return Any
        i += 1
    end
    return T
end

# Type form: eltype(::Type{<:Tuple}).  Upstream (julia/base/tuple.jl:280-309)
# computes the typejoin of the tuple's element types, returning `Union{}` for
# the empty tuple `Tuple{}`.  The VM cannot bind a covariant `::Type{<:Tuple}`
# type parameter, so we dispatch on the bound `Type{<:Tuple}` and read the
# concrete element types from `.parameters`.  A trailing `Vararg{E}` element
# contributes its inner element type `E` to the join.
function eltype(t::Type{<:Tuple})
    params = t.parameters
    n = length(params)
    if n == 0
        return Union{}
    end

    T = Union{}
    first = true
    i = 1
    while i <= n
        p = params[i]
        if isvarargtype(p)
            vp = p.parameters
            elt = length(vp) >= 1 ? vp[1] : Any
        else
            elt = p
        end
        if first
            T = elt
            first = false
        else
            T = typejoin(T, elt)
        end
        T === Any && return Any
        i += 1
    end
    return T
end

# =============================================================================
# first for Tuple
# =============================================================================
# Returns the first element of a tuple.
# Based on Julia's base/tuple.jl:269-270
#
# Examples:
#   first((1, 2, 3)) => 1
#   first((42,)) => 42
#   first(()) => throws ArgumentError

function first(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("tuple must be non-empty"))
    end
    return t[1]
end

# =============================================================================
# last for Tuple
# =============================================================================
# Returns the last element of a tuple.
# Based on Julia's base/tuple.jl (implicit from indexing)
#
# Examples:
#   last((1, 2, 3)) => 3
#   last((42,)) => 42
#   last(()) => throws ArgumentError

function last(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("tuple must be non-empty"))
    end
    return t[n]
end

# =============================================================================
# reverse for Tuple
# =============================================================================
# Returns a new tuple with elements in reverse order.
# Based on Julia's base/tuple.jl:644
#
# Since lambda expressions (i -> ...) are not supported in prelude,
# we implement fixed-size overloads for common tuple sizes.
#
# Examples:
#   reverse((1, 2, 3)) => (3, 2, 1)
#   reverse(()) => ()
#   reverse((42,)) => (42,)

# Tuple reverse - uses runtime dispatch via isa check
function reverse(t::Tuple)
    n = length(t)
    if n == 0
        return ()
    elseif n == 1
        return (t[1],)
    elseif n == 2
        return (t[2], t[1])
    elseif n == 3
        return (t[3], t[2], t[1])
    elseif n == 4
        return (t[4], t[3], t[2], t[1])
    elseif n == 5
        return (t[5], t[4], t[3], t[2], t[1])
    elseif n == 6
        return (t[6], t[5], t[4], t[3], t[2], t[1])
    elseif n == 7
        return (t[7], t[6], t[5], t[4], t[3], t[2], t[1])
    elseif n == 8
        return (t[8], t[7], t[6], t[5], t[4], t[3], t[2], t[1])
    else
        # Fallback for larger tuples: return as array (compatibility mode)
        result = collect(t)
        m = length(result)
        for i in 1:div(m, 2)
            tmp = result[i]
            result[i] = result[m - i + 1]
            result[m - i + 1] = tmp
        end
        return result
    end
end

# =============================================================================
# front for Tuple
# =============================================================================
# Returns a tuple containing all but the last element.
# Based on Julia's base/tuple.jl:339
#
# Examples:
#   front((1, 2, 3)) => (1, 2)
#   front((1, 2)) => (1,)
#   front((1,)) => ()
#   front(()) => throws ArgumentError

function front(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("Cannot call front on an empty tuple."))
    elseif n == 1
        return ()
    elseif n == 2
        return (t[1],)
    elseif n == 3
        return (t[1], t[2])
    elseif n == 4
        return (t[1], t[2], t[3])
    elseif n == 5
        return (t[1], t[2], t[3], t[4])
    elseif n == 6
        return (t[1], t[2], t[3], t[4], t[5])
    elseif n == 7
        return (t[1], t[2], t[3], t[4], t[5], t[6])
    elseif n == 8
        return (t[1], t[2], t[3], t[4], t[5], t[6], t[7])
    else
        # Fallback for larger tuples: return as array
        result = collect(t)
        pop!(result)
        return result
    end
end

# =============================================================================
# tail for Tuple
# =============================================================================
# Returns a tuple containing all but the first element.
# Based on Julia's base/essentials.jl:534
#
# This is the converse of front: tail skips the first entry,
# while front skips the last entry.
#
# Examples:
#   tail((1, 2, 3)) => (2, 3)
#   tail((1, 2)) => (2,)
#   tail((1,)) => ()
#   tail(()) => throws ArgumentError

function tail(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("Cannot call tail on an empty tuple."))
    elseif n == 1
        return ()
    elseif n == 2
        return (t[2],)
    elseif n == 3
        return (t[2], t[3])
    elseif n == 4
        return (t[2], t[3], t[4])
    elseif n == 5
        return (t[2], t[3], t[4], t[5])
    elseif n == 6
        return (t[2], t[3], t[4], t[5], t[6])
    elseif n == 7
        return (t[2], t[3], t[4], t[5], t[6], t[7])
    elseif n == 8
        return (t[2], t[3], t[4], t[5], t[6], t[7], t[8])
    else
        # Fallback for larger tuples: return as array
        result = collect(t)
        popfirst!(result)
        return result
    end
end

# =============================================================================
# safe_tail for Tuple
# =============================================================================
# Version of tail that doesn't throw on empty tuples.
# Based on Julia's base/tuple.jl:318-319
#
# Used internally for array indexing and other operations where
# an empty tuple should silently return empty tuple.
#
# Examples:
#   safe_tail((1, 2, 3)) => (2, 3)
#   safe_tail((1,)) => ()
#   safe_tail(()) => ()  # Unlike tail, doesn't throw

function safe_tail(t::Tuple)
    n = length(t)
    if n == 0
        return ()  # Safe: returns empty tuple instead of throwing
    else
        return tail(t)
    end
end

# =============================================================================
# tuple_type_head / tuple_type_tail / tuple_type_cons (Issue #5119)
# =============================================================================
# Type-level decomposition and construction of Tuple types. These are Base
# internals (not exported); some packages rely on them for recursive generic
# code that walks a `Tuple{...}` signature one parameter at a time.
#
# Upstream Julia 1.12:
#   - tuple_type_head(T) = fieldtype(T, 1)                  (base/deprecated.jl)
#   - tuple_type_cons(::Type, ::Type{Union{}}) = Union{}    (base/deprecated.jl)
#     tuple_type_cons(::Type{S}, ::Type{T}) where {T<:Tuple, S} = Tuple{S, T.parameters...}
#   - tuple_type_tail(T)                                    (base/tuple.jl)
#
# SubsetJuliaVM cannot splat `T.parameters` directly into a `Tuple{...}` type
# constructor (the splat is not expanded by the type-application path), so the
# new tuple type is assembled by the internal `_make_tuple_type` builtin from a
# runtime collection of type objects (Issue #5119). `fieldtype(Tuple{...}, i)`
# also returns `()` in the VM today, so `tuple_type_head` reads the first type
# parameter via `T.parameters[1]`, which is observably equivalent to upstream's
# `fieldtype(T, 1)` for concrete Tuple types.

# tuple_type_head(Tuple{A, B, ...}) === A
function tuple_type_head(T::Type)
    return T.parameters[1]
end

# tuple_type_tail(Tuple{A, B, C}) === Tuple{B, C}; tuple_type_tail(Tuple{A}) === Tuple{}
function tuple_type_tail(T::Type)
    params = T.parameters
    rest = Any[]
    for i in 2:length(params)
        push!(rest, params[i])
    end
    return _make_tuple_type(rest)
end

# tuple_type_cons(S, Tuple{A, B}) === Tuple{S, A, B}; tuple_type_cons(S, Union{}) === Union{}
function tuple_type_cons(S::Type, T::Type)
    if T === Union{}
        return Union{}
    end
    params = T.parameters
    elems = Any[S]
    for i in 1:length(params)
        push!(elems, params[i])
    end
    return _make_tuple_type(elems)
end
