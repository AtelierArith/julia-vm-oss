# =============================================================================
# printf.jl - Printf macros
# =============================================================================
# Based on Julia's stdlib/Printf/src/Printf.jl
#
# These macros delegate to the sprintf builtin function.
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
