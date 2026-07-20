# Issue #9291: a for-head range whose *step* infers as `Any` must take the
# generic typed-range path, not the I64 fast path (which `DynamicToI64`-truncates
# the runtime float step to 0 and iterates zero times). PR #9287 made `/` follow
# its operands (`has_any -> Any`) instead of blanket-inferring Float64, so a
# computed float step can now infer `Any`. Passing the step as an *unannotated
# function parameter* forces `Any` deterministically, independent of how a
# particular literal expression happens to infer — a robust complement to the
# inline `0:(2π/12):2π` case in for_head_nonliteral_float_step_7800.jl.

# Any-typed float step via a function parameter -> 13 iterations (like upstream).
f(st) = (n = 0; for u in 0:st:2π; n += 1; end; n)

# Any-typed integer step must still count correctly (diverting to the generic
# path is semantically correct for integers too, just not the hot path).
g(st) = (n = 0; for u in 1:st:10; n += 1; end; n)

# The loop variable carries the real float value, not a truncated Int.
sumf(st) = (s = 0.0; for u in 0:st:2π; s += u; end; s)

# A plain integer stepless loop must stay on the fast path and count correctly
# (guards against over-diverting `for i in 1:n`).
h(n) = (c = 0; for i in 1:n; c += 1; end; c)

f(2π / 12) == 13 &&
    g(2) == 5 &&
    abs(sumf(2π / 12) - 40.84070449666731) < 1.0e-9 &&
    h(10) == 10 &&
    f(0.5) + g(1) == 23
