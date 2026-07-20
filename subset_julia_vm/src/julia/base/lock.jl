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
    waitq::Vector{Any}
    value::Any

    function Condition()
        new(0, Any[], nothing)
    end
end

"""
    wait(c::Condition)

Park the current task until a notification wakes its continuation.
"""
function _wait_condition(c::Condition)
    waiter = current_task()
    c.waitq = vcat(c.waitq, [waiter])
    c.waiting = c.waiting + 1
    _task_park()
    c.waiting = c.waiting - 1
    return c.value
end

wait(c::Condition) = _wait_condition(c)

"""
    notify(c::Condition)

Notify waiting tasks through the VM scheduler.
"""
notify(c::Condition) = notify(c, nothing)

notify(c::Condition; all::Bool=true) = notify(c, nothing; all=all)

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

# =============================================================================
# @lock - acquire a lock for the duration of a block
# =============================================================================
# Macro version of `lock(f, l::AbstractLock)` but with `expr` instead of `f` function.
# Expands to:
#     lock(l)
#     try
#         expr
#     finally
#         unlock(l)
#     end
#
# The lock is always released, even when an exception is thrown from `expr`.
# `l` is evaluated only once. Mirrors official Julia base/lock.jl.
#
# Usage:
#   lk = ReentrantLock()
#   @lock lk begin
#       # critical section
#   end
#
#   @lock lk x = 1   # single expression body
#
# Issue #3499.
macro lock(l, expr)
    quote
        temp = $(esc(l))
        lock(temp)
        try
            $(esc(expr))
        finally
            unlock(temp)
        end
    end
end
