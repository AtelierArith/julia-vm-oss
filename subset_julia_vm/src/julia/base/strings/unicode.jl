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
