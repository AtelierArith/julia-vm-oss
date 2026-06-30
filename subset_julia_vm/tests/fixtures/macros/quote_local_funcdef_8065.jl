# Issue #8065: a `local f(x) = ...` short-form function definition (and its
# `where` variant) inside a macro's `quote` lowers as a real method definition.
# Previously the parser stopped after the bare name and mis-parsed the
# `(...) = body` remainder, erroring "macro expansion returned unsupported
# assignment expression target" / "unsupported function signature Expr".

macro addone_local()
    quote
        local h(x::Int) = x + 1
        h(41)
    end
end
check_local = (@addone_local() == 42)

# `where` variant with an interpolated parameter type and a free type var.
macro addwhere_local(T)
    quote
        local k(x::$T, y::S) where {S} = x + y
        k(7, 3)
    end
end
check_where = (@addwhere_local(Int) == 10)

# The same `local f(x) = ...` short form also lowers in an ordinary function body.
function uses_local_short()
    local g(x::Int) = x * 3
    g(5)
end
check_outside = (uses_local_short() == 15)

check_local && check_where && check_outside
