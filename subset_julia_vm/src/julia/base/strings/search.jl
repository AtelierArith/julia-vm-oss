# =============================================================================
# strings/search.jl - String search functions
# =============================================================================
# Based on Julia's base/strings/search.jl

# occursin: check if needle appears in haystack.
# Uses byte-level (`ncodeunits`) loop because the comparison underneath uses
# byte-level `codeunit`. Mixing `length` (char count) with `codeunit` produced
# false positives for non-ASCII needles sharing a leading UTF-8 byte (#3604).
function occursin(needle, haystack)
    nlen = ncodeunits(needle)
    hlen = ncodeunits(haystack)
    # Empty needle always matches
    if nlen == 0
        return true
    end
    # Needle longer than haystack cannot match
    if nlen > hlen
        return false
    end
    # Simple substring search
    i = 1
    while i <= hlen - nlen + 1
        # Check if substring starting at i matches needle
        match = true
        j = 1
        while j <= nlen
            if codeunit(haystack, i + j - 1) != codeunit(needle, j)
                match = false
                break
            end
            j = j + 1
        end
        if match
            return true
        end
        i = i + 1
    end
    return false
end

# occursin: Char needle (Issue #3570)
# Julia Base: occursin(c::AbstractChar, s::AbstractString) = any(==(c), s)
# We avoid `length(::Char)` (raised by the generic occursin above) by walking
# the haystack code-point by code-point and comparing to the needle Char.
function occursin(needle::Char, haystack::String)
    n = ncodeunits(haystack)
    i = 1
    while i <= n
        if isvalid(haystack, i)
            if haystack[i] == needle
                return true
            end
        end
        i = i + 1
    end
    return false
end

# occursin: Regex needle (Issue #5705)
# Julia Base: occursin(r::Regex, s::AbstractString; offset=0) tests whether the
# pattern matches anywhere in `s`. We dispatch on the `::Regex` needle (now that a
# `::Regex`-typed parameter used with `match` compiles, Issue #5678) so the generic
# byte-walking method — which calls `ncodeunits(needle)` and rejects a Regex — is
# not reached.
function occursin(needle::Regex, haystack)
    return match(needle, haystack) !== nothing
end

# occursin: curried form (Issue #2100)
# Julia Base: occursin(haystack) returns needle -> occursin(needle, haystack)
function occursin(haystack::String)
    function _occursin_curried(needle)
        return occursin(needle, haystack)
    end
    return _occursin_curried
end

# contains: check if haystack contains needle
# contains: curried form (Issue #2100)
# Julia Base: contains(needle) returns haystack -> contains(haystack, needle)
function contains(needle::String)
    function _contains_curried(haystack)
        return contains(haystack, needle)
    end
    return _contains_curried
end

# This is the reverse argument order of occursin:
# contains(haystack, needle) == occursin(needle, haystack)
function contains(haystack, needle)
    return occursin(needle, haystack)
end

# startswith: check if string starts with prefix.
# Use byte-level `ncodeunits` since the comparison loop uses `codeunit`.
# Previously mixed char-count `length` with byte-indexed `codeunit`, producing
# false positives like `startswith("ê", "é") == true` because both 'é' and 'ê'
# are 2-byte UTF-8 chars sharing the leading 0xC3 byte. (#3602)
function startswith(s, prefix)
    # `startswith(s, re::Regex)`: true iff the regex matches at the START of `s`.
    # The leftmost match (what `match` returns) begins at index 1 exactly when the
    # regex matches anchored at the start, matching upstream (Issue #5676). Handled
    # via `isa` inside the untyped method because a `::Regex`-typed parameter used
    # with `match` currently fails to compile.
    if isa(prefix, Regex)
        m = match(prefix, s)
        return m !== nothing && m.offset == 1
    end
    slen = ncodeunits(s)
    plen = ncodeunits(prefix)
    if plen > slen
        return false
    end
    if plen == 0
        return true
    end
    i = 1
    while i <= plen
        if codeunit(s, i) != codeunit(prefix, i)
            return false
        end
        i = i + 1
    end
    return true
end

# endswith: Regex suffix (Issue #5676)
# Julia Base anchors the regex at the END of `s` (PCRE ENDANCHORED); a leftmost
# `match` cannot decide this (e.g. `endswith("hello", r".")` is true even though the
# leftmost match of `.` is at index 1). We delegate to the internal builtin
# `_endswith_regex`, which rebuilds the pattern with a trailing `$` anchor and tests
# it. A `::Regex`-typed suffix compiles since Issue #5678; the method dispatches
# ahead of the generic `endswith`, which would call `ncodeunits(suffix)`.
function endswith(s, suffix::Regex)
    return _endswith_regex(s, suffix)
end

# keys(m::RegexMatch) (Issue #10173). Upstream base/regex.jl returns a vector of
# keys for all capture groups: the group's name (String) for named groups, its
# 1-based index (Int) otherwise. `keys(m)` on an Any-typed RegexMatch routes
# through method dispatch (which never reaches the DictKeys builtin fallback),
# so this method delegates to the `_regexmatch_keys` helper, which is backed by
# the RegexMatch arm of the DictKeys builtin.
function keys(m::RegexMatch)
    return _regexmatch_keys(m)
end

# startswith: curried form (Issue #2100)
# Julia Base: startswith(prefix) returns s -> startswith(s, prefix)
function startswith(prefix::String)
    function _startswith_curried(s)
        return startswith(s, prefix)
    end
    return _startswith_curried
end

# endswith: check if string ends with suffix.
# Use byte-level `ncodeunits` so the offset and per-byte comparison agree.
# Previously mixed char-count `length` with byte-indexed `codeunit`. (#3603)
function endswith(s, suffix)
    slen = ncodeunits(s)
    suflen = ncodeunits(suffix)
    if suflen > slen
        return false
    end
    if suflen == 0
        return true
    end
    offset = slen - suflen
    i = 1
    while i <= suflen
        if codeunit(s, offset + i) != codeunit(suffix, i)
            return false
        end
        i = i + 1
    end
    return true
end

# endswith: curried form (Issue #2100)
# Julia Base: endswith(suffix) returns s -> endswith(s, suffix)
function endswith(suffix::String)
    function _endswith_curried(s)
        return endswith(s, suffix)
    end
    return _endswith_curried
end

# =============================================================================
# findfirst / findlast / findnext / findprev for strings (Issue #2562)
# =============================================================================
# Based on Julia's base/strings/search.jl
# Char pattern → returns Int64 (byte index) or nothing
# String pattern → returns UnitRange{Int64} or nothing

# --- findnext: Char pattern ---
function findnext(c::Char, s::String, i::Int64)
    n = ncodeunits(s)
    if i < 1 || i > n + 1
        return nothing
    end
    while i <= n
        if s[i] == c
            return i
        end
        i = nextind(s, i)
    end
    return nothing
end

# --- findnext: String pattern ---
# Julia's findnext on String returns a UnitRange of *string indices* (byte
# positions where each character starts). The start of the range is the byte
# index of the first matched character; the end is the byte index of the *start*
# of the last matched character — NOT the last byte of the match. For ASCII
# patterns these coincide, but for multi-byte patterns they don't, e.g.
# `findfirst("é", "éa")` should return `1:1`, not `1:2`. (Issue #3605)
function findnext(pattern::String, s::String, i::Int64)
    n = ncodeunits(s)
    m = ncodeunits(pattern)
    if m == 0
        # Empty pattern matches at position i (returns empty range)
        return i:i-1
    end
    # Byte offset within the pattern of the start of its last character.
    # `thisind(pattern, m)` walks back over UTF-8 continuation bytes so the
    # range end aligns with a character boundary in the haystack.
    last_char_offset = thisind(pattern, m) - 1
    while i + m - 1 <= n
        # Compare bytes
        found = true
        j = 1
        while j <= m
            if codeunit(s, i + j - 1) != codeunit(pattern, j)
                found = false
                break
            end
            j = j + 1
        end
        if found
            return i:i+last_char_offset
        end
        i = i + 1
    end
    return nothing
end

# --- findprev: Char pattern ---
function findprev(c::Char, s::String, i::Int64)
    if i < 1
        return nothing
    end
    n = ncodeunits(s)
    if i > n
        i = n
    end
    # Walk backward to find valid start position
    while i >= 1
        if isvalid(s, i) && s[i] == c
            return i
        end
        i = i - 1
    end
    return nothing
end

# --- findprev: String pattern ---
# As with `findnext`, the returned UnitRange end is the byte index of the start
# of the last matched character, not the last byte of the match. (Issue #3605)
function findprev(pattern::String, s::String, i::Int64)
    n = ncodeunits(s)
    m = ncodeunits(pattern)
    if m == 0
        return i:i-1
    end
    if i > n
        i = n
    end
    # Byte offset within the pattern of the start of its last character.
    last_char_offset = thisind(pattern, m) - 1
    # `i` bounds the start index considered by findprev. Clamp to the last
    # possible byte start for the pattern, but do not shift backward by the
    # pattern length; otherwise findprev("é", "éaéa", 4) skips the match at 4.
    start = i
    last_start = n - m + 1
    if start > last_start
        start = last_start
    end
    if start < 1
        return nothing
    end
    while start >= 1
        found = true
        j = 1
        while j <= m
            if codeunit(s, start + j - 1) != codeunit(pattern, j)
                found = false
                break
            end
            j = j + 1
        end
        if found
            return start:start+last_char_offset
        end
        start = start - 1
    end
    return nothing
end

# --- findfirst / findlast as convenience wrappers ---

function findfirst(c::Char, s::String)
    return findnext(c, s, 1)
end

function findfirst(pattern::String, s::String)
    return findnext(pattern, s, 1)
end

function findlast(c::Char, s::String)
    return findprev(c, s, ncodeunits(s))
end

function findlast(pattern::String, s::String)
    return findprev(pattern, s, ncodeunits(s))
end

# =============================================================================
# findall / count for String and Char patterns (Issue #3726)
# =============================================================================
# Both walk the string with the existing `findnext` overloads, advancing past
# the previous match each iteration. They mirror official Julia semantics:
#
#   - findall(pattern::String, s::String) → Vector{UnitRange{Int64}}
#   - findall(c::Char, s::String)         → Vector{Int64}
#   - count(pattern::String, s::String)   → Int
#   - count(c::Char, s::String)           → Int
#
# Empty-pattern semantics for String patterns mirror official Julia:
#   findall("", s) → length(s)+1 empty UnitRanges at every char boundary
#   count("", s)  → length(s)+1
# These are the sole implementations: the former Rust builtins `StringFindAll`
# and `StringCount` were removed (Issue #6724).

# --- findall: Char pattern → Vector{Int64} of byte positions ---
function findall(c::Char, s::String)
    result = Int64[]
    n = ncodeunits(s)
    i = 1
    while i <= n
        idx = findnext(c, s, i)
        if idx === nothing
            break
        end
        push!(result, idx)
        i = nextind(s, idx)
    end
    return result
end

# --- findall: String pattern → Vector{UnitRange{Int64}} ---
function findall(pattern::String, s::String)
    result = Vector{UnitRange{Int64}}()
    n = ncodeunits(s)
    m = ncodeunits(pattern)
    if m == 0
        # Empty pattern: emit empty range at every character boundary,
        # matching official Julia (length(s)+1 ranges).
        i = 1
        while i <= n + 1
            push!(result, i:i-1)
            if i > n
                break
            end
            i = nextind(s, i)
        end
        return result
    end
    i = 1
    while i <= n
        rng = findnext(pattern, s, i)
        if rng === nothing
            break
        end
        push!(result, rng)
        # Advance past the matched bytes (non-overlapping). The match covered
        # exactly `m` codeunits starting at `first(rng)`.
        i = first(rng) + m
    end
    return result
end

# --- count: Char pattern → Int ---
function count(c::Char, s::String)
    n_matches = 0
    n = ncodeunits(s)
    i = 1
    while i <= n
        idx = findnext(c, s, i)
        if idx === nothing
            break
        end
        n_matches += 1
        i = nextind(s, idx)
    end
    return n_matches
end

# --- count: String pattern → Int ---
function count(pattern::String, s::String)
    n = ncodeunits(s)
    m = ncodeunits(pattern)
    if m == 0
        # Empty pattern: count(\"\", s) == length(s) + 1, matching official Julia.
        return length(s) + 1
    end
    n_matches = 0
    i = 1
    while i <= n
        rng = findnext(pattern, s, i)
        if rng === nothing
            break
        end
        n_matches += 1
        i = first(rng) + m
    end
    return n_matches
end

# =============================================================================
# findall / count for Regex patterns (Issue #6749)
# =============================================================================
# Public regex search wrappers, built in pure Julia on top of `eachmatch` (the
# Rust regex-crate boundary). `occursin(::Regex, s)` already routes through
# `match` (above); these complete the public search API. Both mirror official
# Julia (base/regex.jl) for the default (non-overlapping) case:
#
#   - count(r::Regex, s)   → Int (number of non-overlapping matches)
#   - findall(r::Regex, s) → Vector{UnitRange{Int64}} of byte ranges
#
# A match's byte range is `m.offset : m.offset + ncodeunits(m.match) - 1`, so an
# empty match yields an empty range (offset:offset-1), matching upstream.

# --- count: Regex pattern → Int ---
function count(pat::Regex, s::AbstractString)
    n_matches = 0
    for _ in eachmatch(pat, s)
        n_matches += 1
    end
    return n_matches
end

# --- findall: Regex pattern → Vector{UnitRange{Int64}} ---
function findall(pat::Regex, s::AbstractString)
    result = Vector{UnitRange{Int64}}()
    for m in eachmatch(pat, s)
        start = m.offset
        stop = start + ncodeunits(m.match) - 1
        push!(result, start:stop)
    end
    return result
end

# --- findnext / findfirst: Regex pattern → UnitRange{Int64} or nothing ---
# Mirror upstream `findnext(re::Regex, str, idx)` / `findfirst(re::Regex, s)`
# (base/regex.jl): return the 1-based byte UnitRange of the first match at or
# after byte index `i`, or `nothing` when there is none. The positional search
# runs against the FULL string via the `_regex_findnext` builtin (sjulia's
# analog of upstream's `PCRE.exec(re, str, idx-1)`), so lookbehind, `\b` word
# boundaries and `^` anchors still see the preceding context — unlike a
# substring/`eachmatch` scan, which would miss overlapping matches. The range is
# built with the same `offset : offset + ncodeunits(match) - 1` machinery as
# `findall(::Regex, s)` above. `findlast(::Regex, s)` is intentionally NOT
# defined — upstream Julia itself throws MethodError for it (Issue #10177).
function findnext(re::Regex, s::AbstractString, i::Integer)
    ii = Int(i)
    # Upstream `_findnext_re` throws BoundsError when idx > nextind(str, lastindex(str)),
    # i.e. when it exceeds ncodeunits(s) + 1.
    if ii > ncodeunits(s) + 1
        throw(BoundsError(s, ii))
    end
    # Upstream converts `idx - 1` to the PCRE UInt offset, so a non-positive
    # index raises InexactError (e.g. convert(UInt64, -1)); reproduce that by
    # performing the same conversion instead of silently returning nothing
    # (Issue #10736).
    if ii < 1
        UInt64(ii - 1)
    end
    m = _regex_findnext(re, s, ii)
    m === nothing && return nothing
    start = m.offset
    stop = start + ncodeunits(m.match) - 1
    return start:stop
end

function findfirst(re::Regex, s::AbstractString)
    return findnext(re, s, firstindex(s))
end
