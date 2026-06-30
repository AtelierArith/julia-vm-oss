# =============================================================================
# Set - pure-Julia Dict{T,Nothing} wrapper (Issue #6721)
# =============================================================================
# Based on Julia's base/set.jl and base/abstractset.jl
#
# Upstream layers `Set{T}` directly on top of `Dict{T,Nothing}`, delegating all
# membership/iteration/mutation to the dict. We reproduce that here so that a
# `Set` value is a pure-Julia struct that participates in `Set{T}` parametric
# method-table dispatch exactly like `Dict{K,V}` (a user-defined
# `ft(x::Set{T}) where {T} = T` now extracts the element type — Issue #6721).
#
# The pure-Julia `Dict{T,Nothing}` (base/dict.jl, Issue #6571) is the backing
# store; this file is loaded AFTER dict.jl in `get_base()` so `Dict{T,Nothing}`
# is available as the field type.
#
# The legacy native `Value::Set` carrier and `_set_*` HashSet intrinsics remain
# in the VM only as cache-compatibility / old-bytecode fallbacks; new public
# `Set(...)` construction routes through the struct constructors below.

# =============================================================================
# Set{T} struct definition + constructors
# Reference: julia/base/set.jl:39-65
# =============================================================================

struct Set{T} <: AbstractSet{T}
    dict::Dict{T,Nothing}
end

# Build an empty `Set{T}` from a runtime element-type value `T`.
#
# We cannot write the `Set{T}()`/`Dict{T,Nothing}()` literal with a `where`-bound
# `T` (the compiler eagerly instantiates the parametric type and rejects the type
# variable), so the empty backing `Dict{T,Nothing}` is built from the runtime
# type value `T` via the Dict struct's `_new_dict_kv(K, V, n)` helper — the same
# runtime-type-value pattern `Dict{K,V}()` uses internally — and wrapped in the
# `Set{T}` struct via its default constructor.
#
# `_set_from_eltype` / `_set_with_eltype` are the two helpers the compiler routes
# explicit `Set{T}()` / `Set{T}(itr)` construction to (see
# `try_compile_explicit_public_set_constructor`); `Set{T}()` / `Set{T}(itr)`
# below are the source-level method definitions used by the rest of Base.
function _set_from_eltype(::Type{T}) where {T}
    return Set{T}(_new_dict_kv(T, Nothing, 0))
end

function _set_with_eltype(::Type{T}, itr) where {T}
    s = _set_from_eltype(T)
    union!(s, itr)
    return s
end

Set{T}() where {T} = _set_from_eltype(T)
Set{T}(s::Set{T}) where {T} = _set_with_eltype(T, s)
Set{T}(itr) where {T} = _set_with_eltype(T, itr)
Set() = Set{Any}()

# Set(itr): infer element type from the iterator when it is known, otherwise
# fall back to a Set{Any} grown element-by-element. Mirrors upstream's
# `_Set(itr, IteratorEltype(itr))` dispatch (julia/base/set.jl:58-65) for the
# subset the VM can represent.
Set(itr) = _Set_from_itr(itr, IteratorEltype(itr))
_Set_from_itr(itr, ::HasEltype) = _set_with_eltype(eltype(itr), itr)
_Set_from_itr(itr, ::EltypeUnknown) = _set_with_eltype(Any, itr)

# =============================================================================
# Core Set operations - delegated to the backing Dict{T,Nothing}
# Reference: julia/base/set.jl:90-176
# =============================================================================

push!(s::Set, x) = (s.dict[x] = nothing; s)
delete!(s::Set, x) = (delete!(s.dict, x); s)
# Note: `in` is a parser keyword and cannot be used as a function name, so
# `in(x, s::Set)`/`x in s` is routed through method dispatch to this wrapper,
# which delegates to `haskey` on the backing Dict.
in(x, s::Set) = haskey(s.dict, x)
empty!(s::Set) = (empty!(s.dict); s)
length(s::Set) = length(s.dict)
isempty(s::Set) = isempty(s.dict)
sizehint!(s::Set, newsz) = s

# pop! — remove and return an element (julia/base/set.jl:139-162).
# `pop!(s, x)` removes `x` (KeyError / default if absent); `pop!(s)` removes an
# arbitrary element. We delegate to the backing Dict's `pop!`/`delete!`.
function pop!(s::Set, x)
    if !haskey(s.dict, x)
        throw(KeyError(x))
    end
    delete!(s.dict, x)
    return x
end

function pop!(s::Set, x, default)
    if haskey(s.dict, x)
        delete!(s.dict, x)
        return x
    end
    return default
end

function pop!(s::Set)
    y = iterate(s)
    if y === nothing
        throw(ArgumentError("set must be non-empty"))
    end
    x = y[1]
    delete!(s.dict, x)
    return x
end

# Iteration delegates to the backing Dict's KeySet (the same KeySet struct
# defined in base/dict.jl), so `for x in s` yields the set's elements.
iterate(s::Set) = iterate(KeySet(s.dict))
iterate(s::Set, state) = iterate(KeySet(s.dict), state)

# eltype for sets (Issue #5116).  Upstream
# (julia/base/abstractset.jl:3) defines
# `eltype(::Type{<:AbstractSet{T}}) where {T} = @isdefined(T) ? T : Any` and the
# value form `eltype(x) = eltype(typeof(x))` (julia/base/abstractarray.jl:245).
# The VM cannot bind a covariant `::Type{<:AbstractSet{T}}` type parameter, so
# the type method dispatches on the concrete `Set{T}` parametric type which the
# dispatcher resolves.
eltype(::Type{Set{T}}) where {T} = T
eltype(s::Set) = eltype(typeof(s))

# =============================================================================
# Set algebra operations - Pure Julia (Issue #2575)
# Reference: julia/base/abstractset.jl
# =============================================================================

# union(s1::Set, s2::Set) - set union
# Reference: julia/base/abstractset.jl:18
function union(s1::Set, s2::Set)
    result = Set()
    for x in s1
        result = push!(result, x)
    end
    for x in s2
        result = push!(result, x)
    end
    return result
end

# intersect(s1::Set, s2::Set) - set intersection
# Reference: julia/base/abstractset.jl:157
function intersect(s1::Set, s2::Set)
    result = Set()
    for x in s1
        if x in s2
            result = push!(result, x)
        end
    end
    return result
end

# setdiff(s1::Set, s2::Set) - set difference
# Reference: julia/base/abstractset.jl:277
function setdiff(s1::Set, s2::Set)
    result = Set()
    for x in s1
        if !(x in s2)
            result = push!(result, x)
        end
    end
    return result
end

# symdiff(s1::Set, s2::Set) - symmetric difference
# Reference: julia/base/abstractset.jl:318
function symdiff(s1::Set, s2::Set)
    result = Set()
    for x in s1
        if !(x in s2)
            result = push!(result, x)
        end
    end
    for x in s2
        if !(x in s1)
            result = push!(result, x)
        end
    end
    return result
end

# issubset(a::Set, b::Set) - subset check (a ⊆ b)
# Reference: julia/base/abstractset.jl:368
function issubset(a::Set, b::Set)
    for x in a
        if !(x in b)
            return false
        end
    end
    return true
end

# isdisjoint(a::Set, b::Set) - disjoint check
# Reference: julia/base/abstractset.jl:388
function isdisjoint(a::Set, b::Set)
    for x in a
        if x in b
            return false
        end
    end
    return true
end

# issetequal(a::Set, b::Set) - set equality
# Reference: julia/base/abstractset.jl:500
function issetequal(a::Set, b::Set)
    length(a) == length(b) && issubset(a, b)
end

# =============================================================================
# In-place Set operations - Pure Julia (Issue #2575)
# Reference: julia/base/abstractset.jl
# =============================================================================

# union!(s::Set, itr) - add all elements from itr to s
# Reference: julia/base/abstractset.jl:56
function union!(s::Set, itr)
    for x in itr
        s = push!(s, x)
    end
    return s
end

# intersect!(s::Set, itr) - keep only elements also in itr
# Reference: julia/base/abstractset.jl:172
function intersect!(s::Set, itr::Set)
    for x in s
        if !(x in itr)
            s = delete!(s, x)
        end
    end
    return s
end

# setdiff!(s::Set, itr) - remove elements found in itr
# Reference: julia/base/abstractset.jl:293
function setdiff!(s::Set, itr)
    for x in itr
        s = delete!(s, x)
    end
    return s
end

# symdiff!(s::Set, itr::Set) - symmetric difference in-place
# Reference: julia/base/abstractset.jl:341
function symdiff!(s::Set, itr::Set)
    for x in itr
        if x in s
            s = delete!(s, x)
        else
            s = push!(s, x)
        end
    end
    return s
end

# =============================================================================
# Array (Vector) Set algebra - Pure Julia (Issue #3724)
# Reference: julia/base/abstractset.jl
#
# These mirror the Set-typed methods above but accept any iterable Vector and
# return a Vector preserving insertion order. Membership uses linear search
# (`isequal` semantics via `in`) so any element type the VM can compare works
# — including Float64 (which the internal `Set` / `DictKey` does not yet
# support; Issue #3724 follow-up). Same-element Vector methods allocate via
# `similar(a, 0)` so the returned Vector preserves `T` without falling back
# to `Vector{Any}` (Issue #4018).
# =============================================================================

# Internal helper: linear-search membership using `isequal` (handles NaN, Float64,
# and any element type with isequal/== defined). Mirrors `_unique_into!` style.
function _vec_contains(v, x)
    for y in v
        if isequal(x, y)
            return true
        end
    end
    return false
end

function _union_vector_into!(result, a, b)
    for x in a
        if !_vec_contains(result, x)
            push!(result, x)
        end
    end
    for x in b
        if !_vec_contains(result, x)
            push!(result, x)
        end
    end
    return result
end

function _intersect_vector_into!(result, a, b)
    for x in a
        if _vec_contains(b, x) && !_vec_contains(result, x)
            push!(result, x)
        end
    end
    return result
end

function _setdiff_vector_into!(result, a, b)
    for x in a
        if !_vec_contains(b, x) && !_vec_contains(result, x)
            push!(result, x)
        end
    end
    return result
end

function _symdiff_vector_into!(result, a, b)
    for x in a
        if !_vec_contains(b, x) && !_vec_contains(result, x)
            push!(result, x)
        end
    end
    for x in b
        if !_vec_contains(a, x) && !_vec_contains(result, x)
            push!(result, x)
        end
    end
    return result
end

function _mixed_vector_set_result(a::Vector, b::Vector)
    T = promote_type(eltype(a), eltype(b))
    return _array_undef_from_dims(T, (0,))
end

function union(a::Vector, b::Vector)
    return _union_vector_into!(_mixed_vector_set_result(a, b), a, b)
end

function intersect(a::Vector, b::Vector)
    return _intersect_vector_into!(_mixed_vector_set_result(a, b), a, b)
end

function setdiff(a::Vector, b::Vector)
    return _setdiff_vector_into!(_mixed_vector_set_result(a, b), a, b)
end

function symdiff(a::Vector, b::Vector)
    return _symdiff_vector_into!(_mixed_vector_set_result(a, b), a, b)
end

union(a::Vector{Int64}, b::Vector{Int64}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Float64}, b::Vector{Float64}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Bool}, b::Vector{Bool}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{String}, b::Vector{String}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Char}, b::Vector{Char}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Int8}, b::Vector{Int8}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Int16}, b::Vector{Int16}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Int32}, b::Vector{Int32}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{UInt8}, b::Vector{UInt8}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{UInt16}, b::Vector{UInt16}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{UInt32}, b::Vector{UInt32}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{UInt64}, b::Vector{UInt64}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Float32}, b::Vector{Float32}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Symbol}, b::Vector{Symbol}) = _union_vector_into!(similar(a, 0), a, b)
union(a::Vector{Any}, b::Vector{Any}) = _union_vector_into!(similar(a, 0), a, b)

intersect(a::Vector{Int64}, b::Vector{Int64}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Float64}, b::Vector{Float64}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Bool}, b::Vector{Bool}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{String}, b::Vector{String}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Char}, b::Vector{Char}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Int8}, b::Vector{Int8}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Int16}, b::Vector{Int16}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Int32}, b::Vector{Int32}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{UInt8}, b::Vector{UInt8}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{UInt16}, b::Vector{UInt16}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{UInt32}, b::Vector{UInt32}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{UInt64}, b::Vector{UInt64}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Float32}, b::Vector{Float32}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Symbol}, b::Vector{Symbol}) = _intersect_vector_into!(similar(a, 0), a, b)
intersect(a::Vector{Any}, b::Vector{Any}) = _intersect_vector_into!(similar(a, 0), a, b)

setdiff(a::Vector{Int64}, b::Vector{Int64}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Float64}, b::Vector{Float64}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Bool}, b::Vector{Bool}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{String}, b::Vector{String}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Char}, b::Vector{Char}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Int8}, b::Vector{Int8}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Int16}, b::Vector{Int16}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Int32}, b::Vector{Int32}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{UInt8}, b::Vector{UInt8}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{UInt16}, b::Vector{UInt16}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{UInt32}, b::Vector{UInt32}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{UInt64}, b::Vector{UInt64}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Float32}, b::Vector{Float32}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Symbol}, b::Vector{Symbol}) = _setdiff_vector_into!(similar(a, 0), a, b)
setdiff(a::Vector{Any}, b::Vector{Any}) = _setdiff_vector_into!(similar(a, 0), a, b)

symdiff(a::Vector{Int64}, b::Vector{Int64}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Float64}, b::Vector{Float64}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Bool}, b::Vector{Bool}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{String}, b::Vector{String}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Char}, b::Vector{Char}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Int8}, b::Vector{Int8}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Int16}, b::Vector{Int16}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Int32}, b::Vector{Int32}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{UInt8}, b::Vector{UInt8}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{UInt16}, b::Vector{UInt16}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{UInt32}, b::Vector{UInt32}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{UInt64}, b::Vector{UInt64}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Float32}, b::Vector{Float32}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Symbol}, b::Vector{Symbol}) = _symdiff_vector_into!(similar(a, 0), a, b)
symdiff(a::Vector{Any}, b::Vector{Any}) = _symdiff_vector_into!(similar(a, 0), a, b)

function issubset(a::Vector, b::Vector)
    for x in a
        if !_vec_contains(b, x)
            return false
        end
    end
    return true
end

function isdisjoint(a::Vector, b::Vector)
    for x in a
        if _vec_contains(b, x)
            return false
        end
    end
    return true
end

function issetequal(a::Vector, b::Vector)
    issubset(a, b) && issubset(b, a)
end

# =============================================================================
# Mixed Set/Vector forms (Issue #3724)
# These let calls like `isdisjoint(Set([1]), [2])` and `union(Set([1]), [2])`
# resolve through Pure Julia method dispatch even when the Set/DictKey backend
# cannot represent the Vector's element type (e.g., Float64).
# =============================================================================

function _set_to_vector(s::Set)
    return collect(s)
end

union(s::Set, v::Vector) = union(s, Set(v))
union(v::Vector, s::Set) = union(v, _set_to_vector(s))
intersect(s::Set, v::Vector) = intersect(s, Set(v))
intersect(v::Vector, s::Set) = intersect(v, _set_to_vector(s))
setdiff(s::Set, v::Vector) = setdiff(s, Set(v))
setdiff(v::Vector, s::Set) = setdiff(v, _set_to_vector(s))
symdiff(s::Set, v::Vector) = symdiff(s, Set(v))
symdiff(v::Vector, s::Set) = symdiff(v, _set_to_vector(s))
issubset(s::Set, v::Vector) = issubset(_set_to_vector(s), v)
issubset(v::Vector, s::Set) = issubset(v, _set_to_vector(s))
isdisjoint(s::Set, v::Vector) = isdisjoint(_set_to_vector(s), v)
isdisjoint(v::Vector, s::Set) = isdisjoint(v, _set_to_vector(s))
issetequal(s::Set, v::Vector) = issetequal(_set_to_vector(s), v)
issetequal(v::Vector, s::Set) = issetequal(v, _set_to_vector(s))

# =============================================================================
# Array utility functions (set-like operations on arrays)
# =============================================================================

# unique: return array with duplicate elements removed.
#
# Implementation note (Issues #3580, #3581, #3586):
#   The previous implementation pre-allocated `result = zeros(count)` which
#   hard-coded the result element type to Float64 and rejected non-numeric
#   inputs. We now use single-pass push!-based accumulation. To preserve
#   the input element type (Issue #3580) we provide concrete-type method
#   specializations for the common Vector{T} types — each starts the
#   result from a typed empty literal `T[]`, which the VM allocates as a
#   typed array. Additional concrete Vector paths now route through
#   `similar(arr, 0)` so newer element types do not fall back to `Vector{Any}`
#   (Issue #4018). Abstract typed literals such as `Real[...]` are still
#   tracked separately (Issue #4586).

# Internal helper: scan `arr`, push! unique elements (by `isequal`) into
# the (already-typed) `result`. Returns `result` so that callers can
# write `unique(arr) = _unique_into!(T[], arr)`.
function _unique_into!(result, arr)
    n = length(arr)
    for i in 1:n
        is_unique = true
        for j in 1:(i-1)
            if isequal(arr[i], arr[j])
                is_unique = false
                break
            end
        end
        if is_unique
            push!(result, arr[i])
        end
    end
    return result
end

# Type-preserving specializations (Issue #3580). Each method seeds the
# result with a typed empty literal so the returned `Vector{T}` matches
# the input element type.
unique(arr::Vector{Int64}) = _unique_into!(Int64[], arr)
unique(arr::Vector{Float64}) = _unique_into!(Float64[], arr)
unique(arr::Vector{Bool}) = _unique_into!(Bool[], arr)
unique(arr::Vector{String}) = _unique_into!(String[], arr)
unique(arr::Vector{Char}) = _unique_into!(Char[], arr)
unique(arr::Vector{Int8}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{Int16}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{Int32}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{UInt8}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{UInt16}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{UInt32}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{UInt64}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{Float32}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{Symbol}) = _unique_into!(similar(arr, 0), arr)
unique(arr::Vector{Any}) = _unique_into!(similar(arr, 0), arr)

function unique(arr)
    return _unique_into!([], arr)
end

# unique(f, itr): return elements from itr unique by f(x)
# Based on Julia's base/set.jl:301
# Returns elements of itr for which f(x) is unique (keeps first occurrence).
# Same single-pass push! approach (Issues #3581, #3586) and the same
# dispatch-based type-preservation workaround as `unique(arr)` above
# (Issues #3580, #3648).

function _unique_f_into!(result, f, arr)
    n = length(arr)
    for i in 1:n
        is_unique = true
        for j in 1:(i-1)
            if isequal(f(arr[i]), f(arr[j]))
                is_unique = false
                break
            end
        end
        if is_unique
            push!(result, arr[i])
        end
    end
    return result
end

unique(f::Function, arr::Vector{Int64}) = _unique_f_into!(Int64[], f, arr)
unique(f::Function, arr::Vector{Float64}) = _unique_f_into!(Float64[], f, arr)
unique(f::Function, arr::Vector{Bool}) = _unique_f_into!(Bool[], f, arr)
unique(f::Function, arr::Vector{String}) = _unique_f_into!(String[], f, arr)
unique(f::Function, arr::Vector{Char}) = _unique_f_into!(Char[], f, arr)
unique(f::Function, arr::Vector{Int8}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{Int16}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{Int32}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{UInt8}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{UInt16}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{UInt32}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{UInt64}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{Float32}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{Symbol}) = _unique_f_into!(similar(arr, 0), f, arr)
unique(f::Function, arr::Vector{Any}) = _unique_f_into!(similar(arr, 0), f, arr)

function unique(f::Function, arr)
    return _unique_f_into!([], f, arr)
end

# allunique: check if all elements in array are unique (isequal for NaN/missing)
function allunique(arr)
    n = length(arr)
    for i in 1:n
        for j in (i+1):n
            if isequal(arr[i], arr[j])
                return false
            end
        end
    end
    return true
end

# allequal: check if all elements in array are equal (isequal so NaN equals NaN)
function allequal(arr)
    n = length(arr)
    if n <= 1
        return true
    end
    first = arr[1]
    for i in 2:n
        if !isequal(arr[i], first)
            return false
        end
    end
    return true
end

# =============================================================================
# unique! - Remove duplicate elements in-place
# =============================================================================
# Based on Julia's base/set.jl:470
#
# unique!(A) removes duplicate elements from A in-place, preserving order.
# Returns the modified array.

function unique!(arr)
    n = length(arr)
    if n <= 1
        return arr
    end

    # j is the position where we write the next unique element
    j = 1

    for i in 2:n
        # Check if arr[i] is already in the "seen" part (arr[1:j])
        # (use isequal so NaN==NaN per Julia set semantics)
        is_duplicate = false
        for k in 1:j
            if isequal(arr[i], arr[k])
                is_duplicate = true
                break
            end
        end

        # If not a duplicate, keep it
        if !is_duplicate
            j = j + 1
            if j != i
                arr[j] = arr[i]
            end
        end
    end

    # Resize to keep only unique elements
    resize!(arr, j)
    return arr
end

# =============================================================================
# in! - Check membership and insert if not present
# =============================================================================
# Based on Julia's base/set.jl:125
#
# in!(x, s) checks if x is in s. If not, it inserts x and returns false.
# If x is already in s, returns true without modifying s.

function in!(x, s::Set)
    if x in s
        return true
    else
        push!(s, x)
        return false
    end
end

# =============================================================================
# copy(s::Set) - shallow copy of a Set
# Reference: julia/base/set.jl line 166
# =============================================================================

# copy(s::Set) = copymutable(s)  (julia/base/set.jl:166-170)
# `copymutable(s::Set{T}) = Set{T}(s)` builds a fresh, independent Set that
# preserves the element type. We call `_set_with_eltype(T, s)` directly (rather
# than `Set{T}(s)`) so the fresh backing Dict is built by `union!`; a bare
# `Set{T}(s)` with a `where`-bound `T` would route to the default field
# constructor and alias `s` as the backing dict instead of copying it.
copy(s::Set{T}) where {T} = _set_with_eltype(T, s)

# =============================================================================
# empty(s::Set) - create empty Set of same type
# Reference: julia/base/set.jl line 126
# =============================================================================

# empty(s::AbstractSet{T}, ::Type{U}=T) where {T,U} = Set{U}()
# Creates an empty Set. Type parameter handling is simplified here.
empty(s::Set) = Set()
