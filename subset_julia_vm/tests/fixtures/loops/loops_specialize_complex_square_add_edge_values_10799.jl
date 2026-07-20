# Issue #10799: the runtime specializer's ComplexF64 codegen for
# `z*z+c`/`z^2+c`/`abs2(z)` was rewritten to read a bare split-local
# variable's (re, im) fields directly from its persistent slots (instead of
# compiling+spilling to fresh temp locals every recursion level), so the
# shared Instr-level peephole and the typed-loop predecoder's
# `fuse_typed_loop_ops`/`fuse_complex_mul_add_assign` passes can fuse the
# result the same way they fuse the static compiler's SROA'd output.
#
# This fixture pins CORRECTNESS of that rewrite against upstream `julia`,
# specifically the IEEE edge cases a wrong peephole/fusion match would be
# most likely to corrupt: NaN, +/-Inf, signed zero, and values whose escape
# happens on iteration 1 vs. never within maxiter. Every case below is
# reached via the UNTYPED (runtime-specialized) path — no type annotations
# on `mandelbrot_escape`'s parameters — matching
# `benchmarks/mandelbrot_bench_broadcast_untyped.jl` and
# `benchmarks/mandelbrot_bench_for_untyped.jl`.

function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandelbrot_escape_pow(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z^2 + c
    end
    return maxiter
end

tests_passed = true

# Force runtime specialization on both call shapes before probing values, so
# the assertions below exercise the specialized (not the generic fallback)
# path.
mandelbrot_escape(0.0 + 0.0im, 5)
mandelbrot_escape_pow(0.0 + 0.0im, 5)

# --- Interior point: never escapes within maxiter (c=0 stays at the origin). ---
global tests_passed = tests_passed && mandelbrot_escape(0.0 + 0.0im, 50) == 50
global tests_passed = tests_passed && mandelbrot_escape_pow(0.0 + 0.0im, 50) == 50

# --- Immediate escape: |c| already > 2, so |z|^2 > 4 by iteration 1 (z=0+0im
# does not itself escape; z=c=3+3im on the next check does). ---
global tests_passed = tests_passed && mandelbrot_escape(3.0 + 3.0im, 50) == 1
global tests_passed = tests_passed && mandelbrot_escape_pow(3.0 + 3.0im, 50) == 1

# --- Boundary point on the real axis: stays bounded for the full maxiter
# (verified against upstream `julia`). ---
global tests_passed = tests_passed && mandelbrot_escape(-1.75 + 0.0im, 50) == 50
global tests_passed = tests_passed && mandelbrot_escape_pow(-1.75 + 0.0im, 50) == 50

# --- Signed zero: -0.0 must stay distinct from 0.0 through square+add. ---
c_negzero = -0.0 + 0.0im
r_negzero = mandelbrot_escape(c_negzero, 50)
global tests_passed = tests_passed && r_negzero == 50
global tests_passed = tests_passed && mandelbrot_escape_pow(c_negzero, 50) == r_negzero

# --- Infinity: c with an infinite component must escape almost immediately
# and never let squaring produce a silently-wrong finite value. ---
global tests_passed = tests_passed && mandelbrot_escape(Inf + 0.0im, 50) == 1
global tests_passed = tests_passed && mandelbrot_escape(0.0 + (-Inf) * im, 50) == 1
global tests_passed = tests_passed && mandelbrot_escape_pow(Inf + 0.0im, 50) == 1

# --- NaN: abs2(z) > 4.0 is false whenever z carries NaN (IEEE: any
# comparison with NaN is false), so the loop must run to maxiter, not
# spuriously escape or crash. ---
global tests_passed = tests_passed && mandelbrot_escape(NaN + 0.0im, 7) == 7
global tests_passed = tests_passed && mandelbrot_escape_pow(NaN + 0.0im, 7) == 7

# --- General interior point requiring several iterations, with a
# non-trivial (re, im) pair on both sides of the `z=z*z+c` recurrence —
# the shape the #10799 fusion rewrite specifically targets. ---
r_general = mandelbrot_escape(-0.75 + 0.1im, 100)
global tests_passed = tests_passed && r_general == mandelbrot_escape_pow(-0.75 + 0.1im, 100)
global tests_passed = tests_passed && r_general == 33

# --- abs2 alone, independent of the recurrence, across the same edge values
# (the specializer's abs2 rewrite is a separate code path from the binary
# +/* rewrite; cover it directly too). ---
global tests_passed = tests_passed && abs2(0.0 + 0.0im) == 0.0
global tests_passed = tests_passed && abs2(-0.0 + 0.0im) == 0.0
global tests_passed = tests_passed && abs2(3.0 + 4.0im) == 25.0
global tests_passed = tests_passed && isnan(abs2(NaN + 1.0im))
global tests_passed = tests_passed && abs2(Inf + 0.0im) == Inf

tests_passed
