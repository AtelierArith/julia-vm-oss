# IO Design: show, print, display

This document describes the design philosophy and usage of `show`, `print`, and `display` functions in SubsetJuliaVM, following Julia's official conventions.

## Overview

Julia has three primary functions for text output, each with a distinct purpose:

```
display(x)  ← Interactive environment, selects optimal backend
     ↓
show(io, MIME, x)  ← Format-specific, supports multiple MIME types
     ↓
show(io, x)  ← Basic Julia representation
     ↓
print(io, x)  ← Plain output, minimal decoration
```

## Function Roles

| Function | Paradigm | Purpose |
|----------|----------|---------|
| `show` | **Lisp-style repr** | Julia syntax representation, for programming |
| `print` | **C-style printf** | Human-readable, minimal decoration |
| `display` | **REPL-driven** | Backend-dependent, interactive output |

### show(io, x)

The fundamental function for converting values to their Julia representation.

**Key characteristics:**
- Output should be valid Julia syntax when possible
- Strings are quoted: `"hello"` → `"\"hello\""`
- Characters are quoted: `'a'` → `'\'a\'`
- Symbols include colon: `:foo` → `:foo`
- Arrays show brackets: `[1, 2, 3]`

**When to use:**
- Debugging output
- Serialization to Julia-readable format
- REPL display of values

### print(io, x)

Human-readable output with minimal decoration.

**Key characteristics:**
- Output is for humans, not for parsing
- Strings without quotes: `"hello"` → `hello`
- Characters without quotes: `'a'` → `a`
- Falls back to `show(io, x)` by default

**When to use:**
- User-facing output
- Log messages
- Building strings for display

### display(x)

Interactive output that selects the best representation.

**Key characteristics:**
- Chooses output backend (terminal, notebook, etc.)
- May use rich formatting (colors, HTML, images)
- Calls `show(io, MIME"text/plain", x)` for text output

**When to use:**
- REPL results
- Interactive exploration
- Rich media output

## Comparison Table

| Type | `show(io, x)` | `print(io, x)` |
|------|---------------|----------------|
| String `"hello"` | `"hello"` | `hello` |
| Char `'a'` | `'a'` | `a` |
| Symbol `:foo` | `:foo` | `foo` |
| Number `123` | `123` | `123` |
| Array `[1, 2]` | `[1, 2]` | `[1, 2]` |

## Implementation Hierarchy

### Julia's Design

```julia
# print defaults to show
function print(io::IO, x)
    show(io, x)
end

# println adds newline
println(io::IO, xs...) = print(io, xs..., "\n")

# String has special print (no quotes)
print(io::IO, s::String) = write(io, s)

# display uses MIME system
function display(d::AbstractDisplay, x)
    show(stdout, MIME"text/plain"(), x)
end
```

### SubsetJuliaVM Status

**Last updated**: 2026-06-11

#### Rust Builtins (fully implemented)
- `print(x...)` - Direct value formatting
- `println(x...)` - Print with newline
- `show(x)` - Julia representation (1-arg form)
- `string(x...)` - String conversion

#### Pure Julia Implementation (implemented)
- `IOContext` - Output context wrapper (`base/io.jl`)
  - `iocontext(io, ...)` - Constructor functions
  - `get(ctx, key, default)` / `haskey(ctx, key)` - Public property access
  - `ioget(ctx, key, default)` - Property retrieval
  - `iohaskey(ctx, key)` - Property existence check
  - `iokeys(ctx)` - Property key listing
- `sprint(x)` / `sprint(f, args...)` - String from printing
- `sprint_context(f, args, context)` - Context-aware sprint helper
- `repr(x)` - Pure Julia `IOBuffer` + `show(io, x)` representation
- `display(x)` / `display(mime, x)` - Simplified stdout display
- `dump(x)` - Structure inspection
- `displaysize()` / `displaysize(io)` - Terminal size
- `printstyled(x, color)` - ANSI colored output (`base/util.jl`)
- `show(io, x)` - Generic 2-arg fallback for user structs
- `show(io, x::Type)` and many scalar/container 2-arg specializations
- `show(io, arr::Array)` - 2-arg form for Arrays
- `show(io, ci::CartesianIndex)` - 2-arg form for CartesianIndex
- `show(io, ci::CartesianIndices)` - 2-arg form for CartesianIndices
- `show(io, li::LinearIndices)` - 2-arg form for LinearIndices
- `showerror(io, e)` / `showerror(io, e, bt)` - Exception display (`base/errorshow.jl`)
- `sprint_showerror(e)` - Capturable error string helper

#### Multimedia I/O (implemented)
- `MIME` type - MIME type constructor (`MIME("text/plain")`)
- `@MIME_str` macro - MIME literal syntax (`MIME"text/plain"`)
- `istextmime(mime)` - Check if MIME is text type
- `displayable(mime)` - Check if MIME is displayable (text types)
- `showable(mime, x)` - Check if value is showable (text/plain)
- `show(io, mime::MIME, x)` - Generic MIME fallback to `show(io, x)`
- `redisplay(x)` - Delegates to display
- `AbstractDisplay`, `TextDisplay` - Display type hierarchy
- `pushdisplay`, `popdisplay` - Display stack (stub implementation)

#### Not yet implemented
- Full MIME-specific `show(io, MIME"...", x)` method selection (see #372)
- `repr(x; context=...)` / MIME-aware repr (see #377)
- `showerror(io, e)` writes through stdout for now instead of mutating arbitrary IO streams (Issue #1217)
- Full display stack backend selection - Display chooses stdout only
- `HTML`, `Text` types - Rich media types

## Implementation Guidelines

### For Custom Types

When implementing output for a custom type, follow this order:

```julia
# 1. Required: 2-argument show
function Base.show(io::IO, x::MyType)
    print(io, "MyType(", x.field, ")")
end

# 2. Recommended: MIME"text/plain" for detailed output
function Base.show(io::IO, ::MIME"text/plain", x::MyType)
    println(io, "MyType:")
    println(io, "  field: ", x.field)
end

# 3. Optional: print (only if different from show)
function Base.print(io::IO, x::MyType)
    print(io, x.field)  # No type decoration
end
```

### IOContext Properties

Output format control (planned for future implementation):

| Property | Type | Description |
|----------|------|-------------|
| `:compact` | Bool | Compact display (fewer digits, etc.) |
| `:limit` | Bool | Limit output length |
| `:displaysize` | Tuple | Terminal (rows, cols) |
| `:typeinfo` | Type | Suppress redundant type info |
| `:color` | Bool | Enable ANSI colors |

Usage example:
```julia
# Compact floating point
show(IOContext(io, :compact => true), 1.23456789)  # "1.23457"

# Limit array output
show(IOContext(io, :limit => true), 1:1000)  # "1:1000"
```

## Related Issues

### Open Issues
- #372 - MIME type system and 3-arg show (partial: see PR #1559)
- #373 - show(io, x) 2-arg form for all types
- #376 - display function with display stack (partial: basic display implemented)
- #377 - repr function (full implementation with MIME)
- #378 - Normalize print/show relationship
- #1217 - IOBuffer mutation limitation affects `showerror(io, e)` and non-stdout display

### Closed/Partially Resolved Issues
- #379 - sprint with function argument ✅
- #380 - displaysize function ✅
- #381 - printstyled (ANSI colors) ✅
- #382 - showerror function ✅
- #528 - IOContext minimal wrapper ✅
- #455 - Multimedia I/O (PR #1559) ✅ 部分実装
- #1199 - MIME type infrastructure ✅ 基本実装（PR #1559）

## References

- Julia Base: `base/strings/io.jl` - print, println, sprint
- Julia Base: `base/show.jl` - show implementations
- Julia Base: `base/multimedia.jl` - display, MIME system
- Julia Docs: [Custom pretty-printing](https://docs.julialang.org/en/v1/manual/types/#man-custom-pretty-printing)
