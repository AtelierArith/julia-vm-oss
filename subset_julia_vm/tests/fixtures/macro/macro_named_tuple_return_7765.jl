# Issue #7765: a user-defined macro that returns a named-tuple expression
# (Expr(:tuple, Expr(:(=), :name, value), ...)) must lower through the runtime
# macro engine to a NamedTuple, not a plain Tuple, so field access works.
# Before the fix, `t.value` failed with "Field access requires a struct type,
# got Tuple".
macro mytimed(ex)
    quote
        local result = $(esc(ex))
        (value=result, time=0.0)
    end
end

t = @mytimed 1 + 2
@assert t.value == 3
@assert t.time == 0.0

# named tuple with a function-call argument
add(a, b) = a + b
t2 = @mytimed add(10, 20)
@assert t2.value == 30

# a plain (unnamed) tuple returned by a macro must still lower to a Tuple
macro mypair(a, b)
    quote
        ($(esc(a)), $(esc(b)))
    end
end
p = @mypair 4 5
@assert p == (4, 5)
@assert p[1] == 4

true
