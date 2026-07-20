# Per-element struct-returning driver safepoint regression (Issue #10102 follow-up).
#
# The memory-waterline safepoint was moved off the per-instruction dispatch path
# onto loop back-edges + Call/Return boundaries. The scalar `Return*` arms
# already carried the `handle_pending_call_depth_overflow` safepoint postlude,
# but the container returns (ReturnStruct/ReturnTuple/ReturnNamedTuple/
# ReturnDict/ReturnSet) did NOT. A per-element HOF/broadcast/generator driver
# whose callee returns a struct enters the callee without the `Call` postlude
# and returns via `ReturnStruct` (not a back-edge), so those container returns
# must also carry the postlude to keep struct-heap growth bracketed by a
# safepoint. This fixture drives struct construction per element through a
# broadcast (ReturnStruct callee) and a comprehension, plus tuple/named-tuple/
# dict returns, and checks the results are upstream-correct.

struct Pt
    x::Int
    y::Int
end

# f returns a struct -> ReturnStruct per broadcast element
f(i) = Pt(i, i * i)

# Broadcast of a struct constructor: array of structs, per-element ReturnStruct.
let n = 20000
    ps = f.(1:n)
    sx = sum(p.x for p in ps)
    sy = sum(p.y for p in ps)
    println(sx == div(n * (n + 1), 2))
    println(sy == div(n * (n + 1) * (2 * n + 1), 6))
    println(length(ps) == n)
end

# Comprehension of structs: per-element struct construction through the
# generator-collect driver.
let n = 20000
    ps = [Pt(i, 2 * i) for i in 1:n]
    total = sum(p.x + p.y for p in ps)
    println(total == 3 * div(n * (n + 1), 2))
end

# Tuple-returning callee per element (ReturnTuple postlude).
g(i) = (i, i + 1)
let n = 10000
    ts = g.(1:n)
    s = sum(first(t) + last(t) for t in ts)
    # sum_{i=1}^{n} (i + i+1) = 2*sum(i) + n
    println(s == 2 * div(n * (n + 1), 2) + n)
end

# Named-tuple-returning callee per element (ReturnNamedTuple postlude), driven
# through a comprehension. (Broadcasting a NamedTuple-returning function, i.e.
# `h.(1:n)`, is a separate sjulia gap tracked by Issue #10469 and deliberately
# avoided here.)
h(i) = (a = i, b = 3 * i)
let n = 10000
    nts = [h(i) for i in 1:n]
    s = sum(nt.a + nt.b for nt in nts)
    println(s == 4 * div(n * (n + 1), 2))
end

# A function that RETURNS a Dict (ReturnDict postlude), called in a loop.
mkd(i) = Dict(:v => i)
let n = 5000
    s = 0
    for i in 1:n
        d = mkd(i)
        s += d[:v]
    end
    println(s == div(n * (n + 1), 2))
end

true
