# =============================================================================
# parse.jl - String to number parsing (Int64)
# =============================================================================
# Based on Julia's base/parse.jl
# Float64 parsing remains as Rust intrinsic (uses libc strtod internally).

# Helper: convert character to digit value for a given base
function _digit_value(c::Char, base::Int64)
    if '0' <= c <= '9'
        d = Int(c) - Int('0')
    elseif 'a' <= c <= 'z'
        d = Int(c) - Int('a') + 10
    elseif 'A' <= c <= 'Z'
        d = Int(c) - Int('A') + 10
    else
        return nothing
    end
    if d >= base
        return nothing
    end
    return d
end

# Internal implementation: tryparse with explicit base argument
function _tryparse_int(s::String, base::Int64)
    n = ncodeunits(s)
    i = 1

    # Skip leading whitespace
    while i <= n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r')
        i = i + 1
    end

    if i > n
        return nothing
    end

    # Parse sign
    sign = 1
    if s[i] == '-'
        sign = -1
        i = i + 1
    elseif s[i] == '+'
        i = i + 1
    end

    # Skip whitespace after sign (matches Rust's trim behavior)
    while i <= n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r')
        i = i + 1
    end

    if i > n
        return nothing
    end

    # Parse digits
    result = Int64(0)
    has_digit = false
    while i <= n
        c = s[i]
        # Skip underscores (Julia allows _ as digit separator)
        if c == '_'
            i = i + 1
            continue
        end
        d = _digit_value(c, base)
        if d === nothing
            # Allow trailing whitespace
            while i <= n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r')
                i = i + 1
            end
            if i <= n
                return nothing
            end
            break
        end
        result = result * base + d
        has_digit = true
        i = i + 1
    end

    if !has_digit
        return nothing
    end

    return sign * result
end

# tryparse(::Type{Int64}, s::String) — base-10 default
function tryparse(::Type{Int64}, s::String)
    return _tryparse_int(s, 10)
end

# parse(::Type{Int64}, s::String) — base-10 default
function parse(::Type{Int64}, s::String)
    result = _tryparse_int(s, 10)
    if result === nothing
        throw(ArgumentError("invalid base 10 digit in \"$s\""))
    end
    return result
end
