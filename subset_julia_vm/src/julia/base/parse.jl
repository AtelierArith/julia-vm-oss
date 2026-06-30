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
        # NOTE (Issue #7942): do NOT skip underscores. `_` is a digit separator
        # only in numeric *literals* in source code, not in `parse`/`tryparse`
        # string input. Upstream `parse(Int, "1_000")` throws ArgumentError and
        # `tryparse` returns `nothing`. `_` falls through to `_digit_value`,
        # which returns `nothing` for it, so the string is rejected.
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

# parse(Int, s; base=N) public path (Issue #7875 / docs/COMPARISION.md P1):
# the kwargs form is routed by the compiler (compile_parse_tryparse) to this
# positional pure-Julia helper instead of the former Rust `StringToIntBase`
# builtin. The base-parsing domain logic already lived in pure Julia
# (`_tryparse_int`, which handles sign, whitespace, underscore separators and
# any base); this wrapper only adds upstream's ArgumentError on failure,
# mirroring the base-10 `parse(::Type{Int64}, s)` above.
function _parse_int_base(s::String, base::Int64)
    result = _tryparse_int(s, base)
    if result === nothing
        throw(ArgumentError("invalid base $base digit in \"$s\""))
    end
    return result
end

# Float64 parsing (Issue #6748): the public parse/tryparse wrappers are pure
# Julia; the actual conversion stays in the Rust intrinsic `_tryparse_float64`
# (libc strtod). parse() adds upstream's ArgumentError on failure (the former
# Rust `parse` handler threw a generic error).
function tryparse(::Type{Float64}, s::String)
    return _tryparse_float64(s)
end

function parse(::Type{Float64}, s::String)
    result = _tryparse_float64(s)
    if result === nothing
        throw(ArgumentError("cannot parse \"$s\" as Float64"))
    end
    return result
end

# parse/tryparse(::Type{Bool}, s) (Issue #5766). Accepts "true"/"false"
# (whitespace-stripped) or an integer 0/1; anything else is invalid.
function tryparse(::Type{Bool}, s::String)
    t = strip(s)
    if t == "true"
        return true
    elseif t == "false"
        return false
    end
    n = tryparse(Int64, String(t))
    if n === nothing
        return nothing
    elseif n == 0
        return false
    elseif n == 1
        return true
    else
        return nothing
    end
end

function parse(::Type{Bool}, s::String)
    result = tryparse(Bool, s)
    if result === nothing
        throw(ArgumentError("invalid Bool representation: \"$s\""))
    end
    return result
end
