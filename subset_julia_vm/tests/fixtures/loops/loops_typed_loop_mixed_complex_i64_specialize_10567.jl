# Issue #10567 (round 2): the typed-loop recognizer's narrow mixed-arg
# `f(complex_arg, i64_arg)` specialize call op
# (`TypedLoopOp::CallSpecializeComplexI64Function`), exercising exactly the
# `mandel_count`-shaped idiom (`total += f(cr + ci*im, maxiter)` inside a
# `for` loop) from `benchmarks/mandelbrot_bench_for_untyped.jl`.
#
# Covers:
#   - the accumulator recurrence itself (mandel_point/mandel_count shape)
#   - IEEE edge values (NaN, +/-Inf, signed zero) flowing through the
#     unboxed (re, im) fast path the same as the boxed/frame path
#   - an escape-boundary case that returns/stores the whole Complex value
#     (not just an Int64), forcing materialization back to a boxed
#     Complex{Float64}
#   - a call site the recognizer must still REJECT (three arguments, not the
#     narrow two-argument (complex, i64) shape) — the generic interpreter
#     fallback must still compute the correct answer

function mandel_point(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandel_count(width, height, maxiter)
    total = 0
    for y in 1:height
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        for x in 1:width
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total += mandel_point(cr + ci * im, maxiter)
        end
    end
    total
end

# --- IEEE edge values through a typed loop calling the mixed-arg site once
# per iteration, accumulating an Int64 total (same call shape as above, but a
# fixed complex value swept across NaN/Inf/-0.0 rather than a grid).
function escape_sum(cs, maxiter)
    total = 0
    for i in 1:length(cs)
        total += mandel_point(cs[i], maxiter)
    end
    total
end

# --- Escape-boundary: return the final `z` itself (a full Complex{Float64}),
# not just the Int64 iteration count, so the mixed-arg call's result must
# also be exercised when materializing something OTHER than an I64 back out
# of the typed loop.
function final_z_point(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return z
        end
        z = z * z + c
    end
    return z
end

function collect_final_zs(cs, maxiter)
    total_re = 0.0
    total_im = 0.0
    for i in 1:length(cs)
        z = final_z_point(cs[i], maxiter)
        total_re += real(z)
        total_im += imag(z)
    end
    (total_re, total_im)
end

# --- A three-argument mixed call site the narrow recognizer must NOT
# recognize (only the exact 2-arg (complex, i64) shape is handled) — the
# whole caller loop stays on the generic interpreter, and must still compute
# the right answer.
function mandel_point3(c, maxiter, bias)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1 + bias
        end
        z = z * z + c
    end
    return maxiter + bias
end

function mandel_count3(width, height, maxiter, bias)
    total = 0
    for y in 1:height
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        for x in 1:width
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total += mandel_point3(cr + ci * im, maxiter, bias)
        end
    end
    total
end

tests_passed = true

# --- Main accumulator recurrence (small grid, warms the call site several
# times so the specialization/typed-loop fast path is exercised repeatedly).
r1 = mandel_count(20, 20, 100)
global tests_passed = tests_passed && r1 == 9180
r2 = mandel_count(20, 20, 100) # repeat: exercises the resolved-per-entry cache again
global tests_passed = tests_passed && r2 == r1

# --- IEEE edge values.
edge_cs = Complex{Float64}[
    NaN + 0.0im,
    Inf + 0.0im,
    -0.0 + 0.0im,
    0.0 + -0.0im,
    -Inf + 0.0im,
]
r_edge = escape_sum(edge_cs, 10)
# NaN: abs2(NaN) > 4.0 is false, so it always runs to maxiter (10).
# Inf: escapes at k=2 (see the walkthrough in complex_mixed_arg_call_specialize_10567.jl).
# -0.0 + 0.0im / 0.0 + -0.0im: signed zero behaves like exact zero, runs to maxiter (10).
# -Inf: escapes at k=2 as well (abs2(-Inf) is Inf > 4.0).
global tests_passed = tests_passed && r_edge == (10 + 1 + 10 + 10 + 1)

# --- Escape-boundary materialization: the whole Complex{Float64} `z`
# (not just an Int64) round-trips through the typed loop's mixed-arg call.
# First point escapes immediately (k=1): z == c == 2.0 + 2.0im. Second point
# (-1.0+0.5im) never escapes within 50 iterations for this recurrence, so its
# contribution is whatever the recurrence converges/cycles to after 50 steps
# — pinned exactly against upstream `julia` rather than guessed, since the
# orbit is not simply "stays near its starting value".
zs_cs = Complex{Float64}[2.0 + 2.0im, -1.0 + 0.5im]
(sum_re, sum_im) = collect_final_zs(zs_cs, 50)
global tests_passed = tests_passed && sum_re isa Float64 && sum_im isa Float64
global tests_passed = tests_passed && sum_re ≈ -0.6183929443359375
global tests_passed = tests_passed && sum_im ≈ 2.890380859375

# --- Rejected shape: three-argument call site must still compute correctly
# via the generic interpreter fallback (never natively accepted by the
# narrow two-argument recognizer).
r3 = mandel_count3(20, 20, 100, 5)
global tests_passed = tests_passed && r3 == r1 + 20 * 20 * 5

tests_passed
