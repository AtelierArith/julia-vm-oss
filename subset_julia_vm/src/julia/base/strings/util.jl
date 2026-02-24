# =============================================================================
# strings/util.jl - String manipulation functions
# =============================================================================
# Based on Julia's base/strings/util.jl

# =============================================================================
# String replacement functions
# =============================================================================

# _replace_impl: internal implementation of string replacement
# Replace occurrences of old with new in string s
# maxcount=0 means replace all (default), maxcount=N replaces at most N (Issue #2043)
function _replace_impl(s, old, new, maxcount)
    slen = length(s)
    oldlen = length(old)
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
        # No match or limit reached - append current character
        result = result * string(Char(codeunit(s, i)))
        i = i + 1
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
    return _replace_impl(s, old, new, count)
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

# chopprefix: remove prefix from string if present
function chopprefix(s, prefix)
    if startswith(s, prefix)
        plen = length(prefix)
        return s[plen+1:length(s)]
    end
    return s
end

# chopsuffix: remove suffix from string if present
function chopsuffix(s, suffix)
    if endswith(s, suffix)
        slen = length(suffix)
        return s[1:length(s)-slen]
    end
    return s
end

# lowercasefirst: convert first character to lowercase
function lowercasefirst(s)
    if length(s) == 0
        return s
    end
    first_char = codeunit(s, 1)
    if first_char >= 65 && first_char <= 90
        new_first = Char(first_char + 32)
        if length(s) == 1
            return string(new_first)
        end
        rest = s[2:length(s)]
        return string(new_first) * rest
    end
    return s
end

# uppercasefirst: convert first character to uppercase
function uppercasefirst(s)
    if length(s) == 0
        return s
    end
    first_char = codeunit(s, 1)
    if first_char >= 97 && first_char <= 122
        new_first = Char(first_char - 32)
        if length(s) == 1
            return string(new_first)
        end
        rest = s[2:length(s)]
        return string(new_first) * rest
    end
    return s
end

# escape_string: escape special characters in string
function escape_string(s)
    result = ""
    n = length(s)
    i = 1
    while i <= n
        c = codeunit(s, i)
        if c == 92
            result = result * "\\\\"
        elseif c == 34
            result = result * "\\\""
        elseif c == 10
            result = result * "\\n"
        elseif c == 13
            result = result * "\\r"
        elseif c == 9
            result = result * "\\t"
        elseif c == 0
            result = result * "\\0"
        else
            result = result * string(Char(c))
        end
        i = i + 1
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
    result = ""
    n = length(s)
    i = 1
    while i <= n
        c = codeunit(s, i)
        if c == 92 && i < n  # backslash
            i = i + 1
            c2 = codeunit(s, i)
            if c2 == 110       # \n -> newline (10)
                result = result * string(Char(10))
            elseif c2 == 116   # \t -> tab (9)
                result = result * string(Char(9))
            elseif c2 == 114   # \r -> carriage return (13)
                result = result * string(Char(13))
            elseif c2 == 92    # \\ -> backslash (92)
                result = result * string(Char(92))
            elseif c2 == 34    # \" -> double quote (34)
                result = result * string(Char(34))
            elseif c2 == 48    # \0 -> null (0)
                result = result * string(Char(0))
            elseif c2 == 97    # \a -> bell (7)
                result = result * string(Char(7))
            elseif c2 == 98    # \b -> backspace (8)
                result = result * string(Char(8))
            elseif c2 == 102   # \f -> form feed (12)
                result = result * string(Char(12))
            elseif c2 == 118   # \v -> vertical tab (11)
                result = result * string(Char(11))
            elseif c2 == 101   # \e -> escape (27)
                result = result * string(Char(27))
            elseif c2 == 120   # \x -> 2-digit hex escape
                # Read up to 2 hex digits
                val = 0
                k = 0
                while k < 2 && i + 1 <= n
                    h = _hexval(codeunit(s, i + 1))
                    if h < 0
                        break
                    end
                    val = val * 16 + h
                    i = i + 1
                    k = k + 1
                end
                result = result * string(Char(val))
            elseif c2 == 117   # \u -> 4-digit unicode escape
                # Read up to 4 hex digits
                val = 0
                k = 0
                while k < 4 && i + 1 <= n
                    h = _hexval(codeunit(s, i + 1))
                    if h < 0
                        break
                    end
                    val = val * 16 + h
                    i = i + 1
                    k = k + 1
                end
                result = result * string(Char(val))
            elseif c2 == 85    # \U -> 8-digit unicode escape
                # Read up to 8 hex digits
                val = 0
                k = 0
                while k < 8 && i + 1 <= n
                    h = _hexval(codeunit(s, i + 1))
                    if h < 0
                        break
                    end
                    val = val * 16 + h
                    i = i + 1
                    k = k + 1
                end
                result = result * string(Char(val))
            else
                # Unknown escape: keep as-is
                result = result * string(Char(92)) * string(Char(c2))
            end
        else
            result = result * string(Char(c))
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
function split(str::String, delim::String; limit=0)
    result = String[]
    n = length(str)
    dlen = length(delim)

    # Empty delimiter: split into characters
    if dlen == 0
        i = 1
        while i <= n
            if limit > 0 && length(result) >= limit - 1
                push!(result, str[i:n])
                return result
            end
            push!(result, string(Char(codeunit(str, i))))
            i = i + 1
        end
        return result
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

    return result
end

# split with Char delimiter
function split(str::String, delim::Char; limit=0)
    return split(str, string(delim), limit=limit)
end

# =============================================================================
# rsplit: split string by delimiter, starting from the right
# =============================================================================
# Based on Julia's base/strings/util.jl (lines 968-976)
# rsplit is like split but when limit is applied, only the rightmost
# limit-1 splits are performed, keeping the left part unsplit.

# rsplit with String delimiter (basic, no limit)
function rsplit(str::String, delim::String)
    return split(str, delim)
end

# rsplit with Char delimiter (basic, no limit)
function rsplit(str::String, delim::Char)
    return rsplit(str, string(delim))
end

# rsplit with limit: split from the right, keeping leftmost parts together
function rsplit(str::String, delim::String, limit::Int64)
    if limit <= 0
        return split(str, delim)
    end
    if limit == 1
        result = String[]
        push!(result, str)
        return result
    end

    n = length(str)
    dlen = length(delim)

    if dlen == 0
        # Empty delimiter: split into characters (same as split)
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
        return result
    end

    # limit-1 splits means limit parts
    # rsplit keeps rightmost limit-1 splits, so we skip the first (npos - (limit-1)) positions
    nsplits = limit - 1
    if nsplits >= npos
        # All splits fit within limit - same as regular split
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

    return result
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
