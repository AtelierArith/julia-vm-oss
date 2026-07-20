# =============================================================================
# channels.jl - Channel type for producer/consumer patterns
# =============================================================================
# Based on Julia's base/channels.jl
#
# This implements Channel on SubsetJuliaVM's VM-owned cooperative task
# continuations. Blocking operations park only the current task.

# =============================================================================
# Channel Type
# =============================================================================

# Internal helper functions for channel operations
# These work around SubsetJuliaVM's limitations with push!/popfirst! on struct fields

"""
    Channel{T}(size::Integer=0)

Construct a typed `Channel{T}` with an internal buffer that can hold a maximum of `size` objects.
`Channel(size)` (without type parameter) creates a `Channel{Any}`.

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
    waiters::Any            # (put_waiters, take_waiters), both Vector{Any}

    function Channel{T}(sz::Integer=0) where T
        if sz < 0
            throw(ArgumentError("Channel size must be either 0, a positive integer or Inf"))
        end
        return new{T}(:open, nothing, Any[], sz, (Any[], Any[]))
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
        # An unbuffered Channel has no storage capacity, matching upstream.
        return true
    end
    return length(c.data) >= c.sz_max
end

"""
    isready(c::Channel)

Determine whether a channel has a value available to take without blocking.
Returns `true` if a value is available without blocking.
"""
isready(c::Channel) = length(c.data) > 0

"""
    isempty(c::Channel)

Determine whether a channel has no available values.
"""
isempty(c::Channel) = length(c.data) == 0

function _wake_channel_takers(c::Channel)
    take_waiters = c.waiters[2]
    if length(take_waiters) > 0
        waiters = take_waiters
        waiter = waiters[1]
        c.waiters = (c.waiters[1], waiters[2:end])
        _task_wake(waiter.vm_id)
    end
    return nothing
end

function _wake_channel_putters(c::Channel)
    put_waiters = c.waiters[1]
    if length(put_waiters) > 0
        waiters = put_waiters
        waiter = waiters[1]
        c.waiters = (waiters[2:end], c.waiters[2])
        _task_wake(waiter.vm_id)
    end
    return nothing
end

function _wake_all_channel_waiters(c::Channel)
    for waiter in c.waiters[1]
        _task_wake(waiter.vm_id)
    end
    for waiter in c.waiters[2]
        _task_wake(waiter.vm_id)
    end
    c.waiters = (Any[], Any[])
    return nothing
end

# =============================================================================
# Close Channel
# =============================================================================

"""
    close(c::Channel)

Close a channel.
"""
function close(c::Channel)
    c.state = :closed
    _wake_all_channel_waiters(c)
    return nothing
end

"""
    close(c::Channel, excp::Exception)

Close a channel with an exception.
"""
function close(c::Channel, excp::Exception)
    c.state = :closed
    c.excp = excp
    _wake_all_channel_waiters(c)
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

Append an item `v` to the channel `c`. A full buffered channel parks the
calling task until a `take!` makes room. An unbuffered channel performs a true
rendezvous: the producer resumes only after a consumer takes the value.
"""
function put!(c::Channel, v)
    check_channel_state(c)

    # Buffered producers wait for capacity. For an unbuffered rendezvous, only
    # one producer may publish a value at a time; later producers keep `v` in
    # their suspended frame until the preceding handshake completes.
    while isopen(c) &&
            ((isbuffered(c) && isfull(c)) || (!isbuffered(c) && !isempty(c.data)))
        c.waiters = (vcat(c.waiters[1], [current_task()]), c.waiters[2])
        _task_park()
    end
    check_channel_state(c)

    d = c.data
    d = vcat(d, [v])
    c.data = d
    _wake_channel_takers(c)

    if !isbuffered(c)
        while isopen(c) && !isempty(c.data)
            c.waiters = (vcat(c.waiters[1], [current_task()]), c.waiters[2])
            _task_park()
        end
        check_channel_state(c)
        # The consumer woke the producer whose value it consumed. That
        # producer hands the rendezvous slot to the next FIFO putter.
        _wake_channel_putters(c)
    end

    return v
end

"""
    take!(c::Channel)

Remove and return a value from a Channel, parking until one is available.
"""
function take!(c::Channel)
    while isopen(c) && isempty(c.data)
        c.waiters = (c.waiters[1], vcat(c.waiters[2], [current_task()]))
        _task_park()
    end

    if !isempty(c.data)
        d = c.data
        result = d[1]
        c.data = d[2:end]
        _wake_channel_putters(c)
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
    throw(InvalidStateException("Channel is empty.", :empty))
end

"""
    fetch(c::Channel)

Get the first available item from the Channel without removing it.
Parks until a value is available, without removing it.
"""
function fetch(c::Channel)
    while isopen(c) && isempty(c.data)
        c.waiters = (c.waiters[1], vcat(c.waiters[2], [current_task()]))
        _task_park()
    end

    if !isempty(c.data)
        return c.data[1]
    end

    if !isopen(c)
        excp = c.excp
        if excp !== nothing
            throw(excp)
        end
        throw(InvalidStateException("Channel is closed.", :closed))
    end
    throw(InvalidStateException("Channel is empty.", :empty))
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
    if !isopen(c) && isempty(c)
        return nothing
    end
    return (take!(c), nothing)
end

function iterate(c::Channel, state)
    if !isopen(c) && isempty(c)
        return nothing
    end
    return (take!(c), nothing)
end

"""
    length(c::Channel)

Return the number of immediately available items.
"""
length(c::Channel) = length(c.data)

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

Remove all buffered items and wake producers waiting for space. Returns `c`.
"""
function empty!(c::Channel)
    c.data = Any[]
    while length(c.waiters[1]) > 0
        _wake_channel_putters(c)
    end
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
wait(t)
isopen(c)  # false
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

Create a channel of size `sz` and run `func(channel)` on a scheduled Task.
The channel is closed automatically when the producer task completes or fails.

Enables the do-block producer pattern:

```julia
c = Channel(10) do ch
    for i in 1:5
        put!(ch, i)
    end
end
# The producer is live; consuming the Channel drives it to completion.
collect(c) == [1, 2, 3, 4, 5]
```
"""
function _channel_with_task(c::Channel, func::Function)
    t = Task(() -> func(c))
    bind(c, t)
    schedule(t)
    return c
end

function Channel(func::Function, sz::Integer=0)
    return _channel_with_task(Channel(sz), func)
end

"""
    Channel{T}(func::Function, sz)

Typed do-block producer constructor (Issue #10353), mirroring upstream
`Channel{T}(f::Function, size=0)`: the producer task fills a `Channel{T}`.
The size parameter carries no default here: the default-argument expansion
would synthesize an arity-1 `Channel{T}(func::Function)` method that the
parametric-constructor dispatcher currently confuses with the arity-1 inner
`Channel{T}(sz::Integer)` constructor.
"""
function Channel{T}(func::Function, sz::Integer) where {T}
    return _channel_with_task(Channel{T}(sz), func)
end

function Channel{T}(func::Function, sz::Float64) where {T}
    sz_int = (sz == Inf ? typemax(Int) : convert(Int, sz))
    return Channel{T}(func, sz_int)
end

"""
    Channel(func::Function, sz::Float64)

Float64 variant of the producer constructor (supports `Inf` as size).
"""
function Channel(func::Function, sz::Float64)
    sz_int = (sz == Inf ? typemax(Int) : convert(Int, sz))
    return Channel(func, sz_int)
end
