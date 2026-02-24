# =============================================================================
# Dict — Pure Julia implementation (Issue #2572, #2669, #2573, #2747, #2748)
# =============================================================================
# Based on julia/base/dict.jl
#
# This file contains:
# 1. Existing Pure Julia wrappers over internal intrinsics (for Value::Dict)
# 2. Hash table constants and helpers
# 3. Dict{K,V} mutable struct definition
# 4. Core hash table algorithms
# 5. Public API methods for Dict{K,V} struct (with where {K,V})
#
# The Dict{K,V} struct coexists with Value::Dict:
# - Methods with bare ::Dict annotation dispatch on Value::Dict
# - Methods with ::Dict{K,V} where {K,V} dispatch on StructRef Dict instances
# - The compiler intercepts Dict()/Dict{K,V}() with pair/empty args → Value::Dict
# - Non-pair constructor calls fall through to the struct constructor

# =============================================================================
# Existing Pure Julia wrappers over internal intrinsics
# =============================================================================
# These use Rust builtins while Dict remains a Rust-backed type (Value::Dict).

# haskey(d::Dict, key) - check if key exists
haskey(d::Dict, key) = _dict_haskey(d, key)

# get(d::Dict, key, default) - get value, return default if not found
function get(d::Dict, key, default)
    _dict_haskey(d, key) ? _dict_get(d, key) : default
end

# getkey(d::Dict, key, default) - return key if exists, else default
function getkey(d::Dict, key, default)
    _dict_haskey(d, key) ? key : default
end

# =============================================================================
# keys(d::Dict) - return all keys as Tuple (Issue #2669)
# Reference: julia/base/dict.jl
# For non-Dict types (NamedTuple, Array, Tuple), the Rust builtin handles them.
# =============================================================================

keys(d::Dict) = _dict_keys(d)

# =============================================================================
# values(d::Dict) - return all values as Tuple (Issue #2669)
# Reference: julia/base/dict.jl
# For non-Dict types (NamedTuple, Array, Tuple), the Rust builtin handles them.
# =============================================================================

values(d::Dict) = _dict_values(d)

# =============================================================================
# pairs(d::Dict) - return all key-value pairs as Tuple of Tuples (Issue #2669)
# Reference: julia/base/dict.jl
# For non-Dict types (NamedTuple, Array, Tuple), the Rust builtin handles them.
# =============================================================================

pairs(d::Dict) = _dict_pairs(d)

# =============================================================================
# merge(d1::Dict, d2::Dict) - merge two dictionaries (Issue #2573)
# Reference: julia/base/dict.jl
# When keys overlap, d2's values take precedence.
# =============================================================================

function merge(d1::Dict, d2::Dict)
    result = Dict()
    for pair in d1
        result[pair.first] = pair.second
    end
    for pair in d2
        result[pair.first] = pair.second
    end
    return result
end

# =============================================================================
# copy(d::Dict) - shallow copy of a Dict
# Reference: julia/base/dict.jl line 110
# =============================================================================

copy(d::Dict) = merge(d, Dict())

# =============================================================================
# mergewith! / mergewith - merge dicts with a custom combine function
# Reference: julia/base/abstractdict.jl
# =============================================================================

# mergewith!(combine, d1, d2) -> d1
function mergewith!(combine::Function, d1::Dict, d2::Dict)
    for pair in d2
        k = pair.first
        v = pair.second
        if haskey(d1, k)
            d1[k] = combine(d1[k], v)
        else
            d1[k] = v
        end
    end
    return d1
end

# mergewith(combine, d1, d2) -> Dict
function mergewith(combine::Function, d1::Dict, d2::Dict)
    result = copy(d1)
    mergewith!(combine, result, d2)
    return result
end

# =============================================================================
# Hash Table Constants (Issue #2747)
# =============================================================================
# Reference: julia/base/dict.jl:28-29

const maxallowedprobe = 16
const maxprobeshift   = 6

# Slot states:
#   0       = empty
#   127     = deleted/missing
#   128-255 = filled (128 | shorthash7)

# =============================================================================
# Hash Table Helper Functions
# =============================================================================

# _tablesz(x) - round up to next power of 2, minimum 16
# Reference: julia/base/abstractdict.jl:580
function _tablesz(x)
    x < 16 && return 16
    return 1 << (64 - leading_zeros(x - 1))
end

# _shorthash7(hsh) - extract 7 MSBs and set bit 7
# Reference: julia/base/dict.jl:122
# hash() returns Int64; use >>> (logical right shift)
# Result: 128-255 (bit 7 always set), stored as Int64
function _shorthash7(hsh)
    return (hsh >>> 57) | 128
end

# hashindex(key, sz) - compute slot index and short hash
# Reference: julia/base/dict.jl:127-132
# sz must be a power of 2; returns (1-based index, shorthash7)
function hashindex(key, sz)
    hsh = hash(key)
    idx = (hsh & (sz - 1)) + 1
    return idx, _shorthash7(hsh)
end

# =============================================================================
# Dict{K,V} mutable struct definition (Issue #2748)
# =============================================================================
# Based on Julia's base/dict.jl (Julia 1.11+)
#
# Fields are untyped (like Array{T}) to avoid parametric field type limitations.
# The compiler intercepts Dict()/Dict{K,V}() with empty/pair args to create
# Value::Dict. Non-pair constructor args fall through to this struct.

mutable struct Dict{K,V} <: AbstractDict{K,V}
    slots     # Vector{Int64} - slot metadata (0=empty, 127=deleted, 128+=filled)
    keys      # Vector{Any}   - keys storage
    vals      # Vector{Any}   - values storage
    ndel      # Int64 - number of deleted entries
    count     # Int64 - number of active entries
    age       # Int64 - modification counter
    idxfloor  # Int64 - smallest index that might be occupied
    maxprobe  # Int64 - max probe distance used
end

# =============================================================================
# Constructor helper
# =============================================================================

# Create an empty Dict{K,V} struct with initial capacity
function _new_dict_kv(n)
    sz = _tablesz(n)
    slots = fill!(Vector{Int64}(undef, sz), 0)
    ks = Vector{Any}(undef, sz)
    vs = Vector{Any}(undef, sz)
    return Dict{Any,Any}(slots, ks, vs, 0, 0, 0, 1, 0)
end

# =============================================================================
# Core Hash Table Algorithms (Issue #2747, #2748)
# =============================================================================
# IMPORTANT: All field+index access uses local variables to avoid
# UnsupportedAssignmentTarget errors. Compound assignments on struct
# fields use explicit form (h.count = h.count + 1).

# --- ht_keyindex(h, key) - find key index, return -1 if not found ---
# Reference: julia/base/dict.jl:238-260
function ht_keyindex(h, key)
    h.count == 0 && return -1
    _slots = h.slots
    _keys = h.keys
    sz = length(_keys)
    iter = 0
    maxprb = h.maxprobe
    index, sh = hashindex(key, sz)
    while true
        si = _slots[index]
        si == 0 && return -1
        if sh == si
            k = _keys[index]
            if (key === k || isequal(key, k))
                return index
            end
        end
        index = (index & (sz - 1)) + 1
        iter = iter + 1
        iter > maxprb && return -1
    end
end

# --- ht_keyindex2!(h, key) - find insertion slot ---
# Reference: julia/base/dict.jl:267-319
# Returns (index, sh):
#   index > 0: key found at index
#   index < 0: key not found, insert at -index
function ht_keyindex2!(h, key)
    _keys = h.keys
    sz = length(_keys)
    if sz == 0
        rehash!(h, 4)
        _keys2 = h.keys
        sz2 = length(_keys2)
        index, sh = hashindex(key, sz2)
        return -index, sh
    end
    iter = 0
    maxprb = h.maxprobe
    index, sh = hashindex(key, sz)
    avail = 0
    _slots = h.slots
    while true
        si = _slots[index]
        if si == 0
            if avail < 0
                return avail, sh
            else
                return -index, sh
            end
        end
        if si == 127
            if avail == 0
                avail = -index
            end
        elseif si == sh
            k = _keys[index]
            if key === k || isequal(key, k)
                return index, sh
            end
        end
        index = (index & (sz - 1)) + 1
        iter = iter + 1
        iter > maxprb && break
    end
    avail < 0 && return avail, sh
    maxallowed = max(maxallowedprobe, sz >> maxprobeshift)
    while iter < maxallowed
        si = _slots[index]
        if (si & 128) == 0
            h.maxprobe = iter
            return -index, sh
        end
        index = (index & (sz - 1)) + 1
        iter = iter + 1
    end
    if h.count > 64000
        rehash!(h, sz * 2)
    else
        rehash!(h, sz * 4)
    end
    return ht_keyindex2!(h, key)
end

# --- _setindex!(h, v, key, index, sh) - internal insert at index ---
# Reference: julia/base/dict.jl:324-342
function _setindex!(h, v, key, index, sh)
    _slots = h.slots
    _keys = h.keys
    _vals = h.vals
    if _slots[index] == 127
        h.ndel = h.ndel - 1
    end
    _slots[index] = sh
    _keys[index] = key
    _vals[index] = v
    h.count = h.count + 1
    h.age = h.age + 1
    if index < h.idxfloor
        h.idxfloor = index
    end
    sz = length(_keys)
    if (h.count + h.ndel) * 3 > sz * 2
        if h.count > 64000
            rehash!(h, h.count * 2)
        else
            rehash!(h, max(h.count * 4, 4))
        end
    end
    return nothing
end

# --- _delete!(h, index) - internal delete at index ---
# Reference: julia/base/dict.jl:626-651
function _delete!(h, index)
    _slots = h.slots
    sz = length(_slots)
    ndel = 1
    nextind = (index & (sz - 1)) + 1
    if _slots[nextind] == 0
        while true
            ndel = ndel - 1
            _slots[index] = 0
            index = ((index - 2) & (sz - 1)) + 1
            _slots[index] != 127 && break
        end
    else
        _slots[index] = 127
    end
    h.ndel = h.ndel + ndel
    h.count = h.count - 1
    h.age = h.age + 1
    return h
end

# --- rehash!(h, newsz) - resize hash table ---
# Reference: julia/base/dict.jl:138-192
function rehash!(h, newsz)
    olds = h.slots
    oldk = h.keys
    oldv = h.vals
    sz = length(olds)
    newsz = _tablesz(newsz)
    h.age = h.age + 1
    h.idxfloor = 1
    if h.count == 0
        newslots = fill!(Vector{Int64}(undef, newsz), 0)
        h.slots = newslots
        h.keys = Vector{Any}(undef, newsz)
        h.vals = Vector{Any}(undef, newsz)
        h.ndel = 0
        h.maxprobe = 0
        return h
    end
    slots = fill!(Vector{Int64}(undef, newsz), 0)
    ks = Vector{Any}(undef, newsz)
    vs = Vector{Any}(undef, newsz)
    count = 0
    maxprb = 0
    i = 1
    while i <= sz
        si = olds[i]
        if (si & 128) != 0
            k = oldk[i]
            v = oldv[i]
            index, _ = hashindex(k, newsz)
            index0 = index
            while slots[index] != 0
                index = (index & (newsz - 1)) + 1
            end
            probe = (index - index0) & (newsz - 1)
            if probe > maxprb
                maxprb = probe
            end
            slots[index] = si
            ks[index] = k
            vs[index] = v
            count = count + 1
        end
        i = i + 1
    end
    h.age = h.age + 1
    h.slots = slots
    h.keys = ks
    h.vals = vs
    h.count = count
    h.ndel = 0
    h.maxprobe = maxprb
    return h
end

# --- skip_deleted - iteration helper ---
# Reference: julia/base/dict.jl:684-699
function skip_deleted(h, i)
    _slots = h.slots
    L = length(_slots)
    while i <= L
        if (_slots[i] & 128) != 0
            return i
        end
        i = i + 1
    end
    return 0
end

function skip_deleted_floor!(h)
    idx = skip_deleted(h, h.idxfloor)
    if idx != 0
        h.idxfloor = idx
    end
    return idx
end

# =============================================================================
# Public API for Dict{K,V} struct (Issue #2748)
# =============================================================================
# These methods dispatch on StructRef Dict instances via where {K,V}.

function setindex!(h::Dict{K,V}, v, key) where {K,V}
    index, sh = ht_keyindex2!(h, key)
    if index > 0
        # Key exists, update value only
        _vals = h.vals
        _vals[index] = v
        h.age = h.age + 1
    else
        _setindex!(h, v, key, -index, sh)
    end
    return h
end

function getindex(h::Dict{K,V}, key) where {K,V}
    index = ht_keyindex(h, key)
    if index < 0
        error("KeyError: key not found")
    end
    _vals = h.vals
    return _vals[index]
end

function haskey(h::Dict{K,V}, key) where {K,V}
    return ht_keyindex(h, key) >= 0
end

function get(h::Dict{K,V}, key, default) where {K,V}
    index = ht_keyindex(h, key)
    if index < 0
        return default
    end
    _vals = h.vals
    return _vals[index]
end

function length(h::Dict{K,V}) where {K,V}
    return h.count
end

function isempty(h::Dict{K,V}) where {K,V}
    return h.count == 0
end

function delete!(h::Dict{K,V}, key) where {K,V}
    index = ht_keyindex(h, key)
    if index > 0
        _delete!(h, index)
    end
    return h
end

function empty!(h::Dict{K,V}) where {K,V}
    _slots = h.slots
    fill!(_slots, 0)
    h.ndel = 0
    h.count = 0
    h.age = h.age + 1
    h.idxfloor = 1
    h.maxprobe = 0
    return h
end

function pop!(h::Dict{K,V}, key) where {K,V}
    index = ht_keyindex(h, key)
    if index < 0
        error("KeyError: key not found")
    end
    _vals = h.vals
    val = _vals[index]
    _delete!(h, index)
    return val
end

function pop!(h::Dict{K,V}, key, default) where {K,V}
    index = ht_keyindex(h, key)
    if index < 0
        return default
    end
    _vals = h.vals
    val = _vals[index]
    _delete!(h, index)
    return val
end

# =============================================================================
# Iteration for Dict{K,V} struct
# =============================================================================
# Reference: julia/base/dict.jl:701-715

function iterate(h::Dict{K,V}) where {K,V}
    i = skip_deleted_floor!(h)
    if i == 0
        return nothing
    end
    _keys = h.keys
    _vals = h.vals
    return (Pair(_keys[i], _vals[i]), i + 1)
end

function iterate(h::Dict{K,V}, state) where {K,V}
    i = skip_deleted(h, state)
    if i == 0
        return nothing
    end
    _keys = h.keys
    _vals = h.vals
    return (Pair(_keys[i], _vals[i]), i + 1)
end

# =============================================================================
# keys/values/pairs for Dict{K,V} struct
# =============================================================================

function keys(h::Dict{K,V}) where {K,V}
    result = Any[]
    _slots = h.slots
    _keys = h.keys
    sz = length(_slots)
    i = 1
    while i <= sz
        if (_slots[i] & 128) != 0
            push!(result, _keys[i])
        end
        i = i + 1
    end
    return result
end

function values(h::Dict{K,V}) where {K,V}
    result = Any[]
    _slots = h.slots
    _vals = h.vals
    sz = length(_slots)
    i = 1
    while i <= sz
        if (_slots[i] & 128) != 0
            push!(result, _vals[i])
        end
        i = i + 1
    end
    return result
end

function pairs(h::Dict{K,V}) where {K,V}
    result = Any[]
    _slots = h.slots
    _keys = h.keys
    _vals = h.vals
    sz = length(_slots)
    i = 1
    while i <= sz
        if (_slots[i] & 128) != 0
            push!(result, (_keys[i], _vals[i]))
        end
        i = i + 1
    end
    return result
end
