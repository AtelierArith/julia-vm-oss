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
    Channel(size::Int=0)

Construct a `Channel` with an internal buffer that can hold a maximum of `size` objects.

Note: In SubsetJuliaVM's cooperative model, blocking operations may cause errors
since there is no true task scheduler. Use buffered channels for best results.

# Examples
```julia
ch = Channel(10)  # Buffered channel with capacity 10
put!(ch, 1)
put!(ch, 2)
take!(ch)  # returns 1
take!(ch)  # returns 2
```
"""
mutable struct Channel
    state::Symbol           # :open or :closed
    excp                    # exception to be thrown when state !== :open
    data::Vector{Any}       # buffer for stored items
    sz_max::Int             # maximum size of channel (0 = unbuffered)

    function Channel(sz::Integer=0)
        if sz < 0
            throw(ArgumentError("Channel size must be either 0, a positive integer or Inf"))
        end
        return new(:open, nothing, Any[], sz)
    end
end

# Float64 constructor for Inf
function Channel(sz::Float64)
    sz_int = (sz == Inf ? typemax(Int) : convert(Int, sz))
    return Channel(sz_int)
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
isfull(c::Channel) = length(c.data) >= c.sz_max

"""
    isready(c::Channel)

Determine whether a channel has a value stored in it.
"""
isready(c::Channel) = length(c.data) > 0

"""
    isempty(c::Channel)

Determine whether a channel has no values stored in it.
"""
isempty(c::Channel) = length(c.data) == 0

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

Append an item `v` to the channel `c`.
"""
function put!(c::Channel, v)
    check_channel_state(c)

    # Check if channel is full (for buffered channels)
    if isbuffered(c) && isfull(c)
        throw(InvalidStateException("Channel is full. In cooperative model, cannot block.", :full))
    end

    # For unbuffered channels (sz_max == 0), in cooperative model we allow storing one item
    if !isbuffered(c) && length(c.data) > 0
        throw(InvalidStateException("Unbuffered channel already has pending value. Cannot block in cooperative model.", :full))
    end

    # Work around SubsetJuliaVM limitation: copy data, modify, reassign
    d = c.data
    d = vcat(d, [v])
    c.data = d

    return v
end

"""
    take!(c::Channel)

Remove and return a value from a Channel.
"""
function take!(c::Channel)
    if isempty(c)
        if !isopen(c)
            excp = c.excp
            if excp !== nothing
                throw(excp)
            end
            throw(InvalidStateException("Channel is closed.", :closed))
        end
        throw(InvalidStateException("Channel is empty. In cooperative model, cannot block.", :empty))
    end

    # Work around SubsetJuliaVM limitation: get first element and remove it
    d = c.data
    result = d[1]
    c.data = d[2:end]

    return result
end

"""
    fetch(c::Channel)

Get the first available item from the Channel without removing it.
"""
function fetch(c::Channel)
    if isempty(c)
        if !isopen(c)
            excp = c.excp
            if excp !== nothing
                throw(excp)
            end
            throw(InvalidStateException("Channel is closed.", :closed))
        end
        throw(InvalidStateException("Channel is empty. In cooperative model, cannot block.", :empty))
    end

    # Return first item without removal
    return c.data[1]
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

Return the number of items currently in the channel buffer.
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
# Bind (Stub)
# =============================================================================

"""
    bind(c::Channel, task::Task)

Associate a task with a channel.

Note: This is a stub for API compatibility.
"""
function bind(c::Channel, t::Task)
    return nothing
end
