# Unicode string functions for SubsetJuliaVM
# Based on Julia's base/strings/unicode.jl
#
# Limitation: Only ASCII case conversion is supported.
# Non-ASCII characters are returned unchanged (utf8proc not available).

# =============================================================================
# uppercase / lowercase for Char
# =============================================================================

function uppercase(c::Char)
    if 'a' <= c <= 'z'
        return Char(Int(c) - 32)
    end
    return c
end

function lowercase(c::Char)
    if 'A' <= c <= 'Z'
        return Char(Int(c) + 32)
    end
    return c
end

# =============================================================================
# uppercase / lowercase for String
# =============================================================================

function uppercase(s::String)
    buf = IOBuffer()
    for c in s
        write(buf, uppercase(c))
    end
    return String(take!(buf))
end

function lowercase(s::String)
    buf = IOBuffer()
    for c in s
        write(buf, lowercase(c))
    end
    return String(take!(buf))
end

# =============================================================================
# titlecase for Char and String
# =============================================================================

# titlecase(c::Char) - for ASCII, same as uppercase
function titlecase(c::Char)
    if 'a' <= c <= 'z'
        return Char(Int(c) - 32)
    end
    return c
end

# titlecase(s::String) - capitalize first letter of each word, lowercase rest
# Word separator: any non-letter character (matching Julia's default wordsep = !isletter)
function titlecase(s::String)
    buf = IOBuffer()
    startword = true
    for c in s
        if !isletter(c)
            write(buf, c)
            startword = true
        else
            if startword
                write(buf, titlecase(c))
            else
                write(buf, lowercase(c))
            end
            startword = false
        end
    end
    return String(take!(buf))
end

# =============================================================================
# isnumeric (Issue #6752)
# =============================================================================
# Upstream: `isnumeric(c)` is true iff `c`'s Unicode general category is Nd, Nl,
# or No (`UTF8PROC_CATEGORY_ND <= category_code(c) <= UTF8PROC_CATEGORY_NO`,
# julia/base/strings/unicode.jl). sjulia has no utf8proc binding, so we embed the
# Nd/Nl/No codepoint ranges as a sorted, non-overlapping table generated from
# upstream julia's own utf8proc (the gold standard). `isnumeric` binary-searches
# it. This replaces the Rust `BuiltinId::Isnumeric` (`char::is_numeric()`) with a
# pure-Julia definition correct for non-ASCII Nd/Nl/No (Arabic-Indic digits,
# vulgar fractions, Roman numerals, circled numbers, ...), not just the ASCII
# approximation isdigit/isletter give.
#
# Codepoints are written in decimal (not hex) to avoid Issue #7953
# (`Int[0x30, ...]` fails to convert UInt hex elements to Int in sjulia).
#
# To regenerate after a Unicode version bump, run under the reference julia:
#   r = Tuple{Int,Int}[]; s = -1; p = -1
#   for cp in 0x0:0x10FFFF
#       (0xD800 <= cp <= 0xDFFF) && continue
#       if isnumeric(Char(cp))
#           s == -1 ? (s = cp; p = cp) : (cp == p+1 ? (p = cp) : (push!(r,(s,p)); s = cp; p = cp))
#       end
#   end
#   s != -1 && push!(r,(s,p))

const _ISNUMERIC_RANGE_LO = Int[48, 178, 185, 188, 1632, 1776, 1984, 2406, 2534, 2548, 2662, 2790, 
    2918, 2930, 3046, 3174, 3192, 3302, 3416, 3430, 3558, 3664, 3792, 3872, 
    4160, 4240, 4969, 5870, 6112, 6128, 6160, 6470, 6608, 6784, 6800, 6992, 
    7088, 7232, 7248, 8304, 8308, 8320, 8528, 8581, 9312, 9450, 10102, 11517, 
    12295, 12321, 12344, 12690, 12832, 12872, 12881, 12928, 12977, 42528, 42726, 43056, 
    43216, 43264, 43472, 43504, 43600, 44016, 65296, 65799, 65856, 65930, 66273, 66336, 
    66369, 66378, 66513, 66720, 67672, 67705, 67751, 67835, 67862, 68028, 68032, 68050, 
    68160, 68221, 68253, 68331, 68440, 68472, 68521, 68858, 68912, 68928, 69216, 69405, 
    69457, 69573, 69714, 69872, 69942, 70096, 70113, 70384, 70736, 70864, 71248, 71360, 
    71376, 71472, 71904, 72016, 72688, 72784, 73040, 73120, 73552, 73664, 74752, 90416, 
    92768, 92864, 93008, 93019, 93552, 93824, 118000, 119488, 119520, 119648, 120782, 123200, 
    123632, 124144, 124401, 125127, 125264, 126065, 126125, 126129, 126209, 126255, 127232, 130032]

const _ISNUMERIC_RANGE_HI = Int[57, 179, 185, 190, 1641, 1785, 1993, 2415, 2543, 2553, 2671, 2799, 
    2927, 2935, 3058, 3183, 3198, 3311, 3422, 3448, 3567, 3673, 3801, 3891, 
    4169, 4249, 4988, 5872, 6121, 6137, 6169, 6479, 6618, 6793, 6809, 7001, 
    7097, 7241, 7257, 8304, 8313, 8329, 8578, 8585, 9371, 9471, 10131, 11517, 
    12295, 12329, 12346, 12693, 12841, 12879, 12895, 12937, 12991, 42537, 42735, 43061, 
    43225, 43273, 43481, 43513, 43609, 44025, 65305, 65843, 65912, 65931, 66299, 66339, 
    66369, 66378, 66517, 66729, 67679, 67711, 67759, 67839, 67867, 68029, 68047, 68095, 
    68168, 68222, 68255, 68335, 68447, 68479, 68527, 68863, 68921, 68937, 69246, 69414, 
    69460, 69579, 69743, 69881, 69951, 70105, 70132, 70393, 70745, 70873, 71257, 71369, 
    71395, 71483, 71922, 72025, 72697, 72812, 73049, 73129, 73561, 73684, 74862, 90425, 
    92777, 92873, 93017, 93025, 93561, 93846, 118009, 119507, 119539, 119672, 120831, 123209, 
    123641, 124153, 124410, 125135, 125273, 126123, 126127, 126132, 126253, 126269, 127244, 130041]

function isnumeric(c::Char)
    cp = Int(c)
    # Binary search for the last range whose low bound is <= cp.
    lo = 1
    hi = length(_ISNUMERIC_RANGE_LO)
    idx = 0
    while lo <= hi
        mid = (lo + hi) >>> 1
        if _ISNUMERIC_RANGE_LO[mid] <= cp
            idx = mid
            lo = mid + 1
        else
            hi = mid - 1
        end
    end
    return idx >= 1 && cp <= _ISNUMERIC_RANGE_HI[idx]
end
