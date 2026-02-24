# This file is a part of Julia. License is MIT: https://julialang.org/license

# =============================================================================
# Base64 - Base64 encoding and decoding (Issue #1846)
# =============================================================================
# Based on Julia's stdlib/Base64
#
# Simplified implementation using pure Julia string/byte operations
# without IO pipe dependencies.
#
# Note: Uses div() instead of >> and multiplication instead of <<,
# rem() instead of & (bitwise AND), and + instead of | (bitwise OR)
# because SubsetJuliaVM does not yet support bitwise/shift operators
# in Julia source code.
#
# Supported functions:
#   base64encode(data)  - encode string or byte array to base64 string
#   base64decode(s)     - decode base64 string to Vector{UInt8}

module Base64

export base64encode, base64decode

# _b64_encode_char: convert 6-bit value (0-63) to base64 character
function _b64_encode_char(v::Int64)
    if v < 26
        return Char(v + 65)         # 'A' = 65
    elseif v < 52
        return Char(v - 26 + 97)    # 'a' = 97
    elseif v < 62
        return Char(v - 52 + 48)    # '0' = 48
    elseif v == 62
        return '+'
    else
        return '/'
    end
end

# _b64_decode_char: convert base64 character to 6-bit value (0-63)
# Returns -1 for padding '=' and -2 for invalid characters
function _b64_decode_char(c::Int64)
    if c >= 65 && c <= 90       # 'A'-'Z'
        return c - 65
    elseif c >= 97 && c <= 122  # 'a'-'z'
        return c - 97 + 26
    elseif c >= 48 && c <= 57   # '0'-'9'
        return c - 48 + 52
    elseif c == 43              # '+'
        return 62
    elseif c == 47              # '/'
        return 63
    elseif c == 61              # '='
        return -1
    else
        return -2
    end
end

# base64encode(s::String) - encode a string to base64
# Each group of 3 input bytes is encoded as 4 base64 characters.
# Padding with '=' is added if input length is not a multiple of 3.
function base64encode(s::String)
    n = ncodeunits(s)
    result = ""
    i = 1
    while i + 2 <= n
        # Process 3 bytes at a time
        b1 = Int64(codeunit(s, i))
        b2 = Int64(codeunit(s, i + 1))
        b3 = Int64(codeunit(s, i + 2))

        # Split into 4 groups of 6 bits:
        # c1 = b1 >> 2           (top 6 bits of b1)
        # c2 = (b1 & 3) << 4 | (b2 >> 4)  (bottom 2 of b1 + top 4 of b2)
        # c3 = (b2 & 15) << 2 | (b3 >> 6) (bottom 4 of b2 + top 2 of b3)
        # c4 = b3 & 63          (bottom 6 bits of b3)
        c1 = div(b1, 4)
        c2 = rem(b1, 4) * 16 + div(b2, 16)
        c3 = rem(b2, 16) * 4 + div(b3, 64)
        c4 = rem(b3, 64)

        result = result * string(_b64_encode_char(c1))
        result = result * string(_b64_encode_char(c2))
        result = result * string(_b64_encode_char(c3))
        result = result * string(_b64_encode_char(c4))

        i = i + 3
    end

    # Handle remaining bytes (1 or 2)
    remaining = n - i + 1
    if remaining == 2
        b1 = Int64(codeunit(s, i))
        b2 = Int64(codeunit(s, i + 1))

        c1 = div(b1, 4)
        c2 = rem(b1, 4) * 16 + div(b2, 16)
        c3 = rem(b2, 16) * 4

        result = result * string(_b64_encode_char(c1))
        result = result * string(_b64_encode_char(c2))
        result = result * string(_b64_encode_char(c3))
        result = result * "="
    elseif remaining == 1
        b1 = Int64(codeunit(s, i))

        c1 = div(b1, 4)
        c2 = rem(b1, 4) * 16

        result = result * string(_b64_encode_char(c1))
        result = result * string(_b64_encode_char(c2))
        result = result * "=="
    end

    return result
end

# base64encode(data::Array) - encode byte array to base64
function base64encode(data::Array)
    n = length(data)
    result = ""
    i = 1
    while i + 2 <= n
        b1 = Int64(data[i])
        b2 = Int64(data[i + 1])
        b3 = Int64(data[i + 2])

        c1 = div(b1, 4)
        c2 = rem(b1, 4) * 16 + div(b2, 16)
        c3 = rem(b2, 16) * 4 + div(b3, 64)
        c4 = rem(b3, 64)

        result = result * string(_b64_encode_char(c1))
        result = result * string(_b64_encode_char(c2))
        result = result * string(_b64_encode_char(c3))
        result = result * string(_b64_encode_char(c4))

        i = i + 3
    end

    remaining = n - i + 1
    if remaining == 2
        b1 = Int64(data[i])
        b2 = Int64(data[i + 1])

        c1 = div(b1, 4)
        c2 = rem(b1, 4) * 16 + div(b2, 16)
        c3 = rem(b2, 16) * 4

        result = result * string(_b64_encode_char(c1))
        result = result * string(_b64_encode_char(c2))
        result = result * string(_b64_encode_char(c3))
        result = result * "="
    elseif remaining == 1
        b1 = Int64(data[i])

        c1 = div(b1, 4)
        c2 = rem(b1, 4) * 16

        result = result * string(_b64_encode_char(c1))
        result = result * string(_b64_encode_char(c2))
        result = result * "=="
    end

    return result
end

# base64decode(s::String) - decode base64 string to Vector{UInt8}
function base64decode(s::String)
    n = ncodeunits(s)
    result = UInt8[]

    # Collect valid base64 values
    vals = Int64[]
    i = 1
    while i <= n
        c = Int64(codeunit(s, i))
        v = _b64_decode_char(c)
        if v >= 0
            push!(vals, v)
        end
        # Skip padding '=' (v == -1) and invalid chars (v == -2)
        i = i + 1
    end

    # Process groups of 4 base64 values -> 3 bytes
    nvals = length(vals)
    j = 1
    while j + 3 <= nvals
        v1 = vals[j]
        v2 = vals[j + 1]
        v3 = vals[j + 2]
        v4 = vals[j + 3]

        # byte1 = (v1 << 2) | (v2 >> 4) = v1 * 4 + div(v2, 16)
        # byte2 = ((v2 & 15) << 4) | (v3 >> 2) = rem(v2, 16) * 16 + div(v3, 4)
        # byte3 = ((v3 & 3) << 6) | v4 = rem(v3, 4) * 64 + v4
        push!(result, UInt8(v1 * 4 + div(v2, 16)))
        push!(result, UInt8(rem(v2, 16) * 16 + div(v3, 4)))
        push!(result, UInt8(rem(v3, 4) * 64 + v4))

        j = j + 4
    end

    # Handle remaining values with padding
    remaining_vals = nvals - j + 1
    if remaining_vals == 3
        # 2 output bytes (1 padding '=')
        v1 = vals[j]
        v2 = vals[j + 1]
        v3 = vals[j + 2]
        push!(result, UInt8(v1 * 4 + div(v2, 16)))
        push!(result, UInt8(rem(v2, 16) * 16 + div(v3, 4)))
    elseif remaining_vals == 2
        # 1 output byte (2 padding '==')
        v1 = vals[j]
        v2 = vals[j + 1]
        push!(result, UInt8(v1 * 4 + div(v2, 16)))
    end

    return result
end

end # module Base64
