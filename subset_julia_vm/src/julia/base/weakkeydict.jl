# =============================================================================
# WeakKeyDict
# =============================================================================
# Based on Julia's base/weakkeydict.jl.

mutable struct WeakKeyDict{K,V} <: AbstractDict{K,V}
    keys::Vector
    vals::Vector
    lock::ReentrantLock
    dirty::Bool

    function WeakKeyDict{K,V}() where {K,V}
        return new{K,V}(Any[], Any[], ReentrantLock(), false)
    end
end

WeakKeyDict() = WeakKeyDict{Any,Any}()

function WeakKeyDict(ps)
    wkh = WeakKeyDict()
    for p in ps
        wkh[p.first] = p.second
    end
    return wkh
end

function WeakKeyDict{K,V}(ps) where {K,V}
    wkh = WeakKeyDict{K,V}()
    for p in ps
        wkh[p.first] = p.second
    end
    return wkh
end

function _weakkeydict_find_index(wkh::WeakKeyDict, key)
    i = 1
    while i <= length(wkh.keys)
        wr = wkh.keys[i]
        if wr.value === key
            return i
        end
        i += 1
    end
    return 0
end

function _weakkeydict_cleanup_locked(wkh::WeakKeyDict)
    i = length(wkh.keys)
    while i >= 1
        wr = wkh.keys[i]
        if wr.value === nothing
            deleteat!(wkh.keys, i)
            deleteat!(wkh.vals, i)
        end
        i -= 1
    end
    wkh.dirty = false
    return wkh
end

function _weakkeydict_cleanup(wkh::WeakKeyDict)
    lock(wkh.lock)
    try
        return _weakkeydict_cleanup_locked(wkh)
    finally
        unlock(wkh.lock)
    end
end

function setindex!(wkh::WeakKeyDict, v, key)
    isa(key, Nothing) && error("cannot store nothing as a WeakKeyDict key")
    lock(wkh.lock)
    try
        _weakkeydict_cleanup_locked(wkh)
        idx = _weakkeydict_find_index(wkh, key)
        if idx == 0
            push!(wkh.keys, WeakRef(key))
            push!(wkh.vals, v)
        else
            wkh.keys[idx].value = key
            wkh.vals[idx] = v
        end
    finally
        unlock(wkh.lock)
    end
    return wkh
end

function get(wkh::WeakKeyDict, key, default)
    lock(wkh.lock)
    try
        _weakkeydict_cleanup_locked(wkh)
        idx = _weakkeydict_find_index(wkh, key)
        idx == 0 && return default
        return wkh.vals[idx]
    finally
        unlock(wkh.lock)
    end
end

function getindex(wkh::WeakKeyDict, key)
    lock(wkh.lock)
    try
        _weakkeydict_cleanup_locked(wkh)
        idx = _weakkeydict_find_index(wkh, key)
        idx == 0 && throw(KeyError(string(key)))
        return wkh.vals[idx]
    finally
        unlock(wkh.lock)
    end
end

function haskey(wkh::WeakKeyDict, key)
    lock(wkh.lock)
    try
        _weakkeydict_cleanup_locked(wkh)
        return _weakkeydict_find_index(wkh, key) != 0
    finally
        unlock(wkh.lock)
    end
end

function getkey(wkh::WeakKeyDict, key, default)
    lock(wkh.lock)
    try
        _weakkeydict_cleanup_locked(wkh)
        idx = _weakkeydict_find_index(wkh, key)
        idx == 0 && return default
        return wkh.keys[idx].value
    finally
        unlock(wkh.lock)
    end
end

function delete!(wkh::WeakKeyDict, key)
    lock(wkh.lock)
    try
        idx = _weakkeydict_find_index(wkh, key)
        if idx != 0
            deleteat!(wkh.keys, idx)
            deleteat!(wkh.vals, idx)
        end
    finally
        unlock(wkh.lock)
    end
    return wkh
end

function pop!(wkh::WeakKeyDict, key, default)
    lock(wkh.lock)
    try
        _weakkeydict_cleanup_locked(wkh)
        idx = _weakkeydict_find_index(wkh, key)
        idx == 0 && return default
        val = wkh.vals[idx]
        deleteat!(wkh.keys, idx)
        deleteat!(wkh.vals, idx)
        return val
    finally
        unlock(wkh.lock)
    end
end

function pop!(wkh::WeakKeyDict, key)
    sentinel = WeakRef(nothing)
    val = pop!(wkh, key, sentinel)
    val === sentinel && throw(KeyError(string(key)))
    return val
end

function empty!(wkh::WeakKeyDict)
    lock(wkh.lock)
    try
        empty!(wkh.keys)
        empty!(wkh.vals)
        wkh.dirty = false
    finally
        unlock(wkh.lock)
    end
    return wkh
end

function length(wkh::WeakKeyDict)
    _weakkeydict_cleanup(wkh)
    return length(wkh.keys)
end

isempty(wkh::WeakKeyDict) = length(wkh) == 0

function iterate(wkh::WeakKeyDict)
    _weakkeydict_cleanup(wkh)
    return iterate(wkh, 1)
end

function iterate(wkh::WeakKeyDict, state)
    i = state
    while i <= length(wkh.keys)
        key = wkh.keys[i].value
        val = wkh.vals[i]
        next_i = i + 1
        if !(key === nothing)
            return (Pair(key, val), next_i)
        end
        i = next_i
    end
    return nothing
end
