# =============================================================================
# lock.jl - Synchronization Primitives
# =============================================================================
# Based on Julia's base/lock.jl
#
# This implements simplified lock types for SubsetJuliaVM's
# cooperative single-threaded model.

# =============================================================================
# AbstractLock
# =============================================================================

"""
    AbstractLock

Abstract supertype for all lock types.
"""
abstract type AbstractLock end

# =============================================================================
# ReentrantLock
# =============================================================================

"""
    ReentrantLock()

Create a reentrant lock for synchronizing Tasks.
"""
mutable struct ReentrantLock <: AbstractLock
    locked::Bool
    reentrancy_cnt::Int64

    function ReentrantLock()
        new(false, 0)
    end
end

# =============================================================================
# Lock Operations
# =============================================================================

"""
    lock(lk::ReentrantLock)

Acquire the lock.
"""
function lock(lk::ReentrantLock)
    if lk.locked
        cnt = lk.reentrancy_cnt
        lk.reentrancy_cnt = cnt + 1
    else
        lk.locked = true
        lk.reentrancy_cnt = 1
    end
    return nothing
end

"""
    unlock(lk::ReentrantLock)

Release the lock.
"""
function unlock(lk::ReentrantLock)
    if !lk.locked
        error("unlock: lock is not locked")
    end

    cnt = lk.reentrancy_cnt
    lk.reentrancy_cnt = cnt - 1
    if lk.reentrancy_cnt == 0
        lk.locked = false
    end
    return nothing
end

"""
    trylock(lk::ReentrantLock) -> Bool

Try to acquire the lock without blocking.
"""
function trylock(lk::ReentrantLock)
    if lk.locked
        cnt = lk.reentrancy_cnt
        lk.reentrancy_cnt = cnt + 1
        return true
    else
        lk.locked = true
        lk.reentrancy_cnt = 1
        return true
    end
end

"""
    islocked(lk::ReentrantLock) -> Bool

Check if the lock is currently held.
"""
islocked(lk::ReentrantLock) = lk.locked

# =============================================================================
# Lock with Function
# =============================================================================

"""
    lock(f::Function, lk::AbstractLock)

Acquire the lock, execute `f`, and release the lock.
"""
function lock(f::Function, lk::AbstractLock)
    lock(lk)
    try
        return f()
    finally
        unlock(lk)
    end
end

# =============================================================================
# Condition
# =============================================================================

"""
    Condition()

Create a condition variable.
"""
mutable struct Condition
    waiting::Int64

    function Condition()
        new(0)
    end
end

"""
    wait(c::Condition)

Wait for a notification (not supported in cooperative model).
"""
function wait(c::Condition)
    error("wait(Condition): Cannot block in cooperative model")
end

"""
    notify(c::Condition)

Notify waiting tasks (no-op in cooperative model).
"""
function notify(c::Condition)
    return 0
end

function notify(c::Condition; all::Bool=true)
    return 0
end

# =============================================================================
# SpinLock
# =============================================================================

"""
    SpinLock()

Create a non-reentrant spin lock.
"""
mutable struct SpinLock <: AbstractLock
    locked::Bool

    function SpinLock()
        new(false)
    end
end

"""
    lock(l::SpinLock)

Acquire the spin lock.
"""
function lock(l::SpinLock)
    if l.locked
        error("SpinLock: Deadlock detected")
    end
    l.locked = true
    return nothing
end

"""
    unlock(l::SpinLock)

Release the spin lock.
"""
function unlock(l::SpinLock)
    if !l.locked
        error("SpinLock: Not locked")
    end
    l.locked = false
    return nothing
end

"""
    trylock(l::SpinLock) -> Bool

Try to acquire the spin lock.
"""
function trylock(l::SpinLock)
    if l.locked
        return false
    end
    l.locked = true
    return true
end

islocked(l::SpinLock) = l.locked
