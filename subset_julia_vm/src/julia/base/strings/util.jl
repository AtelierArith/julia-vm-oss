# =============================================================================
# strings/util.jl - String manipulation functions
# =============================================================================
# Based on Julia's base/strings/util.jl

# =============================================================================
# String replacement functions
# =============================================================================

# SubstitutionString: a replacement string that carries capture-group
# references (`\1`..`\9`, `\g<name>`, `\0` = whole match) for use as the value
# of a `replace(s, pat => sub)` pair (Issue #10174). The `s"..."` string macro
# lowers to `SubstitutionString(raw_content)`, mirroring upstream
# `macro s_str(string) SubstitutionString(string) end`. Keeping it distinct from
# a plain `String` is what lets `replace` know to expand the references against
# each match instead of copying them verbatim.
struct SubstitutionString <: AbstractString
    string::String
end

# Behave like the wrapped string when printed (`println(s"...")` / `string(...)`)
# and `s"..."` when shown, mirroring upstream `show`/`print` on a
# SubstitutionString.
function Base.show(io::IO, s::SubstitutionString)
    print(io, "s")
    show(io, s.string)
end

function Base.print(io::IO, s::SubstitutionString)
    print(io, s.string)
end

# AbstractString surface for SubstitutionString outside `replace` (Issue #10735),
# mirroring upstream base/regex.jl (ncodeunits/codeunit/isvalid/iterate forward
# to the wrapped string). sjulia's String comparison/length are builtins with no
# generic AbstractString fallback, so `==`, `length`, and `getindex` forward via
# narrow concrete methods instead of broad `::AbstractString` ones — broad
# methods perturbed String `==` dispatch inside the replace machinery.
Base.ncodeunits(s::SubstitutionString) = ncodeunits(s.string)
# Workaround: upstream's 1-arg `codeunit(s)` type query is omitted (Issue #11751)
# — sjulia's codeunit builtin rejects the 1-arg form at compile time.
Base.codeunit(s::SubstitutionString, i::Integer) = codeunit(s.string, i)
Base.isvalid(s::SubstitutionString, i::Integer) = isvalid(s.string, i)
Base.iterate(s::SubstitutionString) = iterate(s.string)
Base.iterate(s::SubstitutionString, i::Integer) = iterate(s.string, i)
Base.length(s::SubstitutionString) = length(s.string)
Base.getindex(s::SubstitutionString, i::Integer) = getindex(s.string, i)
Base.getindex(s::SubstitutionString, r::AbstractRange{<:Integer}) = getindex(s.string, r)
Base.:(==)(a::SubstitutionString, b::SubstitutionString) = a.string == b.string
Base.:(==)(a::SubstitutionString, b::String) = a.string == b
Base.:(==)(a::String, b::SubstitutionString) = a == b.string
Base.hash(s::SubstitutionString, h::UInt) = hash(s.string, h)
# Workaround: 1-arg hash method needed (Issue #11754) — sjulia's `hash(x)` is
# the `_hash` builtin and does not forward through 2-arg dispatch like
# upstream's `hash(x) = hash(x, zero(UInt))`.
Base.hash(s::SubstitutionString) = hash(s.string)
Base.String(s::SubstitutionString) = s.string
# Value-form eltype, matching the String pattern in strings/basic.jl (the VM
# cannot express upstream's `eltype(::Type{<:AbstractString}) = Char`); keeps
# `collect(s"...")` a Vector{Char} instead of Vector{Any}.
Base.eltype(s::SubstitutionString) = Char

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
            result = result * string(_int_to_char(b1))
            i = i + 1
        elseif b1 < 0xE0
            # 2-byte UTF-8 sequence (covers Latin-1 .. U+07FF)
            b2 = codeunit(s, i + 1)
            cp = (b1 - 0xC0) * 64 + (b2 - 0x80)
            result = result * string(_int_to_char(cp))
            i = i + 2
        elseif b1 < 0xF0
            # 3-byte UTF-8 sequence (covers BMP non-supplementary, incl. CJK)
            b2 = codeunit(s, i + 1)
            b3 = codeunit(s, i + 2)
            cp = (b1 - 0xE0) * 4096 + (b2 - 0x80) * 64 + (b3 - 0x80)
            result = result * string(_int_to_char(cp))
            i = i + 3
        else
            # 4-byte UTF-8 sequence (supplementary planes, e.g. emoji)
            b2 = codeunit(s, i + 1)
            b3 = codeunit(s, i + 2)
            b4 = codeunit(s, i + 3)
            cp = (b1 - 0xF0) * 262144 + (b2 - 0x80) * 4096 + (b3 - 0x80) * 64 + (b4 - 0x80)
            result = result * string(_int_to_char(cp))
            i = i + 4
        end
    end
    return result
end

# replace: replace occurrences of old with new in string s
# Based on Julia's base/strings/util.jl
# Julia signature: replace(s, old => new; count=typemax(Int)) where old => new is a Pair
# Upstream semantics (Issue #10197): count=0 replaces NOTHING (returns the
# string unchanged), count<0 throws DomainError, and the default is unlimited.
# Internally `_replace_impl` / `_regex_replace` keep their maxcount=0 =
# replace-all convention, so the unlimited default is mapped to 0 below.
# SubsetJuliaVM compiles Pair to Tuple, so we accept both forms.
#
# Argument-validation order mirrors upstream (Issue #10237 codex review): the
# public `replace(s::AbstractString, pat::Pair...; count)` validates the
# receiver (via the `AbstractString` bound) and the pair (via the `Pair`
# bound) BEFORE `_replace_` reaches its `count == 0 && return String(str)`
# short-circuit. So we (1) constrain the receiver to `AbstractString` so a
# non-string receiver is a MethodError even when count==0, and (2) extract the
# pair operands (which errors on a malformed pair) BEFORE the count checks and
# the count==0 early return. A valid count==0 still returns the receiver
# unchanged (upstream replaces nothing, Issue #10197).
function replace(s::AbstractString, pair; count=typemax(Int64))
    # pair is a Pair (old, new) from the => syntax; indexing validates it.
    old = pair[1]
    new = pair[2]
    count < 0 && throw(DomainError(count, "`count` must be non-negative."))
    # Non-literal replacements (a Function called per match, or a
    # SubstitutionString whose capture references must be expanded) go through
    # the general combined-scan path (Issues #10174, #10175).
    if isa(new, Function) || isa(new, SubstitutionString)
        count == 0 && return s
        return _replace_general(string(s), (pair,), count)
    end
    # If old is a Regex, delegate to the builtin _regex_replace (Issue #2112)
    if isa(old, Regex)
        count == 0 && return s
        maxcount = count == typemax(Int64) ? 0 : count
        return _regex_replace(s, old, new, maxcount)
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
    count == 0 && return s
    maxcount = count == typemax(Int64) ? 0 : count
    return _replace_impl(s, old, new, maxcount)
end

# replace(s, p1 => r1, p2 => r2, ...): multiple pattern/replacement pairs applied
# left-to-right simultaneously in a single left-to-right scan (Julia 1.7+,
# Issue #10175). Only one pattern applies to any region, and patterns match only
# the input, never the replacement text. Mirrors upstream
# `replace(s::AbstractString, pat_f::Pair...; count)` / `_replace_` in
# base/strings/util.jl. Two `Pair` positional args make this strictly more
# specific (by arity) than the single-pair method above.
function replace(s::AbstractString, p1::Pair, p2::Pair, ps::Pair...; count=typemax(Int64))
    count < 0 && throw(DomainError(count, "`count` must be non-negative."))
    count == 0 && return string(s)
    return _replace_general(string(s), (p1, p2, ps...), count)
end

# _replace_general: upstream-faithful combined-scan replacement supporting any
# mix of String/Char/Regex patterns with String / SubstitutionString / Function
# replacements. Mirrors `_replace_finish` in base/strings/util.jl: at each step
# pick the pattern whose next match starts earliest (ties → earliest pair),
# copy the text before it, emit that pair's replacement, then advance and
# re-find (via `findnext`) the patterns that were consumed. Regex patterns are
# re-found with `_regex_match_from` (the `findnext(re, str, i)` primitive) so a
# greedy pattern re-matches from the current position — precomputed
# non-overlapping `eachmatch` results would diverge for multi-pattern input.
function _replace_general(str::String, pairs, count::Int)
    npat = length(pairs)
    slen = ncodeunits(str)
    e1 = slen + 1
    notfound = e1 + 1  # sentinel start position for "no further match"

    # Pattern / replacement values are collected with `push!` and only ever
    # *read* afterward: storing a Function or Regex into an array slot via
    # `setindex!` currently corrupts the value in the VM (Issue #10720), whereas
    # `push!` and read-back are correct.
    patterns = Any[]     # per pattern: String / Char / Regex
    replaces = Any[]     # per pattern: String / SubstitutionString / Function / ...
    # Numeric bookkeeping (Int slots support `setindex!` safely).
    curstart = Int[]     # byte start of pattern k's current candidate match
    curstop = Int[]      # byte end (curstop < curstart marks a zero-width match)

    for k in 1:npat
        pk = pairs[k]
        push!(patterns, pk.first)
        push!(replaces, pk.second)
        push!(curstart, notfound)
        push!(curstop, 0)
    end
    for k in 1:npat
        _replace_find!(k, patterns, curstart, curstop, str, 1, notfound)
    end

    # Fast path: no pattern matches anywhere.
    anyfound = false
    for k in 1:npat
        if curstart[k] <= e1
            anyfound = true
            break
        end
    end
    anyfound || return str

    result = ""
    a = 1
    i = 1
    n = 1
    while true
        # Pick the pattern with the earliest match start (lowest index on ties).
        p = 1
        for k in 2:npat
            if curstart[k] < curstart[p]
                p = k
            end
        end
        j = curstart[p]
        kend = curstop[p]
        j > e1 && break
        if i == a || i <= kend
            if j > i
                result = result * str[i:prevind(str, j)]
            end
            pat = patterns[p]
            # Re-fetch the chosen Regex match at its known start `j` (returns the
            # same match) so the replacement can read its captures; avoids
            # storing a RegexMatch through `setindex!` (Issue #10720).
            m = isa(pat, Regex) ? _regex_match_from(pat, str, j) : nothing
            result = result * _apply_replacement(replaces[p], pat, m, str, j, kend)
        end
        if kend < j
            # Zero-width match: emit nothing more, step one character forward.
            i = j
            j == e1 && break
            knext = nextind(str, j)
        else
            i = nextind(str, kend)
            knext = i
        end
        n == count && break
        for k in 1:npat
            if curstart[k] < knext
                _replace_find!(k, patterns, curstart, curstop, str, knext, notfound)
            end
        end
        n += 1
    end
    if i <= slen
        result = result * str[i:lastindex(str)]
    end
    return result
end

# _replace_find!: set pattern k's next candidate match at or after byte `from`.
function _replace_find!(k, patterns, curstart, curstop, str, from, notfound)
    p = patterns[k]
    if isa(p, Regex)
        m = _regex_match_from(p, str, from)
        if m === nothing
            curstart[k] = notfound
            curstop[k] = 0
        else
            curstart[k] = m.offset
            curstop[k] = m.offset + ncodeunits(m.match) - 1
        end
    elseif isa(p, Char)
        idx = findnext(p, str, from)
        if idx === nothing
            curstart[k] = notfound
            curstop[k] = 0
        else
            curstart[k] = idx
            curstop[k] = idx
        end
    else
        rng = findnext(p, str, from)
        if rng === nothing
            curstart[k] = notfound
            curstop[k] = 0
        else
            curstart[k] = first(rng)
            curstop[k] = last(rng)
        end
    end
    return nothing
end

# _apply_replacement: produce the text a single match should be replaced with,
# dispatching on the replacement value (String literal / SubstitutionString /
# Function) and the pattern kind. Mirrors upstream `_replace(io, repl, ...)`.
# `_expand_substitution` (Rust boundary) does the capture-reference expansion
# and C-escape unescaping in one place: with a RegexMatch + Regex for a Regex
# pattern, or with the matched substring + `nothing` for a non-Regex pattern
# (where only `\0` / `\g<0>` — the whole match — is a valid group reference).
function _apply_replacement(repl, pat, m, str, j, kend)
    if isa(repl, SubstitutionString)
        if isa(pat, Regex)
            return _expand_substitution(repl.string, m, pat)
        else
            matched = isa(pat, Char) ? string(str[j]) : str[j:kend]
            return _expand_substitution(repl.string, matched, nothing)
        end
    elseif isa(repl, Function)
        if isa(pat, Char)
            return string(repl(str[j]))
        elseif isa(pat, Regex)
            return string(repl(m.match))
        else
            return string(repl(str[j:kend]))
        end
    elseif isa(repl, AbstractString)
        return repl
    else
        return string(repl)
    end
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
    result = ""
    first = true
    for x in arr
        if first
            result = string(x)
            first = false
        else
            result = result * delim * string(x)
        end
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
    result = ""
    previous = ""
    have_previous = false
    emitted = false

    for x in arr
        current = string(x)
        if !have_previous
            previous = current
            have_previous = true
        else
            if emitted
                result = result * delim * previous
            else
                result = previous
                emitted = true
            end
            previous = current
        end
    end

    if !have_previous
        return ""
    end
    if !emitted
        return previous
    end
    return result * last * previous
end

# =============================================================================
# String trimming functions
# =============================================================================

# lstrip: remove leading whitespace from string
function lstrip(s)
    i = firstindex(s)
    e = lastindex(s)
    while i <= e && isspace(s[i])
        i = nextind(s, i)
    end
    return i <= e ? s[i:e] : ""
end

# lstrip with predicate function (Issue #2057)
function lstrip(pred::Function, s::String)
    i = firstindex(s)
    e = lastindex(s)
    while i <= e
        c = s[i]
        if !pred(c)
            return s[i:e]
        end
        i = nextind(s, i)
    end
    return ""
end

# rstrip: remove trailing whitespace from string
function rstrip(s)
    i = lastindex(s)
    b = firstindex(s)
    while i >= b && isspace(s[i])
        i = prevind(s, i)
    end
    return i >= b ? s[b:i] : ""
end

# rstrip with predicate function (Issue #2057)
function rstrip(pred::Function, s::String)
    i = lastindex(s)
    b = firstindex(s)
    while i >= b
        c = s[i]
        if !pred(c)
            return s[b:i]
        end
        i = prevind(s, i)
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
    i = firstindex(s)
    e = lastindex(s)
    while i <= e
        c = s[i]
        if !pred(c)
            break
        end
        i = nextind(s, i)
    end
    if i > e
        return ""
    end
    j = e
    while j >= i
        c = s[j]
        if !pred(c)
            break
        end
        j = prevind(s, j)
    end
    return s[i:j]
end

# chomp: remove trailing newline (LF or CRLF) from string
# Mirrors upstream base/strings/util.jl chomp: walk character indices with
# lastindex/prevind. The previous byte probe used length(s) — a CHARACTER
# count — as a codeunit BYTE index, so any multibyte content shifted the probe
# off the trailing newline and chomp returned the string unchanged
# (Issue #11642).
function chomp(s)
    i = lastindex(s)
    if i < 1 || s[i] != '\n'
        return s
    end
    j = prevind(s, i)
    if j < 1
        return ""
    end
    if s[j] != '\r'
        return s[1:j]
    end
    k = prevind(s, j)
    if k < 1
        return ""
    end
    return s[1:k]
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

# chopprefix: remove prefix from string if present. Byte counts locate the
# first remaining character; `prevind` supplies the final valid character-start
# endpoint required by public String range indexing (#3606, #11618).
function chopprefix(s, prefix)
    if startswith(s, prefix)
        start = ncodeunits(prefix) + 1
        if start > ncodeunits(s)
            return ""
        end
        return s[start:prevind(s, ncodeunits(s) + 1)]
    end
    return s
end

# chopsuffix: remove suffix from string if present. As upstream does for its
# SubString endpoint, step back from the suffix start to a valid character
# boundary instead of using an arbitrary byte position (#3606, #11618).
function chopsuffix(s, suffix)
    if !isempty(suffix) && endswith(s, suffix)
        suffix_start = ncodeunits(s) - ncodeunits(suffix) + 1
        final = prevind(s, suffix_start)
        if final < 1
            return ""
        end
        return s[1:final]
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
        new_first = string(_int_to_char(b1 + 32))
        if n == 1
            return new_first
        end
        rest = s[nextind(s, 1):lastindex(s)]
        return new_first * rest
    end

    # Latin-1 uppercase (2-byte UTF-8 starting with 0xC3): À-Ö, Ø-Þ
    if b1 == 0xC3 && n >= 2
        b2 = codeunit(s, 2)
        cp = (b1 - 0xC0) * 64 + (b2 - 0x80)
        if (cp >= 0xC0 && cp <= 0xD6) || (cp >= 0xD8 && cp <= 0xDE)
            new_first = string(_int_to_char(cp + 32))
            if n == 2
                return new_first
            end
            rest = s[nextind(s, 1):lastindex(s)]
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
        new_first = string(_int_to_char(b1 - 32))
        if n == 1
            return new_first
        end
        rest = s[nextind(s, 1):lastindex(s)]
        return new_first * rest
    end

    # Latin-1 lowercase (2-byte UTF-8 starting with 0xC3): à-ö, ø-þ
    if b1 == 0xC3 && n >= 2
        b2 = codeunit(s, 2)
        cp = (b1 - 0xC0) * 64 + (b2 - 0x80)
        if (cp >= 0xE0 && cp <= 0xF6) || (cp >= 0xF8 && cp <= 0xFE)
            new_first = string(_int_to_char(cp - 32))
            if n == 2
                return new_first
            end
            rest = s[nextind(s, 1):lastindex(s)]
            return new_first * rest
        end
    end

    return s
end

function _escape_string_hex_digit(n)
    if n < 10
        return string(_int_to_char(48 + n))
    end
    return string(_int_to_char(87 + n))
end

function _escape_string_hex_byte(b)
    ib = Int(b)
    return "\\x" * _escape_string_hex_digit(div(ib, 16)) * _escape_string_hex_digit(ib % 16)
end

function _escape_string_ascii(result, b)
    if b == 92
        return result * "\\\\"
    elseif b == 34
        return result * "\\\""
    elseif b == 10
        return result * "\\n"
    elseif b == 13
        return result * "\\r"
    elseif b == 9
        return result * "\\t"
    elseif b == 0
        return result * "\\0"
    elseif b < 0x20 || b == 0x7f
        return result * _escape_string_hex_byte(b)
    end
    return result * string(_int_to_char(b))
end

_escape_string_continuation_byte(b) = b >= 0x80 && b <= 0xbf

# escape_string: escape special characters in string.
# Decode valid UTF-8 manually so multi-byte characters are emitted intact
# (Issue #3599), but preserve invalid UTF-8 bytes with \xNN escapes so
# `show(::String)`/`repr(::String)` round-trip byte-backed strings (Issue #9589).
function escape_string(s)
    result = ""
    i = 1
    n = ncodeunits(s)
    while i <= n
        b1 = codeunit(s, i)
        if b1 < 0x80
            result = _escape_string_ascii(result, b1)
            i = i + 1
        elseif b1 >= 0xc2 && b1 <= 0xdf && i + 1 <= n
            b2 = codeunit(s, i + 1)
            if _escape_string_continuation_byte(b2)
                cp = (b1 - 0xc0) * 64 + (b2 - 0x80)
                result = result * string(_int_to_char(cp))
                i = i + 2
            else
                result = result * _escape_string_hex_byte(b1)
                i = i + 1
            end
        elseif b1 >= 0xe0 && b1 <= 0xef && i + 2 <= n
            b2 = codeunit(s, i + 1)
            b3 = codeunit(s, i + 2)
            valid_second = (b1 == 0xe0 && b2 >= 0xa0 && b2 <= 0xbf) ||
                           (b1 >= 0xe1 && b1 <= 0xec && _escape_string_continuation_byte(b2)) ||
                           (b1 == 0xed && b2 >= 0x80 && b2 <= 0x9f) ||
                           (b1 >= 0xee && b1 <= 0xef && _escape_string_continuation_byte(b2))
            if valid_second && _escape_string_continuation_byte(b3)
                cp = (b1 - 0xe0) * 4096 + (b2 - 0x80) * 64 + (b3 - 0x80)
                result = result * string(_int_to_char(cp))
                i = i + 3
            else
                result = result * _escape_string_hex_byte(b1)
                i = i + 1
            end
        elseif b1 >= 0xf0 && b1 <= 0xf4 && i + 3 <= n
            b2 = codeunit(s, i + 1)
            b3 = codeunit(s, i + 2)
            b4 = codeunit(s, i + 3)
            valid_second = (b1 == 0xf0 && b2 >= 0x90 && b2 <= 0xbf) ||
                           (b1 >= 0xf1 && b1 <= 0xf3 && _escape_string_continuation_byte(b2)) ||
                           (b1 == 0xf4 && b2 >= 0x80 && b2 <= 0x8f)
            if valid_second && _escape_string_continuation_byte(b3) && _escape_string_continuation_byte(b4)
                cp = (b1 - 0xf0) * 262144 + (b2 - 0x80) * 4096 + (b3 - 0x80) * 64 + (b4 - 0x80)
                result = result * string(_int_to_char(cp))
                i = i + 4
            else
                result = result * _escape_string_hex_byte(b1)
                i = i + 1
            end
        else
            result = result * _escape_string_hex_byte(b1)
            i = i + 1
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
                result = result * string(_int_to_char(10))
            elseif e == 't'    # \t -> tab (9)
                result = result * string(_int_to_char(9))
            elseif e == 'r'    # \r -> carriage return (13)
                result = result * string(_int_to_char(13))
            elseif e == '\\'   # \\ -> backslash (92)
                result = result * string(_int_to_char(92))
            elseif e == '"'    # \" -> double quote (34)
                result = result * string(_int_to_char(34))
            elseif e == '0'    # \0 -> null (0)
                result = result * string(_int_to_char(0))
            elseif e == 'a'    # \a -> bell (7)
                result = result * string(_int_to_char(7))
            elseif e == 'b'    # \b -> backspace (8)
                result = result * string(_int_to_char(8))
            elseif e == 'f'    # \f -> form feed (12)
                result = result * string(_int_to_char(12))
            elseif e == 'v'    # \v -> vertical tab (11)
                result = result * string(_int_to_char(11))
            elseif e == 'e'    # \e -> escape (27)
                result = result * string(_int_to_char(27))
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
                result = result * string(_int_to_char(val))
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
                result = result * string(_int_to_char(val))
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
                result = result * string(_int_to_char(val))
            else
                # Unknown escape: keep as-is. (Upstream raises ArgumentError;
                # the subset's lenient behavior and octal escapes are tracked
                # separately — Issue #6724 covers the regex-free migration only.)
                result = result * string(_int_to_char(92)) * string(e)
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
function _split_push_part(result, part, keepempty)
    if keepempty != 0 || !isempty(part)
        push!(result, part)
    end
end

function split(str::String, delim::String; limit=0, keepempty=true)
    result = String[]
    n = ncodeunits(str)
    dlen = ncodeunits(delim)

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
                # Public String range endpoints must both be character starts.
                # `nbytes` is inside the final character when it is non-ASCII,
                # so mirror upstream's `prevind` endpoint construction instead
                # of relying on the old permissive range-slice bug (#11618).
                push!(result, str[i:prevind(str, nbytes + 1)])
                # Issue #3574: retag as Vector{SubString{String}} so the show
                # form matches Julia 1.12 (`SubString{String}["a", "b"]`).
                if keepempty != 0; return _substring_retag(result); else; return _filter_nonempty(result); end
            end
            ni = nextind(str, i)
            # A one-character String slice uses the character's valid start
            # index for both inclusive endpoints (e.g. `"é"[1:1]`).
            push!(result, str[i:i])
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
                _split_push_part(result, str[start:prevind(str, i)], keepempty)
            else
                _split_push_part(result, "", keepempty)
            end
            start = i + dlen
            i = start
        else
            i = i + 1
        end
    end

    # Add remaining part after last delimiter
    if start <= n
        _split_push_part(result, str[start:prevind(str, n + 1)], keepempty)
    else
        _split_push_part(result, "", keepempty)
    end

    # Issue #3574: see comment above.
    return _substring_retag(result)
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

# split with Regex delimiter (Issue #10176). Delegates to the `_regex_split`
# builtin, which implements upstream's `SplitIterator` semantics (limit /
# keepempty) over the regex matches. `_substring_retag` gives the result the
# `Vector{SubString{String}}` show form that Julia 1.12 produces. Note there is
# deliberately no `rsplit(::String, ::Regex)` method — upstream Julia 1.12 itself
# throws `MethodError: no method matching findprev(::Regex, ...)` for it.
function split(str::String, delim::Regex; limit=0, keepempty=true)
    return _substring_retag(_regex_split(str, delim, limit, keepempty))
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
        push!(result, str[start:prevind(str, i)])
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
# Calls into the private `_rsplit_limit(str, delim, limit)` helper below, which
# keeps the leftmost part unsplit when limit > 0 and falls back to `split`
# otherwise. Upstream exposes only the `limit=` keyword form — the positional
# 3-arg spelling `rsplit(s, delim, limit)` is a MethodError upstream, so the
# helper is underscore-private and NOT a user-callable `rsplit` method
# (Issue #10324 item 2).
function rsplit(str::String, delim::String; limit=0, keepempty=true)
    if keepempty
        return _rsplit_limit(str, delim, limit)
    end
    return _rsplit_nonempty(str, delim, limit)
end

# rsplit with Char delimiter — accepts `limit` and `keepempty` keywords
# (Issues #3610, #3651).
function rsplit(str::String, delim::Char; limit=0, keepempty=true)
    if keepempty
        return _rsplit_limit(str, string(delim), limit)
    end
    return _rsplit_nonempty(str, string(delim), limit)
end

# _rsplit_limit: split from the right, keeping leftmost parts together.
# Private helper backing the `limit=` keyword form (Issue #10324 item 2).
function _rsplit_limit(str::String, delim::String, limit::Int64)
    if limit <= 0
        # Delegates to split, which already retags (Issue #3574).
        return split(str, delim)
    end
    if limit == 1
        result = String[]
        push!(result, str)
        return _substring_retag(result)
    end

    n = ncodeunits(str)
    dlen = ncodeunits(delim)

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
        push!(result, str[1:prevind(str, first_split_pos)])
    else
        push!(result, "")
    end

    # Remaining parts: between consecutive used delimiters
    k = skip + 1
    while k <= npos
        start_pos = positions[k] + dlen
        if k < npos
            end_pos = prevind(str, positions[k + 1])
        else
            end_pos = prevind(str, n + 1)
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

function _rsplit_nonempty(str::String, delim::String, limit)
    if limit <= 0
        return split(str, delim; keepempty=false)
    end

    n = ncodeunits(str)
    dlen = ncodeunits(delim)

    if dlen == 0
        return split(str, delim; limit=limit, keepempty=false)
    end

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

    right_parts = String[]
    right_end = prevind(str, n + 1)
    kept = 0
    k = length(positions)
    while k >= 1
        if kept >= limit - 1
            break
        end
        pos = positions[k]
        start_pos = pos + dlen
        if start_pos <= right_end
            part = str[start_pos:right_end]
        else
            part = ""
        end
        if !isempty(part)
            push!(right_parts, part)
            kept = kept + 1
        end
        right_end = prevind(str, pos)
        k = k - 1
    end

    result = String[]
    if right_end >= 1
        left = str[1:right_end]
        if !isempty(left)
            push!(result, left)
        end
    end

    k = length(right_parts)
    while k >= 1
        push!(result, right_parts[k])
        k = k - 1
    end

    return _substring_retag(result)
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
            throw(ArgumentError("invalid ASCII in string"))
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

# bytes2hex(a) - convert byte iterator/array to hexadecimal string.
# Upstream requires `eltype(itr) === UInt8` and throws otherwise (Issue #10324
# item 1); mirror that instead of silently converting any integer eltype.
# Note: uses div/rem instead of >> / & since bit-shift operators are not yet lowered
function bytes2hex(a)
    eltype(a) === UInt8 || throw(ArgumentError("eltype of iterator not UInt8"))
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

# hex2bytes(s::String) - convert hexadecimal string to a `Vector{UInt8}`.
# Upstream returns `Vector{UInt8}` (Issue #10324 item 1); returning UInt8 (not
# Int64) keeps `bytes2hex(hex2bytes(s))` round-trips valid now that bytes2hex
# rejects non-UInt8 eltypes.
function hex2bytes(s::String)
    n = length(s)
    if n % 2 != 0
        throw(ArgumentError("hex2bytes: string length must be even"))
    end
    if n == 0
        return UInt8[]
    end
    result = UInt8[]
    i = 1
    while i <= n
        hi = _number_from_hex(s[i])
        lo = _number_from_hex(s[i + 1])
        push!(result, UInt8(hi * 16 + lo))
        i = i + 2
    end
    return result
end
