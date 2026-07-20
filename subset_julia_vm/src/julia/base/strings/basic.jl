# =============================================================================
# strings/basic.jl - Character classification functions
# =============================================================================
# Based on Julia's base/strings/basic.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
# Function names MUST match Julia exactly (e.g., isdigit NOT is_digit).

# ndigits: moved to intfuncs.jl with keyword argument support (Issue #2020)

# Julia's string element type is Char. Upstream defines
# `eltype(::Type{<:AbstractString}) = Char`; keep the value method in
# Pure Julia so `eltype(itr)` works when `itr` is a runtime String.
function eltype(s::String)
    return Char
end

# Type form `eltype(String) === Char` (Issue #5116): the VM cannot bind a
# covariant `::Type{<:AbstractString}` type parameter, so dispatch on the
# concrete `String` type. Mirrors upstream `eltype(::Type{<:AbstractString})`.
eltype(::Type{String}) = Char

# Julia Base defines this as a Union-typed vararg method in
# `base/strings/basic.jl`. sjulia does not support AnnotatedString yet, so the
# subset implementation keeps the same dispatch shape and uses the plain string
# branch.
function Base.:*(s::Union{AbstractChar, AbstractString}, t::Union{AbstractChar, AbstractString}...)
    return string(s, t...)
end

# Public string construction wrappers.
#
# Upstream implements `string(xs...)` through print-to-IOBuffer and defines
# `codeunits` as a lightweight wrapper over `ncodeunits`/`codeunit`. Keep the
# same public shape here; the lowest storage/formatting boundaries remain VM
# primitives.
string() = ""
string(s::AbstractString) = s

function string(x)
    io = IOBuffer()
    print(io, x)
    return String(take!(io))
end

function string(x, y, zs...)
    io = IOBuffer()
    print(io, x)
    print(io, y)
    for z in zs
        print(io, z)
    end
    return String(take!(io))
end

String(s::String) = s
String(s::Symbol) = string(s)
String(v::Vector{UInt8}) = _string_from_chars(v)
String(v::Vector{Char}) = _string_from_chars(v)
String(m::Memory{UInt8}) = _string_from_chars(collect(m))
String(m::Memory{Char}) = _string_from_chars(collect(m))
String(m::Memory) = _string_from_chars(collect(m))
String(v::Array) = _string_from_chars(v)

struct CodeUnits{T,S} <: AbstractVector
    s::S
end

codeunits(s::String) = CodeUnits{UInt8,String}(s)
length(c::CodeUnits) = ncodeunits(c.s)
size(c::CodeUnits) = (length(c),)
eltype(::Type{CodeUnits{T,S}}) where {T,S} = T
eltype(c::CodeUnits{T,S}) where {T,S} = T
getindex(c::CodeUnits, i::Int) = codeunit(c.s, i)

function iterate(c::CodeUnits)
    if length(c) == 0
        return nothing
    end
    return (c[1], 2)
end

function iterate(c::CodeUnits, i)
    if i > length(c)
        return nothing
    end
    return (c[i], i + 1)
end

String(c::CodeUnits{UInt8,String}) = c.s
String(c::CodeUnits) = _string_from_chars(collect(c))

# isdigit: check if character code is a digit
# ASCII: '0' = 48, '9' = 57
function isdigit(c)
    c = Int(c)
    return c >= 48 && c <= 57
end

# isletter: check if character code is a letter.
# Conservative subset of Unicode "Letter" categories — covers ASCII,
# Latin-1, Greek, Cyrillic, plus the main CJK / Hangul / Kana ranges
# (matching the wide-char ranges in `textwidth`). Full coverage would
# require utf8proc tables. (Issue #3601)
function isletter(c)
    cp = Int(c)
    # ASCII A-Z and a-z
    if cp >= 65 && cp <= 90
        return true
    end
    if cp >= 97 && cp <= 122
        return true
    end
    # Latin-1 Supplement letters: À-Ö (192-214), Ø-ö (216-246), ø-ÿ (248-255)
    # Excluding × (215) and ÷ (247).
    if cp >= 192 && cp <= 214
        return true
    end
    if cp >= 216 && cp <= 246
        return true
    end
    if cp >= 248 && cp <= 255
        return true
    end
    # Latin Extended-A and Latin Extended-B (most are letters)
    if cp >= 256 && cp <= 696
        return true
    end
    # Greek and Coptic letters (excluding punctuation / symbols)
    if cp >= 880 && cp <= 1023
        return true
    end
    # Cyrillic, Cyrillic Supplement
    if cp >= 1024 && cp <= 1279
        return true
    end
    # CJK Unified Ideographs (mirrors the wide-char ranges in textwidth #3598)
    if cp >= 0x3041 && cp <= 0x33FF
        return true
    end
    if cp >= 0x3400 && cp <= 0x4DBF
        return true
    end
    if cp >= 0x4E00 && cp <= 0x9FFF
        return true
    end
    # Hangul Syllables
    if cp >= 0xAC00 && cp <= 0xD7A3
        return true
    end
    # CJK Compatibility Ideographs
    if cp >= 0xF900 && cp <= 0xFAFF
        return true
    end
    return false
end

# isuppercase: check if character is uppercase. Covers ASCII + Latin-1 +
# common Greek/Cyrillic uppercase ranges. (#3601 sister coverage)
function isuppercase(c)
    cp = Int(c)
    # ASCII A-Z
    if cp >= 65 && cp <= 90
        return true
    end
    # Latin-1: À-Ö (192-214), Ø-Þ (216-222)
    if cp >= 192 && cp <= 214
        return true
    end
    if cp >= 216 && cp <= 222
        return true
    end
    # Greek capital letters (Α-Ρ 913-929, Σ-Ω 931-937; 930 is gap)
    if cp >= 913 && cp <= 929
        return true
    end
    if cp >= 931 && cp <= 937
        return true
    end
    # Cyrillic capital letters (А-Я: 1040-1071)
    if cp >= 1040 && cp <= 1071
        return true
    end
    return false
end

# islowercase: check if character is lowercase. Covers ASCII + Latin-1 +
# common Greek/Cyrillic lowercase ranges. (#3601 sister coverage)
function islowercase(c)
    cp = Int(c)
    # ASCII a-z
    if cp >= 97 && cp <= 122
        return true
    end
    # Latin-1: ß-ö (223-246), ø-ÿ (248-255)
    if cp >= 223 && cp <= 246
        return true
    end
    if cp >= 248 && cp <= 255
        return true
    end
    # Greek small letters (α-ρ 945-961, σ-ω 963-969; 962 ς is also lowercase)
    if cp >= 945 && cp <= 969
        return true
    end
    # Cyrillic small letters (а-я: 1072-1103)
    if cp >= 1072 && cp <= 1103
        return true
    end
    return false
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

# ispunct: check if character is punctuation (Issue #10321)
# =============================================================================
# Upstream: `ispunct(c)` is true iff `c`'s Unicode general category begins with
# 'P' — Pc/Pd/Ps/Pe/Pi/Pf/Po (`UTF8PROC_CATEGORY_PC <= category_code(c) <=
# UTF8PROC_CATEGORY_PO`, julia/base/strings/unicode.jl). This is NOT the C-locale
# ASCII table: the ASCII symbol chars `$ + < = > ^ ` | ~` are Unicode Sc/Sm/Sk
# and must return false, while non-ASCII punctuation (`¡ ¿ « » § ¶ · – — …` …)
# must return true. sjulia has no utf8proc binding, so we embed the P* codepoint
# ranges as a sorted, non-overlapping table generated from upstream julia's own
# utf8proc (the gold standard) and binary-search it, mirroring `isnumeric`.
#
# Codepoints are written in decimal (not hex) to avoid Issue #7953
# (`Int[0x30, ...]` fails to convert UInt hex elements to Int in sjulia).
#
# To regenerate after a Unicode version bump, run under the reference julia:
#   function gen()
#       r = Tuple{Int,Int}[]; s = -1; p = -1
#       for cp in 0x0:0x10FFFF
#           (0xD800 <= cp <= 0xDFFF) && continue
#           if ispunct(Char(cp))
#               s == -1 ? (s = cp; p = cp) : (cp == p+1 ? (p = cp) : (push!(r,(s,p)); s = cp; p = cp))
#           end
#       end
#       s != -1 && push!(r,(s,p)); return r
#   end
const _ISPUNCT_RANGE_LO = Int[33, 37, 44, 58, 63, 91, 95, 123, 125, 161, 167, 171,
    182, 187, 191, 894, 903, 1370, 1417, 1470, 1472, 1475, 1478, 1523,
    1545, 1548, 1563, 1565, 1642, 1748, 1792, 2039, 2096, 2142, 2404, 2416,
    2557, 2678, 2800, 3191, 3204, 3572, 3663, 3674, 3844, 3860, 3898, 3973,
    4048, 4057, 4170, 4347, 4960, 5120, 5742, 5787, 5867, 5941, 6100, 6104,
    6144, 6468, 6686, 6816, 6824, 6990, 7002, 7037, 7164, 7227, 7294, 7360,
    7379, 8208, 8240, 8261, 8275, 8317, 8333, 8968, 9001, 10088, 10181, 10214,
    10627, 10712, 10748, 11513, 11518, 11632, 11776, 11824, 11858, 12289, 12296, 12308,
    12336, 12349, 12448, 12539, 42238, 42509, 42611, 42622, 42738, 43124, 43214, 43256,
    43260, 43310, 43359, 43457, 43486, 43612, 43742, 43760, 44011, 64830, 65040, 65072,
    65108, 65123, 65128, 65130, 65281, 65285, 65292, 65306, 65311, 65339, 65343, 65371,
    65373, 65375, 65792, 66463, 66512, 66927, 67671, 67871, 67903, 68176, 68223, 68336,
    68409, 68505, 68974, 69293, 69461, 69510, 69703, 69819, 69822, 69952, 70004, 70085,
    70093, 70107, 70109, 70200, 70313, 70612, 70615, 70731, 70746, 70749, 70854, 71105,
    71233, 71264, 71353, 71484, 71739, 72004, 72162, 72255, 72346, 72350, 72448, 72673,
    72769, 72816, 73463, 73539, 73727, 74864, 77809, 92782, 92917, 92983, 92996, 93549,
    93847, 94178, 113823, 121479, 124415, 125278]

const _ISPUNCT_RANGE_HI = Int[35, 42, 47, 59, 64, 93, 95, 123, 125, 161, 167, 171,
    183, 187, 191, 894, 903, 1375, 1418, 1470, 1472, 1475, 1478, 1524,
    1546, 1549, 1563, 1567, 1645, 1748, 1805, 2041, 2110, 2142, 2405, 2416,
    2557, 2678, 2800, 3191, 3204, 3572, 3663, 3675, 3858, 3860, 3901, 3973,
    4052, 4058, 4175, 4347, 4968, 5120, 5742, 5788, 5869, 5942, 6102, 6106,
    6154, 6469, 6687, 6822, 6829, 6991, 7008, 7039, 7167, 7231, 7295, 7367,
    7379, 8231, 8259, 8273, 8286, 8318, 8334, 8971, 9002, 10101, 10182, 10223,
    10648, 10715, 10749, 11516, 11519, 11632, 11822, 11855, 11869, 12291, 12305, 12319,
    12336, 12349, 12448, 12539, 42239, 42511, 42611, 42622, 42743, 43127, 43215, 43258,
    43260, 43311, 43359, 43469, 43487, 43615, 43743, 43761, 44011, 64831, 65049, 65106,
    65121, 65123, 65128, 65131, 65283, 65290, 65295, 65307, 65312, 65341, 65343, 65371,
    65373, 65381, 65794, 66463, 66512, 66927, 67671, 67871, 67903, 68184, 68223, 68342,
    68415, 68508, 68974, 69293, 69465, 69513, 69709, 69820, 69825, 69955, 70005, 70088,
    70093, 70107, 70111, 70205, 70313, 70613, 70616, 70735, 70747, 70749, 70854, 71127,
    71235, 71276, 71353, 71486, 71739, 72006, 72162, 72262, 72348, 72354, 72457, 72673,
    72773, 72817, 73464, 73551, 73727, 74868, 77810, 92783, 92917, 92987, 92996, 93551,
    93850, 94178, 113823, 121483, 124415, 125279]

function ispunct(c)
    cp = Int(c)
    # Binary search for the last range whose low bound is <= cp.
    lo = 1
    hi = length(_ISPUNCT_RANGE_LO)
    idx = 0
    while lo <= hi
        mid = (lo + hi) >>> 1
        if _ISPUNCT_RANGE_LO[mid] <= cp
            idx = mid
            lo = mid + 1
        else
            hi = mid - 1
        end
    end
    return idx >= 1 && cp <= _ISPUNCT_RANGE_HI[idx]
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
    # Unicode codepoint as UInt32 (upstream returns UInt32). Char is the UInt32
    # codepoint in the subset; UInt32(c) is unsupported so go via Int64. (#6747)
    return UInt32(Int64(c))
end

Char(x::Integer) = _int_to_char(x)
Int(c::Char) = _char_to_int(c)

# Non-Int64 integer constructors from a Char/AbstractChar (Issue #11406).
# Upstream `julia/base/char.jl`:
#   (::Type{T})(x::AbstractChar) where {T<:Union{Number,AbstractChar}} = T(codepoint(x))
# `T(codepoint(x))` is a Number->Number conversion, so upstream's own
# range-check (InexactError for out-of-range codepoints, e.g. UInt8('あ'))
# falls out of the existing numeric constructors without any extra logic
# here. Int64(::Char) already has a Rust boundary special case (`_char_to_int`
# via `Int`), so it is intentionally left alone; every other fixed-width
# integer constructor was missing this method entirely and fell through to a
# generic `convert` MethodError.
Int8(c::AbstractChar) = Int8(codepoint(c))
Int16(c::AbstractChar) = Int16(codepoint(c))
Int32(c::AbstractChar) = Int32(codepoint(c))
Int128(c::AbstractChar) = Int128(codepoint(c))
UInt8(c::AbstractChar) = UInt8(codepoint(c))
UInt16(c::AbstractChar) = UInt16(codepoint(c))
UInt32(c::AbstractChar) = UInt32(codepoint(c))
UInt64(c::AbstractChar) = UInt64(codepoint(c))
UInt128(c::AbstractChar) = UInt128(codepoint(c))

# =============================================================================
# Text width functions
# =============================================================================

# textwidth: get display width of string (for monospace fonts)
# Based on Julia's base/strings/width.jl
#
# Iterates characters (not bytes) and delegates to the per-character
# textwidth, which classifies wide (width-2) characters by a conservative
# subset of the Unicode East Asian Width "Wide" / "Fullwidth" ranges.
# (Issue #3598)
function textwidth(s::String)
    width = 0
    for c in s
        width = width + textwidth(c)
    end
    return width
end

# textwidth for single character — conservative East Asian Width approximation.
# Returns 0 for control characters, 2 for the main CJK / Hangul / Kana / Fullwidth
# ranges, and 1 for everything else (including Latin-1 letters like 'é' that the
# previous all-non-ASCII-is-width-2 version misclassified as 2). (Issue #3598)
function textwidth(c::Char)
    cp = codepoint(c)
    # Control characters (C0/C1) have zero display width
    if cp < 32 || (cp >= 0x7F && cp < 0xA0)
        return 0
    end
    # Wide ranges (East Asian Width = Wide or Fullwidth, conservative subset).
    # Covers Chinese, Japanese (Hiragana/Katakana/Kanji), Korean (Hangul
    # Syllables), CJK punctuation, fullwidth forms, and CJK Extensions A and B.
    # Specific ranges (codepoint inclusive lo .. hi):
    #   0x1100..0x115F   Hangul Jamo init consonants
    #   0x2E80..0x303E   CJK Radicals / Kangxi / CJK Symbols (subset)
    #   0x3041..0x33FF   Hiragana .. Enclosed CJK
    #   0x3400..0x4DBF   CJK Extension A
    #   0x4E00..0x9FFF   CJK Unified Ideographs
    #   0xA000..0xA4CF   Yi Syllables / Yi Radicals
    #   0xAC00..0xD7A3   Hangul Syllables
    #   0xF900..0xFAFF   CJK Compatibility Ideographs
    #   0xFE30..0xFE4F   CJK Compatibility Forms
    #   0xFF00..0xFF60   Fullwidth Forms (excl. halfwidth)
    #   0xFFE0..0xFFE6   Fullwidth Signs
    #   0x20000..0x2FFFD CJK Extensions B–F
    #   0x30000..0x3FFFD CJK Extension G
    if cp >= 0x1100 && cp <= 0x115F
        return 2
    end
    if cp >= 0x2E80 && cp <= 0x303E
        return 2
    end
    if cp >= 0x3041 && cp <= 0x33FF
        return 2
    end
    if cp >= 0x3400 && cp <= 0x4DBF
        return 2
    end
    if cp >= 0x4E00 && cp <= 0x9FFF
        return 2
    end
    if cp >= 0xA000 && cp <= 0xA4CF
        return 2
    end
    if cp >= 0xAC00 && cp <= 0xD7A3
        return 2
    end
    if cp >= 0xF900 && cp <= 0xFAFF
        return 2
    end
    if cp >= 0xFE30 && cp <= 0xFE4F
        return 2
    end
    if cp >= 0xFF00 && cp <= 0xFF60
        return 2
    end
    if cp >= 0xFFE0 && cp <= 0xFFE6
        return 2
    end
    if cp >= 0x20000 && cp <= 0x2FFFD
        return 2
    end
    if cp >= 0x30000 && cp <= 0x3FFFD
        return 2
    end
    return 1
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

# String indexing is byte-based, but the final valid index is the character
# start at or before `ncodeunits(s)`, not necessarily the byte length itself.
# This mirrors upstream `lastindex(::AbstractString)` (Issues #3662/#11624).
function lastindex(s::String)
    return thisind(s, ncodeunits(s))
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

function _utf8_sequence_width(b::UInt8)
    if b < 0xc0
        return 1
    elseif b < 0xe0
        return 2
    elseif b < 0xf0
        return 3
    elseif b < 0xf8
        return 4
    end
    return 1
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
    if !_is_continuation_byte(codeunit(s, i))
        return i
    end
    j = i
    while j > 1 && i - j < 3 && _is_continuation_byte(codeunit(s, j))
        j -= 1
    end
    width = _utf8_sequence_width(codeunit(s, j))
    if width > 1 && i - j < width
        return j
    end
    return i
end

# nextind(s, i) - next valid string index after i
function nextind(s::String, i::Int64)
    if i == 0
        return 1
    end
    n = ncodeunits(s)
    if i < 0 || i > n
        throw(BoundsError(s, i))
    end
    start = thisind(s, i)
    stop = start + _utf8_sequence_width(codeunit(s, start))
    i += 1
    while i <= n && i < stop && _is_continuation_byte(codeunit(s, i))
        i += 1
    end
    return i
end

# prevind(s, i) - previous valid string index before i
function prevind(s::String, i::Int64)
    if i < 1
        throw(BoundsError(s, i))
    end
    if i == 1
        return 0
    end
    n = ncodeunits(s)
    if i > n + 1
        throw(BoundsError(s, i))
    end
    return thisind(s, i - 1)
end

# reverseind(s, i) - index in s corresponding to index i in reverse(s)
# Julia's actual implementation: thisind(s, ncodeunits(s) - i + 1)
reverseind(s::String, i::Int64) = thisind(s, ncodeunits(s) - i + 1)

# isvalid(s, i) — true if `i` is a valid character boundary in `s`.
# Equivalent to Julia's `isvalid(::String, ::Integer)` (Issue #3726).
# Mirrors official Julia semantics:
#   - i < 1 or i > ncodeunits(s) → false
#   - a continuation byte consumed by a preceding character → false
#   - a standalone malformed continuation byte starts its own character → true
function isvalid(s::String, i::Int64)
    n = ncodeunits(s)
    if i < 1 || i > n
        return false
    end
    return thisind(s, i) == i
end

# Generic Integer overload — covers UInt and other integer widths so callers
# such as `isvalid("é", 0x1)` resolve to the Pure Julia method instead of
# falling back to the (now removed) Rust builtin.
function isvalid(s::String, i::Integer)
    return isvalid(s, Int64(i))
end

# One-arg isvalid (Issue #8995): whole-value validity. A String is valid iff
# its bytes are valid UTF-8 (invalid bytes live in the StrBytes carrier); a
# Char is valid iff it is a Unicode scalar (malformed Chars come from
# iterating/indexing invalid byte sequences). `_isvalid_value` is the VM
# intrinsic behind both.
isvalid(s::String) = _isvalid_value(s)
isvalid(c::Char) = _isvalid_value(c)

# =============================================================================
# String bounds checking over ranges (Issue #10958)
# =============================================================================
# Upstream julia/base/strings/basic.jl:209-218. The range form returns nothing
# in bounds and throws a catchable BoundsError otherwise — the shape the
# upstream SubString{T}(s, i, j) inner constructor relies on.
checkbounds(::Type{Bool}, s::AbstractString, i::Integer) = 1 <= i <= ncodeunits(s)
function checkbounds(::Type{Bool}, s::AbstractString, r::AbstractRange{<:Integer})
    return isempty(r) || (1 <= minimum(r) && maximum(r) <= ncodeunits(s))
end
function checkbounds(s::AbstractString, I)
    if checkbounds(Bool, s, I)
        return nothing
    end
    throw(BoundsError(s, I))
end
