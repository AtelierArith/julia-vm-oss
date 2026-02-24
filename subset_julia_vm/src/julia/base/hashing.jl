# =============================================================================
# Hash functions
# =============================================================================
# Based on julia/base/hashing.jl
#
# The hash function maps values to integer hash codes for use in Dict and Set.
#
# Architecture:
#   - _hash(x) is a Rust intrinsic that computes the core hash (Issue #2582)
#   - Pure Julia methods provide the public API and composability
#   - hash(x, h) combines hash values for compound types
#
# Julia's hash contract:
#   - isequal(x, y) implies hash(x) == hash(y)
#   - hash returns an integer value

"""
    hash(x)

Compute an integer hash code such that `isequal(x,y)` implies `hash(x)==hash(y)`.

The hash value is not guaranteed to be stable across Julia versions or sessions.

# Examples
```julia
hash(42)
hash("hello")
hash(3.14)
```
"""
hash(x) = _hash(x)

"""
    hash(x, h)

Compute a hash code for `x`, mixed with the hash value `h`.

This 2-argument form is used to incrementally compute hash values for
compound types (arrays, tuples, etc.).

# Examples
```julia
h = hash(1)
h = hash(2, h)  # combine hashes
```
"""
hash(x, h) = hash(xor(_hash(x), h))

# Type-specific hash methods that ensure isequal contract
# isequal(-0.0, 0.0) is true, so they must have the same hash
hash(x::Float64) = _hash(x == -0.0 ? 0.0 : x)
hash(x::Float32) = _hash(Float64(x == Float32(-0.0) ? Float32(0.0) : x))
hash(x::Float16) = _hash(Float64(x))

# Bool hashes should be consistent with integer hashes
# isequal(true, 1) is false in Julia, so hash(true) != hash(1) is OK
hash(x::Bool) = _hash(x)

# Nothing and Missing have distinct hashes
hash(x::Nothing) = _hash(x)
hash(x::Missing) = _hash(x)
