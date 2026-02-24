# =============================================================================
# Version - Version number type and VERSION constant
# =============================================================================
# Based on Julia's base/version.jl
#
# In official Julia, VersionNumber has additional prerelease and build fields
# with complex validation and comparison operators.
#
# In SubsetJuliaVM, we provide a simplified version with just major, minor,
# and patch fields, which covers the most common use cases.

# =============================================================================
# VersionNumber Type
# =============================================================================

"""
    VersionNumber

Version number type following semantic versioning (semver) format.
Composed of major, minor, and patch numeric values.

# Examples
```julia
julia> v = VersionNumber(1, 2, 3)
v"1.2.3"

julia> v.major
1

julia> v.minor
2

julia> v.patch
3

julia> string(v)
"1.2.3"
```

Note: In official Julia, VersionNumber also has `prerelease` and `build` tuple
fields for pre-release and build metadata. SubsetJuliaVM provides a simplified
version with just the core version fields.
"""
struct VersionNumber
    major::Int64
    minor::Int64
    patch::Int64
end

# Convenience constructor with default patch = 0
VersionNumber(major::Integer, minor::Integer) = VersionNumber(major, minor, 0)

# Convenience constructor with default minor = 0 and patch = 0
VersionNumber(major::Integer) = VersionNumber(major, 0, 0)

# =============================================================================
# VERSION Constant
# =============================================================================

"""
    VERSION

The version of SubsetJuliaVM currently in use.

# Examples
```julia
julia> VERSION
v"0.6.6"

julia> VERSION.major
0

julia> string(VERSION)
"0.6.6"
```
"""
const VERSION = VersionNumber(0, 6, 6)
