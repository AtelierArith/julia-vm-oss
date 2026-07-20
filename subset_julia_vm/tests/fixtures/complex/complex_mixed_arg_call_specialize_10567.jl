# Issue #10567: the runtime specializer's frame-less fast path for
# `CallSpecialize`/`CallSpecializeInbounds` call sites whose argument list is
# genuinely mixed-type (e.g. a boxed `ComplexF64` value plus an `Int64`
# counter, as at `mandel_point(c, maxiter)`'s call site in
# `benchmarks/mandelbrot_bench_for_untyped.jl`). Exercises:
#   - the escape-count recurrence itself (same shape as the Mandelbrot kernel)
#   - IEEE edge values: NaN, +/-Inf, signed zero, propagated through the
#     unboxed `(re, im)` fast path exactly as the boxed/frame path would
#   - repeated calls at the same call site so the specialization cache/fast
#     path is exercised more than once (not just a cold first call)
#   - an early-return escape boundary (materializes back to a genuine
#     `Complex{Float64}` return value)
#   - a call whose argument does *not* fit the fast path (a plain `Int64`
#     pair) to confirm the generic/fallback path still runs correctly
#     alongside the new fast path

function escape_point(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

# Returns the whole final `z` (a genuine escape-boundary materialization back
# to a boxed `Complex{Float64}` return value).
function final_z(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return z
        end
        z = z * z + c
    end
    return z
end

function add_ints(a, b)
    a + b
end

tests_passed = true

# --- Ordinary escape counts (finite values), called several times to warm
# the specialization / fast-path cache at this call site.
r1 = escape_point(0.1 + 0.2im, 50)
r2 = escape_point(-1.0 + 0.0im, 50)
r3 = escape_point(0.1 + 0.2im, 50) # repeat: same signature, cache should hit
global tests_passed = tests_passed && r1 == 50
global tests_passed = tests_passed && r2 == 50
global tests_passed = tests_passed && r3 == r1

# --- NaN propagates through abs2/comparison exactly like upstream: `NaN > 4.0`
# is `false`, so the loop runs to `maxiter` without ever taking the escape
# branch.
r_nan = escape_point(NaN + 0.0im, 10)
global tests_passed = tests_passed && r_nan == 10

# --- Inf: `z` starts at exact zero, so the *first* iteration's guard sees
# `abs2(0.0) > 4.0 == false` and updates `z = 0*0 + c == c == Inf + 0.0im`;
# the *second* iteration's guard then sees `abs2(Inf) == Inf > 4.0 == true`
# and escapes at `k=2`, returning `k - 1 == 1`.
r_inf = escape_point(Inf + 0.0im, 10)
global tests_passed = tests_passed && r_inf == 1

# --- Signed zero: `-0.0 + 0.0im` behaves identically to `0.0 + 0.0im` for
# this recurrence (repeated squaring of exact zero stays exact zero), so the
# loop must run to `maxiter` without spuriously escaping.
r_negzero = escape_point(-0.0 + 0.0im, 5)
global tests_passed = tests_passed && r_negzero == 5

# --- Escape-boundary materialization: the full final `z` value round-trips
# back through a boxed `Complex{Float64}` (this `c` escapes at k=1, so the
# returned `z` is exactly `c` itself, per the recurrence's first update).
z1 = final_z(2.0 + 2.0im, 50)
global tests_passed = tests_passed && real(z1) isa Float64 && imag(z1) isa Float64
global tests_passed = tests_passed && z1 == 2.0 + 2.0im
global tests_passed = tests_passed && abs2(z1) > 4.0

# --- Non-complex mixed args at a *different* call site: confirms the
# fallback/generic path (not the new ComplexF64 fast path) still produces
# correct results when neither argument is complex.
r_plain = add_ints(3, 4)
global tests_passed = tests_passed && r_plain == 7

tests_passed
