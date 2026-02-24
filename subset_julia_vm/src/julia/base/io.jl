# =============================================================================
# Julia IO Type Hierarchy
# =============================================================================
# This defines the IO type hierarchy for custom show methods.
# The actual IO operations are handled by the VM.

# Note: abstract type IO is NOT defined here because our IOBuffer uses
# the built-in IO ValueType, not a Julia struct.

# =============================================================================
# IOContext - Wrapper for IO with properties
# =============================================================================
# Based on Julia's base/show.jl
#
# IOContext provides a mechanism for passing output configuration settings
# among show methods. It wraps an IO stream and stores key-value properties.
#
# In official Julia, IOContext uses ImmutableDict for efficient property storage.
# SubsetJuliaVM uses a simplified Array-based implementation that provides
# Julia-compatible API (get, haskey, etc.) while working within VM limitations.

"""
    IOContext

`IOContext` provides a mechanism for passing output configuration settings
among [`show`](@ref) methods.

In short, it is an immutable dictionary that is a subclass of `IO`.
It supports standard dictionary operations such as `get` and `haskey`,
and can also be used as an I/O stream.

# Common properties
- `:compact`: Boolean specifying that values should be printed compactly
- `:limit`: Boolean specifying that containers should be truncated
- `:displaysize`: A tuple (rows, cols) giving the size for text output
- `:typeinfo`: A Type characterizing the information already printed
- `:color`: Boolean specifying whether ANSI color codes are supported

# Constructors

```julia
IOContext(io::IO)                           # empty properties
IOContext(io::IO, :key => value)            # single property
IOContext(io::IO, :k1 => v1, :k2 => v2)     # multiple properties
IOContext(io::IO, context::IOContext)       # inherit properties from context
```

# Examples

```julia
io = IOContext(stdout, :compact => true, :limit => true)
show(io, [1.123456789, 2.987654321])
# Output: [1.12, 2.99] (compact)

# Check properties
get(io, :compact, false)  # => true
haskey(io, :limit)        # => true
```

Note: SubsetJuliaVM uses an Array-based property storage for simplicity.
The `properties` field stores an array of Pairs.
"""
struct IOContext
    io
    properties
end

# =============================================================================
# IOContext Construction Helpers
# =============================================================================
# Note: In SubsetJuliaVM, the struct's implicit constructor takes precedence
# over outer constructor functions. To work around this, use the `iocontext()`
# function instead of `IOContext()` when creating IOContext with properties.
#
# Example:
#   ctx = iocontext(stdout, :compact => true)   # Works correctly
#   ctx = IOContext(stdout, :compact => true)   # Uses struct constructor directly (not recommended)

# Helper function to get properties from an IO
# Returns empty array for plain IO, or the properties from IOContext
function _ioproperties(io)
    if isa(io, IOContext)
        return io.properties
    else
        return []
    end
end

# Helper function to add a property to a properties array
function _add_property(props, key::Symbol, value)
    # Create new array with the property prepended (most recent first)
    result = [key => value]
    n = length(props)
    i = 1
    while i <= n
        push!(result, props[i])
        i = i + 1
    end
    return result
end

# =============================================================================
# IOContext Property Access
# =============================================================================
# In official Julia, IOContext supports get(io, key, default) and haskey(io, key).
# SubsetJuliaVM now supports these as well via non-Dict StructRef dispatch (Issue #3152).
# The VM's DictGet/DictHasKey builtins first check for user-defined methods on
# non-Dict structs before falling back to dict operations.
#
# Primary API (Julia-compatible):
#   - get(ctx, key, default) - retrieve property value (dispatches to ioget)
#   - haskey(ctx, key) - check if property exists (dispatches to iohaskey)
#
# Internal helpers (still available for backward compatibility):
#   - ioget(ctx, key, default) - retrieve property value
#   - iohaskey(ctx, key) - check if property exists

"""
    ioget(ctx::IOContext, key::Symbol, default)

Retrieve the value associated with `key` from the IOContext properties.
Returns `default` if the key is not found.

Note: Use `ioget` instead of `get` because SubsetJuliaVM intercepts `get`
as a builtin for Dict operations.

# Example
```julia
ctx = IOContext(stdout, :compact => true)
ioget(ctx, :compact, false)  # => true
ioget(ctx, :limit, false)    # => false (not set, returns default)
```
"""
function ioget(ctx::IOContext, key::Symbol, default)
    n = length(ctx.properties)
    i = 1
    while i <= n
        p = ctx.properties[i]
        if p[1] === key
            return p[2]
        end
        i = i + 1
    end
    return default
end

"""
    ioget(io::IO, key::Symbol, default)

For plain IO streams (not IOContext), always returns `default`.
"""
ioget(io::IO, key::Symbol, default) = default

"""
    iohaskey(ctx::IOContext, key::Symbol)

Check if `key` exists in the IOContext properties.

Note: Use `iohaskey` instead of `haskey` because SubsetJuliaVM intercepts
`haskey` as a builtin for Dict operations.

# Example
```julia
ctx = IOContext(stdout, :compact => true)
iohaskey(ctx, :compact)  # => true
iohaskey(ctx, :limit)    # => false
```
"""
function iohaskey(ctx::IOContext, key::Symbol)
    n = length(ctx.properties)
    i = 1
    while i <= n
        p = ctx.properties[i]
        if p[1] === key
            return true
        end
        i = i + 1
    end
    return false
end

"""
    iohaskey(io::IO, key::Symbol)

For plain IO streams (not IOContext), always returns `false`.
"""
iohaskey(io::IO, key::Symbol) = false

# =============================================================================
# Julia-compatible get/haskey for IO types (Issue #3152)
# =============================================================================
# These methods allow using standard get(io, key, default) and haskey(io, key)
# syntax with IOContext, matching official Julia's API.
# The VM dispatches these via non-Dict StructRef dispatch in DictGet/DictHasKey.

"""
    get(ctx::IOContext, key::Symbol, default)

Retrieve the value associated with `key` from the IOContext properties.
Returns `default` if the key is not found.

# Example
```julia
ctx = IOContext(stdout, :compact => true)
get(ctx, :compact, false)  # => true
get(ctx, :limit, false)    # => false (not set, returns default)
```
"""
get(ctx::IOContext, key::Symbol, default) = ioget(ctx, key, default)

"""
    get(io::IO, key::Symbol, default)

For plain IO streams (not IOContext), always returns `default`.
"""
get(io::IO, key::Symbol, default) = default

"""
    haskey(ctx::IOContext, key::Symbol)

Check if `key` exists in the IOContext properties.

# Example
```julia
ctx = IOContext(stdout, :compact => true)
haskey(ctx, :compact)  # => true
haskey(ctx, :limit)    # => false
```
"""
haskey(ctx::IOContext, key::Symbol) = iohaskey(ctx, key)

"""
    haskey(io::IO, key::Symbol)

For plain IO streams (not IOContext), always returns `false`.
"""
haskey(io::IO, key::Symbol) = false

"""
    iokeys(ctx::IOContext)

Return an array of all property keys in the IOContext.
"""
function iokeys(ctx::IOContext)
    result = Symbol[]
    n = length(ctx.properties)
    i = 1
    while i <= n
        push!(result, ctx.properties[i][1])
        i = i + 1
    end
    return result
end

"""
    iokeys(io::IO)

For plain IO streams, returns an empty array.
"""
iokeys(io::IO) = Symbol[]

# =============================================================================
# IOContext IO Delegation
# =============================================================================
# IOContext acts as a pipe, delegating IO operations to the wrapped stream.

"""
    pipe_reader(io::IOContext)

Return the underlying IO stream for reading.
"""
pipe_reader(ctx::IOContext) = ctx.io

"""
    pipe_writer(io::IOContext)

Return the underlying IO stream for writing.
"""
pipe_writer(ctx::IOContext) = ctx.io

# =============================================================================
# Backward Compatibility - iocontext function
# =============================================================================
# The iocontext() function is an alias for IOContext() and is kept for
# backward compatibility. New code can use either.

"""
    iocontext(io)
    iocontext(io, key => value, ...)

Create an IOContext wrapping `io` with optional properties.

This is the recommended way to create IOContext with properties in SubsetJuliaVM.
Using `IOContext()` directly with properties may not work as expected due to
struct constructor precedence.

# Examples
```julia
ctx = iocontext(io)                                        # empty properties
ctx = iocontext(io, :compact => true)                      # single property
ctx = iocontext(io, :compact => true, :limit => true)      # multiple properties
ctx = iocontext(buf, existing_ctx)                         # inherit from another context
```
"""
function iocontext(io)
    return IOContext(io, _ioproperties(io))
end

function iocontext(io, first_pair::Tuple)
    props = _ioproperties(io)
    new_props = _add_property(props, first_pair[1], first_pair[2])
    return IOContext(io, new_props)
end

function iocontext(io, p1::Tuple, p2::Tuple)
    props = _ioproperties(io)
    props = _add_property(props, p1[1], p1[2])
    props = _add_property(props, p2[1], p2[2])
    return IOContext(io, props)
end

function iocontext(io, p1::Tuple, p2::Tuple, p3::Tuple)
    props = _ioproperties(io)
    props = _add_property(props, p1[1], p1[2])
    props = _add_property(props, p2[1], p2[2])
    props = _add_property(props, p3[1], p3[2])
    return IOContext(io, props)
end

function iocontext(io, p1::Tuple, p2::Tuple, p3::Tuple, p4::Tuple)
    props = _ioproperties(io)
    props = _add_property(props, p1[1], p1[2])
    props = _add_property(props, p2[1], p2[2])
    props = _add_property(props, p3[1], p3[2])
    props = _add_property(props, p4[1], p4[2])
    return IOContext(io, props)
end

function iocontext(io, context::IOContext)
    return IOContext(io, context.properties)
end

# =============================================================================
# sprint - Return string from printing
# =============================================================================
# Based on Julia's base/strings/io.jl
#
# sprint(f, args...; context=nothing, sizehint=0)
#
# Call the function `f` with an IOBuffer and the given arguments,
# returning the resulting string.
#
# Note: Our IOBuffer is immutable (functional style)
# write(io, x) returns a new IOBuffer with x appended
#
# Current implementation: For compatibility, we directly write values
# to the buffer. Full function invocation support (f(io, args...))
# would require VM-level support for passing IOBuffer to Julia functions.

# Single argument: sprint(x) -> string(x)
sprint(x) = take!(write(IOBuffer(), x))

# Varargs version without context: sprint(f, args...)
# Writes all args to buffer (simplified implementation)
# Note: The function f is currently not invoked directly due to VM limitations.
# Instead, we write the args directly as print would.
function sprint(f, args...)
    io = IOBuffer()
    for arg in args
        io = write(io, arg)
    end
    return take!(io)
end

# Internal helper for context-aware sprint
# Called from compile/expr/call.rs when sprint is called with context kwarg
# See Issue #334: IOContext support for sprint
function sprint_context(f, args, context)
    io = IOBuffer()
    io_ctx = nothing
    if isa(context, IOContext)
        io_ctx = IOContext(io, context)
    elseif isa(context, Tuple)
        # Handle :key => value (parsed as Tuple in SubsetJuliaVM)
        io_ctx = iocontext(io, context)
    end

    if !isnothing(io_ctx)
        for arg in args
            io = _write_with_context(io, io_ctx, arg)
        end
    else
        for arg in args
            io = write(io, arg)
        end
    end
    return take!(io)
end

# Helper function to write values respecting IOContext properties
function _write_with_context(io, ctx::IOContext, x)
    # Check for :compact property
    compact = ioget(ctx, :compact, false)

    # Use natural Julia isa() check - Issue #1267 fix
    if compact && isa(x, Float64)
        # Compact mode: limit decimal places for floats
        # Use round to 4 significant digits after decimal point
        s = _compact_float_string(x)
        return write(io, s)
    else
        return write(io, x)
    end
end

# Format a float in compact mode (similar to Julia's compact printing)
function _compact_float_string(x)
    if isnan(x)
        return "NaN"
    elseif isinf(x)
        return x > 0 ? "Inf" : "-Inf"
    elseif x == 0.0
        return "0.0"
    else
        # Round to 4 decimal places for compact display
        s = string(round(x, digits=4))
        return s
    end
end

# =============================================================================
# display - Formatted output of values
# =============================================================================
# display(x) prints a formatted representation of x
# The full display stack implementation is in multimedia.jl
# which provides AbstractDisplay, TextDisplay, pushdisplay, popdisplay,
# and the display function with proper display stack support.

# =============================================================================
# dump - Show internal structure of values
# =============================================================================
# dump(x) shows every part of the representation of a value
# This is useful for debugging and understanding data structures
#
# Implementation Note: This uses explicit isa() checks instead of multiple
# dispatch because runtime dispatch for Any-typed parameters is not fully
# supported in the VM. Workaround: uses isa() checks instead of dispatch.

# Helper to get symbol name without ':' prefix
function _symbol_name(x::Symbol)
    s = string(x)
    if startswith(s, ":")
        return s[2:length(s)]
    else
        return s
    end
end

# Helper to print type name consistently
function _type_name(x)
    t = typeof(x)
    s = string(t)
    # Remove "Vector{" prefix and convert to Array{T}(size,) format
    # This matches Julia's dump output format
    return s
end

# Internal dump implementation using explicit type checking
# This avoids the dispatch limitation with Any-typed parameters
function _dump_impl(x, indent::String, maxdepth::Int64)
    # Check types in order of specificity
    if isa(x, Symbol)
        print("Symbol ")
        println(_symbol_name(x))
    elseif isa(x, Expr)
        println("Expr")
        print(indent)
        print("  head: Symbol ")
        println(_symbol_name(x.head))
        print(indent)
        print("  args: Array{Any}((")
        print(length(x.args))
        println(",))")
        if maxdepth > 0
            newindent = indent * "    "
            for i in 1:length(x.args)
                print(indent)
                print("    ")
                print(i)
                print(": ")
                _dump_impl(x.args[i], newindent, maxdepth - 1)
            end
        end
    elseif isa(x, LineNumberNode)
        # LineNumberNode is a special internal type for source locations
        # Just print its basic info - field access is not well supported for Any-typed params
        println("LineNumberNode")
    elseif isa(x, QuoteNode)
        # QuoteNode is a special internal type for quoted values
        # Just print its basic info - field access is not well supported for Any-typed params
        println("QuoteNode")
    elseif isa(x, Bool)
        # Check Bool before Integer (Bool <: Integer)
        print("Bool ")
        println(x)
    elseif isa(x, Int8)
        print("Int8 ")
        println(x)
    elseif isa(x, Int16)
        print("Int16 ")
        println(x)
    elseif isa(x, Int32)
        print("Int32 ")
        println(x)
    elseif isa(x, Int64)
        print("Int64 ")
        println(x)
    elseif isa(x, UInt8)
        print("UInt8 ")
        println(x)
    elseif isa(x, UInt16)
        print("UInt16 ")
        println(x)
    elseif isa(x, UInt32)
        print("UInt32 ")
        println(x)
    elseif isa(x, UInt64)
        print("UInt64 ")
        println(x)
    elseif isa(x, Float32)
        print("Float32 ")
        println(x)
    elseif isa(x, Float64)
        print("Float64 ")
        println(x)
    elseif isa(x, String)
        print("String ")
        println(repr(x))
    elseif isa(x, Char)
        print("Char ")
        println(repr(x))
    elseif isa(x, Nothing)
        println("Nothing nothing")
    elseif isa(x, Tuple)
        # Print tuple type with element types
        print("Tuple{")
        n = length(x)
        for i in 1:n
            if i > 1
                print(", ")
            end
            print(typeof(x[i]))
        end
        println("}")
        if maxdepth > 0
            newindent = indent * "  "
            for i in 1:n
                print(indent)
                print("  ")
                print(i)
                print(": ")
                _dump_impl(x[i], newindent, maxdepth - 1)
            end
        end
    elseif isa(x, Array)
        # Print array type with element type and size (Julia format)
        print("Array{")
        print(eltype(x))
        print("}((")
        print(length(x))
        print(",)) ")
        # For numeric arrays, show inline representation (like Julia does)
        n = length(x)
        if n <= 10 && eltype(x) <: Number
            print(x)
            println()
        else
            println()
            if maxdepth > 0 && n > 0
                newindent = indent * "  "
                # Limit output for large arrays
                show_count = min(n, 10)
                for i in 1:show_count
                    print(indent)
                    print("  ")
                    print(i)
                    print(": ")
                    _dump_impl(x[i], newindent, maxdepth - 1)
                end
                if n > 10
                    print(indent)
                    println("  ...")
                end
            end
        end
    elseif isa(x, NamedTuple)
        # NamedTuple - show type and fields
        println(typeof(x))
        if maxdepth > 0
            newindent = indent * "  "
            ks = keys(x)
            vs = values(x)
            n = length(ks)
            for i in 1:n
                print(indent)
                print("  ")
                print(string(ks[i]))
                print(": ")
                _dump_impl(vs[i], newindent, maxdepth - 1)
            end
        end
    elseif isstructtype(typeof(x))
        # User-defined struct - show struct name and fields with nested introspection
        # Uses _getfield(x, i) for runtime field access by index
        t = typeof(x)
        println(t)
        if maxdepth > 0
            newindent = indent * "  "
            # Get field names for this type
            names = fieldnames(t)
            n = length(names)
            for i in 1:n
                print(indent)
                print("  ")
                # Get field name - convert to string if it's a Symbol
                fname = names[i]
                if isa(fname, Symbol)
                    print(_symbol_name(fname))
                else
                    print(fname)
                end
                print(": ")
                # Get field value using runtime field access by index
                fval = _getfield(x, i)
                _dump_impl(fval, newindent, maxdepth - 1)
            end
        end
    else
        # Generic fallback for unknown types
        print(typeof(x))
        print(" ")
        println(x)
    end
    nothing
end

# Public API: dump(x), dump(io, x), and dump(x; maxdepth=8)
function dump(x)
    _dump_impl(x, "", 8)
    nothing
end

# dump(io, x) - for use with sprint()
# The io parameter is handled by sprint's output redirection mechanism
function dump(io, x)
    _dump_impl(x, "", 8)
    nothing
end

function dump(x, maxdepth::Int64)
    _dump_impl(x, "", maxdepth)
    nothing
end

# =============================================================================
# displaysize - Terminal display size
# =============================================================================
# Based on Julia's base/stream.jl
#
# displaysize() returns a tuple (rows, columns) representing the terminal size.
# In Julia, this checks environment variables LINES and COLUMNS with defaults.
#
# Note: SubsetJuliaVM doesn't have full ENV support, so we use fixed defaults.
# The IOContext version checks for a :displaysize property first.

"""
    displaysize()

Return a tuple (rows, columns) representing the size of the terminal display.

Returns default values (24, 80) since SubsetJuliaVM doesn't have full
environment variable support.

See also [`IOContext`](@ref) for passing custom display sizes.
"""
function displaysize()
    return (24, 80)
end

"""
    displaysize(io)

Return the size of the display for output to `io`.

For IOContext, checks for a `:displaysize` property first.
Otherwise returns the default display size.
"""
function displaysize(io)
    return displaysize()
end

function displaysize(ctx::IOContext)
    if iohaskey(ctx, :displaysize)
        return ioget(ctx, :displaysize, (24, 80))
    else
        return displaysize(ctx.io)
    end
end

# =============================================================================
# show - Display representation of values
# =============================================================================
# Based on Julia's base/show.jl
#
# show(io, x) writes a representation of x to the IO stream.
# These methods use eltype() to display the correct element type.
#
# Note: Due to VM limitations with typed parameters like ::Matrix,
# we use a single show(io, arr) function with runtime ndims checking.

# Internal helper for showing 1D arrays (vectors)
function _show_vector(io, v)
    n = length(v)
    et = eltype(v)
    print(io, n, "-element Vector{", et, "}:")
    println(io)
    for i in 1:n
        print(io, " ")
        println(io, v[i])
    end
end

# Internal helper for showing 2D arrays (matrices)
function _show_matrix(io, m)
    s = size(m)
    rows = s[1]
    cols = s[2]
    et = eltype(m)
    print(io, rows, "×", cols, " Matrix{", et, "}:")
    println(io)
    for r in 1:rows
        print(io, " ")
        for c in 1:cols
            print(io, m[r, c])
            if c < cols
                print(io, "  ")
            end
        end
        println(io)
    end
end

"""
    show(io::IO, arr::Array)

Display an array with its element type using `eltype`.
For 1D arrays (Vectors), shows "n-element Vector{T}".
For 2D arrays (Matrices), shows "m×n Matrix{T}".
For higher dimensions, shows a summary.

# Examples
```julia
julia> show(stdout, [1, 2, 3])
3-element Vector{Int64}:
 1
 2
 3

julia> show(stdout, [1 2; 3 4])
2×2 Matrix{Int64}:
 1  2
 3  4
```
"""
function show(io::IO, arr::Array)
    nd = ndims(arr)
    if nd == 1
        _show_vector(io, arr)
    elseif nd == 2
        _show_matrix(io, arr)
    else
        # Higher dimensional arrays - show summary
        s = size(arr)
        et = eltype(arr)
        print(io, "Array{", et, ", ", nd, "} with size ", s)
        println(io)
    end
end

# =============================================================================
# show - Basic Types
# =============================================================================
# Based on Julia's base/show.jl
#
# These show methods handle the 2-argument form: show(io, x)
# They write a textual representation of x to the IO stream.

"""
    show(io::IO, x::Bool)

Display a Bool value as "true" or "false".
"""
show(io::IO, x::Bool) = print(io, string(x))

"""
    show(io::IO, ::Nothing)

Display the nothing value.
"""
show(io::IO, ::Nothing) = print(io, "nothing")

"""
    show(io::IO, x::Int8)

Display an Int8 value.
"""
show(io::IO, x::Int8) = print(io, x)

"""
    show(io::IO, x::Int16)

Display an Int16 value.
"""
show(io::IO, x::Int16) = print(io, x)

"""
    show(io::IO, x::Int32)

Display an Int32 value.
"""
show(io::IO, x::Int32) = print(io, x)

"""
    show(io::IO, x::Int64)

Display an Int64 value.
"""
show(io::IO, x::Int64) = print(io, x)

"""
    show(io::IO, x::Int128)

Display an Int128 value.
"""
show(io::IO, x::Int128) = print(io, x)

"""
    show(io::IO, x::UInt8)

Display a UInt8 value in hexadecimal format.
"""
show(io::IO, x::UInt8) = print(io, "0x", string(x, base=16, pad=2))

"""
    show(io::IO, x::UInt16)

Display a UInt16 value in hexadecimal format.
"""
show(io::IO, x::UInt16) = print(io, "0x", string(x, base=16, pad=4))

"""
    show(io::IO, x::UInt32)

Display a UInt32 value in hexadecimal format.
"""
show(io::IO, x::UInt32) = print(io, "0x", string(x, base=16, pad=8))

"""
    show(io::IO, x::UInt64)

Display a UInt64 value in hexadecimal format.
"""
show(io::IO, x::UInt64) = print(io, "0x", string(x, base=16, pad=16))

"""
    show(io::IO, x::UInt128)

Display a UInt128 value in hexadecimal format.
"""
show(io::IO, x::UInt128) = print(io, "0x", string(x, base=16, pad=32))

"""
    show(io::IO, x::Float16)

Display a Float16 value.
"""
show(io::IO, x::Float16) = print(io, x)

"""
    show(io::IO, x::Float32)

Display a Float32 value.
"""
show(io::IO, x::Float32) = print(io, x)

"""
    show(io::IO, x::Float64)

Display a Float64 value.
"""
show(io::IO, x::Float64) = print(io, x)

"""
    show(io::IO, x::Char)

Display a Char value with single quotes.
"""
show(io::IO, x::Char) = print(io, "'", x, "'")

"""
    show(io::IO, x::String)

Display a String value with double quotes.
"""
show(io::IO, x::String) = print(io, '"', x, '"')

"""
    show(io::IO, x::Symbol)

Display a Symbol value with a colon prefix.
"""
show(io::IO, x::Symbol) = print(io, x)

# =============================================================================
# show - Container Types
# =============================================================================

"""
    show(io::IO, t::Tuple)

Display a Tuple with parentheses. Single-element tuples have a trailing comma.
"""
function show(io::IO, t::Tuple)
    print(io, "(")
    n = length(t)
    for i in 1:n
        show(io, t[i])
        if i < n
            print(io, ", ")
        elseif n == 1
            print(io, ",")
        end
    end
    print(io, ")")
end

"""
    show(io::IO, p::Pair)

Display a Pair with the => operator.
"""
function show(io::IO, p::Pair)
    show(io, p[1])
    print(io, " => ")
    show(io, p[2])
end

"""
    show(io::IO, nt::NamedTuple)

Display a NamedTuple with named fields.
"""
function show(io::IO, nt::NamedTuple)
    print(io, "(")
    ks = keys(nt)
    vs = values(nt)
    n = length(ks)
    for i in 1:n
        print(io, ks[i], " = ")
        show(io, vs[i])
        if i < n
            print(io, ", ")
        end
    end
    if n == 1
        print(io, ",")
    end
    print(io, ")")
end

# =============================================================================
# show - Range Types
# =============================================================================

"""
    show(io::IO, r::UnitRange)

Display a UnitRange as start:stop.
"""
function show(io::IO, r::UnitRange)
    show(io, first(r))
    print(io, ":")
    show(io, last(r))
end

"""
    show(io::IO, r::StepRange)

Display a StepRange as start:step:stop.
"""
function show(io::IO, r::StepRange)
    show(io, first(r))
    print(io, ":")
    show(io, step(r))
    print(io, ":")
    show(io, last(r))
end

# =============================================================================
# show - Numeric Types (Complex, Rational)
# =============================================================================

"""
    show(io::IO, z::Complex)

Display a Complex number in the form "a + bi" or "a - bi".
"""
function show(io::IO, z::Complex)
    r = real(z)
    i = imag(z)
    show(io, r)
    if i < 0
        print(io, " - ")
        show(io, -i)
    else
        print(io, " + ")
        show(io, i)
    end
    print(io, "im")
end

"""
    show(io::IO, x::Rational)

Display a Rational number as numerator//denominator.
"""
function show(io::IO, x::Rational)
    show(io, numerator(x))
    print(io, "//")
    show(io, denominator(x))
end

# Rational{BigInt} show specialization (Issue #2497)
function show(io::IO, x::Rational{BigInt})
    print(io, x.num)
    print(io, "//")
    print(io, x.den)
end

# =============================================================================
# print - Write human-readable output
# =============================================================================
# Based on Julia's base/strings/io.jl
#
# print writes a human-readable representation of values.
# Unlike show, print does NOT add quotes around strings/chars.
#
# The key difference from show:
# - show(io, "hello") → "hello" (with quotes)
# - print(io, "hello") → hello (without quotes)
#
# Note: The basic print functionality is handled by Rust builtins for
# efficiency. These Julia methods document the expected behavior and can
# be extended for user-defined types.
#
# For user-defined types, the default is to call show (matching Julia).

"""
    print(io::IO, x)

Write a human-readable representation of `x` to `io`.

For most types, this delegates to `show(io, x)`. The exceptions are:
- String: printed without quotes
- Char: printed without quotes

This is the semantic meaning - the actual implementation uses optimized
Rust builtins for basic types.

# Examples
```julia
julia> print(stdout, "hello")
hello
julia> print(stdout, 'a')
a
julia> print(stdout, 42)
42
```
"""
# Note: print(io, x) is handled by Rust builtin for basic types

"""
    println(io::IO, xs...)

Print values to `io`, followed by a newline.

Equivalent to `print(io, xs...); print(io, '\\n')`.
"""
# Note: println is handled by Rust builtin

# =============================================================================
# repr - String representation of values
# =============================================================================
# Based on Julia's base/strings/io.jl
#
# repr(x) returns a string representation of x, typically by calling
# show(io, x). The result should be a valid Julia expression that can
# be parsed back.
#
# For String and Char values, repr adds quotes around the value.
# For other values, it uses the show function output.

"""
    repr(x)

Return a string representation of the value `x`.

For strings, returns the value with quotes. For other types,
returns the output of `show(io, x)`.

The output should be parseable Julia code that recreates the value:
```julia
julia> repr("hello")
"\"hello\""

julia> repr(42)
"42"

julia> repr([1, 2, 3])
"[1, 2, 3]"
```

# Examples
```julia
julia> repr(1)
"1"

julia> repr(:symbol)
":symbol"

julia> repr((1, 2))
"(1, 2)"
```
"""
function repr(x)
    io = IOBuffer()
    show(io, x)
    return take!(io)
end

# =============================================================================
# summary - Return a string giving a brief description of a value
# =============================================================================
# summary(x) returns a string describing the type of x.
# For arrays, it returns a description like "3-element Vector{Int64}".
#
# Examples:
# ```julia
# julia> summary(1)
# "Int64"
#
# julia> summary([1, 2, 3])
# "3-element Vector{Int64}"
#
# julia> summary(zeros(2, 3))
# "2×3 Matrix{Float64}"
# ```

# Generic fallback: return the type name as a string
function summary(x)
    return string(typeof(x))
end

# Two-argument form: write summary to IO stream
function summary(io::IO, x)
    print(io, typeof(x))
end

# Specialized summary for arrays: "N-element Vector{T}" or "M×N Matrix{T}"
function summary(a::AbstractArray)
    dims = size(a)
    ndim = ndims(a)
    if ndim == 1
        return string(dims[1], "-element ", typeof(a))
    elseif ndim == 2
        return string(dims[1], "×", dims[2], " ", typeof(a))
    else
        # For higher dimensions, use ×-separated sizes
        dimstr = join(dims, "×")
        return string(dimstr, " ", typeof(a))
    end
end

# IO form for arrays
function summary(io::IO, a::AbstractArray)
    print(io, summary(a))
end
