# =============================================================================
# strings/basic.jl - Character classification functions
# =============================================================================
# Based on Julia's base/strings/basic.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
# Function names MUST match Julia exactly (e.g., isdigit NOT is_digit).

# ndigits: moved to intfuncs.jl with keyword argument support (Issue #2020)

# isdigit: check if character code is a digit
# ASCII: '0' = 48, '9' = 57
function isdigit(c)
    c = Int(c)
    return c >= 48 && c <= 57
end

# isletter: check if character code is a letter (ASCII)
function isletter(c)
    c = Int(c)
    if c >= 65 && c <= 90
        return true  # A-Z
    end
    if c >= 97 && c <= 122
        return true  # a-z
    end
    return false
end

# isuppercase: check if character is uppercase
function isuppercase(c)
    c = Int(c)
    return c >= 65 && c <= 90
end

# islowercase: check if character is lowercase
function islowercase(c)
    c = Int(c)
    return c >= 97 && c <= 122
end

# Note: uppercase(s::String) and lowercase(s::String) are builtins
# that handle proper Unicode string case conversion.

# isspace: check if character is whitespace
# Space = 32, Tab = 9, Newline = 10, CR = 13
function isspace(c)
    c = Int(c)
    if c == 32
        return true
    end
    if c == 9
        return true
    end
    if c == 10
        return true
    end
    if c == 13
        return true
    end
    return false
end

# =============================================================================
# Additional character classification functions
# =============================================================================

# isascii: check if character code is in ASCII range (0-127)
function isascii(c::Char)
    c = Int(c)
    return c >= 0 && c < 128
end

# isascii for String: check all characters are ASCII (Issue #2046)
function isascii(s::String)
    for c in s
        isascii(c) || return false
    end
    return true
end

# iscntrl: check if character is a control character
# Control chars: 0-31 and 127 (DEL)
function iscntrl(c)
    c = Int(c)
    return c < 32 || c == 127
end

# isprint: check if character is printable (including space)
# Printable: 32-126
function isprint(c)
    c = Int(c)
    return c >= 32 && c < 127
end

# ispunct: check if character is punctuation
# Punctuation: 33-47, 58-64, 91-96, 123-126
function ispunct(c)
    c = Int(c)
    if c >= 33 && c <= 47
        return true  # ! " # $ % & ' ( ) * + , - . /
    end
    if c >= 58 && c <= 64
        return true  # : ; < = > ? @
    end
    if c >= 91 && c <= 96
        return true  # [ \ ] ^ _ `
    end
    if c >= 123 && c <= 126
        return true  # { | } ~
    end
    return false
end

# isxdigit: check if character is a hexadecimal digit (0-9, A-F, a-f)
function isxdigit(c)
    c = Int(c)
    if c >= 48 && c <= 57
        return true  # 0-9
    end
    if c >= 65 && c <= 70
        return true  # A-F
    end
    if c >= 97 && c <= 102
        return true  # a-f
    end
    return false
end

# =============================================================================
# Character code point functions
# =============================================================================

# codepoint: get Unicode code point of a character
# Based on Julia's base/char.jl
# For ASCII characters, this is just the character code
function codepoint(c::Char)
    # In SubsetJuliaVM, Char is represented as UInt32 code point
    # We can use the character directly as its code point
    # For now, we'll use a simple implementation that works with ASCII
    # Note: This is a simplified version. Full Unicode support would require
    # proper UTF-8 decoding, but for ASCII characters this works.
    return Int64(c)
end

# =============================================================================
# Text width functions
# =============================================================================

# textwidth: get display width of string (for monospace fonts)
# Based on Julia's base/strings/width.jl
# Simplified version: ASCII characters have width 1, others have width 2
# This is a basic implementation. Full Unicode support would require
# proper East Asian Width property handling.
function textwidth(s::String)
    width = 0
    n = length(s)
    i = 1
    while i <= n
        c = codeunit(s, i)
        # ASCII printable characters (32-126) have width 1
        if c >= 32 && c < 127
            width = width + 1
        else
            # Non-ASCII characters have width 2 (simplified)
            width = width + 2
        end
        i = i + 1
    end
    return width
end

# textwidth for single character
function textwidth(c::Char)
    cp = codepoint(c)
    if cp >= 32 && cp < 127
        return 1
    else
        return 2
    end
end

# =============================================================================
# String repeat functions
# =============================================================================
# Based on Julia's base/strings/basic.jl

# repeat: repeat string n times
# Uses simple loop concatenation for correctness
function repeat(s::String, n::Int64)
    if n < 0
        throw(ArgumentError("repeat count must be non-negative"))
    end
    if n == 0
        return ""
    end
    if n == 1
        return s
    end

    result = ""
    for _ in 1:n
        result = result * s
    end
    return result
end

# repeat: repeat a character n times, returning a String (Issue #2057)
function repeat(c::Char, n::Int64)
    return repeat(string(c), n)
end

# Note: The ^(s, n) operator for strings (alias for repeat) requires
# compiler support for dispatching String^Int to repeat().
# See GitHub issue for tracking this enhancement.

# =============================================================================
# String reverse function
# =============================================================================
# Based on Julia's base/strings/basic.jl:183
# first(s::AbstractString) = s[firstindex(s)]
# We use s[1] since firstindex(::String) == 1
function first(s::String)
    if isempty(s)
        throw(ArgumentError("string must be non-empty"))
    end
    return s[1]
end

# Based on Julia's base/abstractarray.jl:530
# last(a) = a[end]
function last(s::String)
    if isempty(s)
        throw(ArgumentError("string must be non-empty"))
    end
    return s[lastindex(s)]
end

# Based on Julia's base/strings/basic.jl
# reverse(s::String) returns a reversed String (not Vector{Char})
# Without this typed method, the generic reverse(arr) in array.jl
# catches strings and returns Vector{Char} instead. (Issue #2053)

function reverse(s::String)
    chars = collect(s)
    n = length(chars)
    for i in 1:div(n, 2)
        tmp = chars[i]
        chars[i] = chars[n - i + 1]
        chars[n - i + 1] = tmp
    end
    return String(chars)
end

# =============================================================================
# String map function
# =============================================================================
# map(f, s::String) returns a String (not Vector{Any})
# Without this typed method, the generic map(f, A) in iterators.jl
# returns a Vector{Any} of characters instead. (Issue #2609)
# Based on Julia's base/strings/basic.jl:656-670

function map(f::Function, s::String)
    buf = IOBuffer()
    for c in s
        c2 = f(c)
        if isa(c2, Char)
            write(buf, c2)
        else
            throw(ArgumentError("map(f, s::AbstractString) requires f to return AbstractChar; try map(f, collect(s)) or a comprehension instead"))
        end
    end
    return String(take!(buf))
end

# =============================================================================
# String filter function
# =============================================================================
# filter(f, s::String) returns a String (not Vector{Char})
# Without this typed method, the generic filter(f, A) in iterators.jl
# returns a Vector{Any} of characters instead. (Issue #2062)

function filter(f::Function, s::String)
    chars = collect(s)
    result = Char[]
    for c in chars
        if f(c)
            push!(result, c)
        end
    end
    return String(result)
end

# count(f, s::String) - count characters satisfying predicate (Issue #2081)
# The HOF builtin CountFunc cannot handle String iteration (Char values
# cause type errors in the accumulator path), so we use Pure Julia dispatch.
function count(f::Function, s::String)
    n = 0
    for c in collect(s)
        if f(c)
            n = n + 1
        end
    end
    return n
end

# =============================================================================
# UTF-8 string index navigation functions (Issue #2564)
# =============================================================================
# Based on Julia's base/strings/basic.jl
# These use codeunit(s, i) and ncodeunits(s) intrinsics for byte-level access.

# _is_continuation_byte: check if byte is a UTF-8 continuation byte (10xxxxxx)
# Continuation bytes have value >= 128 (0x80) and < 192 (0xC0)
# This avoids using bitwise & which is not supported in the lowering.
function _is_continuation_byte(b::UInt8)
    return b >= 0x80 && b < 0xc0
end

# thisind(s, i) - start of character containing byte index i
function thisind(s::String, i::Int64)
    if i == 0
        return 0
    end
    n = ncodeunits(s)
    if i == n + 1
        return i
    end
    if i < 1 || i > n
        throw(BoundsError(s, i))
    end
    while i > 1 && _is_continuation_byte(codeunit(s, i))
        i -= 1
    end
    return i
end

# nextind(s, i) - next valid string index after i
function nextind(s::String, i::Int64)
    if i == 0
        return 1
    end
    n = ncodeunits(s)
    if i >= n
        return n + 1
    end
    i += 1
    while i <= n && _is_continuation_byte(codeunit(s, i))
        i += 1
    end
    return i
end

# prevind(s, i) - previous valid string index before i
function prevind(s::String, i::Int64)
    if i <= 1
        return 0
    end
    n = ncodeunits(s)
    if i > n + 1
        throw(BoundsError(s, i))
    end
    if i == n + 1
        i = n
    else
        i -= 1
    end
    while i > 0 && _is_continuation_byte(codeunit(s, i))
        i -= 1
    end
    if i < 0
        return 0
    end
    return i
end

# reverseind(s, i) - index in s corresponding to index i in reverse(s)
# Julia's actual implementation: thisind(s, ncodeunits(s) - i + 1)
reverseind(s::String, i::Int64) = thisind(s, ncodeunits(s) - i + 1)
