# =============================================================================
# strings/util.jl - String manipulation functions
# =============================================================================
# Based on Julia's base/strings/util.jl

# =============================================================================
# String replacement functions
# =============================================================================

# _replace_impl: internal implementation of string replacement.
# Replace occurrences of old with new in string s.
# maxcount=0 means replace all (default), maxcount=N replaces at most N (Issue #2043)
#
# Uses byte counts (`ncodeunits`) for the loop bounds and pattern length so the
# byte-by-byte comparison underneath agrees, and `nextind` + `s[i:next_i-1]`
# to advance one full character (1-4 UTF-8 bytes) at a time on no-match. The
# previous `length(...)` + `Char(codeunit(s, i))` mix corrupted multi-byte
# UTF-8 chars by treating each byte as a separate Char. (Issue #3607)
function _replace_impl(s, old, new, maxcount)
    slen = ncodeunits(s)
    oldlen = ncodeunits(old)
    # Empty old string: return original
    if oldlen == 0
        return s
    end
    # Build result by finding and replacing occurrences
    result = ""
    i = 1
    replaced = 0
    while i <= slen
        # Check if old starts at position i
        if i <= slen - oldlen + 1 && (maxcount == 0 || replaced < maxcount)
            match = true
            j = 1
            while j <= oldlen
                if codeunit(s, i + j - 1) != codeunit(old, j)
                    match = false
                    break
                end
                j = j + 1
            end
            if match
                # Found match - append new and skip old
                result = result * new
                i = i + oldlen
                replaced = replaced + 1
                continue
            end
        end
        # No match or limit reached - append the current full character.
        # Decode the UTF-8 byte sequence at i into a codepoint manually so the
        # whole multi-byte character is added in one go (rather than the
        # original byte-by-byte append which produced mojibake). Avoids
        # `s[i]` / `s[i:j]` because the VM's type inference returns Any for
        # those and breaks `result * ...` concatenation.
        b1 = codeunit(s, i)
        if b1 < 0x80
            # 1-byte ASCII
            result = result * string(Char(b1))
            i = i + 1
        elseif b1 < 0xE0
            # 2-byte UTF-8 sequence (covers Latin-1 .. U+07FF)
            b2 = codeunit(s, i + 1)
            cp = (b1 - 0xC0) * 64 + (b2 - 0x80)
            result = result * string(Char(cp))
            i = i + 2
        elseif b1 < 0xF0
            # 3-byte UTF-8 sequence (covers BMP non-supplementary, incl. CJK)
            b2 = codeunit(s, i + 1)
            b3 = codeunit(s, i + 2)
            cp = (b1 - 0xE0) * 4096 + (b2 - 0x80) * 64 + (b3 - 0x80)
            result = result * string(Char(cp))
            i = i + 3
        else
            # 4-byte UTF-8 sequence (supplementary planes, e.g. emoji)
            b2 = codeunit(s, i + 1)
            b3 = codeunit(s, i + 2)
            b4 = codeunit(s, i + 3)
            cp = (b1 - 0xF0) * 262144 + (b2 - 0x80) * 4096 + (b3 - 0x80) * 64 + (b4 - 0x80)
            result = result * string(Char(cp))
            i = i + 4
        end
    end
    return result
end

# replace: replace occurrences of old with new in string s
# Based on Julia's base/strings/util.jl
# Julia signature: replace(s, old => new; count=0) where old => new is a Pair
# count=0 means replace all (default), count=N replaces at most N (Issue #2043)
# SubsetJuliaVM compiles Pair to Tuple, so we accept both forms
function replace(s, pair; count=0)
    # pair is a Tuple (old, new) from the => syntax
    old = pair[1]
    new = pair[2]
    # If old is a Regex, delegate to the builtin _regex_replace (Issue #2112)
    if isa(old, Regex)
        return _regex_replace(s, old, new, count)
    end
    # Normalize Char arguments to single-character strings so the underlying
    # `_replace_impl` (which calls `length(old)` / `codeunit`) can handle them.
    # Julia's `replace` natively accepts `Char => Char`, `Char => String`, etc.
    # (Issue #3596)
    if isa(old, Char)
        old = string(old)
    end
    if isa(new, Char)
        new = string(new)
    end
    return _replace_impl(s, old, new, count)
end

# replace(collection, old => new, ...): return a copy of an array with every
# element matching a pair's first value (by `isequal`) replaced by its second.
# Matching is by EQUALITY, not predicate — `replace([1,2,3,4], iseven=>0)` is
# `[1,2,3,4]` upstream (no element equals the function `iseven`). The string form
# above stays the most specific method for `AbstractString` arguments (Issue #5670).
function replace(collection::AbstractArray, pairs...)
    result = copy(collection)
    for i in eachindex(result)
        for p in pairs
            if isequal(result[i], p.first)
                result[i] = p.second
                break
            end
        end
    end
    return result
end

# =============================================================================
# String joining functions
# =============================================================================

# join: concatenate collection elements into a string with delimiter
# Based on Julia's base/strings/io.jl
function join(arr, delim)
    n = length(arr)
    if n == 0
        return ""
    end
    result = string(arr[1])
    i = 2
    while i <= n
        result = result * delim * string(arr[i])
        i = i + 1
    end
    return result
end

# join with single argument (no delimiter) - concatenate all elements
function join(arr)
    return join(arr, "")
end

# join(arr, delim, last): use a distinct separator `last` before the FINAL
# element, e.g. join([1,2,3], ", ", " and ") == "1, 2 and 3" and
# join([1,2], ", ", " and ") == "1 and 2". Matches upstream
# `join(io, iterator, delim, last)` (base/strings/io.jl).
function join(arr, delim, last)
    n = length(arr)
    if n == 0
        return ""
    end
    if n == 1
        return string(arr[1])
    end
    result = string(arr[1])
    i = 2
    while i < n
        result = result * delim * string(arr[i])
        i = i + 1
    end
    return result * last * string(arr[n])
end

# =============================================================================
# String trimming functions
# =============================================================================

# lstrip: remove leading whitespace from string
function lstrip(s)
    n = length(s)
    i = 1
    while i <= n && isspace(codeunit(s, i))
        i = i + 1
    end
    return s[i:n]
end

# lstrip with predicate function (Issue #2057)
function lstrip(pred::Function, s::String)
    n = length(s)
    i = 1
    while i <= n
        c = s[i]
        if !pred(c)
            return s[i:n]
        end
        i = i + 1
    end
    return ""
end

# rstrip: remove trailing whitespace from string
function rstrip(s)
    n = length(s)
    i = n
    while i >= 1 && isspace(codeunit(s, i))
        i = i - 1
    end
    return s[1:i]
end

# rstrip with predicate function (Issue #2057)
function rstrip(pred::Function, s::String)
    n = length(s)
    i = n
    while i >= 1
        c = s[i]
        if !pred(c)
            return s[1:i]
        end
        i = i - 1
    end
    return ""
end

# strip: remove leading and trailing whitespace from string
function strip(s)
    return lstrip(rstrip(s))
end

# 2-arg `strip(s, c::Char)` — strips occurrences of `c` from both ends.
# Equivalent to Julia's `strip(s, chars) = strip(in(chars), s)` for the
# single-Char case. (Issue #3668)
function strip(s::String, c::Char)
    function _strip_eq_pred(x)
        return x == c
    end
    return strip(_strip_eq_pred, s)
end

# 2-arg `lstrip(s, c::Char)` — strips occurrences of `c` from the left.
function lstrip(s::String, c::Char)
    function _lstrip_eq_pred(x)
        return x == c
    end
    return lstrip(_lstrip_eq_pred, s)
end

# 2-arg `rstrip(s, c::Char)` — strips occurrences of `c` from the right.
function rstrip(s::String, c::Char)
    function _rstrip_eq_pred(x)
        return x == c
    end
    return rstrip(_rstrip_eq_pred, s)
end

# strip with predicate function (Issue #2126)
function strip(pred::Function, s::String)
    n = length(s)
    # Find first index where predicate is false (from left)
    i = 1
    while i <= n
        c = s[i]
        if !pred(c)
            break
        end
        i = i + 1
    end
    # If all chars match predicate, return empty string
    if i > n
        return ""
    end
    # Find last index where predicate is false (from right)
    j = n
    while j >= i
        c = s[j]
        if !pred(c)
            break
        end
        j = j - 1
    end
    return s[i:j]
end

# chomp: remove trailing newline (LF or CRLF) from string
function chomp(s)
    n = length(s)
    if n == 0
        return s
    end
    if codeunit(s, n) == 10  # LF (newline)
        if n >= 2 && codeunit(s, n - 1) == 13  # CRLF
            return s[1:n-2]
        end
        return s[1:n-1]
    end
    if codeunit(s, n) == 13  # CR only
        return s[1:n-1]
    end
    return s
end

# chop: remove characters from start and end of string
# head=0: number of characters to remove from the start
# tail=1: number of characters to remove from the end (default 1)
# Based on Julia's base/strings/util.jl (Issue #2045)
function chop(s; head=0, tail=1)
    n = length(s)
    start = head + 1
    stop = n - tail
    if start > stop
        return ""
    end
    return s[start:stop]
end

# =============================================================================
# String padding functions
# =============================================================================

# lpad: left-pad string to specified length
function lpad(s, n::Int64)
    return lpad(s, n, ' ')
end

function lpad(s, n::Int64, c::Char)
    len = length(s)
    if len >= n
        return s
    end
    pad_len = n - len
    padding = ""
    for _ in 1:pad_len
        padding = padding * string(c)
    end
    return padding * s
end

function lpad(s, n::Int64, pad::String)
    len = length(s)
    if len >= n
        return s
    end
    pad_len = n - len
    pad_str_len = length(pad)
    if pad_str_len == 0
        return s
    end
    # Repeat pad string enough times
    full_repeats = div(pad_len, pad_str_len)
    remainder = pad_len - full_repeats * pad_str_len
    padding = ""
    for _ in 1:full_repeats
        padding = padding * pad
    end
    if remainder > 0
        padding = padding * pad[1:remainder]
    end
    return padding * s
end

# rpad: right-pad string to specified length
function rpad(s, n::Int64)
    return rpad(s, n, ' ')
end

function rpad(s, n::Int64, c::Char)
    len = length(s)
    if len >= n
        return s
    end
    pad_len = n - len
    padding = ""
    for _ in 1:pad_len
        padding = padding * string(c)
    end
    return s * padding
end

function rpad(s, n::Int64, pad::String)
    len = length(s)
    if len >= n
        return s
    end
    pad_len = n - len
    pad_str_len = length(pad)
    if pad_str_len == 0
        return s
    end
    # Repeat pad string enough times
    full_repeats = div(pad_len, pad_str_len)
    remainder = pad_len - full_repeats * pad_str_len
    padding = ""
    for _ in 1:full_repeats
        padding = padding * pad
    end
    if remainder > 0
        padding = padding * pad[1:remainder]
    end
    return s * padding
end

# =============================================================================
# chopprefix / chopsuffix - remove prefix/suffix from string
# =============================================================================
# Based on Julia's base/strings/util.jl

# chopprefix: remove prefix from string if present.
# Use byte counts (`ncodeunits`) so multi-byte UTF-8 prefixes slice on a
# valid char boundary. Previously `length(prefix)+1` (char-count + 1) was
# used as a byte index and threw `StringIndexError` for any non-ASCII
# prefix. (Issue #3606)
function chopprefix(s, prefix)
    if startswith(s, prefix)
        return s[ncodeunits(prefix) + 1 : ncodeunits(s)]
    end
    return s
end

# chopsuffix: remove suffix from string if present. Same byte-count fix
# as chopprefix (sister potential bug — Julia 1.12 also uses ncodeunits).
function chopsuffix(s, suffix)
    if endswith(s, suffix)
        return s[1 : ncodeunits(s) - ncodeunits(suffix)]
    end
    return s
end

# lowercasefirst: convert first character to lowercase. Handles ASCII A-Z and
# Latin-1 À-Ö, Ø-Þ (UTF-8 byte sequences starting with 0xC3) by inspecting
# raw bytes and computing the new codepoint. (Issue #3608)
function lowercasefirst(s)
    if length(s) == 0
        return s
    end
    n = ncodeunits(s)
    b1 = codeunit(s, 1)

    # ASCII A-Z (1-byte UTF-8)
    if b1 >= 65 && b1 <= 90
        new_first = string(Char(b1 + 32))
        if n == 1
            return new_first
        end
        rest = s[2:n]
        return new_first * rest
    end

    # Latin-1 uppercase (2-byte UTF-8 starting with 0xC3): À-Ö, Ø-Þ
    if b1 == 0xC3 && n >= 2
        b2 = codeunit(s, 2)
        cp = (b1 - 0xC0) * 64 + (b2 - 0x80)
        if (cp >= 0xC0 && cp <= 0xD6) || (cp >= 0xD8 && cp <= 0xDE)
            new_first = string(Char(cp + 32))
            if n == 2
                return new_first
            end
            rest = s[3:n]
            return new_first * rest
        end
    end

    return s
end

# uppercasefirst: convert first character to uppercase. Handles ASCII a-z and
# Latin-1 à-ö, ø-þ (UTF-8 byte sequences starting with 0xC3). (Issue #3609)
function uppercasefirst(s)
    if length(s) == 0
        return s
    end
    n = ncodeunits(s)
    b1 = codeunit(s, 1)

    # ASCII a-z (1-byte UTF-8)
    if b1 >= 97 && b1 <= 122
        new_first = string(Char(b1 - 32))
        if n == 1
            return new_first
        end
        rest = s[2:n]
        return new_first * rest
    end

    # Latin-1 lowercase (2-byte UTF-8 starting with 0xC3): à-ö, ø-þ
    if b1 == 0xC3 && n >= 2
        b2 = codeunit(s, 2)
        cp = (b1 - 0xC0) * 64 + (b2 - 0x80)
        if (cp >= 0xE0 && cp <= 0xF6) || (cp >= 0xF8 && cp <= 0xFE)
            new_first = string(Char(cp - 32))
            if n == 2
                return new_first
            end
            rest = s[3:n]
            return new_first * rest
        end
    end

    return s
end

# escape_string: escape special characters in string.
# Iterate by character (not byte) so multi-byte UTF-8 chars are emitted intact.
# Previously the loop did `c = codeunit(s, i); ... Char(c)` which emitted each
# UTF-8 byte as a separate Char (mojibake on non-ASCII). The escape branches
# below all check ASCII codepoints (≤ 127, single byte), so they're unchanged
# by switching to character iteration. (Issue #3599)
function escape_string(s)
    result = ""
    for ch in s
        cp = Int(ch)
        if cp == 92
            result = result * "\\\\"
        elseif cp == 34
            result = result * "\\\""
        elseif cp == 10
            result = result * "\\n"
        elseif cp == 13
            result = result * "\\r"
        elseif cp == 9
            result = result * "\\t"
        elseif cp == 0
            result = result * "\\0"
        else
            result = result * string(ch)
        end
    end
    return result
end

# _hexval: helper to convert hex digit character code to integer value
# Returns -1 if not a valid hex digit
function _hexval(c)
    if c >= 48 && c <= 57     # '0'-'9'
        return c - 48
    elseif c >= 97 && c <= 102  # 'a'-'f'
        return c - 97 + 10
    elseif c >= 65 && c <= 70   # 'A'-'F'
        return c - 65 + 10
    else
        return -1
    end
end

# unescape_string: reverse escape sequences in string (Issue #2086)
# Based on Julia's base/strings/io.jl
# This is the inverse of escape_string: converts escape sequences back to
# their corresponding characters.
# Supports: \n \t \r \\ \" \0 \a \b \f \v \e \xHH \uHHHH \UHHHHHHHH
function unescape_string(s::String)
    # Iterate over characters (not codeunits) so multibyte text is copied
    # verbatim; escape sequences are all ASCII and parsed character by
    # character. (Pure-Julia migration of the former Rust builtin, Issue #6724.)
    result = ""
    chars = collect(s)
    n = length(chars)
    i = 1
    while i <= n
        c = chars[i]
        if c == '\\' && i < n  # backslash
            i = i + 1
            e = chars[i]
            if e == 'n'        # \n -> newline (10)
                result = result * string(Char(10))
            elseif e == 't'    # \t -> tab (9)
                result = result * string(Char(9))
            elseif e == 'r'    # \r -> carriage return (13)
                result = result * string(Char(13))
            elseif e == '\\'   # \\ -> backslash (92)
                result = result * string(Char(92))
            elseif e == '"'    # \" -> double quote (34)
                result = result * string(Char(34))
            elseif e == '0'    # \0 -> null (0)
                result = result * string(Char(0))
            elseif e == 'a'    # \a -> bell (7)
                result = result * string(Char(7))
            elseif e == 'b'    # \b -> backspace (8)
                result = result * string(Char(8))
            elseif e == 'f'    # \f -> form feed (12)
                result = result * string(Char(12))
            elseif e == 'v'    # \v -> vertical tab (11)
                result = result * string(Char(11))
            elseif e == 'e'    # \e -> escape (27)
                result = result * string(Char(27))
            elseif e == 'x'    # \x -> up to 2-digit hex escape
                val = 0
                k = 0
                while k < 2 && i + 1 <= n
                    h = _hexval(Int(chars[i + 1]))
                    if h < 0
                        break
                    end
                    val = val * 16 + h
                    i = i + 1
                    k = k + 1
                end
                result = result * string(Char(val))
            elseif e == 'u'    # \u -> up to 4-digit unicode escape
                val = 0
                k = 0
                while k < 4 && i + 1 <= n
                    h = _hexval(Int(chars[i + 1]))
                    if h < 0
                        break
                    end
                    val = val * 16 + h
                    i = i + 1
                    k = k + 1
                end
                result = result * string(Char(val))
            elseif e == 'U'    # \U -> up to 8-digit unicode escape
                val = 0
                k = 0
                while k < 8 && i + 1 <= n
                    h = _hexval(Int(chars[i + 1]))
                    if h < 0
                        break
                    end
                    val = val * 16 + h
                    i = i + 1
                    k = k + 1
                end
                result = result * string(Char(val))
            else
                # Unknown escape: keep as-is. (Upstream raises ArgumentError;
                # the subset's lenient behavior and octal escapes are tracked
                # separately — Issue #6724 covers the regex-free migration only.)
                result = result * string(Char(92)) * string(e)
            end
        else
            result = result * string(c)
        end
        i = i + 1
    end
    return result
end

# =============================================================================
# String splitting functions
# =============================================================================
# Based on Julia's base/strings/util.jl

# split: split string by delimiter
# Returns a Vector{String} containing the substrings
# limit=0 means no limit (default), limit=N means at most N substrings (Issue #2040)
# keepempty=true (Julia default) keeps "" entries between consecutive delimiters
# and at the start/end; keepempty=false drops them (Issue #3651).
function split(str::String, delim::String; limit=0, keepempty=true)
    result = String[]
    n = length(str)
    dlen = length(delim)

    # Empty delimiter: split into characters (Issue #3597)
    # Walk by character using nextind so multi-byte UTF-8 characters stay
    # intact. Previously this loop ran 1:length(str) and built chars via
    # `Char(codeunit(str, i))`, which split each non-ASCII character into
    # its raw UTF-8 bytes (e.g. "éa" became ["Ã", "©"]).
    if dlen == 0
        nbytes = ncodeunits(str)
        i = 1
        while i <= nbytes
            if limit > 0 && length(result) >= limit - 1
                push!(result, str[i:nbytes])
                # Issue #3574: retag as Vector{SubString{String}} so the show
                # form matches Julia 1.12 (`SubString{String}["a", "b"]`).
                if keepempty != 0; return _substring_retag(result); else; return _filter_nonempty(result); end
            end
            ni = nextind(str, i)
            push!(result, str[i:ni-1])
            i = ni
        end
        if keepempty != 0; return _substring_retag(result); else; return _filter_nonempty(result); end
    end

    start = 1
    i = 1
    while i <= n - dlen + 1
        # If we've reached limit-1 parts, add the rest as the last part
        if limit > 0 && length(result) >= limit - 1
            break
        end

        # Check if delimiter matches at position i
        match = true
        j = 1
        while j <= dlen
            if codeunit(str, i + j - 1) != codeunit(delim, j)
                match = false
                break
            end
            j = j + 1
        end

        if match
            # Found delimiter - add substring before it
            if start <= i - 1
                push!(result, str[start:i-1])
            else
                push!(result, "")
            end
            start = i + dlen
            i = start
        else
            i = i + 1
        end
    end

    # Add remaining part after last delimiter
    if start <= n
        push!(result, str[start:n])
    else
        push!(result, "")
    end

    # Issue #3574: see comment above.
    if keepempty != 0; return _substring_retag(result); else; return _filter_nonempty(result); end
end

# Internal helper: return a new vector with all "" entries removed. Used by
# split/rsplit when `keepempty=false` (Issue #3651). Always returns a fresh
# Vector{SubString{String}}-tagged array so split/rsplit results show as
# Julia's `SubString{String}[...]` regardless of whether `keepempty` filters.
# Issue #3574.
function _filter_nonempty(result::Vector{String})
    filtered = String[]
    for s in result
        if !isempty(s)
            push!(filtered, s)
        end
    end
    return _substring_retag(filtered)
end

function _filter_nonempty(result)
    filtered = String[]
    for s in result
        if !isempty(s)
            push!(filtered, s)
        end
    end
    return _substring_retag(filtered)
end

# split with Char delimiter
function split(str::String, delim::Char; limit=0, keepempty=true)
    return split(str, string(delim); limit=limit, keepempty=keepempty)
end

# split with no delimiter: split on any whitespace, collapse runs, drop empties
# Issue #3571 — Julia: split(s::AbstractString; limit=0, keepempty=false) =
#                       split(s, isspace; limit, keepempty)
# We provide a dedicated method that hardcodes whitespace tokenisation rather
# than dispatching through a Function predicate, which keeps lowering simple.
function split(str::String)
    result = String[]
    n = ncodeunits(str)
    i = 1
    while i <= n
        # Skip a run of whitespace.
        while i <= n && isspace(codeunit(str, i))
            i = i + 1
        end
        if i > n
            break
        end
        # Accumulate non-whitespace bytes until the next whitespace or EOS.
        start = i
        while i <= n && !isspace(codeunit(str, i))
            i = i + 1
        end
        push!(result, str[start:i-1])
    end
    # Issue #3574: retag for Vector{SubString{String}} display.
    return _substring_retag(result)
end

# =============================================================================
# rsplit: split string by delimiter, starting from the right
# =============================================================================
# Based on Julia's base/strings/util.jl (lines 968-976)
# rsplit is like split but when limit is applied, only the rightmost
# limit-1 splits are performed, keeping the left part unsplit.

# rsplit with String delimiter — accepts `limit` keyword (Issue #3610).
# Default limit=0 means "no limit" (same output as split for an unbounded split).
# Calls into the positional `rsplit(str, delim, limit)` method below, which
# keeps the leftmost part unsplit when limit > 0 and falls back to `split`
# otherwise.
function rsplit(str::String, delim::String; limit=0, keepempty=true)
    parts = rsplit(str, delim, limit)
    if keepempty
        return parts
    end
    return _filter_nonempty(parts)
end

# rsplit with Char delimiter — accepts `limit` and `keepempty` keywords
# (Issues #3610, #3651).
function rsplit(str::String, delim::Char; limit=0, keepempty=true)
    parts = rsplit(str, string(delim), limit)
    if keepempty
        return parts
    end
    return _filter_nonempty(parts)
end

# rsplit with limit: split from the right, keeping leftmost parts together
function rsplit(str::String, delim::String, limit::Int64)
    if limit <= 0
        # Delegates to split, which already retags (Issue #3574).
        return split(str, delim)
    end
    if limit == 1
        result = String[]
        push!(result, str)
        return _substring_retag(result)
    end

    n = length(str)
    dlen = length(delim)

    if dlen == 0
        # Empty delimiter: split into characters (same as split, already retagged).
        return split(str, delim)
    end

    # Find all delimiter positions from left to right
    positions = Int64[]
    i = 1
    while i <= n - dlen + 1
        match = true
        j = 1
        while j <= dlen
            if codeunit(str, i + j - 1) != codeunit(delim, j)
                match = false
                break
            end
            j = j + 1
        end
        if match
            push!(positions, i)
            i = i + dlen
        else
            i = i + 1
        end
    end

    npos = length(positions)
    if npos == 0
        result = String[]
        push!(result, str)
        return _substring_retag(result)
    end

    # limit-1 splits means limit parts
    # rsplit keeps rightmost limit-1 splits, so we skip the first (npos - (limit-1)) positions
    nsplits = limit - 1
    if nsplits >= npos
        # All splits fit within limit - same as regular split (already retagged).
        return split(str, delim)
    end

    # Skip first (npos - nsplits) delimiter positions
    skip = npos - nsplits
    result = String[]

    # First part: everything up to the (skip+1)-th delimiter
    first_split_pos = positions[skip + 1]
    if first_split_pos > 1
        push!(result, str[1:first_split_pos - 1])
    else
        push!(result, "")
    end

    # Remaining parts: between consecutive used delimiters
    k = skip + 1
    while k <= npos
        start_pos = positions[k] + dlen
        if k < npos
            end_pos = positions[k + 1] - 1
        else
            end_pos = n
        end
        if start_pos <= end_pos
            push!(result, str[start_pos:end_pos])
        else
            push!(result, "")
        end
        k = k + 1
    end

    # Issue #3574: retag for Vector{SubString{String}} display.
    return _substring_retag(result)
end

# rsplit with Char delimiter and limit
function rsplit(str::String, delim::Char, limit::Int64)
    return rsplit(str, string(delim), limit)
end

# =============================================================================
# ASCII validation
# =============================================================================

# ascii(s::String) - validate that string contains only ASCII characters (Issue #1842)
# Returns s unchanged if all characters are ASCII (code points 0-127).
# Throws ArgumentError if any non-ASCII character is found.
function ascii(s::String)
    n = ncodeunits(s)
    i = 1
    while i <= n
        if codeunit(s, i) >= 0x80
            error("ArgumentError: invalid ASCII in string")
        end
        i = i + 1
    end
    return s
end

# =============================================================================
# bytes2hex / hex2bytes (Issue #2567)
# =============================================================================
# Based on Julia's base/strings/util.jl

# Internal helper: convert a hex character to its numeric value (0-15)
function _number_from_hex(c::Char)
    if '0' <= c <= '9'
        return Int(c) - Int('0')
    elseif 'a' <= c <= 'f'
        return Int(c) - Int('a') + 10
    elseif 'A' <= c <= 'F'
        return Int(c) - Int('A') + 10
    else
        throw(ArgumentError("invalid hex digit: $c"))
    end
end

# bytes2hex(a) - convert byte array/vector to hexadecimal string
# Note: uses div/rem instead of >> / & since bit-shift operators are not yet lowered
function bytes2hex(a)
    hex_chars = "0123456789abcdef"
    buf = IOBuffer()
    for b in a
        v = Int(b) % 256  # ensure 0-255 range
        hi = div(v, 16)
        lo = v % 16
        write(buf, hex_chars[hi + 1])
        write(buf, hex_chars[lo + 1])
    end
    return String(take!(buf))
end

# hex2bytes(s::String) - convert hexadecimal string to byte vector
function hex2bytes(s::String)
    n = length(s)
    if n % 2 != 0
        throw(ArgumentError("hex2bytes: string length must be even"))
    end
    if n == 0
        return Int64[]
    end
    result = Int64[]
    i = 1
    while i <= n
        hi = _number_from_hex(s[i])
        lo = _number_from_hex(s[i + 1])
        push!(result, hi * 16 + lo)
        i = i + 2
    end
    return result
end
