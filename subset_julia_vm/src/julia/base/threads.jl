# =============================================================================
# threads.jl - Base.Threads single-thread compatibility surface
# =============================================================================
# Based on Julia's base/threads.jl and base/threadingconstructs.jl.
#
# SubsetJuliaVM VM instances are intentionally single-threaded
# (docs/vm/SINGLE_THREADED_VM.md). This module provides the API surface that
# upstream Julia exposes even when started with one thread.

module Threads

export nthreads, threadid, maxthreadid
export Atomic, atomic_add!, atomic_xchg!
export SpinLock, lock, unlock, trylock, islocked

nthreads() = 1
nthreads(_pool) = 1
threadid() = 1
maxthreadid() = 1
maxthreadid(_pool) = 1

mutable struct Atomic{T}
    value::T
end

getindex(a::Atomic) = a.value

function setindex!(a::Atomic{T}, value) where {T}
    a.value = convert(T, value)
    return a.value
end

function atomic_add!(a::Atomic{T}, value) where {T}
    old = a.value
    a.value = convert(T, old + value)
    return old
end

function atomic_xchg!(a::Atomic{T}, value) where {T}
    old = a.value
    a.value = convert(T, value)
    return old
end

mutable struct SpinLock
    locked::Bool

    function SpinLock()
        new(false)
    end
end

function lock(l::SpinLock)
    if l.locked
        error("SpinLock: Deadlock detected")
    end
    l.locked = true
    return nothing
end

function unlock(l::SpinLock)
    if !l.locked
        error("SpinLock: Not locked")
    end
    l.locked = false
    return nothing
end

function trylock(l::SpinLock)
    if l.locked
        return false
    end
    l.locked = true
    return true
end

islocked(l::SpinLock) = l.locked

end # module Threads
