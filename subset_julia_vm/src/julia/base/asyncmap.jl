# =============================================================================
# asyncmap.jl - Async/parallel version of map (Issue #3500)
# =============================================================================
# Workaround: sequential implementation pending real Task scheduler (Issue #3502 / Issue #3500).
# Based on Julia's base/asyncmap.jl
#
# `asyncmap(f, c...; ntasks=0, batch_size=nothing)` is the async/parallel
# variant of `map`. SubsetJuliaVM's cooperative single-threaded task model
# (see task.jl) executes scheduled tasks immediately, so this implementation
# is functionally equivalent to `map`. We deliberately do not allocate a
# `Task` per element here — the prelude lowering pipeline does not yet
# support arrow-function expressions inside Base function bodies, and the
# observable result is identical because tasks would run inline anyway.
#
# Differences from upstream Julia:
#   * No true parallelism (cooperative single thread).
#   * `ntasks` may be supplied as a non-negative `Number`; the zero-arg
#     `Function` form is not supported and raises `ArgumentError`.
#   * `batch_size` follows upstream: if supplied, `f` receives a `Vector` of
#     argument tuples (one per input element of the batch) and must return a
#     `Vector` of results of the same length.

# -----------------------------------------------------------------------------
# Argument validation (mirrors upstream Julia)
# -----------------------------------------------------------------------------

function _asyncmap_verify_batch_size(batch_size)
    if batch_size === nothing
        return nothing
    elseif isa(batch_size, Number)
        bs = Int(batch_size)
        if bs < 1
            throw(ArgumentError(string(
                "batch_size must be specified as a positive integer. batch_size=",
                batch_size,
            )))
        end
        return bs
    else
        throw(ArgumentError(string(
            "batch_size must be specified as a positive integer. batch_size=",
            batch_size,
        )))
    end
end

function _asyncmap_verify_ntasks(ntasks)
    if !(isa(ntasks, Number) && ntasks >= 0)
        throw(ArgumentError(string(
            "ntasks must be specified as a non-negative integer. ntasks=", ntasks,
        )))
    end
    return Int(ntasks)
end

# -----------------------------------------------------------------------------
# Validate batched function output (shared between 1- and 2-collection forms)
# -----------------------------------------------------------------------------
function _asyncmap_validate_batch_result(out, expected_len)
    if !isa(out, AbstractVector)
        throw(ArgumentError(string(
            "asyncmap with batch_size: batch function must return a Vector, got ",
            typeof(out),
        )))
    end
    if length(out) != expected_len
        throw(ArgumentError(string(
            "asyncmap with batch_size: batch function returned ", length(out),
            " results for a batch of ", expected_len,
        )))
    end
    return out
end

# -----------------------------------------------------------------------------
# Public entry points (single- and two-collection forms)
# -----------------------------------------------------------------------------

"""
    asyncmap(f, c; ntasks=0, batch_size=nothing)
    asyncmap(f, c1, c2; ntasks=0, batch_size=nothing)

Apply `f` to each element of the collection(s) using cooperative tasks.

In SubsetJuliaVM the cooperative single-threaded task model means tasks run
to completion as soon as they are scheduled, so this returns the same result
as [`map`](@ref) and accepts the `ntasks` / `batch_size` keyword arguments.

`ntasks` must be a non-negative `Number`; the value `0` selects the default.
The zero-arg `Function` form supported by upstream Julia is not implemented.

If `batch_size` is supplied, `f` must accept a `Vector` of argument tuples
and return a `Vector` of results of the same length.

# Examples
```jldoctest
julia> asyncmap(x -> x*2, [1, 2, 3])
3-element Vector{Int64}:
 2
 4
 6

julia> asyncmap(+, [1, 2, 3], [10, 20, 30]; ntasks=2)
3-element Vector{Int64}:
 11
 22
 33
```

See also: [`map`](@ref), [`Task`](@ref), [`schedule`](@ref).
"""
function asyncmap(f, a; ntasks=0, batch_size=nothing)
    _asyncmap_verify_ntasks(ntasks)
    bs = _asyncmap_verify_batch_size(batch_size)
    if bs === nothing
        results = []
        for x in a
            push!(results, f(x))
        end
        return results
    else
        # Materialise the input so we can index in fixed-size chunks.
        items = []
        for x in a
            push!(items, x)
        end
        n = length(items)
        results = []
        i = 1
        while i <= n
            j = min(i + bs - 1, n)
            batch = []
            k = i
            while k <= j
                push!(batch, (items[k],))
                k += 1
            end
            out = f(batch)
            _asyncmap_validate_batch_result(out, length(batch))
            for v in out
                push!(results, v)
            end
            i = j + 1
        end
        return results
    end
end

function asyncmap(f, a, b; ntasks=0, batch_size=nothing)
    _asyncmap_verify_ntasks(ntasks)
    bs = _asyncmap_verify_batch_size(batch_size)
    if bs === nothing
        results = []
        iter = iterate(zip(a, b))
        while iter !== nothing
            (pair, state) = iter
            push!(results, f(pair[1], pair[2]))
            iter = iterate(zip(a, b), state)
        end
        return results
    else
        # Materialise pairs as 2-tuples so they can be sliced into batches.
        items = []
        iter = iterate(zip(a, b))
        while iter !== nothing
            (pair, state) = iter
            push!(items, (pair[1], pair[2]))
            iter = iterate(zip(a, b), state)
        end
        n = length(items)
        results = []
        i = 1
        while i <= n
            j = min(i + bs - 1, n)
            batch = []
            k = i
            while k <= j
                push!(batch, items[k])
                k += 1
            end
            out = f(batch)
            _asyncmap_validate_batch_result(out, length(batch))
            for v in out
                push!(results, v)
            end
            i = j + 1
        end
        return results
    end
end
