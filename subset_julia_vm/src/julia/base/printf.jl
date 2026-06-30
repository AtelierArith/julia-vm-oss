# =============================================================================
# printf.jl - Pure-Julia C-style Printf engine + macros (Issue #6746)
# =============================================================================
# Based on Julia's stdlib/Printf/src/Printf.jl
#
# `sprintf(fmt, args...)` is a pure-Julia C-style format engine: it parses the
# format string (flags / width / .precision / conversion) and lays out integers,
# strings and chars itself. The float conversions (%f %e %E %g %G) delegate to
# the Rust `_printf_fmt_float` boundary (the Ryu/Grisu float→string entry point).
# The @printf / @sprintf macros expand to sprintf.

# Unsigned magnitude of an integer in the given base (no sign).
function _printf_digits(m::Integer, base::Integer, upper::Bool)
    m == 0 && return "0"
    chars = upper ? "0123456789ABCDEF" : "0123456789abcdef"
    s = ""
    while m > 0
        d = Int(m % base)
        s = string(chars[d + 1]) * s
        m = m ÷ base
    end
    return s
end

# Pad `prefix * body` to `width`, honoring left-justify and zero-fill (zeros go
# between the prefix/sign and the body, spaces go outside).
function _printf_pad(prefix::AbstractString, body::AbstractString, width::Int, left::Bool, zero::Bool)
    total = length(prefix) + length(body)
    total >= width && return prefix * body
    padn = width - total
    if left
        return prefix * body * repeat(" ", padn)
    elseif zero
        return prefix * repeat("0", padn) * body
    else
        return repeat(" ", padn) * prefix * body
    end
end

# First `n` characters of `s` (for %s precision).
function _printf_take(s::AbstractString, n::Int)
    n <= 0 && return ""
    out = ""
    c = 0
    for ch in s
        out = out * string(ch)
        c += 1
        c >= n && break
    end
    return out
end

# Format one conversion field.
function _printf_one(minus::Bool, plus::Bool, space::Bool, zeroflag::Bool, alt::Bool,
                     width::Int, precision::Int, hasprec::Bool, conv::Char, arg)
    zero = zeroflag && !minus
    if conv == 'd' || conv == 'i' || conv == 'u'
        n = Int64(arg)
        neg = n < 0
        body = _printf_digits(neg ? -n : n, 10, false)
        if hasprec
            zero = false
            if precision == 0 && n == 0
                body = ""
            elseif length(body) < precision
                body = repeat("0", precision - length(body)) * body
            end
        end
        sign = neg ? "-" : (plus ? "+" : (space ? " " : ""))
        return _printf_pad(sign, body, width, minus, zero)
    elseif conv == 'x' || conv == 'X'
        upper = conv == 'X'
        n = Int64(arg)
        body = _printf_digits(n < 0 ? -n : n, 16, upper)
        prefix = (alt && n != 0) ? (upper ? "0X" : "0x") : ""
        if hasprec
            zero = false
            if length(body) < precision
                body = repeat("0", precision - length(body)) * body
            end
        end
        return _printf_pad(prefix, body, width, minus, zero)
    elseif conv == 'o'
        n = Int64(arg)
        body = _printf_digits(n < 0 ? -n : n, 8, false)
        if alt && (length(body) == 0 || body[1] != '0')
            body = "0" * body
        end
        return _printf_pad("", body, width, minus, zero)
    elseif conv == 'f' || conv == 'F' || conv == 'e' || conv == 'E' || conv == 'g' || conv == 'G'
        x = Float64(arg)
        neg = signbit(x)
        s = _printf_fmt_float(neg ? -x : x, conv, hasprec ? precision : -1)
        sign = neg ? "-" : (plus ? "+" : (space ? " " : ""))
        return _printf_pad(sign, s, width, minus, zero)
    elseif conv == 's'
        s = string(arg)
        if hasprec
            s = _printf_take(s, precision)
        end
        return _printf_pad("", s, width, minus, false)
    elseif conv == 'c'
        s = isa(arg, Char) ? string(arg) : string(Char(Int(arg)))
        return _printf_pad("", s, width, minus, false)
    else
        return string(arg)
    end
end

# sprintf(fmt, args...): pure-Julia C-style formatting.
function sprintf(fmt::AbstractString, args...)
    cs = collect(fmt)
    n = length(cs)
    result = ""
    i = 1
    argi = 1
    while i <= n
        c = cs[i]
        if c != '%'
            result = result * string(c)
            i += 1
            continue
        end
        i += 1
        if i > n
            result = result * "%"
            break
        end
        if cs[i] == '%'
            result = result * "%"
            i += 1
            continue
        end
        # flags
        minus = false
        plus = false
        space = false
        zeroflag = false
        alt = false
        while i <= n && (cs[i] == '-' || cs[i] == '+' || cs[i] == ' ' || cs[i] == '0' || cs[i] == '#')
            f = cs[i]
            if f == '-'
                minus = true
            elseif f == '+'
                plus = true
            elseif f == ' '
                space = true
            elseif f == '0'
                zeroflag = true
            else
                alt = true
            end
            i += 1
        end
        # width
        width = 0
        while i <= n && cs[i] >= '0' && cs[i] <= '9'
            width = width * 10 + (Int(cs[i]) - Int('0'))
            i += 1
        end
        # precision
        precision = 0
        hasprec = false
        if i <= n && cs[i] == '.'
            hasprec = true
            i += 1
            while i <= n && cs[i] >= '0' && cs[i] <= '9'
                precision = precision * 10 + (Int(cs[i]) - Int('0'))
                i += 1
            end
        end
        if i > n
            break
        end
        conv = cs[i]
        i += 1
        arg = args[argi]
        argi += 1
        result = result * _printf_one(minus, plus, space, zeroflag, alt, width, precision, hasprec, conv, arg)
    end
    return result
end

# These macros delegate to the sprintf function.
# Due to macro system limitations, we define fixed-arity versions.

# =============================================================================
# @sprintf - Format string (returns String)
# =============================================================================

macro sprintf(fmt)
    quote
        sprintf($(esc(fmt)))
    end
end

macro sprintf(fmt, a1)
    quote
        sprintf($(esc(fmt)), $(esc(a1)))
    end
end

macro sprintf(fmt, a1, a2)
    quote
        sprintf($(esc(fmt)), $(esc(a1)), $(esc(a2)))
    end
end

macro sprintf(fmt, a1, a2, a3)
    quote
        sprintf($(esc(fmt)), $(esc(a1)), $(esc(a2)), $(esc(a3)))
    end
end

macro sprintf(fmt, a1, a2, a3, a4)
    quote
        sprintf($(esc(fmt)), $(esc(a1)), $(esc(a2)), $(esc(a3)), $(esc(a4)))
    end
end

# =============================================================================
# @printf - Format and print string (returns nothing)
# =============================================================================

macro printf(fmt)
    quote
        print(sprintf($(esc(fmt))))
    end
end

macro printf(fmt, a1)
    quote
        print(sprintf($(esc(fmt)), $(esc(a1))))
    end
end

macro printf(fmt, a1, a2)
    quote
        print(sprintf($(esc(fmt)), $(esc(a1)), $(esc(a2))))
    end
end

macro printf(fmt, a1, a2, a3)
    quote
        print(sprintf($(esc(fmt)), $(esc(a1)), $(esc(a2)), $(esc(a3))))
    end
end

macro printf(fmt, a1, a2, a3, a4)
    quote
        print(sprintf($(esc(fmt)), $(esc(a1)), $(esc(a2)), $(esc(a3)), $(esc(a4))))
    end
end
