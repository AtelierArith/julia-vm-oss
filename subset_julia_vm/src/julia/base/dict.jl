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
# - Legacy bytecode can still carry Value::Dict
# - Public Dict()/Dict{K,V}() construction routes through the struct methods below

# =============================================================================
# Generic Dict helpers (Issue #6731)
# =============================================================================
# `Dict` is a pure-Julia `Dict{K,V}` struct (no Rust `Value::Dict` carrier).
# `haskey`/`get`/`getkey`/`getindex`/`setindex!`/`get!`/`keys`/`values`/
# `length`/`delete!`/`empty!`/`pop!` all live on the parametric `Dict{K,V}`
# methods further below; only the bare `::Dict` helpers that have no parametric
# counterpart remain here.

# pairs(d::Dict) - return the dictionary itself (Issue #3474)
pairs(d::Dict) = d

function first(d::Dict)
    x = iterate(d)
    if x === nothing
        throw(ArgumentError("collection must be non-empty"))
    end
    return x[1]
end

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
const _dict_empty_slot = UInt8(0)
const _dict_deleted_slot = UInt8(127)
const _dict_filled_mask = UInt8(128)

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
# hash() returns UInt64; use >>> (logical right shift)
# Result: 128-255 (bit 7 always set), stored as UInt8
function _shorthash7(hsh)
    return UInt8((hsh >>> 57) | 128)
end

# hashindex(key, sz) - compute slot index and short hash
# Reference: julia/base/dict.jl:127-132
# sz must be a power of 2; returns (1-based index, shorthash7)
function hashindex(key, sz)
    hsh = reinterpret(UInt64, hash(key))
    idx = Int64((hsh & UInt64(sz - 1)) + UInt64(1))
    return idx, _shorthash7(hsh)
end

# =============================================================================
# Dict{K,V} mutable struct definition (Issue #2748)
# =============================================================================
# Based on Julia's base/dict.jl (Julia 1.11+)
#
# Fields mirror Julia's upstream Memory-backed Dict shape.
# Public Dict construction routes through the outer constructors below. The VM
# still keeps legacy Value::Dict support for cache-compatible bytecode.

mutable struct Dict{K,V} <: AbstractDict{K,V}
    slots::Memory{UInt8}  # slot metadata (0=empty, 127=deleted, 128+=filled)
    keys::Memory{K}       # keys storage
    vals::Memory{V}       # values storage
    ndel::Int64           # number of deleted entries
    count::Int64          # number of active entries
    age::Int64            # modification counter
    idxfloor::Int64       # smallest index that might be occupied
    maxprobe::Int64       # max probe distance used
end

# =============================================================================
# Constructor helper
# =============================================================================

function _new_dict_kv(::Type{K}, ::Type{V}, n) where {K,V}
    sz = _tablesz(n)
    slots = fill!(Memory{UInt8}(undef, sz), _dict_empty_slot)
    ks = Memory{K}(undef, sz)
    vs = Memory{V}(undef, sz)
    return Dict{K,V}(slots, ks, vs, 0, 0, 0, 1, 0)
end

# Create an empty Dict{Any,Any} struct with initial capacity
function _new_dict_kv(n)
    return _new_dict_kv(Any, Any, n)
end

function _dict_pair_splat_eltypes(ps)
    n = length(ps)
    if n == 0
        return Any, Any
    end
    p = ps[1]
    K = typeof(p.first)
    V = typeof(p.second)
    i = 2
    while i <= n
        p = ps[i]
        K = typejoin(K, typeof(p.first))
        V = typejoin(V, typeof(p.second))
        i = i + 1
    end
    return K, V
end

function _dict_iterable_eltypes(kv)
    count = 0
    K = Any
    V = Any
    for item in kv
        k = item[1]
        v = item[2]
        if count == 0
            K = typeof(k)
            V = typeof(v)
        else
            K = typejoin(K, typeof(k))
            V = typejoin(V, typeof(v))
        end
        count = count + 1
    end
    return K, V, count
end

function _dict_from_explicit_types(K, V, ps::Pair...)
    h = _new_dict_kv(K, V, length(ps))
    for p in ps
        h[p.first] = p.second
    end
    return h
end

function _dict_from_explicit_types(K, V, kv)
    h = _new_dict_kv(K, V, 0)
    for item in kv
        h[item[1]] = item[2]
    end
    return h
end

function Dict{K,V}() where {K,V}
    return _dict_from_explicit_types(K, V)
end

function Dict{K,V}(p::Pair) where {K,V}
    return _dict_from_explicit_types(K, V, p)
end

function Dict{K,V}(ps::Pair...) where {K,V}
    return _dict_from_explicit_types(K, V, ps...)
end

function Dict{K,V}(kv) where {K,V}
    return _dict_from_explicit_types(K, V, kv)
end

# Outer constructors matching the upstream `julia/base/dict.jl` surface for Pair
# values and iterable key/value pairs (Issues #6531, #6618, #6619).
function Dict(ps::Pair...)
    K, V = _dict_pair_splat_eltypes(ps)
    h = _new_dict_kv(K, V, length(ps))
    for p in ps
        h[p.first] = p.second
    end
    return h
end

function Dict(kv)
    K, V, n = _dict_iterable_eltypes(kv)
    h = _new_dict_kv(K, V, n)
    for item in kv
        h[item[1]] = item[2]
    end
    return h
end

# Build the `ENV` dictionary from the raw OS `(key, value)` pairs supplied by the
# `PushEnv` VM instruction. Kept as a pure-Julia helper so `ENV` is an ordinary
# `Dict{String,String}` StructRef rather than a `Value::Dict` carrier
# (Issue #6731).
_env_from_pairs(pairs) = Dict{String,String}(pairs)

# =============================================================================
# Core Hash Table Algorithms (Issue #2747, #2748)
# =============================================================================
# IMPORTANT: All field+index access uses local variables to avoid
# UnsupportedAssignmentTarget errors. Compound assignments on struct
# fields use explicit form (h.count = h.count + 1).

function _dict_keyindex_linear(h, key)
    _slots = h.slots
    _keys = h.keys
    sz = length(_slots)
    i = 1
    while i <= sz
        if (_slots[i] & _dict_filled_mask) != _dict_empty_slot
            k = _keys[i]
            if key === k || isequal(key, k)
                return i
            end
        end
        i = i + 1
    end
    return -1
end

_dict_linear_fallback_enabled(key) = false
_dict_linear_fallback_enabled(key::Float64) = true
_dict_linear_fallback_enabled(key::Float32) = true
_dict_linear_fallback_enabled(key::Float16) = true

function _dict_keyindex_linear_or_missing(h, key)
    _dict_linear_fallback_enabled(key) ? _dict_keyindex_linear(h, key) : -1
end

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
        si == _dict_empty_slot && return _dict_keyindex_linear_or_missing(h, key)
        if sh == si
            k = _keys[index]
            if (key === k || isequal(key, k))
                return index
            end
        end
        index = (index & (sz - 1)) + 1
        iter = iter + 1
        iter > maxprb && return _dict_keyindex_linear_or_missing(h, key)
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
        if si == _dict_empty_slot
            found = _dict_keyindex_linear_or_missing(h, key)
            found > 0 && return found, sh
            if avail < 0
                return avail, sh
            else
                return -index, sh
            end
        end
        if si == _dict_deleted_slot
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
    found = _dict_keyindex_linear_or_missing(h, key)
    found > 0 && return found, sh
    avail < 0 && return avail, sh
    maxallowed = max(maxallowedprobe, sz >> maxprobeshift)
    while iter < maxallowed
        si = _slots[index]
        if (si & _dict_filled_mask) == _dict_empty_slot
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
    if _slots[index] == _dict_deleted_slot
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
    if _slots[nextind] == _dict_empty_slot
        while true
            ndel = ndel - 1
            _slots[index] = _dict_empty_slot
            index = ((index - 2) & (sz - 1)) + 1
            _slots[index] != _dict_deleted_slot && break
        end
    else
        _slots[index] = _dict_deleted_slot
    end
    h.ndel = h.ndel + ndel
    h.count = h.count - 1
    h.age = h.age + 1
    return h
end

# --- rehash!(h, newsz) - resize hash table ---
# Reference: julia/base/dict.jl:138-192
function rehash!(h::Dict{K,V}, newsz) where {K,V}
    olds = h.slots
    oldk = h.keys
    oldv = h.vals
    sz = length(olds)
    newsz = _tablesz(newsz)
    h.age = h.age + 1
    h.idxfloor = 1
    if h.count == 0
        newslots = fill!(Memory{UInt8}(undef, newsz), _dict_empty_slot)
        h.slots = newslots
        h.keys = Memory{K}(undef, newsz)
        h.vals = Memory{V}(undef, newsz)
        h.ndel = 0
        h.maxprobe = 0
        return h
    end
    slots = fill!(Memory{UInt8}(undef, newsz), _dict_empty_slot)
    ks = Memory{K}(undef, newsz)
    vs = Memory{V}(undef, newsz)
    count = 0
    maxprb = 0
    i = 1
    while i <= sz
        si = olds[i]
        if (si & _dict_filled_mask) != _dict_empty_slot
            k = oldk[i]
            v = oldv[i]
            index, _ = hashindex(k, newsz)
            index0 = index
            while slots[index] != _dict_empty_slot
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
        if (_slots[i] & _dict_filled_mask) != _dict_empty_slot
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
        throw(KeyError(string(key)))
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

function getkey(h::Dict{K,V}, key, default) where {K,V}
    index = ht_keyindex(h, key)
    index < 0 && return default
    return h.keys[index]
end

function get!(h::Dict{K,V}, key, default) where {K,V}
    if haskey(h, key)
        return h[key]
    end
    h[key] = default
    return default
end

function get!(default::Function, h::Dict{K,V}, key) where {K,V}
    if haskey(h, key)
        return h[key]
    end
    value = default()
    h[key] = value
    return value
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
    fill!(_slots, _dict_empty_slot)
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
        throw(KeyError(string(key)))
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

struct KeySet{K} <: AbstractSet{K}
    dict
end

struct ValueIterator{V}
    dict
end

KeySet(h::Dict{K,V}) where {K,V} = KeySet{K}(h)
ValueIterator(h::Dict{K,V}) where {K,V} = ValueIterator{V}(h)

length(v::KeySet{K}) where K = length(v.dict)
length(v::ValueIterator{V}) where V = length(v.dict)
isempty(v::KeySet{K}) where K = isempty(v.dict)
isempty(v::ValueIterator{V}) where V = isempty(v.dict)

eltype(::Type{KeySet{K}}) where K = K
eltype(v::KeySet{K}) where K = K
eltype(::Type{ValueIterator{V}}) where V = V
eltype(v::ValueIterator{V}) where V = V
IteratorEltype(::Type{KeySet{K}}) where K = HasEltype()
IteratorEltype(v::KeySet{K}) where K = HasEltype()
IteratorEltype(::Type{ValueIterator{V}}) where V = HasEltype()
IteratorEltype(v::ValueIterator{V}) where V = HasEltype()
IteratorSize(::Type{KeySet{K}}) where K = HasLength()
IteratorSize(v::KeySet{K}) where K = HasLength()
IteratorSize(::Type{ValueIterator{V}}) where V = HasLength()
IteratorSize(v::ValueIterator{V}) where V = HasLength()

show(io::IO, iter::Union{KeySet,ValueIterator}) = show(io, collect(iter))

function sum(iter::Union{KeySet,ValueIterator}; dims=0, init=nothing)
    if dims != 0
        error("sum: dims must be 1 or 2 for matrices")
    end
    return _sum_iterable(iter, init)
end

function prod(iter::Union{KeySet,ValueIterator}; dims=0, init=nothing)
    if dims != 0
        error("prod: dims must be 1 or 2 for matrices")
    end
    return _prod_iterable(iter, init)
end

function minimum(iter::Union{KeySet,ValueIterator}; dims=0, init=nothing)
    if dims != 0
        error("minimum: dims must be 1 or 2 for matrices")
    end
    return _minimum_iterable(iter, init)
end

function maximum(iter::Union{KeySet,ValueIterator}; dims=0, init=nothing)
    if dims != 0
        error("maximum: dims must be 1 or 2 for matrices")
    end
    return _maximum_iterable(iter, init)
end

function iterate(v::KeySet{K}) where K
    y = iterate(v.dict)
    y === nothing && return nothing
    return (y[1].first, y[2])
end

function iterate(v::KeySet{K}, state) where K
    y = iterate(v.dict, state)
    y === nothing && return nothing
    return (y[1].first, y[2])
end

function iterate(v::ValueIterator{V}) where V
    y = iterate(v.dict)
    y === nothing && return nothing
    return (y[1].second, y[2])
end

function iterate(v::ValueIterator{V}, state) where V
    y = iterate(v.dict, state)
    y === nothing && return nothing
    return (y[1].second, y[2])
end

in(key, v::KeySet{K}) where K = haskey(v.dict, key)

function _value_in_iterator(value, v::ValueIterator)
    for item in v
        if item == value
            return true
        end
    end
    return false
end

in(value, v::ValueIterator) = _value_in_iterator(value, v)

function keys(h::Dict{K,V}) where {K,V}
    return KeySet(h)
end

function values(h::Dict{K,V}) where {K,V}
    return ValueIterator(h)
end

function pairs(h::Dict{K,V}) where {K,V}
    return h
end

function first(h::Dict{K,V}) where {K,V}
    x = iterate(h)
    if x === nothing
        throw(ArgumentError("collection must be non-empty"))
    end
    return x[1]
end

function copy(h::Dict{K,V}) where {K,V}
    result = _new_dict_kv(K, V, length(h))
    for pair in h
        result[pair.first] = pair.second
    end
    return result
end

function merge!(d1::Dict{K,V}, d2::Dict{K2,V2}) where {K,V,K2,V2}
    for pair in d2
        d1[pair.first] = pair.second
    end
    return d1
end

function merge(d1::Dict{K,V}, d2::Dict{K2,V2}) where {K,V,K2,V2}
    result = _new_dict_kv(typejoin(K, K2), typejoin(V, V2), length(d1) + length(d2))
    for pair in d1
        result[pair.first] = pair.second
    end
    for pair in d2
        result[pair.first] = pair.second
    end
    return result
end

function mergewith!(combine::Function, d1::Dict{K,V}, d2::Dict{K2,V2}) where {K,V,K2,V2}
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

function mergewith(combine::Function, d1::Dict{K,V}, d2::Dict{K2,V2}) where {K,V,K2,V2}
    result = copy(d1)
    mergewith!(combine, result, d2)
    return result
end

function _dict_pair_in(p::Pair, h::Dict{K,V}) where {K,V}
    haskey(h, p.first) || return false
    return h[p.first] == p.second
end

in(p::Pair, h::Dict{K,V}) where {K,V} = _dict_pair_in(p, h)

function filter!(f::Function, h::Dict{K,V}) where {K,V}
    _slots = h.slots
    _keys = h.keys
    _vals = h.vals
    sz = length(_slots)
    i = 1
    while i <= sz
        if (_slots[i] & _dict_filled_mask) != _dict_empty_slot
            pair = Pair(_keys[i], _vals[i])
            if !f(pair)
                _delete!(h, i)
            end
        end
        i = i + 1
    end
    return h
end

function filter(f::Function, h::Dict{K,V}) where {K,V}
    result = copy(h)
    filter!(f, result)
    return result
end

function ==(a::Dict{K,V}, b::Dict{K2,V2}) where {K,V,K2,V2}
    length(a) == length(b) || return false
    for pair in a
        if !haskey(b, pair.first)
            return false
        end
        if !(b[pair.first] == pair.second)
            return false
        end
    end
    return true
end

function isequal(a::Dict{K,V}, b::Dict{K2,V2}) where {K,V,K2,V2}
    length(a) == length(b) || return false
    for pair in a
        if !haskey(b, pair.first)
            return false
        end
        if !isequal(b[pair.first], pair.second)
            return false
        end
    end
    return true
end

function hash(h::Dict{K,V}) where {K,V}
    result = hash(length(h))
    for pair in h
        result = xor(result, hash(pair.second, hash(pair.first)))
    end
    return hash(result)
end

# =============================================================================
# keytype / valtype (Issue #5117)
# =============================================================================
# Based on julia/base/abstractdict.jl:300-325. Upstream defines these on
# `::Type{<:AbstractDict{K}}` / `::Type{<:AbstractDict{<:Any,V}}`; in the VM the
# covariant `<:` form does not bind type parameters, so the type method is
# written against the concrete `Dict{K,V}` parametric type (which the dispatcher
# resolves). The value form delegates through `typeof`, exactly as upstream does
# for any `AbstractDict`.

"""
    keytype(type)

Get the key type of a dictionary type. Behaves similarly to [`eltype`](@ref).

# Examples
```jldoctest
julia> keytype(Dict(Int32(1) => "foo"))
Int32
```
"""
keytype(::Type{Dict{K,V}}) where {K,V} = K
keytype(d::AbstractDict) = keytype(typeof(d))

"""
    valtype(type)

Get the value type of a dictionary type. Behaves similarly to [`eltype`](@ref).

# Examples
```jldoctest
julia> valtype(Dict(Int32(1) => "foo"))
String
```
"""
valtype(::Type{Dict{K,V}}) where {K,V} = V
valtype(d::AbstractDict) = valtype(typeof(d))

# eltype for dictionaries (Issue #5116).  Upstream
# (julia/base/abstractdict.jl:490) defines
# `eltype(::Type{<:AbstractDict{K,V}}) where {K,V} = Pair{K,V}` and the value
# form `eltype(x) = eltype(typeof(x))` (julia/base/abstractarray.jl:245).
# As with keytype/valtype above, the VM cannot bind type parameters through a
# covariant `::Type{<:AbstractDict{K,V}}`, so the type method is written against
# the concrete `Dict{K,V}` parametric type which the dispatcher resolves.
eltype(::Type{Dict{K,V}}) where {K,V} = Pair{K,V}
eltype(d::AbstractDict) = eltype(typeof(d))
