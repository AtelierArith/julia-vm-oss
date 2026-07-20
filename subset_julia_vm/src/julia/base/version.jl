# =============================================================================
# Version - Version number type and VERSION constant
# =============================================================================
# Based on Julia's base/version.jl.
#
# sjulia keeps the upstream data model: major/minor/patch plus prerelease and
# build identifier tuples. `v"..."` literals are parsed during lowering and call
# the five-argument constructor directly.

struct VersionNumber
    major::Int64
    minor::Int64
    patch::Int64
    prerelease
    build
end

VersionNumber(major::Integer, minor::Integer, patch::Integer, pre::Tuple, bld::Tuple) =
    VersionNumber(Int64(major), Int64(minor), Int64(patch), pre, bld)

VersionNumber(major::Integer, minor::Integer, patch::Integer) =
    VersionNumber(Int64(major), Int64(minor), Int64(patch), (), ())

VersionNumber(major::Integer, minor::Integer) = VersionNumber(major, minor, 0)
VersionNumber(major::Integer) = VersionNumber(major, 0, 0)

function _version_core_string(v::VersionNumber)
    io = IOBuffer()
    print(io, v.major)
    print(io, ".")
    print(io, v.minor)
    print(io, ".")
    print(io, v.patch)
    if !isempty(v.prerelease)
        print(io, "-")
        i = 1
        while i <= length(v.prerelease)
            if i > 1
                print(io, ".")
            end
            print(io, v.prerelease[i])
            i = i + 1
        end
    end
    if !isempty(v.build)
        print(io, "+")
        i = 1
        while i <= length(v.build)
            if i > 1
                print(io, ".")
            end
            print(io, v.build[i])
            i = i + 1
        end
    end
    return String(take!(io))
end

print(io::IO, v::VersionNumber) = print(io, _version_core_string(v))
show(io::IO, v::VersionNumber) = print(io, "v\"", _version_core_string(v), "\"")

function _version_ident_cmp(a::Integer, b::Integer)
    if a < b
        return -1
    elseif a > b
        return 1
    end
    return 0
end

_version_ident_cmp(a::Integer, b::String) = isempty(b) ? 1 : -1
_version_ident_cmp(a::String, b::Integer) = isempty(a) ? -1 : 1

function _version_ident_cmp(a::String, b::String)
    if a < b
        return -1
    elseif a > b
        return 1
    end
    return 0
end

function _version_tuple_cmp(a, b)
    n = min(length(a), length(b))
    i = 1
    while i <= n
        c = _version_ident_cmp(a[i], b[i])
        if c != 0
            return c
        end
        i = i + 1
    end
    if length(a) < length(b)
        return -1
    elseif length(a) > length(b)
        return 1
    end
    return 0
end

function ==(a::VersionNumber, b::VersionNumber)
    return a.major == b.major &&
           a.minor == b.minor &&
           a.patch == b.patch &&
           _version_tuple_cmp(a.prerelease, b.prerelease) == 0 &&
           _version_tuple_cmp(a.build, b.build) == 0
end

function isless(a::VersionNumber, b::VersionNumber)
    if a.major != b.major
        return a.major < b.major
    end
    if a.minor != b.minor
        return a.minor < b.minor
    end
    if a.patch != b.patch
        return a.patch < b.patch
    end
    if !isempty(a.prerelease) && isempty(b.prerelease)
        return true
    end
    if isempty(a.prerelease) && !isempty(b.prerelease)
        return false
    end
    c = _version_tuple_cmp(a.prerelease, b.prerelease)
    if c < 0
        return true
    elseif c > 0
        return false
    end
    return _version_tuple_cmp(a.build, b.build) < 0
end

<(a::VersionNumber, b::VersionNumber) = isless(a, b)
<=(a::VersionNumber, b::VersionNumber) = a == b || isless(a, b)
>(a::VersionNumber, b::VersionNumber) = isless(b, a)
>=(a::VersionNumber, b::VersionNumber) = a == b || isless(b, a)

# =============================================================================
# VERSION Constant
# =============================================================================

"""
    VERSION

The version of SubsetJuliaVM currently in use.

# Examples
```julia
julia> VERSION
v"0.11.1"

julia> VERSION.major
0

julia> string(VERSION)
"0.11.1"
```
"""
const VERSION = VersionNumber(0, 11, 1)
