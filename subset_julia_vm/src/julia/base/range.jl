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

# isempty for LinRange
function isempty(r::LinRange)
    return r.len == 0
end

# collect for LinRange
function collect(r::LinRange)
    result = Float64[]
    for x in r
        push!(result, x)
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

# isempty for StepRangeLen
function isempty(r::StepRangeLen)
    return r.len == 0
end

# collect for StepRangeLen
function collect(r::StepRangeLen)
    result = Float64[]
    for x in r
        push!(result, x)
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
# Julia: function range_start_step_length(a, step, len::Integer)
# Returns a lazy StepRangeLen for compatibility with Julia.
function range_start_step_length(start, step, len)
    return StepRangeLen(start * 1.0, step * 1.0, Int64(len), 1)
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

# first: get the first element of a collection
function first(arr)
    return arr[1]
end

# first(arr, n): get the first n elements of a collection
# Based on Julia's base/abstractarray.jl:505
function first(arr, n::Int64)
    if n < 0
        throw(ArgumentError("Number of elements must be non-negative"))
    end
    len = length(arr)
    m = min(n, len)
    return arr[1:m]
end

# last: get the last element of a collection
function last(arr)
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

# isempty for OneTo
function isempty(r::OneTo)
    return r.stop == 0
end

# collect for OneTo
function collect(r::OneTo)
    result = Int64[]
    for x in r
        push!(result, x)
    end
    return result
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

# collect for LogRange
function collect(r::LogRange)
    result = Float64[]
    for x in r
        push!(result, x)
    end
    return result
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
