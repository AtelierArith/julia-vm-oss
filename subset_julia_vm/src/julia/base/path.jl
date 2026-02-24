# =============================================================================
# Path manipulation functions
# =============================================================================
# Based on Julia's base/path.jl
#
# These functions handle file path string manipulation.
# They do not perform actual filesystem operations.
#
# Note: This is a simplified implementation that assumes Unix-style paths.
# Windows-style paths (with backslashes and drive letters) are not fully supported.

# Path separator constants (Unix-style) - defined for documentation but not used
# in function bodies due to SubsetJuliaVM limitations with global const access.
# Functions use literal "/" and '/' instead.
const pathsep_str = "/"
const pathsep_char = '/'

# Helper function to find the last occurrence of a character in a string
# Uses === for character comparison since == on Char is not fully supported
function _findlast_char(c::Char, s::AbstractString)
    n = length(s)
    i = n
    while i >= 1
        if s[i] === c
            return i
        end
        i = i - 1
    end
    return nothing
end

# Helper function to find the last occurrence of '.' in a string
function _findlast_dot(s::AbstractString)
    return _findlast_char('.', s)
end

"""
    splitdir(path::AbstractString) -> (dir, base)

Split a path into a tuple of the directory name and file name.

# Examples
```julia
julia> splitdir("/home/myuser")
("/home", "myuser")

julia> splitdir("/home/myuser/")
("/home/myuser", "")
```
"""
function splitdir(path::AbstractString)
    i = _findlast_char('/', path)
    if isnothing(i)
        return ("", path)
    elseif i == 1
        return ("/", path[2:end])
    else
        return (path[1:i-1], path[i+1:end])
    end
end

"""
    dirname(path::AbstractString) -> String

Get the directory part of a path. Trailing characters ('/' or '\\') in the path are
counted as part of the path.

# Examples
```julia
julia> dirname("/home/myuser")
"/home"

julia> dirname("/home/myuser/")
"/home/myuser"
```
"""
function dirname(path::AbstractString)
    return splitdir(path)[1]
end

"""
    basename(path::AbstractString) -> String

Get the file name part of a path.

# Examples
```julia
julia> basename("/home/myuser/example.jl")
"example.jl"

julia> basename("/home/myuser/")
""
```
"""
function basename(path::AbstractString)
    return splitdir(path)[2]
end

"""
    splitext(path::AbstractString) -> (path_without_extension, extension)

If the last component of a path contains one or more dots, split the path into
everything before the last dot and everything including and after the dot.
Otherwise, return the path unchanged with an empty extension.

# Examples
```julia
julia> splitext("/home/myuser/example.jl")
("/home/myuser/example", ".jl")

julia> splitext("/home/myuser/example.tar.gz")
("/home/myuser/example.tar", ".gz")

julia> splitext("/home/myuser/example")
("/home/myuser/example", "")
```
"""
function splitext(path::AbstractString)
    dir, base = splitdir(path)
    i = _findlast_dot(base)
    if isnothing(i) || i == 1
        return (path, "")
    else
        ext = base[i:end]
        stem = base[1:i-1]
        if isempty(dir)
            return (stem, ext)
        else
            return (string(dir, "/", stem), ext)
        end
    end
end

"""
    splitpath(path::AbstractString) -> Vector{String}

Split a path into all its path components.

Returns a vector of strings where each element is a path component.
An absolute path starts with "/" as the first component.

# Examples
```julia
julia> splitpath("/home/myuser/example.jl")
["/", "home", "myuser", "example.jl"]

julia> splitpath("a/b/c")
["a", "b", "c"]

julia> splitpath("")
[""]
```
"""
# Helper function to count path components
function _count_path_components(path::AbstractString)
    count = 0
    p = path
    while !isempty(p)
        dir, base = splitdir(p)
        if length(dir) >= length(p)
            # Root node
            count = count + 1
            break
        end
        if !isempty(base)
            count = count + 1
        end
        p = dir
    end
    return count
end

# Helper function to get nth path component from end (1 = last, 2 = second to last, etc.)
function _get_path_component_from_end(path::AbstractString, n::Int64)
    idx = 0
    p = path
    while !isempty(p)
        dir, base = splitdir(p)
        if length(dir) >= length(p)
            # Root node
            idx = idx + 1
            if idx >= n
                return dir
            end
            break
        end
        if !isempty(base)
            idx = idx + 1
            if idx >= n
                return base
            end
        end
        p = dir
    end
    return ""  # Should not reach here
end

function splitpath(path::AbstractString)
    out = String[]
    if isempty(path)
        push!(out, "")
        return out
    end

    # First count the components
    n = _count_path_components(path)

    # Then build the array in correct order (root first)
    i = n
    while i >= 1
        component = _get_path_component_from_end(path, i)
        push!(out, component)
        i = i - 1
    end

    return out
end

"""
    joinpath(path::AbstractString, paths::AbstractString...) -> String

Join path components into a full path.

# Examples
```julia
julia> joinpath("/home", "myuser", "example.jl")
"/home/myuser/example.jl"

julia> joinpath("a", "b", "c")
"a/b/c"
```
"""
function joinpath(a::AbstractString)
    return string(a)
end

function joinpath(a::AbstractString, b::AbstractString)
    if startswith(b, "/")
        return b
    elseif isempty(a)
        return b
    elseif endswith(a, "/")
        return string(a, b)
    else
        return string(a, "/", b)
    end
end

function joinpath(a::AbstractString, b::AbstractString, c::AbstractString)
    return joinpath(joinpath(a, b), c)
end

function joinpath(a::AbstractString, b::AbstractString, c::AbstractString, d::AbstractString)
    return joinpath(joinpath(joinpath(a, b), c), d)
end

function joinpath(a::AbstractString, b::AbstractString, c::AbstractString, d::AbstractString, e::AbstractString)
    return joinpath(joinpath(joinpath(joinpath(a, b), c), d), e)
end

"""
    isabspath(path::AbstractString) -> Bool

Check if a path is absolute.

# Examples
```julia
julia> isabspath("/home/myuser")
true

julia> isabspath("myuser")
false
```
"""
function isabspath(path::AbstractString)
    return startswith(path, "/")
end

"""
    isdirpath(path::AbstractString) -> Bool

Check if a path represents a directory (ends with a path separator).

# Examples
```julia
julia> isdirpath("/home/myuser/")
true

julia> isdirpath("/home/myuser")
false
```
"""
function isdirpath(path::AbstractString)
    return endswith(path, "/")
end
