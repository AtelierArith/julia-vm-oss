# =============================================================================
# channels.jl - Channel type for producer/consumer patterns
# =============================================================================
# Based on Julia's base/channels.jl
#
# This implements a simplified Channel type for SubsetJuliaVM's
# cooperative multitasking model.
#
# Note: SubsetJuliaVM has limitations with struct field operations.
# This implementation works around those limitations.

# =============================================================================
# Channel Type
# =============================================================================

# Internal helper functions for channel operations
# These work around SubsetJuliaVM's limitations with push!/popfirst! on struct fields

"""
    Channel{T}(size::Integer=0)

Construct a typed `Channel{T}` with an internal buffer that can hold a maximum of `size` objects.
`Channel(size)` (without type parameter) creates a `Channel{Any}`.

Note: In SubsetJuliaVM's cooperative model, blocking operations may cause errors
since there is no true task scheduler. Use buffered channels for best results.

# Examples
```julia
ch = Channel{Int}(10)  # Typed buffered channel with capacity 10
put!(ch, 1)
put!(ch, 2)
take!(ch)  # returns 1

ch2 = Channel(5)  # Channel{Any} with capacity 5
```
"""
mutable struct Channel{T}
    state::Symbol           # :open or :closed
    excp                    # exception to be thrown when state !== :open
    data::Vector{Any}       # buffer for stored items (untyped for VM compatibility)
    sz_max::Int             # maximum size of channel (0 = unbuffered)
    pending_puts::Vector{Any}  # overflow queue: values queued when buffer is full (Issue #3451)

    function Channel{T}(sz::Integer=0) where T
        if sz < 0
            throw(ArgumentError("Channel size must be either 0, a positive integer or Inf"))
        end
        return new{T}(:open, nothing, Any[], sz, Any[])
    end
end

# Outer constructor: no type param → Channel{Any}
function Channel(sz::Integer=0)
    return Channel{Any}(sz)
end

# Float64 constructor for Inf
function Channel(sz::Float64)
    sz_int = (sz == Inf ? typemax(Int) : convert(Int, sz))
    return Channel{Any}(sz_int)
end

# =============================================================================
# Channel State Functions
# =============================================================================

"""
    isopen(c::Channel)

Determine whether a channel is open (can still accept values via `put!`).
"""
isopen(c::Channel) = c.state === :open

"""
    isbuffered(c::Channel)

Determine whether a channel has a buffer.
"""
isbuffered(c::Channel) = c.sz_max > 0

"""
    isfull(c::Channel)

Determine whether a channel is full.
"""
function isfull(c::Channel)
    if !isbuffered(c)
        return length(c.data) > 0
    end
    return length(c.data) >= c.sz_max
end

"""
    isready(c::Channel)

Determine whether a channel has a value available to take without blocking.
Returns `true` if the buffer or pending queue is non-empty.
"""
isready(c::Channel) = length(c.data) > 0 || length(c.pending_puts) > 0

"""
    isempty(c::Channel)

Determine whether a channel has no values (buffer and pending queue both empty).
"""
isempty(c::Channel) = length(c.data) == 0 && length(c.pending_puts) == 0

# =============================================================================
# Close Channel
# =============================================================================

"""
    close(c::Channel)

Close a channel.
"""
function close(c::Channel)
    c.state = :closed
    return nothing
end

"""
    close(c::Channel, excp::Exception)

Close a channel with an exception.
"""
function close(c::Channel, excp::Exception)
    c.state = :closed
    c.excp = excp
    return nothing
end

# =============================================================================
# Put and Take Operations
# =============================================================================

# Helper to check channel state
function check_channel_state(c::Channel)
    if !isopen(c)
        excp = c.excp
        if excp !== nothing
            throw(excp)
        end
        throw(InvalidStateException("Channel is closed.", :closed))
    end
end

"""
    put!(c::Channel, v)

Append an item `v` to the channel `c`. If the buffer is full, the value is queued
in a pending overflow queue and will be drained into the buffer on the next `take!`.
This approximates blocking semantics within SubsetJuliaVM's cooperative model (Issue #3451).
"""
function put!(c::Channel, v)
    check_channel_state(c)

    # Buffer full for a buffered channel: queue in pending overflow
    if isbuffered(c) && isfull(c)
        pq = c.pending_puts
        pq = vcat(pq, [v])
        c.pending_puts = pq
        return v
    end

    # Unbuffered channel: at most one item lives in data at a time; overflow to pending
    if !isbuffered(c) && length(c.data) > 0
        pq = c.pending_puts
        pq = vcat(pq, [v])
        c.pending_puts = pq
        return v
    end

    # Normal path: add directly to buffer
    d = c.data
    d = vcat(d, [v])
    c.data = d

    return v
end

"""
    take!(c::Channel)

Remove and return a value from a Channel. After taking from the buffer, one pending
put (if any) is drained into the buffer, approximating blocking semantics (Issue #3451).
"""
function take!(c::Channel)
    # Buffer has items: take from buffer, then drain one pending put
    if !isempty(c.data)
        d = c.data
        result = d[1]
        c.data = d[2:end]

        if length(c.pending_puts) > 0
            pq = c.pending_puts
            val = pq[1]
            c.pending_puts = pq[2:end]
            nd = c.data
            nd = vcat(nd, [val])
            c.data = nd
        end

        return result
    end

    # Buffer empty but pending queue has items: return directly
    if length(c.pending_puts) > 0
        pq = c.pending_puts
        result = pq[1]
        c.pending_puts = pq[2:end]
        return result
    end

    # Truly empty
    if !isopen(c)
        excp = c.excp
        if excp !== nothing
            throw(excp)
        end
        throw(InvalidStateException("Channel is closed.", :closed))
    end
    throw(InvalidStateException("Channel is empty. In cooperative model, cannot block.", :empty))
end

"""
    fetch(c::Channel)

Get the first available item from the Channel without removing it.
Checks the pending queue when the buffer is empty (Issue #3451).
"""
function fetch(c::Channel)
    if !isempty(c.data)
        return c.data[1]
    end

    if length(c.pending_puts) > 0
        return c.pending_puts[1]
    end

    if !isopen(c)
        excp = c.excp
        if excp !== nothing
            throw(excp)
        end
        throw(InvalidStateException("Channel is closed.", :closed))
    end
    throw(InvalidStateException("Channel is empty. In cooperative model, cannot block.", :empty))
end

# =============================================================================
# Iteration Protocol
# =============================================================================

"""
    iterate(c::Channel)
    iterate(c::Channel, state)

Iterate over a Channel.
"""
function iterate(c::Channel)
    if isempty(c)
        return nothing
    end
    return (take!(c), nothing)
end

function iterate(c::Channel, state)
    if isempty(c)
        return nothing
    end
    return (take!(c), nothing)
end

"""
    length(c::Channel)

Return the number of items currently in the channel buffer plus any pending puts.
"""
length(c::Channel) = length(c.data) + length(c.pending_puts)

# =============================================================================
# Collection Interface
# =============================================================================

"""
    push!(c::Channel, v)

Equivalent to `put!(c, v)`. Returns the channel.
"""
function push!(c::Channel, v)
    put!(c, v)
    return c
end

"""
    popfirst!(c::Channel)

Equivalent to `take!(c)`.
"""
popfirst!(c::Channel) = take!(c)

# =============================================================================
# empty!
# =============================================================================

"""
    empty!(c::Channel)

Remove all items from the channel buffer and pending queue. Returns `c`.
"""
function empty!(c::Channel)
    c.data = Any[]
    c.pending_puts = Any[]
    return c
end

# =============================================================================
# Bind
# =============================================================================

"""
    bind(c::Channel, task::Task)

Associate the channel `c` with `task`. When `task` terminates (successfully or
with failure), `c` is automatically closed. If `task` fails, `c` is closed with
a `TaskFailedException`.

In SubsetJuliaVM's cooperative model, `schedule(task)` must be called after
`bind` to execute the task and trigger the channel close.

# Examples
```julia
c = Channel(10)
t = Task(() -> put!(c, 42))
bind(c, t)
schedule(t)
isopen(c)  # false - closed after task completed
```
"""
function bind(c::Channel, task::Task)
    if istaskdone(task)
        if isopen(c)
            if istaskfailed(task)
                close(c, TaskFailedException(task))
            else
                close(c)
            end
        end
        return c
    end

    if task.storage === nothing
        task.storage = Dict()
    end

    storage = task.storage
    key = :__bound_channels__
    channels = get(storage, key, nothing)
    if channels === nothing
        channels = Any[]
        storage[key] = channels
    end
    push!(channels, c)

    return c
end

# =============================================================================
# Producer Constructor
# =============================================================================

"""
    Channel(func::Function, sz::Integer=0)

Create a buffered channel of size `sz` and execute `func(channel)` synchronously.
The channel is closed automatically when the function completes (or throws).

Enables the do-block producer pattern:

```julia
c = Channel(10) do ch
    for i in 1:5
        put!(ch, i)
    end
end
# c has 5 items buffered; isopen(c) == false
```
"""
function Channel(func::Function, sz::Integer=0)
    c = Channel(sz)
    try
        func(c)
        if isopen(c)
            close(c)
        end
    catch e
        if isopen(c)
            close(c)
        end
        rethrow()
    end
    return c
end

"""
    Channel(func::Function, sz::Float64)

Float64 variant of the producer constructor (supports `Inf` as size).
"""
function Channel(func::Function, sz::Float64)
    sz_int = (sz == Inf ? typemax(Int) : convert(Int, sz))
    return Channel(func, sz_int)
end
