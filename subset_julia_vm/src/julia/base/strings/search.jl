# =============================================================================
# strings/search.jl - String search functions
# =============================================================================
# Based on Julia's base/strings/search.jl

# occursin: check if needle appears in haystack
# Based on Julia's base/strings/search.jl
function occursin(needle, haystack)
    nlen = length(needle)
    hlen = length(haystack)
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

# startswith: check if string starts with prefix
function startswith(s, prefix)
    slen = length(s)
    plen = length(prefix)
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

# startswith: curried form (Issue #2100)
# Julia Base: startswith(prefix) returns s -> startswith(s, prefix)
function startswith(prefix::String)
    function _startswith_curried(s)
        return startswith(s, prefix)
    end
    return _startswith_curried
end

# endswith: check if string ends with suffix
function endswith(s, suffix)
    slen = length(s)
    suflen = length(suffix)
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
function findnext(pattern::String, s::String, i::Int64)
    n = ncodeunits(s)
    m = ncodeunits(pattern)
    if m == 0
        # Empty pattern matches at position i (returns empty range)
        return i:i-1
    end
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
            return i:i+m-1
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
function findprev(pattern::String, s::String, i::Int64)
    n = ncodeunits(s)
    m = ncodeunits(pattern)
    if m == 0
        return i:i-1
    end
    if i > n
        i = n
    end
    # Start position is where pattern could end at position i
    start = i - m + 1
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
            return start:start+m-1
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
