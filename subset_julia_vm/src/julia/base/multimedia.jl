# =============================================================================
# Multimedia - MIME type system and display stack for rich display
# =============================================================================
# Based on Julia's base/multimedia.jl
#
# Provides the MIME{mime} type for representing standard internet data formats.
# This enables Julia's dispatch mechanism for type-specific display rendering.
#
# SubsetJuliaVM supports:
# - MIME type literals: MIME"text/plain", MIME"text/html", etc.
# - Basic show(io, mime, x) dispatch
# - istextmime for determining text vs binary MIME types
# - AbstractDisplay type hierarchy
# - Display stack with pushdisplay/popdisplay
# - display(x) function with stack-based display selection

# =============================================================================
# MIME Type
# =============================================================================

"""
    MIME

A type representing a standard internet data format. "MIME" stands for
"Multipurpose Internet Mail Extensions", since the standard was originally
used to describe multimedia attachments to email messages.

A `MIME` object can be passed as the second argument to [`show`](@ref) to
request output in that format.

# Examples
```julia
show(stdout, MIME("text/plain"), "hi")
```
"""
struct MIME{mime} end

# Internal constructor that creates MIME instances
# This is called by the @MIME_str macro expansion
function _mime_construct(s::String)
    # Create MIME{Symbol(s)}() - the parametric type instance
    MIME{Symbol(s)}()
end

# Constructor from string
# MIME("text/plain") -> MIME{Symbol("text/plain")}()
function MIME(s::String)
    return _mime_construct(s)
end

# =============================================================================
# @MIME_str - String macro for MIME types
# =============================================================================

"""
    @MIME_str

A convenience macro for writing [`MIME`](@ref) types, typically used when
adding methods to [`show`](@ref).
For example the syntax `show(io::IO, ::MIME"text/html", x::MyType) = ...`
could be used to define how to write an HTML representation of `MyType`.

Note: In SubsetJuliaVM, `MIME"text/plain"` is lowered to `MIME_str("text/plain")`.

# Examples
```julia
MIME"text/plain"  # Creates MIME{Symbol("text/plain")}()
```
"""
function MIME_str(s::String)
    return _mime_construct(s)
end

# =============================================================================
# MIME Display Methods
# =============================================================================

# show(io, ::MIME{T}) - display the MIME type itself
function show(io::IO, m::MIME{T}) where T
    print(io, "MIME type ")
    print(io, string(T))
end

# =============================================================================
# istextmime - Check if MIME type is text
# =============================================================================

"""
    istextmime(m::MIME)

Determine whether a MIME type is text data. MIME types are assumed to be binary
data except for a set of types known to be text data (possibly Unicode).

# Examples
```julia
istextmime(MIME("text/plain"))  # true
istextmime(MIME("image/png"))   # false
```
"""
function istextmime(m::MIME{T}) where T
    s = string(T)
    # Text types start with "text/"
    if startswith(s, "text/")
        return true
    end
    # Additional known text types
    if s == "application/atom+xml"
        return true
    end
    if s == "application/ecmascript"
        return true
    end
    if s == "application/javascript"
        return true
    end
    if s == "application/julia"
        return true
    end
    if s == "application/json"
        return true
    end
    if s == "application/postscript"
        return true
    end
    if s == "application/rdf+xml"
        return true
    end
    if s == "application/rss+xml"
        return true
    end
    if s == "application/x-latex"
        return true
    end
    if s == "application/xhtml+xml"
        return true
    end
    if s == "application/xml"
        return true
    end
    if s == "application/xml-dtd"
        return true
    end
    if s == "image/svg+xml"
        return true
    end
    if s == "model/vrml"
        return true
    end
    if s == "model/x3d+vrml"
        return true
    end
    if s == "model/x3d+xml"
        return true
    end
    return false
end

# String overload
function istextmime(m::String)
    istextmime(MIME(m))
end

# =============================================================================
# Helper to get MIME type string
# =============================================================================

# Get the MIME type string from a MIME instance
function _mime_string(m::MIME{T}) where T
    return string(T)
end

# =============================================================================
# 3-argument show with MIME
# =============================================================================

# Generic fallback for MIME types - call 2-arg show
# Uses Any type to avoid multiple type parameters issue
function show(io::IO, m::MIME, x)
    show(io, x)
end

# String-based dispatch
function show(io::IO, m::String, x)
    show(io, MIME(m), x)
end

# =============================================================================
# showable - Check if object can be shown as MIME type
# =============================================================================

"""
    showable(mime, x)

Return a boolean value indicating whether or not the object `x` can be written
as the given `mime` type.

(By default, this is determined automatically by the existence of the
corresponding [`show`](@ref) method for `typeof(x)`. Some types provide custom
`showable` methods; for example, if the available MIME formats depend on the
*value* of `x`.)

# Examples
```julia
showable(MIME("text/plain"), rand(5))  # true
showable("image/png", rand(5))         # false
```

Note: In SubsetJuliaVM, this returns true for text/plain for all types,
and false for other MIME types unless a specific show method is defined.
"""
function showable(m::MIME, x)
    # Simplified implementation: text/plain is always showable
    s = _mime_string(m)
    if s == "text/plain"
        return true
    end
    # For other MIME types, return false by default
    return false
end

# String overload
function showable(m::String, x)
    showable(MIME(m), x)
end

# =============================================================================
# displayable - Check if MIME type can be displayed
# =============================================================================

"""
    displayable(mime)::Bool
    displayable(d::AbstractDisplay, mime)::Bool

Return a boolean value indicating whether the given `mime` type (string) is displayable by
any of the displays in the current display stack, or specifically by the display `d` in the
second variant.

Note: In SubsetJuliaVM, the display stack is not fully functional due to global mutable
state limitations. This function always returns `true` for text MIME types.
"""
function displayable(m::MIME)
    # Simplified: text MIME types are always displayable
    istextmime(m)
end

# String overload
function displayable(m::String)
    displayable(MIME(m))
end

# Display-specific overloads
function displayable(d::AbstractDisplay, m::MIME)
    # Generic fallback: assume text MIME types are displayable
    istextmime(m)
end

function displayable(d::AbstractDisplay, m::String)
    displayable(d, MIME(m))
end

# TextDisplay can display all text MIME types
function displayable(d::TextDisplay, m::MIME)
    istextmime(m)
end

# =============================================================================
# repr with MIME - TODO: implement when needed
# =============================================================================
# Note: repr with MIME type is not yet implemented in SubsetJuliaVM
# as it conflicts with the builtin repr function.
# Future implementation will add:
#   repr(m::MIME{T}, x) where T
#   repr(m::String, x)

# =============================================================================
# AbstractDisplay - Base type for display backends
# =============================================================================
# Based on Julia's base/multimedia.jl
#
# The display system allows Julia to select the best available backend
# for showing values. Display backends are managed in a stack, with
# the most recently pushed display being tried first.

"""
    AbstractDisplay

Abstract supertype for display backends.

Custom displays should inherit from `AbstractDisplay` and implement
`display(d::CustomDisplay, mime::MIME, x)` methods for supported MIME types.

See also: [`TextDisplay`](@ref), [`pushdisplay`](@ref), [`popdisplay`](@ref), [`display`](@ref).
"""
abstract type AbstractDisplay end

# =============================================================================
# TextDisplay - Text-based display backend
# =============================================================================

"""
    TextDisplay <: AbstractDisplay

A display backend that outputs text representations to an IO stream.

`TextDisplay` is the default display backend used in the REPL. It renders
objects using the `text/plain` MIME type.

# Examples
```julia
d = TextDisplay(stdout)
display(d, [1, 2, 3])
```

See also: [`AbstractDisplay`](@ref), [`display`](@ref).
"""
struct TextDisplay <: AbstractDisplay
    io
end

# =============================================================================
# Display Stack
# =============================================================================
# The display stack holds all active display backends.
# Displays are tried top-to-bottom (last pushed = highest priority).
#
# NOTE: SubsetJuliaVM does not yet fully support global mutable state
# accessed from within functions. The display stack (pushdisplay, popdisplay)
# is provided for API compatibility but the stack-based display selection
# is not functional. display(x) always uses stdout directly.
#
# The `displays` array is not used because global arrays cannot be accessed
# from function bodies in the prelude. These functions are stubs that indicate
# the feature is not yet supported.

# =============================================================================
# Display Stack Management
# =============================================================================

"""
    pushdisplay(d::AbstractDisplay)

Push a new display backend `d` onto the global display stack.

After calling this, `d` will be the first display backend tried by
[`display`](@ref) when rendering values.

Note: In SubsetJuliaVM, this function is a stub. Display stack management
is not yet supported due to global mutable state limitations.

# Examples
```julia
pushdisplay(TextDisplay(io))
display(x)  # Will try TextDisplay first
```

See also: [`popdisplay`](@ref), [`display`](@ref).
"""
function pushdisplay(d::AbstractDisplay)
    # Stub: Display stack not yet supported in SubsetJuliaVM
    # Silently accept the display but don't store it
    return nothing
end

"""
    popdisplay()

Remove and return the topmost display backend from the global display stack.

Throws an error if the display stack is empty.

Note: In SubsetJuliaVM, this function is a stub. Display stack management
is not yet supported due to global mutable state limitations.

# Examples
```julia
pushdisplay(d)
popdisplay()  # Returns d, stack is now as before
```

See also: [`pushdisplay`](@ref).
"""
function popdisplay()
    # Stub: Display stack not yet supported in SubsetJuliaVM
    error("popdisplay: display stack not supported in SubsetJuliaVM")
end

"""
    popdisplay(d::AbstractDisplay)

Remove a specific display `d` from the display stack.

The display is searched from top to bottom (most recent first).
Throws an error if `d` is not found in the stack.

Note: In SubsetJuliaVM, this function is a stub. Display stack management
is not yet supported due to global mutable state limitations.

# Examples
```julia
d = TextDisplay(io)
pushdisplay(d)
popdisplay(d)  # Removes d specifically
```

See also: [`pushdisplay`](@ref).
"""
function popdisplay(d::AbstractDisplay)
    # Stub: Display stack not yet supported in SubsetJuliaVM
    error("popdisplay: display stack not supported in SubsetJuliaVM")
end

# =============================================================================
# Display Functions - Core display mechanism
# =============================================================================
# The display function tries each display backend in the stack until
# one successfully handles the value.

"""
    display(x)

Display the value `x` using the best available display backend.

The function iterates through the display stack from top to bottom,
trying each display backend until one successfully renders `x`.
If no display in the stack can handle `x`, falls back to showing
`x` as `text/plain` on stdout.

# Examples
```julia
display([1, 2, 3])  # Shows array using current display backend
display("hello")   # Shows string
```

See also: [`pushdisplay`](@ref), [`popdisplay`](@ref), [`show`](@ref).
"""
function display(x)
    # Note: In SubsetJuliaVM, global mutable state (displays array) is not
    # yet accessible from functions. We always use stdout directly.
    # In full Julia, this would iterate through the display stack.
    #
    # For now, we use println for simple output. The full show(io, MIME, x)
    # dispatch will be implemented when array printing is fully working.
    println(x)
    return nothing
end

"""
    display(d::AbstractDisplay, x)

Display the value `x` on a specific display backend `d`.

This bypasses the display stack and directly uses the specified backend.

# Examples
```julia
d = TextDisplay(stdout)
display(d, [1, 2, 3])
```

See also: [`display`](@ref), [`TextDisplay`](@ref).
"""
function display(d::AbstractDisplay, x)
    _display_internal(d, x)
    return nothing
end

"""
    display(d::AbstractDisplay, m::MIME, x)

Display the value `x` using MIME type `m` on display backend `d`.

# Examples
```julia
d = TextDisplay(stdout)
display(d, MIME("text/plain"), [1, 2, 3])
```
"""
function display(d::AbstractDisplay, m::MIME, x)
    _display_internal(d, m, x)
    return nothing
end

"""
    display(m::MIME, x)

Display the value `x` using the specified MIME type `m`.

The function iterates through the display stack from top to bottom,
trying each display backend with the specified MIME type.

# Examples
```julia
display(MIME("text/plain"), [1, 2, 3])
```
"""
function display(m::MIME, x)
    # Note: In SubsetJuliaVM, global mutable state (displays array) is not
    # yet accessible from functions. We always use stdout directly.
    # Using println for simple output until MIME-based show is fully working.
    println(x)
    return nothing
end

# =============================================================================
# redisplay - Refresh existing display
# =============================================================================

"""
    redisplay(x)
    redisplay(d::AbstractDisplay, x)
    redisplay(mime, x)
    redisplay(d::AbstractDisplay, mime, x)

By default, the `redisplay` functions simply call [`display`](@ref).
However, some display backends may override `redisplay` to modify an existing
display of `x` (if any).
"""
function redisplay(x)
    display(x)
end

function redisplay(d::AbstractDisplay, x)
    display(d, x)
end

function redisplay(m::MIME{T}, x) where T
    display(m, x)
end

function redisplay(d::AbstractDisplay, m::MIME{T}, x) where T
    display(d, m, x)
end

# =============================================================================
# Internal Display Helpers
# =============================================================================

# Internal helper: display on AbstractDisplay (fallback to text/plain)
function _display_internal(d::AbstractDisplay, x)
    _display_internal(d, MIME("text/plain"), x)
end

# Internal helper: display with MIME on AbstractDisplay (generic fallback)
function _display_internal(d::AbstractDisplay, m::MIME, x)
    # Note: In SubsetJuliaVM, IOBuffer is functional/immutable style.
    # println(io, x) doesn't mutate io, so we can't easily redirect output
    # to arbitrary IO streams. For now, we always use stdout.
    println(x)
end

# TextDisplay implementation: display as text/plain
function _display_internal(d::TextDisplay, m::MIME, x)
    # Note: SubsetJuliaVM's IOBuffer is functional style - write(io, x)
    # returns a new IOBuffer. This makes redirecting output difficult.
    # For stdout (the default), println works correctly.
    # For other IO streams, this limitation means output goes to stdout.
    if d.io === stdout
        println(x)
    else
        # Attempt to write to the io stream
        # Due to immutable IOBuffer, this may not capture the output
        println(x)
    end
end

# =============================================================================
# Default Display Initialization
# =============================================================================
# Note: Default TextDisplay initialization is handled by the REPL.
# In official Julia, pushdisplay(TextDisplay(stdout)) is called at startup.
# Here we defer this to avoid issues with prelude loading order.
#
# Users should call: pushdisplay(TextDisplay(stdout))
# to enable the default display, or the REPL will do this automatically.
