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
# Normalize properties stored by direct IOContext constructor calls.
# The implicit struct constructor can store a single Pair directly
# (`IOContext(io, :key => value)`, Issue #6409) or an existing IOContext
# (`IOContext(io, ctx)`, Issue #6467), while Julia's public constructor API
# treats both forms as property collections.
function _normalize_ioproperties(props)
    if isa(props, Pair)
        return [props]
    elseif isa(props, IOContext)
        return _ioproperties(props)
    else
        return props
    end
end

# Helper function to get properties from an IO.
# Returns empty array for plain IO, or normalized properties from IOContext.
function _ioproperties(io)
    if isa(io, IOContext)
        return _normalize_ioproperties(io.properties)
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

function _iocontext_with_pair(io, pair)
    props = _ioproperties(io)
    if isa(pair, Pair)
        props = _add_property(props, pair.first, pair.second)
    elseif length(pair) > 0 && isa(pair[1], Pair)
        i = 1
        n = length(pair)
        while i <= n
            p = pair[i]
            props = _add_property(props, p.first, p.second)
            i = i + 1
        end
    else
        props = _add_property(props, pair[1], pair[2])
    end
    return IOContext(io, props)
end

function _iocontext_with_pairs(io, p1, p2)
    props = _ioproperties(io)
    props = _add_property(props, p1[1], p1[2])
    props = _add_property(props, p2[1], p2[2])
    return IOContext(io, props)
end

function _iocontext_with_pairs(io, p1, p2, p3)
    props = _ioproperties(io)
    props = _add_property(props, p1[1], p1[2])
    props = _add_property(props, p2[1], p2[2])
    props = _add_property(props, p3[1], p3[2])
    return IOContext(io, props)
end

function _iocontext_with_pairs(io, p1, p2, p3, p4)
    props = _ioproperties(io)
    props = _add_property(props, p1[1], p1[2])
    props = _add_property(props, p2[1], p2[2])
    props = _add_property(props, p3[1], p3[2])
    props = _add_property(props, p4[1], p4[2])
    return IOContext(io, props)
end

IOContext(io::IOContext) = io
IOContext(io) = IOContext(io, _ioproperties(io))
IOContext(io, context::IOContext) = IOContext(io, _ioproperties(context))
IOContext(io, pair::Pair) = _iocontext_with_pair(io, pair)
IOContext(io, pair::Tuple) = _iocontext_with_pair(io, pair)
IOContext(io, p1::Pair, p2::Pair) = _iocontext_with_pairs(io, p1, p2)
IOContext(io, p1::Tuple, p2::Tuple) = _iocontext_with_pairs(io, p1, p2)
IOContext(io, p1::Pair, p2::Pair, p3::Pair) = _iocontext_with_pairs(io, p1, p2, p3)
IOContext(io, p1::Tuple, p2::Tuple, p3::Tuple) = _iocontext_with_pairs(io, p1, p2, p3)
IOContext(io, p1::Pair, p2::Pair, p3::Pair, p4::Pair) = _iocontext_with_pairs(io, p1, p2, p3, p4)
IOContext(io, p1::Tuple, p2::Tuple, p3::Tuple, p4::Tuple) = _iocontext_with_pairs(io, p1, p2, p3, p4)

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
    props = _ioproperties(ctx)
    n = length(props)
    i = 1
    while i <= n
        p = props[i]
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
    props = _ioproperties(ctx)
    n = length(props)
    i = 1
    while i <= n
        p = props[i]
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
    props = _ioproperties(ctx)
    n = length(props)
    i = 1
    while i <= n
        push!(result, props[i][1])
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

This helper is kept for backward compatibility with older SubsetJuliaVM code.
New code can use the standard `IOContext(io, :key => value, ...)` constructor.

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

function iocontext(io, first_pair::Pair)
    return _iocontext_with_pair(io, first_pair)
end

function iocontext(io, first_pair::Tuple)
    return _iocontext_with_pair(io, first_pair)
end

function iocontext(io, p1::Pair, p2::Pair)
    return _iocontext_with_pairs(io, p1, p2)
end

function iocontext(io, p1::Tuple, p2::Tuple)
    return _iocontext_with_pairs(io, p1, p2)
end

function iocontext(io, p1::Pair, p2::Pair, p3::Pair)
    return _iocontext_with_pairs(io, p1, p2, p3)
end

function iocontext(io, p1::Tuple, p2::Tuple, p3::Tuple)
    return _iocontext_with_pairs(io, p1, p2, p3)
end

function iocontext(io, p1::Pair, p2::Pair, p3::Pair, p4::Pair)
    return _iocontext_with_pairs(io, p1, p2, p3, p4)
end

function iocontext(io, p1::Tuple, p2::Tuple, p3::Tuple, p4::Tuple)
    return _iocontext_with_pairs(io, p1, p2, p3, p4)
end

function iocontext(io, context::IOContext)
    return IOContext(io, _ioproperties(context))
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
# Single argument: sprint(x) -> string(x)
function sprint(x)
    io = IOBuffer()
    print(io, x)
    return String(take!(io))
end

# Varargs version without context: sprint(f, args...)
function sprint(f, args...)
    io = IOBuffer()
    f(io, args...)
    return String(take!(io))
end

# Internal helper for context-aware sprint
# Called from compile/expr/call.rs when sprint is called with context kwarg
# See Issue #334: IOContext support for sprint
function sprint_context(f, args, context)
    io = IOBuffer()
    io_ctx = nothing
    if isa(context, IOContext)
        io_ctx = IOContext(io, context)
    elseif isa(context, Pair)
        # Handle :key => value keyword context.
        io_ctx = iocontext(io, context)
    elseif isa(context, Tuple)
        # Older lowering paths may still pass :key => value as a Tuple.
        io_ctx = iocontext(io, context)
    end

    if !isnothing(io_ctx)
        if length(args) == 1 && isa(args[1], Float64)
            _write_with_context(io, io_ctx, args[1])
        else
            f(io_ctx, args...)
        end
    else
        f(io, args...)
    end
    return String(take!(io))
end

function _redirect_stdio_call(f, stderr_stream)
    if stderr_stream === nothing
        return f()
    else
        return redirect_stderr(f, stderr_stream)
    end
end

function redirect_stdio(; stdin=nothing, stderr=nothing, stdout=nothing)
    if !(stdin === nothing)
        throw(ArgumentError("redirect_stdio stdin is not supported"))
    end
    if !(stderr === nothing)
        redirect_stderr(stderr)
    end
    if !(stdout === nothing)
        redirect_stdout(stdout)
    end
    return nothing
end

function redirect_stdio(f; stdin=nothing, stderr=nothing, stdout=nothing)
    if !(stdin === nothing)
        throw(ArgumentError("redirect_stdio stdin is not supported"))
    end
    if stdout === nothing
        return _redirect_stdio_call(f, stderr)
    else
        return redirect_stdout(() -> _redirect_stdio_call(f, stderr), stdout)
    end
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
        write(io, s)
    else
        print(io, x)
    end
    return io
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

# Whether a *value* `x`'s concrete type is "implicit" for array-show purposes,
# mirroring upstream Julia's `typeinfo_implicit` (`julia/base/arrayshow.jl`).
# Implicit values (`Int64`/`Float64`/`Char`/`String`/`Symbol`, and `Tuple`/`Pair`
# whose components are all implicit) print WITHOUT a `T[...]` prefix; everything
# else (other numeric widths, `Bool`, `Complex`, user structs, …) is prefixed.
#
# This value-driven path is still needed for arrays whose element type is widened
# to `Any` even though upstream would infer a precise composite eltype. In that
# case the implicit-ness of each `Pair`/`Tuple`/`NamedTuple` element is decided
# from its actual value.
function _value_typeinfo_implicit(@nospecialize(x))
    T = typeof(x)
    (T === Float64 || T === Int64 || T === Char || T === String || T === Symbol) && return true
    if x isa NamedTuple
        return _type_parameters_typeinfo_implicit(T)
    end
    if x isa Pair
        return _value_typeinfo_implicit(x.first) && _value_typeinfo_implicit(x.second)
    end
    if x isa Tuple
        for e in x
            _value_typeinfo_implicit(e) || return false
        end
        return true
    end
    # Nested arrays: `Array{T,N}` of an implicit eltype is implicit
    # (upstream `typeinfo_implicit`), so `[[1, 2], [3, 4]]` prints bare.
    if x isa AbstractArray
        return _type_typeinfo_implicit(eltype(x))
    end
    return false
end

# Per-element implicit/type info for value-driven prefix derivation over
# `Any`-eltype arrays (`Any[1, "x"]` etc.). Returns `(typename::String,
# implicit::Bool)`. For homogeneous implicit elements the type name is unused
# (the prefix is dropped); for non-implicit elements it becomes the `T[...]`
# prefix (e.g. `Foo`, `Complex{Int64}`).
function _elem_show_type(@nospecialize(x))
    return (string(typeof(x)), _value_typeinfo_implicit(x))
end

# Whether a (non-`Any`) element *type* `T` is implicit. Used for arrays whose
# `eltype` is precise: implicit scalars, arrays/dicts with implicit element or
# key/value types, and tuple-like containers whose field types are all implicit
# print bare; other concrete types keep the `T[...]` prefix.
function _type_parameters_typeinfo_implicit(@nospecialize(T))
    for FT in T.parameters
        _type_typeinfo_implicit(FT) || return false
    end
    return true
end

function _fieldtypes_typeinfo_implicit(@nospecialize(T))
    for FT in fieldtypes(T)
        _type_typeinfo_implicit(FT) || return false
    end
    return true
end

function _type_typeinfo_implicit(@nospecialize(T))
    (T === Float64 || T === Int64 || T === Char || T === String || T === Symbol) && return true
    isconcretetype(T) || return false
    if T <: AbstractArray
        return _type_typeinfo_implicit(eltype(T))
    end
    if T <: Pair || T <: Tuple
        return _fieldtypes_typeinfo_implicit(T)
    end
    if T <: NamedTuple
        return _type_parameters_typeinfo_implicit(T)
    end
    if T <: Dict
        return _type_parameters_typeinfo_implicit(T)
    end
    return false
end

# Whether a *value* `x` has a type that sjulia widens to `Any` in an array
# literal where upstream Julia would have inferred a precise element type —
# `Pair`, `Tuple`, and nested `AbstractArray` (see docs/vm/UNIMPLEMENTED.md).
# These are the only element kinds for which the value-driven prefix derivation
# may drop the `Any[...]` prefix; a *scalar* element under an `Any` eltype means
# an explicit `Any[...]` literal, which keeps its prefix (Issue #7303).
function _value_is_inference_widened_composite(@nospecialize(x))
    return x isa Pair || x isa Tuple || x isa NamedTuple || x isa AbstractArray
end

# Compute the array-show type prefix and whether the eltype is implicit,
# mirroring upstream `typeinfo_prefix`/`typeinfo_implicit`. For a precise
# (non-`Any`) eltype the answer comes straight from `eltype(v)`; for an `Any`
# eltype the effective type is derived from the element values. A genuine
# `Vector{Any}` keeps the `Any[...]` prefix (upstream's `typeinfo_implicit(Any)`
# is `false`, so `Any[1, 2, 3]` prints `Any[...]`, not bare) — the prefix is
# dropped only for a homogeneous run of an inference-widened composite eltype
# (`Pair`/`Tuple`/nested array, e.g. `[1 => 2]`) that sjulia stores under the
# `Any` tag but upstream infers precisely. Issues #5236 / #5237 / #7303.
function _array_show_prefix(v)
    et = eltype(v)
    if et !== Any
        return _type_typeinfo_implicit(et) ? ("", true) : (string(et), false)
    end
    # Any eltype: derive from element values.
    n = length(v)
    n == 0 && return ("Any", false)
    name, implicit = _elem_show_type(v[firstindex(v)])
    all_widened = _value_is_inference_widened_composite(v[firstindex(v)])
    for i in (firstindex(v) + 1):lastindex(v)
        nm, im = _elem_show_type(v[i])
        if nm != name
            return ("Any", false)
        end
        implicit = implicit && im
        all_widened = all_widened && _value_is_inference_widened_composite(v[i])
    end
    # A homogeneous *scalar* implicit run under an `Any` eltype is an explicit
    # `Any[...]` literal → keep the `Any[...]` prefix; only inference-widened
    # composites drop it.
    implicit || return (name, false)
    return all_widened ? ("", true) : ("Any", false)
end

# Render one array element honoring upstream's `:typeinfo`-aware show: when the
# array carries a `Float32`/`Float16` type prefix, the per-element decorations
# (`1.5f0`, `Float16(1.5)`) are dropped because the context already records the
# eltype (e.g. `Float32[1.0, 2.0]`). Bool elements render as `1`/`0`.
function _show_array_elem(io, x, et)
    if et === Bool
        print(io, x ? "1" : "0")
    elseif et === Float32 || et === Float16
        print(io, x)
    else
        show(io, x)
    end
end

# Internal helper for showing 1D arrays (vectors) — compact 2-arg
# `show(io, v)` form: "[a, b, c]" (Issue #4731). Matches upstream
# Julia's `show(io, ::AbstractVector)`, which prints inline without
# newlines so `repr(v)` returns "[1, 2, 3]".
#
# Issue #4733: empty typed vectors render as "<eltype>[]" (e.g.
# "Int64[]"), preserving the element type the way upstream Julia
# does. Without this special case the compact form would drop the
# type info and an `eval(Meta.parse(...))` round-trip would land in
# `Vector{Any}` instead of the original `Vector{T}`.
#
# Issues #5236 / #5237: non-implicit eltypes carry the upstream
# `typeinfo_prefix` type prefix (`Int8[...]`, `Float32[...]`,
# `Complex{Int64}[...]`, `Foo[...]`, `Bool[1, 0]`, `Any[1, "x"]`),
# while implicit eltypes (`Int64`/`Float64`/`Char`/`String`/`Symbol`/
# implicit `Tuple`/`Pair`) print bare (`[1, 2]`, `[1 => 2]`). See
# `_array_show_prefix` / `_typeinfo_implicit_T`.
function _show_vector_compact(io, v)
    n = length(v)
    if n == 0
        print(io, eltype(v), "[]")
        return
    end
    prefix, _implicit = _array_show_prefix(v)
    et = eltype(v)
    print(io, prefix, "[")
    for i in 1:n
        _show_array_elem(io, v[i], et)
        if i < n
            print(io, ", ")
        end
    end
    print(io, "]")
end

# Internal helper for showing 2D arrays (matrices) — compact 2-arg
# `show(io, m)` form: "[1 2; 3 4]" (Issue #4731). For empty matrices
# (Issue #4733), match upstream's `Matrix{T}(undef, rows, cols)`
# constructor form so the element type and dimensions are preserved
# across round-trips.
#
# Issues #5236 / #5237: non-implicit eltypes carry the upstream
# `typeinfo_prefix` type prefix (`Bool[1 0; 0 1]`,
# `Complex{Int64}[1 + 1im 2 + 2im; ...]`, `Any[...]`), while implicit
# eltypes print bare. Empty matrices keep the
# `Matrix{T}(undef, r, c)` form above, which already matches upstream.
function _show_matrix_compact(io, m)
    s = size(m)
    rows = s[1]
    cols = s[2]
    if rows == 0 || cols == 0
        print(io, "Matrix{", eltype(m), "}(undef, ", rows, ", ", cols, ")")
        return
    end
    prefix, _implicit = _array_show_prefix(m)
    et = eltype(m)
    print(io, prefix, "[")
    for r in 1:rows
        for c in 1:cols
            _show_array_elem(io, m[r, c], et)
            if c < cols
                print(io, " ")
            end
        end
        if r < rows
            print(io, "; ")
        end
    end
    print(io, "]")
end

"""
    show(io::IO, arr::Array)

Display an array in its compact form (Issue #4731). For 1D arrays
this is `"[a, b, c]"`; for 2D arrays `"[a b; c d]"`. The multi-line
"n-element Vector{T}:" / "m×n Matrix{T}:" form is the
`MIME"text/plain"` representation used by REPL display, kept in
`_show_vector` and `_show_matrix` for that purpose.

# Examples
```julia
julia> show(stdout, [1, 2, 3])
[1, 2, 3]

julia> show(stdout, [1 2; 3 4])
[1 2; 3 4]
```
"""
function show(io::IO, arr::Array)
    nd = ndims(arr)
    if nd == 1 || nd == 2
        # Issue #7893: route the compact 1D/2D form through `print(io, arr)`.
        # The VM's `print` path renders each array element via its registered
        # `Base.show(io, ::T)` (e.g. `Symbolics.Num`), which the per-element
        # `show(io, x)` inside `_show_vector_compact`/`_show_matrix_compact`
        # cannot do: a direct `show` call in this Base-library function freezes
        # its candidate method set at Base-compile time, so a user `show`
        # registered later is never a dispatch candidate and the element falls
        # to the generic struct dump. `print(io, arr)` and the pure-Julia
        # compact helpers produce identical output for every other eltype
        # (numbers/strings/chars/symbols/nested containers all quote/format the
        # same), so this only changes the previously-wrong struct-element case.
        print(io, arr)
    else
        # Higher dimensional arrays: delegate to the VM `print` path like the
        # 2-d branch above — it renders upstream's nested `;;`-literal compact
        # form (`[0.0 0.0; 0.0 0.0;;; 0.0 0.0; 0.0 0.0]`) for rank >= 3
        # (Issue #10385; previously printed a nonstandard
        # "Array{T, N} with size (...)" summary).
        print(io, arr)
    end
end

# =============================================================================
# show - Containers
# =============================================================================
# Based on Julia's base/dict.jl and base/show.jl
#
# `show(io::IO, d::AbstractDict)` writes the compact `Dict(...)` form.
# Without this method, dispatching `show(io, d)` falls onto whichever
# AbstractUser-typed `show` happens to match an Any-shaped Dict — the
# stdlib `show(io, ::CartesianIndex)` was wrongly picked, then crashed
# trying to `getfield(d, 1)` on the Dict (Issue #4737). The format
# matches `format_dict_value` from PR #4736 (Issue #4735) so `repr(d)`
# and `string(d)` agree.

"""
    show(io::IO, d::AbstractDict)

Compact `Dict("key" => value, ...)` display, matching upstream Julia's
2-arg `show` form. Quotes String keys and values, prefixes `:` on
Symbol keys (Issue #4737, follows PR #4736).
"""
function _show_dict_compact(io::IO, d)
    print(io, "Dict(")
    first = true
    for (k, v) in d
        if !first
            print(io, ", ")
        end
        first = false
        show(io, k)
        print(io, " => ")
        show(io, v)
    end
    print(io, ")")
end

show(io::IO, d::AbstractDict) = _show_dict_compact(io, d)
show(io::IO, d::Dict{K,V}) where {K,V} = _show_dict_compact(io, d)

"""
    show(io::IO, s::AbstractSet)

Compact `Set([e1, e2, ...])` display (Issue #4739). Without this
method, `repr(Set(...))` would mis-dispatch to
`show(IO, CartesianIndex)` (same family of dispatch crashes as the
Dict case fixed in Issue #4737).
"""
function show(io::IO, s::AbstractSet)
    print(io, "Set([")
    first = true
    for x in s
        if !first
            print(io, ", ")
        end
        first = false
        show(io, x)
    end
    print(io, "])")
end

# =============================================================================
# show - Basic Types
# =============================================================================
# Based on Julia's base/show.jl
#
# These show methods handle the 2-argument form: show(io, x)
# They write a textual representation of x to the IO stream.

"""
    show(x)

Single-argument `show` writes a textual representation of `x` to the standard
output stream, mirroring upstream Julia's `show(x) = show(stdout::IO, x)`
(Issue #4988). This lets `show(m)` and other reflection probes run without an
explicit IO argument.
"""
show(x) = show(stdout, x)

# Workaround: `Irrational{sym}`'s bare symbol name (`"π"`, `"ℯ"`, ...), as plain text. (Issue #8869)
# Upstream reads this via `sym` directly (`show(io::IO,
# x::Irrational{sym}) where {sym} = print(io, sym)`,
# `julia/base/irrationals.jl`) — a Symbol-valued `where`-clause type
# variable, or equivalently `typeof(x).parameters[1]`. Both currently lose
# `Symbol` identity for *non-ASCII* symbols in sjulia (`typeof` reports
# `DataType`, and `print`/`string` render the quoted `:sym` show-form instead
# of the bare name) — exactly the case that matters here, since every
# `Irrational` singleton (`π`, `ℯ`, ...) is named with a non-ASCII symbol.
# Parse the symbol out of `string(typeof(x))` (`"Irrational{:π}"`) instead:
# that string is built from the correctly-encoded type name text, not the
# broken value-parameter reflection. (Issue #8869)
function _irrational_symbol_text(x::AbstractIrrational)
    type_name = string(typeof(x))
    # Keep both public range endpoints on character starts (Issue #11618).
    symbol_start = ncodeunits("Irrational{:") + 1
    closing_brace = prevind(type_name, ncodeunits(type_name) + 1)
    symbol_end = prevind(type_name, closing_brace)
    return type_name[symbol_start:symbol_end]
end

"""
    show(io::IO, x)

Generic fallback for user-defined structs (Issue #4768). Without it,
`repr(user_struct)` had no matching `show(io, ::T)` method and
silently mis-dispatched to a built-in arm that tried to index the
value, producing "indexing not supported for I64(N)".

Mirrors upstream Julia's default `Base.show_default`: print the
type name, then a parenthesized list of field values in show form.
The specific arms below for built-in types (`::Bool`, `::Int8`,
`::String`, `::Pair`, `::Dict`, ...) take precedence via the
most-specific-method rule.
"""
function show(io::IO, x)
    # Irrational singletons (π, ℯ) reach this generic fallback when called with a
    # statically-Any argument (e.g. inside `repr`'s `show(io, x)` or
    # `sprint(show, x)`), bypassing the more specific `show(io, ::Irrational)`
    # method below. Catch them at runtime (Issue #5656). Route through
    # `_irrational_symbol_text` rather than `string(x)` (which would re-enter
    # this same fallback and recurse forever, Issue #8875).
    if x isa AbstractIrrational
        print(io, _irrational_symbol_text(x))
    elseif x isa Array
        # Statically-Any call sites such as `sprint(show, collect(...))` can
        # resolve to this fallback before the runtime Array value is known.
        # Preserve the specific compact Array show form by delegating to the VM
        # print formatter used by `show(io::IO, arr::Array)` above (Issue #8819).
        print(io, x)
    else
        # Keep statically-Any show calls on the VM print path. That path can
        # re-enter a registered `Base.show(io, ::T)` using the runtime value,
        # while still falling back to the default struct field display when no
        # user method exists (Issues #9364/#9456).
        print(io, x)
    end
end

"""
    show(io::IO, x::Type)

Display a Type value by its name (Issue #5010). Without this method,
Type values (e.g. `Symbol`, `typeof(:foo)`, `Int64`) fell through to
the generic struct fallback `show(io::IO, x)`, which printed
`typeof(x)` = `DataType` followed by a parenthesized field list,
producing the wrong `DataType()` text in `repr`/`show`.

`string(::Type)` already renders the correct name, so we route through
it. The most-specific-method rule selects this `::Type` arm over the
generic `show(io::IO, x)` fallback.
"""
show(io::IO, x::Type) = print(io, string(x))

# Irrational singletons (π, ℯ, ...) show as their symbol name, not the generic
# `Irrational{:π}()` struct dump (Issue #5656). Bound directly to the concrete
# `Irrational` type (not the abstract `AbstractIrrational`) and routing
# through `_irrational_symbol_text` rather than `string(x)` (as this used to):
# `string(x)`'s single-arg fast path can now resolve a `show` method through
# an abstract supertype (Issue #8875), so calling it here would re-enter this
# same method forever.
show(io::IO, x::Irrational) = print(io, _irrational_symbol_text(x))

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
    show(io::IO, ::Missing)

Display the missing value (Issue #4743). Without an explicit method,
`repr(missing)` would mis-dispatch into
`show(io, ::CartesianIndex)` (same family as Dict #4737 / Set #4739)
and crash on `getfield(missing, 1)`.
"""
show(io::IO, ::Missing) = print(io, "missing")

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

Display a Float16 value with the `Float16(...)` constructor wrapper
(Issue #4747). This preserves the element type across `repr` round
trips: `eval(Meta.parse(repr(Float16(1.5)))) === Float16(1.5)`.
Special values Inf16 / -Inf16 / NaN16 are written without the wrapper
(Issue #8884): `repr(Float16(Inf)) === "Inf16"`.
"""
function show(io::IO, x::Float16)
    if isnan(x)
        print(io, "NaN16")
    elseif isinf(x)
        print(io, x < 0 ? "-Inf16" : "Inf16")
    else
        print(io, "Float16(", x, ")")
    end
end

"""
    show(io::IO, x::Float32)

Display a Float32 value with the `f0` typed-literal suffix
(Issue #4747). This preserves the element type across `repr` round
trips: `eval(Meta.parse(repr(Float32(1.5)))) === Float32(1.5)`.
Special values Inf32 / -Inf32 / NaN32 are written without the suffix
(Issue #8884): `repr(Float32(Inf)) === "Inf32"`.
"""
function show(io::IO, x::Float32)
    if isnan(x)
        print(io, "NaN32")
    elseif isinf(x)
        print(io, x < 0 ? "-Inf32" : "Inf32")
    else
        s = string(x)
        if occursin("e", s)
            print(io, replace(s, "e" => "f"))
        else
            print(io, s, "f0")
        end
    end
end

"""
    show(io::IO, x::Float64)

Display a Float64 value.
"""
show(io::IO, x::Float64) = print(io, x)

"""
    show(io::IO, x::BigInt)

Display a BigInt value (Issue #3530 — show on BigInt was not previously
exercised because literals were narrowed to Int64).
"""
show(io::IO, x::BigInt) = print(io, x)

"""
    show(io::IO, x::BigFloat)

Display a BigFloat value (Issue #3530).
"""
show(io::IO, x::BigFloat) = print(io, x)

"""
    show(io::IO, x::Char)

Display a Char value with single quotes, escaping special characters
(newline, tab, backslash, single quote, etc.) so the result is a
valid Julia source literal (Issue #4749).
"""
function show(io::IO, x::Char)
    if !isvalid(x)
        # Malformed Char from invalid UTF-8 (Issue #8995): upstream shows each
        # raw pattern byte as a \xNN escape (e.g. show('\xff') → '\xff').
        print(io, _char_repr_invalid(x))
    elseif x == '\\'
        print(io, "'\\\\'")
    elseif x == '\''
        print(io, "'\\''")
    elseif x == '\n'
        print(io, "'\\n'")
    elseif x == '\r'
        print(io, "'\\r'")
    elseif x == '\t'
        print(io, "'\\t'")
    elseif x == '\0'
        print(io, "'\\0'")
    else
        print(io, "'", x, "'")
    end
end

"""
    show(io::IO, x::String)

Display a String value with double quotes, escaping special
characters via `escape_string` so the result is a valid Julia
source literal (Issue #4749).
"""
show(io::IO, x::String) = print(io, '"', escape_string(x), '"')

"""
    show(io::IO, x::Symbol)

Display a Symbol value with a leading `:` prefix. Distinct from
`print(io, ::Symbol)`, which writes only the bare name. Before
Issue #4741 the `:` happened to come from sjulia's print path
treating Symbols as show-form, so `show(io, x) = print(io, x)`
accidentally produced the right output. After PR #4742 (#4741)
print is correctly bare, so `show` must write the `:` explicitly.
"""
show(io::IO, x::Symbol) = print(io, ":", x)

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
    if n == 0
        print(io, ")")
    else
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
end

"""
    show(io::IO, p::Pair)

Display a Pair with the => operator.
"""
_pair_first_any(p) = p[1]
_pair_second_any(p) = p[2]

function show(io::IO, p::Pair)
    show(io, _pair_first_any(p))
    print(io, " => ")
    show(io, _pair_second_any(p))
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
        # Issue #4739: print bare field name like "x = 1", not ":x = 1".
        # sjulia's `print(io, sym)` falls back to the show-form (with `:`
        # prefix) for Symbols, so route through `string(sym)` to drop
        # the colon — matches upstream's `(x = 1, y = 2)` NamedTuple repr.
        print(io, string(ks[i]), " = ")
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

string(r::UnitRange) = repr(r)
string(r::StepRange) = repr(r)

# Issue #4759: VM-native Value::Range is reported as StepRangeLen/LinRange but
# is not backed by struct fields, so Pure-Julia field-access methods like
# first(r::StepRangeLen) crash on it. Forwarding through an untyped helper
# forces dynamic dispatch, which routes first/last/step to the VM Range
# builtins for VM-native ranges while still working for real struct values.

"""
    show(io::IO, r::StepRangeLen)

Display a StepRangeLen as start:step:stop (Issue #4759).
"""
function show(io::IO, r::StepRangeLen)
    _show_steprangelen_dynamic(io, r)
end

function _show_steprangelen_dynamic(io, r)
    st = step(r)
    if !iszero(st)
        show(io, first(r))
        print(io, ":")
        show(io, st)
        print(io, ":")
        show(io, last(r))
    else
        # Upstream show(io, ::StepRangeLen): a zero step has no valid colon
        # form (`1.0:0.0:1.0` would not even parse back to this range), so
        # print the constructor form `StepRangeLen(1.0, 0.0, 3)` (Issue #11440).
        print(io, "StepRangeLen(")
        show(io, first(r))
        print(io, ", ")
        show(io, st)
        print(io, ", ")
        show(io, length(r))
        print(io, ")")
    end
end

"""
    show(io::IO, r::LinRange)

Display a LinRange as LinRange{T}(start, stop, len) (Issue #4759).
"""
function show(io::IO, r::LinRange)
    _show_linrange_dynamic(io, r)
end

function _show_linrange_dynamic(io, r)
    print(io, "LinRange{")
    print(io, eltype(r))
    print(io, "}(")
    show(io, first(r))
    print(io, ", ")
    show(io, last(r))
    print(io, ", ")
    show(io, length(r))
    print(io, ")")
end

# =============================================================================
# show - Numeric Types (Complex, Rational)
# =============================================================================

# Note: `show(io::IO, z::Complex)` lives in base/complex.jl (matching upstream
# Julia's file layout) and handles the `Complex{Bool}` / imaginary-unit special
# cases and `:compact` context. Keeping it there avoids a duplicate, ambiguous
# registration under the shared "Complex" show-method key (Issue #5155).

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
    # Issue #8884: repr must route ALL types through show(io, x) via an IOBuffer
    # so that typed representations (e.g. UInt8 → "0x06", Float16 → "Float16(3.5)",
    # Float32 → "3.5f0") are produced. Previously only String/Char/Symbol went
    # through IOBuffer; other types called string(x) which used the print format.
    io = IOBuffer()
    show(io, x)
    return String(take!(io))
end

# =============================================================================
# eachline - File line iterator
# =============================================================================
# Vector-backed filename iteration for package initialization paths that expect
# Base.eachline(filename), including MacroTools animals loading (Issue #7593).
struct EachLine
    lines
end

function eachline(filename::AbstractString; keep::Bool=false)
    lines = readlines(filename)
    if keep
        n = length(lines)
        i = 1
        while i <= n
            lines[i] = string(lines[i], "\n")
            i = i + 1
        end
    end
    return EachLine(lines)
end

function iterate(iter::EachLine)
    return iterate(iter, 1)
end

function iterate(iter::EachLine, state)
    if state > length(iter.lines)
        return nothing
    end
    return (iter.lines[state], state + 1)
end

length(iter::EachLine) = length(iter.lines)
collect(iter::EachLine) = copy(iter.lines)
map(f, iter::EachLine) = map(f, iter.lines)

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

# Generic fallback: return the type name as a string.
# Issue #4706: also handle AbstractString and AbstractDict here rather
# than via separate `summary(::AbstractString)` / `summary(::AbstractDict)`
# method overloads, because adding those overloads currently triggers an
# `AmbiguousMethod` error for `summary([1,2,3])` (the dispatcher does not
# filter the AbstractString / AbstractDict candidates out for `Array`).
function summary(x)
    if x isa AbstractString
        prefix = isempty(x) ? "empty" : string(ncodeunits(x), "-codeunit")
        return string(prefix, " ", typeof(x))
    elseif x isa AbstractDict
        n = length(x)
        suffix = n == 1 ? " entry" : " entries"
        return string(typeof(x), " with ", n, suffix)
    end
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
