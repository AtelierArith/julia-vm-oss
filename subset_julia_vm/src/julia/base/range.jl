# =============================================================================
# Range - Range utilities
# =============================================================================
# Based on Julia's base/range.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
# Removed functions (not in Julia Base):
#   - linspace (deprecated in Julia, use range(start, stop, length=n))
#   - logspace (not in Julia, was NumPy/MATLAB)
#   - geomspace (not in Julia, was NumPy)
#   - isin (use `in` or `∈`)
#   - stepsize (renamed to step)
#   - range_length (internal)

# VM-native range collect bridges.
#
# Upstream Julia defines `collect(r::AbstractRange) = Array(r)`. sjulia still
# represents parser-created range values as VM-native `Value::Range`, so use
# the same public `_collect` trait path for numeric UnitRange/StepRange slices
# that can run without field access on a Julia struct. Runtime `Any` non-unit
# ranges still retain a VM-native materialization fallback until they have safe
# Julia wrappers.
function _collect_vm_range(r)
    n = length(r)
    if n == 0
        if isa(r, StepRange{Int64,Int64})
            return _array_undef_from_dims(Int64, (0,))
        end
        return []
    end
    if isa(r, StepRange{Int64,Int64})
        result = _array_undef_from_dims(Int64, (n,))
        for i in 1:n
            result[i] = Int64(r[i])
        end
        return result
    end
    first_value = r[1]
    if isa(first_value, Int64)
        result = _array_undef_from_dims(Int64, (n,))
    elseif isa(first_value, Float64)
        result = _array_undef_from_dims(Float64, (n,))
    elseif isa(first_value, Float32)
        result = _array_undef_from_dims(Float32, (n,))
    else
        result = _array_undef_from_dims(Any, (n,))
    end
    result[1] = first_value
    for i in 2:n
        value = r[i]
        result[i] = value
    end
    return result
end

function _collect_vm_range_as(::Type{T}, r) where {T}
    n = length(r)
    if n == 0
        if T === Int64
            return _array_undef_from_dims(Int64, (0,))
        elseif T === Float64
            return _array_undef_from_dims(Float64, (0,))
        elseif T === Float32
            return _array_undef_from_dims(Float32, (0,))
        end
        return []
    end
    if T === Int64
        result = _array_undef_from_dims(Int64, (n,))
        for i in 1:n
            result[i] = Int64(r[i])
        end
        return result
    elseif T === Float64
        result = _array_undef_from_dims(Float64, (n,))
        for i in 1:n
            result[i] = Float64(r[i])
        end
        return result
    elseif T === Float32
        result = _array_undef_from_dims(Float32, (n,))
        for i in 1:n
            result[i] = Float32(r[i])
        end
        return result
    end
    return _collect_vm_range(r)
end

function collect(r::UnitRange{T}) where {T}
    return _collect_vm_range_as(T, r)
end

function collect(r::UnitRange)
    return _collect_vm_range(r)
end

function collect(r::StepRange{T,S}) where {T,S}
    return _collect_vm_range_as(T, r)
end

function collect(r::StepRange)
    return _collect_vm_range(r)
end

# eltype for the parametric range types (Issue #5116).  Upstream derives these
# from `eltype(::Type{<:AbstractArray{E}}) where {E} = E`
# (julia/base/abstractarray.jl:246), since every `AbstractRange <: AbstractArray`.
# The VM cannot bind a covariant `::Type{<:AbstractArray{E}}` type parameter, so
# `UnitRange{T}` / `StepRange{T,S}` are written as concrete-parametric methods
# that the dispatcher resolves; the value forms delegate through `typeof`.
eltype(::Type{UnitRange{T}}) where {T} = T
eltype(r::UnitRange) = eltype(typeof(r))
eltype(::Type{StepRange{T,S}}) where {T,S} = T
eltype(r::StepRange) = eltype(typeof(r))

# `reverse` of a range is itself a (lazy) range, not a materialized Vector —
# `reverse(1:5) === 5:-1:1`, `reverse(1:2:9) === 9:-2:1`. Without this method the
# generic `reverse(arr)` collects the range first (Issue #5661). The colon
# operator reconstructs the appropriate range type (StepRange for integers,
# StepRangeLen for floats) from the reversed endpoints and negated step.
reverse(r::AbstractRange) = last(r):(-step(r)):first(r)

# ==(r::AbstractRange, ...) — element-wise equality (Issue #5666). Ranges are
# `AbstractArray`s in Julia, so `==` compares element-wise: `1:5 == 1:5`,
# `1:1:5 == 1:5`, and `1:5 == [1,2,3,4,5]` are all true. The compiler routes a
# `==`/`!=` with a range operand to these methods (the numeric fast path cannot
# coerce a `Range`); a range compared with a non-array scalar falls back to
# identity (`false`).
function ==(r::AbstractRange, s::AbstractRange)
    if length(r) != length(s)
        return false
    end
    for i in 1:length(r)
        if !(r[i] == s[i])
            return false
        end
    end
    return true
end

function ==(r::AbstractRange, a::AbstractArray)
    if length(r) != length(a)
        return false
    end
    for i in 1:length(r)
        if !(r[i] == a[i])
            return false
        end
    end
    return true
end

==(a::AbstractArray, r::AbstractRange) = (r == a)

# Range predicate count follows upstream `count(f, itr)` semantics through
# iteration. Keep this public path in Julia; the VM `CountFunc` range branch is
# only a compatibility fallback for old bytecode.
function count(f::Function, r::AbstractRange)
    n = 0
    for x in r
        if f(x)
            n = n + 1
        end
    end
    return n
end

# =============================================================================
# LinRange - linearly spaced range defined by start, stop, and length
# =============================================================================
# Based on Julia's base/range.jl
#
# LinRange{T,L} represents a range with `len` linearly spaced elements
# between `start` and `stop`. Unlike StepRange, the spacing is controlled
# by length rather than step.

struct LinRange{T<:Real}
    start::T
    stop::T
    len::Int64
    lendiv::Int64
end

# Constructor with type inference
function LinRange(start, stop, len::Int64)
    if len < 0
        error("LinRange: negative length")
    end
    if len == 1 && start != stop
        error("LinRange: endpoints differ with length=1")
    end
    lendiv = max(len - 1, Int64(1))
    T = typeof((stop - start) / 1)
    return LinRange{T}(T(start), T(stop), len, lendiv)
end

# Constructor with integer len conversion
function LinRange(start, stop, len::Integer)
    return LinRange(start, stop, Int64(len))
end

# length for LinRange
function length(r::LinRange)
    return r.len
end

# first element
function first(r::LinRange)
    return r.start
end

# last element
function last(r::LinRange)
    return r.stop
end

# step for LinRange (computed, not stored)
function step(r::LinRange)
    return (r.stop - r.start) / r.lendiv
end

# Linear interpolation helper for LinRange indexing
function _linrange_getindex(r::LinRange, i::Int64)
    # lerp formula: (1 - t) * start + t * stop where t = (i-1) / lendiv
    if r.len == 0
        error("BoundsError: attempt to access empty LinRange")
    end
    if i < 1 || i > r.len
        error("BoundsError: attempt to access LinRange at index $i")
    end
    if r.len == 1
        return r.start
    end
    t = (i - 1) / r.lendiv
    return (1 - t) * r.start + t * r.stop
end

# getindex for LinRange
function getindex(r::LinRange, i::Int64)
    return _linrange_getindex(r, i)
end

function getindex(r::LinRange, i::Integer)
    return _linrange_getindex(r, Int64(i))
end

# iterate for LinRange (following the iteration protocol)
function iterate(r::LinRange)
    if r.len == 0
        return nothing
    end
    return (_linrange_getindex(r, 1), 1)
end

function iterate(r::LinRange, state::Int64)
    next_i = state + 1
    if next_i > r.len
        return nothing
    end
    return (_linrange_getindex(r, next_i), next_i)
end

function iterate(r::LinRange, state::Integer)
    return iterate(r, Int64(state))
end

# size for LinRange (1D collection)
function size(r::LinRange)
    return (r.len,)
end

function IteratorSize(::Type{LinRange{T}}) where {T}
    return HasShape{1}()
end

function IteratorSize(r::LinRange)
    return IteratorSize(typeof(r))
end

function IteratorEltype(::Type{LinRange{T}}) where {T}
    return HasEltype()
end

function IteratorEltype(r::LinRange)
    return IteratorEltype(typeof(r))
end

function eltype(::Type{LinRange{T}}) where {T}
    return T
end

function eltype(r::LinRange)
    return eltype(typeof(r))
end

# isempty for LinRange
function isempty(r::LinRange)
    return r.len == 0
end

# collect for LinRange
function collect(r::LinRange)
    n = length(r)
    T = eltype(r)
    tname = string(T)
    if tname == "Int64"
        result = _array_undef_from_dims(Int64, (n,))
    elseif tname == "Float64"
        result = _array_undef_from_dims(Float64, (n,))
    elseif tname == "Float32"
        result = _array_undef_from_dims(Float32, (n,))
    else
        result = _array_undef_from_dims(Any, (n,))
    end
    for i in 1:n
        value = r[i]
        result[i] = value
    end
    return result
end

# =============================================================================
# StepRangeLen - range parameterized by reference value, step, and length
# =============================================================================
# Based on Julia's base/range.jl
#
# StepRangeLen{T,R,S} represents a range where r[i] = ref + (i - offset) * step.
# The reference value `ref` is the value at index `offset` (default 1).
# This type is useful for floating-point ranges where step is exact.

struct StepRangeLen{T<:Real}
    ref::T
    step::T
    len::Int64
    offset::Int64
end

# Constructor with type inference
function StepRangeLen(ref, step, len::Int64, offset::Int64)
    if len < 0
        error("StepRangeLen: negative length")
    end
    maxoffset = max(Int64(1), len)
    if offset < 1 || offset > maxoffset
        error("StepRangeLen: offset must be in [1,$maxoffset], got $offset")
    end
    T = typeof(ref + zero(step))
    return StepRangeLen{T}(T(ref), T(step), len, offset)
end

# Constructor with default offset
function StepRangeLen(ref, step, len::Int64)
    return StepRangeLen(ref, step, len, Int64(1))
end

# Constructor with integer conversion
function StepRangeLen(ref, step, len::Integer, offset::Integer)
    return StepRangeLen(ref, step, Int64(len), Int64(offset))
end

# Constructor with integer len and default offset
function StepRangeLen(ref, step, len::Integer)
    return StepRangeLen(ref, step, Int64(len), Int64(1))
end

# length for StepRangeLen
function length(r::StepRangeLen)
    return r.len
end

# first element
function first(r::StepRangeLen)
    return r.ref + (1 - r.offset) * r.step
end

# last element
function last(r::StepRangeLen)
    return r.ref + (r.len - r.offset) * r.step
end

# step for StepRangeLen
function step(r::StepRangeLen)
    return r.step
end

# Internal getindex
function _steprangelen_getindex(r::StepRangeLen, i::Int64)
    if r.len == 0
        error("BoundsError: attempt to access empty StepRangeLen")
    end
    if i < 1 || i > r.len
        error("BoundsError: attempt to access StepRangeLen at index $i")
    end
    return r.ref + (i - r.offset) * r.step
end

# getindex for StepRangeLen
function getindex(r::StepRangeLen, i::Int64)
    return _steprangelen_getindex(r, i)
end

function getindex(r::StepRangeLen, i::Integer)
    return _steprangelen_getindex(r, Int64(i))
end

# iterate for StepRangeLen (following the iteration protocol)
function iterate(r::StepRangeLen)
    if r.len == 0
        return nothing
    end
    return (_steprangelen_getindex(r, 1), 1)
end

function iterate(r::StepRangeLen, state::Int64)
    next_i = state + 1
    if next_i > r.len
        return nothing
    end
    return (_steprangelen_getindex(r, next_i), next_i)
end

function iterate(r::StepRangeLen, state::Integer)
    return iterate(r, Int64(state))
end

# size for StepRangeLen (1D collection)
function size(r::StepRangeLen)
    return (r.len,)
end

function IteratorSize(::Type{StepRangeLen{T}}) where {T}
    return HasShape{1}()
end

function IteratorSize(r::StepRangeLen)
    return IteratorSize(typeof(r))
end

function IteratorEltype(::Type{StepRangeLen{T}}) where {T}
    return HasEltype()
end

function IteratorEltype(r::StepRangeLen)
    return IteratorEltype(typeof(r))
end

function eltype(::Type{StepRangeLen{T}}) where {T}
    return T
end

function eltype(r::StepRangeLen)
    return eltype(typeof(r))
end

# isempty for StepRangeLen
function isempty(r::StepRangeLen)
    return r.len == 0
end

# collect for StepRangeLen
function collect(r::StepRangeLen)
    n = length(r)
    T = eltype(r)
    tname = string(T)
    if tname == "Int64"
        result = _array_undef_from_dims(Int64, (n,))
    elseif tname == "Float64"
        result = _array_undef_from_dims(Float64, (n,))
    elseif tname == "Float32"
        result = _array_undef_from_dims(Float32, (n,))
    else
        result = _array_undef_from_dims(Any, (n,))
    end
    for i in 1:n
        value = r[i]
        result[i] = value
    end
    return result
end

# =============================================================================
# range: construct evenly spaced arrays
# =============================================================================
# Based on Julia's base/range.jl
#
# Implementation that returns lazy Range types (LinRange, StepRangeLen)
# for better compatibility with Julia.
#
# Supported call patterns (matching Julia's API):
#   range(start, stop, length::Integer)  - positional args
#   range(start, stop; length=N)         - keyword arg
#   range(start; stop=s, length=N)       - keyword args

# range(start, stop, length) - positional args version
# Julia: range(start, stop, length::Integer) = _range(start, nothing, stop, length)
function range(start, stop, length::Int64)
    return _range(start, nothing, stop, length)
end

# range(start, stop; length=N, step=S) - two positional + keyword args version
# Julia: range(start, stop; length=nothing, step=nothing) = _range(start, step, stop, length)
function range(start, stop; length=nothing, step=nothing)
    return _range(start, step, stop, length)
end

# range(start; stop=S, length=N, step=S) - one positional + keyword args version
# Julia: range(start; stop=nothing, length=nothing, step=nothing) = _range(start, step, stop, length)
function range(start; stop=nothing, length=nothing, step=nothing)
    return _range(start, step, stop, length)
end

# =============================================================================
# _range: dispatcher function
# =============================================================================
# Julia uses multiple dispatch on Nothing vs Any for 16 combinations.
# We implement the subset we support.

# _range(start, step, stop, len) - main dispatcher
function _range(start, step, stop, len)
    if start === nothing && step === nothing && stop === nothing && len === nothing
        error("range requires at least one argument")
    elseif start !== nothing && step === nothing && stop !== nothing && len === nothing
        # range(start, stop) - use step=1
        return range_start_stop(start, stop)
    elseif start !== nothing && step === nothing && stop !== nothing && len !== nothing
        # range(start, stop; length=N) or range(start, stop, length)
        return range_start_stop_length(start, stop, len)
    elseif start !== nothing && step !== nothing && stop === nothing && len !== nothing
        # range(start; step=s, length=N)
        return range_start_step_length(start, step, len)
    elseif start !== nothing && step !== nothing && stop !== nothing && len === nothing
        # range(start; step=s, stop=s)
        return range_start_step_stop(start, step, stop)
    elseif start !== nothing && step === nothing && stop === nothing && len !== nothing
        # range(start; length=N) => start:(start+len-1)
        return range_start_length(start, len)
    elseif start !== nothing && step !== nothing && stop !== nothing && len !== nothing
        error("range: too many arguments specified (start, step, stop, and length)")
    else
        error("invalid arguments to range")
    end
end

# =============================================================================
# range_* helper functions (matching Julia's naming)
# =============================================================================

# range_start_stop(start, stop) - equivalent to start:stop
# Julia: range_start_stop(start, stop) = start:stop
# Returns a lazy UnitRange instead of materialized array.
function range_start_stop(start, stop)
    return start:stop
end

# range_start_stop_length(start, stop, len) - the core implementation
# Julia: range_start_stop_length(start, stop, len::Integer) = LinRange(start, stop, len)
# Returns a lazy LinRange for compatibility with Julia.
function range_start_stop_length(start, stop, len)
    return LinRange(start, stop, Int64(len))
end

# range_start_step_length(start, step, len) - start and step with length
# (Issue #5135) Upstream returns `StepRange{typeof(stop),typeof(step)}` when the
# recomputed stop is Signed and a `StepRangeLen` otherwise
# (julia/base/range.jl:222-229). For integer `start`/`step` the previous
# `StepRangeLen(start * 1.0, step * 1.0, ...)` form silently float-promoted the
# element type, so `range(1, step=2, length=5)` produced `[1.0, 3.0, ...]`
# instead of the integer `[1, 3, 5, 7, 9]`. Route the all-integer case through
# the VM colon operator (`start:step:stop`), which materializes the correct
# `StepRange{Int64,Int64}` with integer elements. Non-integer arguments keep the
# existing float `StepRangeLen` representation, matching upstream values there.
function range_start_step_length(start, step, len)
    n = Int64(len)
    if isa(start, Integer) && isa(step, Integer)
        stop = start + step * (n - 1)
        return start:step:stop
    end
    return StepRangeLen(start * 1.0, step * 1.0, n, 1)
end

# range_start_length(start, len) - start and length, step=1
# Julia: range_start_length(a, len::Integer) = a:(a + len - 1) for integers
function range_start_length(start, len)
    stop = start + (Int64(len) - 1)
    return start:stop
end

# range_start_step_stop(start, step, stop) - equivalent to start:step:stop
# Julia: range_start_step_stop(start, step, stop) = start:step:stop
# Returns a lazy StepRange directly.
function range_start_step_stop(start, step, stop)
    return (start * 1.0):(step * 1.0):(stop * 1.0)
end

# first: get the first element of a collection.
# Special-case empty Range (Issue #3734 follow-up): the generic `arr[1]`
# raises BoundsError on `1:0` even though Julia semantics define
# `first(1:0) == 1` (returns `r.start`). Pure Julia cannot read `r.start`
# directly on a `Value::Range`, so route through the `_tuple_first`
# internal alias (BuiltinId::TupleFirst Rust handler) for that case only.
# We test `AbstractRange` rather than `UnitRange` because the parametric
# `isa(r, UnitRange)` only matches with the exact element-type parameter
# whereas `AbstractRange` covers all empty Range values.
function first(arr)
    if isa(arr, AbstractRange) && length(arr) == 0
        return _tuple_first(arr)
    end
    return arr[1]
end

# first(arr, n): get the first n elements of a collection
# Based on Julia's base/abstractarray.jl:505
function first(arr, n::Int64)
    if n < 0
        throw(ArgumentError("Number of elements must be non-negative"))
    end
    # Indexable collections slice directly so the result type is preserved
    # (`first(1:10, 3) === 1:3`, `first("hello", 3) === "hel"`).
    if isa(arr, AbstractArray) || isa(arr, AbstractRange) || isa(arr, AbstractString)
        len = length(arr)
        m = min(n, len)
        return arr[1:m]
    end
    # General iterators (tuples, generators, Iterators.cycle, …) are not
    # indexable: take the first `n` by iteration, returning a Vector — matching
    # upstream `first(itr, n)` (Issue #5750).
    return collect(Iterators.take(arr, n))
end

# last: get the last element of a collection.
# Special-case empty Range (Issue #3734 follow-up): see `first(arr)`.
function last(arr)
    if isa(arr, AbstractRange) && length(arr) == 0
        return _tuple_last(arr)
    end
    return arr[length(arr)]
end

# last(arr, n): get the last n elements of a collection
# Based on Julia's base/abstractarray.jl:557-559
function last(arr, n::Int64)
    if n < 0
        throw(ArgumentError("Number of elements must be non-negative"))
    end
    len = length(arr)
    m = min(n, len)
    return arr[(len - m + 1):len]
end

# step: get the step of a range (for arrays, returns 1)
# Note: For actual Range values, this is handled by VM
function step(arr)
    if length(arr) < 2
        return 1
    end
    return arr[2] - arr[1]
end

# eachindex: create indices for array iteration
function eachindex(arr)
    return 1:length(arr)
end

# firstindex: get the first valid index (always 1 in Julia)
# INTENTIONAL_NOOP (Issue #4703): upstream
# `firstindex(a::AbstractArray) = first(eachindex(IndexLinear(), a))`
# (julia/base/abstractarray.jl:452). sjulia has no OffsetArrays, so every
# supported AbstractArray is 1-based and the constant `return 1` matches
# the upstream result for all values this method sees.
function firstindex(arr)
    return 1
end

# firstindex with dimension: get the first valid index along dimension d (Issue #2349)
# Used for dimension-aware begin keyword: m[begin, end] -> m[firstindex(m, 1), lastindex(m, 2)]
function firstindex(arr, d::Int64)
    return first(axes(arr, d))
end

function firstindex(arr, d::Integer)
    return firstindex(arr, Int64(d))
end

# lastindex: get the last valid index
function lastindex(arr)
    return length(arr)
end

# lastindex with dimension: get the last valid index along dimension d (Issue #2349)
# Used for dimension-aware end keyword: m[begin, end] -> m[firstindex(m, 1), lastindex(m, 2)]
function lastindex(arr, d::Int64)
    return last(axes(arr, d))
end

function lastindex(arr, d::Integer)
    return lastindex(arr, Int64(d))
end

# isempty: check if collection is empty
function isempty(arr)
    return length(arr) == 0
end

# =============================================================================
# OneTo - AbstractUnitRange that behaves like 1:n
# =============================================================================
# Based on Julia's base/range.jl:470-492
#
# OneTo(n) represents a range that behaves like 1:n, with the added
# distinction that the lower limit is guaranteed (by the type system) to be 1.
# This is commonly used for array indices.
#
# Examples:
#   OneTo(5)     # represents 1:5
#   OneTo(0)     # empty range (1:0)
#   oneto(5)     # equivalent to OneTo(5)

struct OneTo
    stop::Int64
end

# Constructor ensuring non-negative stop - generic version
# Works with any numeric type (Int64, Float64, etc.)
function OneTo(n)
    return OneTo(max(Int64(0), Int64(floor(n))))
end

# oneto function - convenience constructor matching Julia's API
function oneto(n)
    return OneTo(n)
end

# length for OneTo
function length(r::OneTo)
    return r.stop
end

# first element - always 1
function first(r::OneTo)
    return Int64(1)
end

# last element
function last(r::OneTo)
    return r.stop
end

# step for OneTo - always 1
function step(r::OneTo)
    return Int64(1)
end

# getindex for OneTo
function getindex(r::OneTo, i::Int64)
    if r.stop == 0
        error("BoundsError: attempt to access empty OneTo")
    end
    if i < 1 || i > r.stop
        error("BoundsError: attempt to access OneTo at index $i")
    end
    return i
end

function getindex(r::OneTo, i::Integer)
    return getindex(r, Int64(i))
end

# Range slicing (Issue #5751): indexing a range with a range index returns a
# sub-range, preserving the step (`(1:10)[1:3] === 1:3`, `(1:2:20)[1:3] === 1:2:5`,
# reverse indices like `(1:10)[3:-1:1]` give `3:-1:1`). Indexing with a single Int
# is handled by the per-range-type `getindex` methods above.
function getindex(r::AbstractRange, inds::AbstractRange)
    n = length(inds)
    s = step(r) * step(inds)
    if n == 0
        f = first(r) + (first(inds) - 1) * step(r)
        stop = s < 0 ? f + 1 : f - 1
        if s == 1
            return f:stop
        else
            return f:s:stop
        end
    end
    f = r[first(inds)]
    if s == 1
        return f:(f + n - 1)
    else
        return f:s:(f + (n - 1) * s)
    end
end

# Indexing a range with a vector of indices (or a Bool mask) materializes a
# Vector of the selected elements, mirroring `getindex(::AbstractArray,
# ::AbstractVector)` for ordinary arrays. Without this, `(1:10)[[1, 3, 5]]` failed
# with "no method" while `collect(1:10)[[1, 3, 5]]` worked (Issue #5754).
function getindex(r::AbstractRange, inds::AbstractVector{<:Integer})
    return [r[i] for i in inds]
end

function getindex(r::AbstractRange, mask::AbstractVector{Bool})
    return [r[i] for i in 1:length(r) if mask[i]]
end

# iterate for OneTo (following the iteration protocol)
function iterate(r::OneTo)
    if r.stop == 0
        return nothing
    end
    return (Int64(1), Int64(1))
end

function iterate(r::OneTo, state::Int64)
    next_i = state + 1
    if next_i > r.stop
        return nothing
    end
    return (next_i, next_i)
end

function iterate(r::OneTo, state::Integer)
    return iterate(r, Int64(state))
end

# size for OneTo (1D collection)
function size(r::OneTo)
    return (r.stop,)
end

function IteratorSize(::Type{OneTo})
    return HasShape{1}()
end

function IteratorSize(r::OneTo)
    return IteratorSize(typeof(r))
end

function IteratorEltype(::Type{OneTo})
    return HasEltype()
end

function IteratorEltype(r::OneTo)
    return IteratorEltype(typeof(r))
end

function eltype(::Type{OneTo})
    return Int64
end

function eltype(r::OneTo)
    return Int64
end

# isempty for OneTo
function isempty(r::OneTo)
    return r.stop == 0
end

# collect for OneTo
function collect(r::OneTo)
    return _collect(1:1, r, IteratorEltype(r), IteratorSize(r))
end

# eachindex for OneTo
function eachindex(r::OneTo)
    return 1:r.stop
end

# firstindex for OneTo
function firstindex(r::OneTo)
    return 1
end

# lastindex for OneTo
function lastindex(r::OneTo)
    return r.stop
end

# =============================================================================
# LogRange - logarithmically spaced range (Issue #1833)
# =============================================================================
# Based on Julia's base/range.jl (lines 1538-1711)
#
# LogRange{T} represents a range with `len` logarithmically spaced elements
# between `start` and `stop`. Elements form a geometric sequence:
#   r[i] = exp((len-i)/(len-1) * log(start) + (i-1)/(len-1) * log(stop))
#
# The first and last elements are exactly `start` and `stop`.

struct LogRange{T<:Real}
    start::T
    stop::T
    len::Int64
    log_start_div::Float64   # log(start) / (len - 1)
    log_stop_div::Float64    # log(stop)  / (len - 1)
end

# Constructor with validation
function LogRange(start::Real, stop::Real, len::Int64)
    if start <= 0 || stop <= 0
        error("DomainError: LogRange does not accept zero or negative numbers")
    end
    if !isfinite(Float64(start)) || !isfinite(Float64(stop))
        error("DomainError: LogRange is only defined for finite start & stop")
    end
    if len < 0
        error("ArgumentError: LogRange: negative length")
    end
    if len == 1 && start != stop
        error("ArgumentError: LogRange: endpoints differ with length=1")
    end
    T = typeof(Float64(start))
    s = Float64(start)
    e = Float64(stop)
    if len <= 1
        return LogRange{T}(s, e, len, 0.0, 0.0)
    end
    lsd = log(s) / (len - 1)
    led = log(e) / (len - 1)
    return LogRange{T}(s, e, len, lsd, led)
end

function LogRange(start::Real, stop::Real, len::Integer)
    return LogRange(start, stop, Int64(len))
end

# logrange function — main entry point
# Julia: logrange(start, stop, length) = LogRange(start, stop, Int(length))
function logrange(start::Real, stop::Real, length::Integer)
    return LogRange(start, stop, Int64(length))
end

# length for LogRange
function length(r::LogRange)
    return r.len
end

# size for LogRange
function size(r::LogRange)
    return (r.len,)
end

function IteratorSize(r::LogRange)
    return HasShape{1}()
end

function IteratorEltype(r::LogRange)
    return HasEltype()
end

function eltype(r::LogRange)
    return typeof(r.start)
end

function collect(r::LogRange)
    return _collect(1:1, r, IteratorEltype(r), IteratorSize(r))
end

# first element
function first(r::LogRange)
    return r.start
end

# last element
function last(r::LogRange)
    return r.stop
end

# Internal getindex helper
function _logrange_getindex(r::LogRange, i::Int64)
    if r.len == 0
        error("BoundsError: attempt to access empty LogRange")
    end
    if i < 1 || i > r.len
        error("BoundsError: attempt to access LogRange at index $i")
    end
    # Exact endpoints
    if i == 1
        return r.start
    end
    if i == r.len
        return r.stop
    end
    # Logarithmic interpolation:
    # logx = (len - i) * log(start)/(len-1) + (i - 1) * log(stop)/(len-1)
    logx = (r.len - i) * r.log_start_div + (i - 1) * r.log_stop_div
    return exp(logx)
end

# getindex for LogRange
function getindex(r::LogRange, i::Int64)
    return _logrange_getindex(r, i)
end

function getindex(r::LogRange, i::Integer)
    return _logrange_getindex(r, Int64(i))
end

# iterate for LogRange
function iterate(r::LogRange)
    if r.len == 0
        return nothing
    end
    return (_logrange_getindex(r, 1), 1)
end

function iterate(r::LogRange, state::Int64)
    next_i = state + 1
    if next_i > r.len
        return nothing
    end
    return (_logrange_getindex(r, next_i), next_i)
end

function iterate(r::LogRange, state::Integer)
    return iterate(r, Int64(state))
end

# isempty for LogRange
function isempty(r::LogRange)
    return r.len == 0
end

# eachindex for LogRange
function eachindex(r::LogRange)
    return 1:r.len
end

# firstindex for LogRange
function firstindex(r::LogRange)
    return 1
end

# lastindex for LogRange
function lastindex(r::LogRange)
    return r.len
end
